# p64tool — Retevis MateTalk P64 / P4 programmer for Linux

[![CI](https://github.com/oetiker/p64tool/actions/workflows/ci.yml/badge.svg)](https://github.com/oetiker/p64tool/actions/workflows/ci.yml)

A native Linux tool to configure the **Retevis MateTalk P64** (a.k.a. Retevis P4)
DMR walkie-talkie over its USB programming cable — **no Windows, no Wine, no VM**.

The manufacturer only ships a Windows configuration program (CPS). `p64tool`
replaces it on Linux: it reads the radio's entire configuration, lets you edit it
as a plain-text file, and writes it back. It has full feature parity with the
Windows CPS — channels, contacts, talkgroups, zones, scan lists, text messages,
emergency systems, one-touch buttons and encryption keys.

The radio's serial protocol and memory layout were recovered by decompiling the
Windows CPS to obtain the interface information needed for interoperability (see
[Legal & disclaimers](#legal--disclaimers)). The wire format is documented in
[PROTOCOL.md](PROTOCOL.md).

## How it works

The radio stores all its settings in one binary blob called a **codeplug**. The
Windows CPS reads that blob, shows it in a GUI, and writes it back. `p64tool`
does the same, but exposes the codeplug as an editable [TOML](https://toml.io)
file instead of a GUI:

```
        read                 decode                edit               write
 radio ──────► raw dump ───────────► radio.toml ─────────► radio.toml ──────► radio
        (13 regions)          (human-readable)     (your editor)      (verified)
```

1. **`read`** dumps the codeplug off the radio into a directory of raw region
   files (also a handy backup).
2. **`decode`** turns that dump into a readable `radio.toml`.
3. You **edit** `radio.toml` in any text editor.
4. **`write`** applies your edits back onto the codeplug and sends it to the radio.

### Why it is safe to write to your radio

Writing a bad codeplug can misconfigure a radio, so the tool is built to be
conservative and self-checking:

- **Only mapped fields are touched.** `write` starts from the radio's *actual
  current codeplug* and changes only the bytes for settings you edited. Every
  byte the tool doesn't understand is preserved verbatim — it never rewrites the
  codeplug from scratch.
- **Only changed regions are sent.** The codeplug is 13 independent regions; a
  channel-name edit sends just the one region that changed, shrinking the blast
  radius. (`--all` forces a full write.)
- **Post-write read-back verification.** After writing, the tool reads the whole
  codeplug back and compares it against the image it intended to write. If a
  single byte differs, it says so and tells you not to trust the write.
- **A built-in round-trip proof.** The `roundtrip` command reads a codeplug,
  decodes it to the config model, re-encodes it, and asserts the result is
  **byte-for-byte identical** to the original across all 13 regions. This is the
  core correctness guarantee: it proves the decode and encode paths agree, so an
  edit can only change exactly what you asked for. The pure encoders (DMR-ID BCD,
  CTCSS/DCS tones, encryption-key byte order, UTF-16 names) also have unit tests
  that run in CI.

A good habit regardless: keep the backup that `read` produces, so you can always
restore a known-good codeplug.

## Installation & radio setup

**Get the binary.** Download a prebuilt archive from the
[releases page](https://github.com/oetiker/p64tool/releases) and unpack it. The
Linux builds are fully static (musl), so the `p64tool` binary runs on any Linux
box with no dependencies or toolchain. Or build from source:

```sh
cargo build --release                                    # native
cargo build --release --target x86_64-unknown-linux-musl # static/portable
```

**Connect the radio.** Plug in the programming cable and turn the radio on. The
cable is a USB-to-serial adapter, so it shows up as a serial port:

```sh
ls -l /dev/ttyUSB*        # usually /dev/ttyUSB0
dmesg | tail              # confirm which device appeared
```

**Get access to the port.** Either add yourself to the `dialout` group (then log
out and back in), or prefix the commands with `sudo`:

```sh
sudo usermod -aG dialout "$USER"
```

**Check the connection:**

```sh
p64tool info --port /dev/ttyUSB0
# expect: Radio responded to connect (149 bytes).
```

## DMR concepts you need to know

An **analog** channel is simple: a frequency and an optional sub-tone. A
**digital (DMR)** channel is *addressable* — closer to a small phone/group
network — and several coordination layers must line up between radios before
they can talk. Understanding these makes the config file obvious.

### The addressing layers (coarse to fine)

1. **Frequency — *where*.** On PMR446 this is one of 16 fixed channels; you don't
   change it. Two radios must share a frequency to communicate at all.

2. **Timeslot (TS1 / TS2).** DMR splits each frequency into two time-interleaved
   slots (TDMA), so one frequency can carry two independent conversations. For
   plain simplex PMR446, everyone stays on TS1; both slots are only usable with
   the P4 firmware's "TDMA direct mode". Config field: `time_slot`.

3. **Color code (0–15) — *not a colour*.** A digital access tag, the DMR
   equivalent of an analog CTCSS "privacy tone" (the name is from the DMR
   standard). A radio only un-mutes for transmissions whose color code matches
   its own. Everyone in a group shares one; a different group on the same
   frequency and timeslot with a different color code is simply ignored. Field:
   `color_code`.

4. **Device IDs — *who*.** DMR identifies radios, not just frequencies:
   - **Your radio's own ID** (`radio_dmr_id`) is its identity — sent as caller-ID
     on every transmission, and the number others dial to call it privately. Give
     every radio in your fleet a **unique** ID.
   - **Contacts** (`[[contact]]`) are the things you call. Each has an ID and a
     `call_type`: `group` (the ID is a **talkgroup** everyone tuned to it hears),
     `private` (the ID is **one specific radio**, a one-to-one call), or `all`
     (the special ID `16777215`, heard by everyone).

5. **RX group list** (`[[rx_group]]`) — the set of **talkgroups a channel
   listens to**. A channel transmits on its `contact` and un-mutes for every
   talkgroup in its `rx_group`, so you can monitor several groups at once.

6. **Encryption** (`encrypt_key`) — privacy layered *on top*. It hides the
   content from anyone without the key.

### How two groups stay separate

Un-muting is decided *before* the audio is played, so separation happens at these
layers. Make **at least one** differ and two groups won't hear each other:

| Layer      | Field                  | Same value →                  | Different value →              |
|------------|------------------------|-------------------------------|--------------------------------|
| Frequency  | (the channel)          | share the airwaves            | fully separate                 |
| Timeslot   | `time_slot`            | same slot                     | independent conversations      |
| Color code | `color_code`           | radios consider each other    | radios ignore each other       |
| Talkgroup  | `contact` / `rx_group` | hear the group                | not addressed to them          |

**Encryption is deliberately not in this table.** Different keys keep the content
secret, but the radios still share the airwaves: they will still *interfere* if
they transmit at once, and a radio may un-mute to unintelligible noise. Separate
groups with frequency / color code / talkgroup; use encryption for *secrecy*, not
for separation.

### The channel knob and zones

The P64 has **no display**. You operate it with a 16-position **channel knob**, a
top volume/power knob, the PTT (large side button) and **two programmable small
side buttons** (each with a short- and long-press function).

Because there's no screen, a "channel" is really a whole **operating profile** —
frequency, mode, tones or color-code/talkgroup, power, encryption, etc. — and
turning the knob selects *how you're operating*, not just a frequency.

Channels are organised into **zones**. A zone is an **ordered list of up to 16
channels**, and **knob position P selects the P-th entry in the current zone's
list**. A side key assigned to "Zone Selection" switches zones. With 16 zones ×
16 positions the radio addresses all 256 channel slots. The channel *slot number*
is just storage — the **order of a zone's list is the knob layout**. A channel
may appear in several zones (e.g. a common calling channel on position 16 of
every zone).

With `voice_prompt` enabled the radio announces the channel *number* by voice
when you turn the knob — the closest thing to a display it has.

### Emergency alarms

The radio can send a DMR **emergency alarm**: a panic signal that alerts your
group and can auto-open your microphone. An *alarm profile* (`[[alarm]]`) defines
how it behaves; a channel references one via `emergency_system`; and you fire it
with a side key assigned to "Emergency On". The **man-down** (tilt/fall) and
**work-alone** (missed check-in) settings can trigger the same alarm
automatically. See [Emergency setup](#emergency--lone-worker) below.

## Command reference

All commands are subcommands of `p64tool`. Run `p64tool <cmd> --help` for the
exact flags.

| Command     | What it does                                                              |
|-------------|---------------------------------------------------------------------------|
| `info`      | Connect, print the radio's identity, disconnect. A quick liveness check.  |
| `read`      | Dump the whole codeplug into a directory of raw region files (+ a backup).|
| `decode`    | Turn a dump into an editable `radio.toml`.                                 |
| `check`     | Validate a `radio.toml` against a country regulation profile.             |
| `write`     | Apply a config onto the codeplug and write it to the radio (verified).    |
| `roundtrip` | Self-test: decode a dump, re-encode it, assert byte-identical.            |

### `read`

```sh
p64tool read --port /dev/ttyUSB0 --out mydump
```

Writes `mydump/`: one `.bin` per memory region (`r01.bin` … `rML.bin`),
`codeplug_raw.bin` (all regions concatenated), and `manifest.txt`. Add
`--verbose` to see every command/response.

### `decode`

```sh
p64tool decode mydump -o radio.toml --country CH
```

- `--comments` — annotate every setting with a one-line explanation.
- `--expert` / `-x` — also emit fields hidden by default (see
  [Hidden / expert fields](#hidden--expert-fields)).
- `--country CH` — record the country profile in the file (drives `check`).

### `check`

```sh
p64tool check radio.toml --country CH
```

Validates against the country profile (band limits, simplex-only, bandwidth) and
reports errors and warnings. `write` runs this automatically and refuses to
proceed if there are errors.

### `write`

```sh
p64tool write --port /dev/ttyUSB0 radio.toml --yes
```

- The config is applied onto a **base codeplug**: the radio's live contents by
  default, or a saved dump if you pass `--from-dump mydump`.
- Only regions that actually changed are written; add `--all` to write every
  region (also used for an identity write with no config).
- `--yes` is required to actually write (safety gate).
- After writing, the codeplug is read back and verified; `--no-verify` skips that.
- `--country` selects the profile for the mandatory pre-write regulation check.

### `roundtrip`

```sh
p64tool roundtrip mydump
```

Reads the dump, decodes and re-encodes it, and confirms the result is
byte-identical across all regions. Useful after a firmware change to confirm the
codeplug layout still matches before trusting a write.

## The config file

`decode` produces a TOML file with a `[general]` block of radio-wide settings and
repeated `[[...]]` blocks for each list (channels, contacts, …). Edit it in any
text editor, then `check` and `write` it.

### Structure

| Section              | Contents                                                          |
|----------------------|-------------------------------------------------------------------|
| `[general]`          | Radio-wide settings: channel mode, squelch, VOX, volumes, mic gains, voice prompt, power-save, radio DMR ID, man-down, work-alone, side-key assignments, programming password. |
| `[general.tones]`    | The ~22 alert-tone toggles, talk-permit tone, master mute.        |
| `[[channel]]`        | One per channel: name, mode, power; digital adds color code / contact / RX group / encryption / emergency / scan; analog adds CTCSS/DCS tones. |
| `[[contact]]`        | A talkgroup or a radio to call: name, DMR ID, call type.          |
| `[[rx_group]]`       | A named list of contacts (talkgroups) a channel listens to.       |
| `[[zone]]`           | A named, ordered list of channels = one knob layout.              |
| `[[scan]]`           | A named list of channels to scan, plus priority channels.         |
| `[[message]]`        | A preset text message.                                            |
| `[[alarm]]`          | An emergency/alarm profile.                                       |
| `[[one_touch]]`      | The 6 one-touch call/message slots.                               |
| `[[enc_key]]`        | An encryption key (ARC4 / AES128 / AES256).                       |

Not stored in the codeplug on this model (confirmed from the CPS), so not in the
file: GPS settings and the real-time clock (the clock is set by a separate live
command).

### How list references work

Lists are referenced by **1-based position**. A channel's `contact = 1` means
"the first `[[contact]]` block", `scan_list = 2` means "the second `[[scan]]`",
and so on; `0` means "none". **Reordering a list changes what the references
point at** — keep the `index` fields and the reference numbers in sync.

### Comments and hidden fields

Run `decode --comments` and every setting gets an inline explanation:

```toml
squelch = 3            # 0 (open) .. 9 (tight)
color_code = 1         # DMR color code 0..15 (must match the other radios)
contact = 1            # digital TX target: the [[contact]] (talkgroup) you transmit to
```

### Hidden / expert fields

By default the file omits settings that are fixed or illegal to change for
PMR446, so you only see things worth editing. Pass `--expert` to reveal them:

- **Channel frequencies** (`rx_mhz` / `tx_mhz`) — the PMR446 grid is fixed.
- **Timeslot** (`time_slot`) — must stay TS1 unless you run TDMA direct mode.
- **Bandwidth** (`bandwidth_khz`) — fixed at 12.5 kHz; 25 kHz would be illegal.
- **`[general.dmr_service]`** — advanced DMR timing/decode internals (preamble,
  call hang-times, remote monitor/kill).

**A field left out of the file is preserved on the radio, never zeroed.** So
hiding these keeps whatever the radio already has.

### Encryption keys

`[[enc_key]]` holds the key as a plain hex string — the same order the Windows
CPS shows and exactly what `openssl rand -hex` produces:

```toml
[[enc_key]]
index = 1
name = "team-a"
key_type = "aes256"        # arc4 | aes128 | aes256
key = "bfcf0af0…a24ac5c"   # 64 hex chars (AES256); 32 for AES128; variable for ARC4
```

```sh
openssl rand -hex 32   # generate an AES256 key
openssl rand -hex 16   # generate an AES128 key
```

Key length is validated against the type. A channel enables encryption by
referencing a key slot: `encrypt_key = 1`.

## Use cases

Start from a decoded backup and edit `radio.toml`:

```sh
p64tool read   --port /dev/ttyUSB0 --out backup
p64tool decode backup -o radio.toml --country CH --comments
$EDITOR radio.toml
p64tool check  radio.toml --country CH
p64tool write  --port /dev/ttyUSB0 --from-dump backup radio.toml --yes
```

### Rename channels for your team

Names are labels used here and in the CPS to keep track of channels — the radio
has no display, so it neither shows nor speaks them (with `voice_prompt` it
announces the channel *number*). Keep the channel/zone order meaningful.

```toml
[[channel]]
index = 1
name = "Ops-1"
mode = "analog"
```

### Analog privacy tones (CTCSS/DCS)

So you only hear your own group. Use the same tone on RX and TX on every radio:

```toml
[[channel]]
index = 1
name = "Ops-1"
mode = "analog"
rx_tone = "88.5"     # CTCSS 88.5 Hz  (or "D023N" for DCS)
tx_tone = "88.5"
```

### A digital talkgroup

Define the talkgroup as a contact, then point a digital channel at it. Every
radio needs the same `color_code`:

```toml
[[contact]]
index = 1
name = "Team"
dmr_id = 9            # talkgroup number
call_type = "group"

[[channel]]
index = 1
name = "Team-D"
mode = "digital"
color_code = 1
contact = 1          # -> [[contact]] #1
```

### A minimal encrypted digital net

No registration is needed or possible on PMR446 — pick IDs locally, just keep
them consistent within your group:

1. Give each radio a unique `radio_dmr_id` (1, 2, 3, …).
2. Define one talkgroup contact.
3. On your digital channels set a shared `color_code`, a `contact`, and an
   `rx_group` containing that contact.
4. Add an encryption key and reference it:

```toml
[[enc_key]]
index = 1
name = "team-a"
key_type = "aes256"
key = "…64 hex chars from: openssl rand -hex 32…"

[[channel]]
index = 1
name = "Secure"
mode = "digital"
color_code = 1
contact = 1
encrypt_key = 1      # -> [[enc_key]] #1
```

Don't reuse amateur-network DMR IDs (e.g. 7-digit Brandmeister IDs) on PMR446 —
those belong to the ham system, not license-free use.

### Lay out the knob with zones

A zone's `channels` list *is* the knob order — position 1 is the first entry,
etc. This puts channel 40 on knob position 1, channel 12 on position 2, …:

```toml
[[zone]]
index = 1
name = "Day trip"
channels = [40, 12, 3, 9]
```

(Factory layout: zone 1 = the 16 digital channels, zone 2 = the 16 analog ones.)

### Emergency / lone-worker

An alarm profile plus a channel that references it. Since there's no dedicated
emergency button, assign **Emergency On** (key code `13`) to a side key:

```toml
[[alarm]]
index = 1
name = "SOS"
alarm_type = 2          # 0=none 1=siren 2=normal 3=silent 4=silent+voice
alarm_mode = 2          # 0=alarm 1=alarm+call 2=alarm+open-mic
revert_channel = 0      # 0 = send on the current channel
impolite_retries = 15   # push through even a busy channel
polite_retries = 0
hot_mic_s = 15          # keep the mic open 15 s after triggering
rx_dwell_s = 10

[[channel]]
index = 1
name = "Work"
mode = "digital"
emergency_system = 1    # -> alarm profile #1

[general]
radio_dmr_id = 3
key_side2_long = 13     # long-press small side key 2 = Emergency On
```

The `[general]` man-down and work-alone settings can fire the same alarm
automatically for cases where the operator can't press a button.

## Building from source

```sh
cargo build --release                                    # native
cargo build --release --target x86_64-unknown-linux-musl # static/portable
cargo test                                               # run the unit tests
cargo run --release -- roundtrip mydump                  # self-check against a dump
```

## License

MIT — see [LICENSE](LICENSE). Copyright © 2026 Tobi Oetiker.

## Legal & disclaimers

- **No affiliation.** This is an independent, unofficial tool. "Retevis",
  "MateTalk" and "P64" are trademarks of their respective owners, used here only
  to describe compatibility. This project is **not affiliated with, authorised by
  or endorsed by Retevis.**
- **How it was made.** The codeplug protocol and field layout were determined by
  decompiling the manufacturer's freely-distributed Windows programming software
  to obtain the interface information needed for interoperability — permitted for
  that purpose under Swiss copyright law (URG/CopA Art. 21) and the equivalent EU
  provision. **No manufacturer code, binaries, installers or data files are
  included in this repository;** all source here is original work.
- **No cryptographic algorithms.** The tool only reads and writes encryption
  *key bytes* in the codeplug; the ARC4/AES ciphers run on the radio, not here.
- **No warranty.** Provided "as is" (see LICENSE). Writing to a radio can
  misconfigure it; use at your own risk, and keep the backup that `read` makes.
- **Your responsibility to operate legally.** Radio regulations vary by country.
  The built-in profiles (e.g. PMR446 for CH/CEPT) help, but **you** are
  responsible for ensuring your configuration and use comply with the rules in
  your jurisdiction and for your licence class.

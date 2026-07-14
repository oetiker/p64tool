# p64tool — Retevis MateTalk P64 / P4 programmer for Linux

[![CI](https://github.com/oetiker/p64tool/actions/workflows/ci.yml/badge.svg)](https://github.com/oetiker/p64tool/actions/workflows/ci.yml)

A native Linux tool to talk to the **Retevis MateTalk P64** (a.k.a. Retevis P4)
DMR radio over its USB programming cable — no Windows, no Wine.

The protocol was recovered by decompiling the Windows CPS (`P64 V1.4.exe`, a
.NET assembly). See `PROTOCOL.md` for the wire format.

## Status

- **`read`** — dumps the radio's entire codeplug. ✅ Hardware-validated.
- **`info`** — quick liveness check. ✅ Hardware-validated.
- **`decode`** — codeplug dump → editable TOML. ✅ Full codeplug coverage.
- **`check`** — validate a TOML against a country regulation profile. ✅
- **`roundtrip`** — proves decode→apply is byte-faithful across all 13 regions. ✅

### What the TOML covers (feature parity with the Windows CPS)

- **`[general]`** — channel mode, squelch, VOX, volumes, password, mic gains/AGC,
  TX-end tone, voice prompt, RX sensitivity, scan method, power-save, DMR radio
  ID + service settings (preamble, hang times, remote monitor, decode flags,
  remote-kill ID), man-down, work-alone, side-key assignments, long-press time.
- **`[general.tones]`** — all ~22 alert-tone toggles + talk-permit + master mute.
- **`[[channel]]`** — name, mode, RX/TX, power, bandwidth, RX-only; digital:
  color code, time slot, contact, RX group, encryption key, emergency system,
  scan list; analog: CTCSS/DCS RX & TX tones.
- **`[[contact]]`** — name, DMR ID, call type. **`[[rx_group]]`** — name + members.
- **`[[zone]]`** / **`[[scan]]`** — name + member channels (scan also priorities).
- **`[[message]]`** — quick-text. **`[[alarm]]`** — emergency/alarm systems.
- **`[[one_touch]]`** — the 6 one-touch slots. **`[[enc_key]]`** — ARC4/AES128/AES256.

Not codeplug-stored on this model (confirmed from the CPS), so not in the TOML:
GPS settings and the real-time clock (the clock is a separate live serial command).

### Encryption keys

`[[enc_key]]` holds the key as a plain hex string (`key`), in the same order the
Windows CPS shows and exactly what `openssl rand -hex` produces:

```toml
[[enc_key]]
index = 1
name = "team-a"
key_type = "aes256"        # arc4 | aes128 | aes256
key = "bfcf0af0...a24ac5c" # 64 hex chars (AES256); 32 for AES128; variable for ARC4
```

Generate keys with openssl:
```sh
openssl rand -hex 32   # AES256 (32-byte) key
openssl rand -hex 16   # AES128 (16-byte) key
```
Length is validated against the type. A channel uses a key by referencing its
slot: set `encrypt_key = 1` on the `[[channel]]` to enable key #1.

List references are **1-based positions** (a channel's `contact = 1` means the
first `[[contact]]`; reordering the list changes the references).

**`decode` flags:** `--comments` annotates every setting with a one-line
explanation; `--expert` (`-x`) additionally emits fields that are hidden by
default because they shouldn't be touched in normal PMR446 use: the channel
**frequencies** (the PMR446 grid is fixed), the digital **timeslot** (must stay
TS1 unless you run TDMA direct mode), the **bandwidth** (fixed at 12.5 kHz;
25 kHz would be illegal), and the advanced **`[general.dmr_service]`** timing/
decode internals (preamble, call hang-times, remote monitor/kill). When a field
is absent from the TOML,
`write` keeps the radio's current value — so hiding these preserves them, never
zeroes them.
- **`write`** — write a codeplug back to the radio, with a pre-write regulation
  check and post-write read-back verification. ✅ Logic validated end-to-end
  against a mock radio; **NOT yet run on real hardware** — do the identity test
  below first, on ONE radio, keeping the other as an untouched backup.

## Edit workflow

```sh
./p64tool read   --port /dev/ttyUSB0 --out mydump          # backup + dump
./p64tool decode mydump -o radio.toml --country CH          # -> editable TOML
$EDITOR radio.toml                                          # edit channels/settings
./p64tool check  radio.toml --country CH                    # validate (band, simplex, ...)
./p64tool write  --port /dev/ttyUSB0 radio.toml --yes       # apply, check, write, verify
```

Notes:
- `write` writes **only the regions that changed** vs the base (e.g. a channel
  edit writes just `r08`), then reads everything back to verify. Use `--all` to
  write every region.
- `--from-dump DIR` is **optional**. Without it, `write` reads the radio live as
  the base. With it, your edits are applied onto that saved dump instead. Keeping
  a backup dump is still recommended as a restore point.

## Feature index — what the radio can do, and where to set it

The P64 has far more capability than "16 channels." Here's the menu, each mapped
to the config field/section that unlocks it (run `decode --comments` to get these
explanations inline in your own file):

| Want to… | Set this |
|---|---|
| Rename channels / lay out the knob | `[[channel]].name`, and `[[zone]].channels` order |
| Analog privacy tones (CTCSS/DCS) | `[[channel]].rx_tone` / `tx_tone` |
| Go digital (DMR) | `[[channel]].mode = "digital"` + `color_code`, `contact` |
| Talk to a group (talkgroup) | `[[contact]] call_type="group"`, then channel `contact` |
| One-to-one (private) call | `[[contact]] call_type="private"` (the other radio's ID) |
| Hear several talkgroups at once | `[[rx_group]]` + channel `rx_group` |
| Encrypt a channel (ARC4/AES) | `[[enc_key]]` + channel `encrypt_key` |
| Scan several channels | `[[scan]]` + channel `scan_list` |
| Group channels into knob banks | `[[zone]]` (Zone-select side key switches banks) |
| Emergency / panic alarm | `[[alarm]]` + channel `emergency_system` + an Emergency-On side key |
| Man-down (tilt/fall) auto-alarm | `[general] man_down_*` |
| Lone-worker check-in alarm | `[general] work_alone_*` |
| Preset text messages | `[[message]]` |
| One-touch call/message button | `[[one_touch]]` + a One-Call side key |
| Hands-free (voice-activated TX) | `[general] vox_enable` / `vox_level` |
| Reprogram the side buttons | `[general] key_side1_*` / `key_side2_*` (codes in PROTOCOL.md) |
| Announce channel number by voice | `[general] voice_prompt` |
| Roger beep, alert tones | `[general.tones]` |
| Battery-save, RX sensitivity | `[general] power_save_*`, `rf_sensitivity_enhanced` |
| Lock out reprogramming | `[general] password_enable` |

The sections below explain the two richest areas — **digital addressing** and
**emergency/alarm** — in depth.

## Understanding DMR digital addressing

An analog channel is just a frequency plus an optional sub-tone. A **digital
(DMR)** channel is *addressable* — more like a small phone/group network — so
several coordination layers have to line up between radios before they can talk.
From coarse to fine:

**1. Frequency — *where*.** On PMR446 this is one of the 16 fixed channels; you
don't change it. Two radios must be on the same frequency to talk at all.

**2. Timeslot (TS1 / TS2).** DMR splits each frequency into two time-interleaved
slots (TDMA), so one frequency can carry two independent conversations. For plain
simplex PMR446 use, leave everyone on TS1; using both slots is what the P4
firmware's "TDMA direct mode" unlocks. Field: `time_slot`.

**3. Color code (0–15) — *not a colour*.** It's a digital access tag, the DMR
equivalent of an analog CTCSS "privacy tone" (the name comes straight from the
DMR standard). A radio only un-mutes for transmissions whose color code matches
its own. Everyone in a group shares one color code; a different group on the same
frequency+timeslot with a different color code is simply ignored. Field:
`color_code`. (Analog equivalent: the `rx_tone`/`tx_tone` CTCSS/DCS.)

**4. Device IDs — *who*.** DMR identifies radios, not just frequencies:
- **Your radio's own ID** (`[general] radio_dmr_id`) is its identity — sent as
  caller-ID on every transmission, and what someone dials to call this radio
  privately. Give every radio in your fleet a **unique** ID.
- **Contacts** (`[[contact]]`) are the things you call — each has an ID and a
  `call_type`:
  - `group` → the ID is a **talkgroup**; every radio listening to that talkgroup
    hears it (normal "everyone together" operation).
  - `private` → the ID is **one specific radio's** ID; a one-to-one call.
  - `all` → the special ID **16777215**, heard by every radio.

**5. RX group list** (`[[rx_group]]`, referenced by a channel's `rx_group`) — the
set of **talkgroups this channel listens to**. A channel transmits on its
`contact` and un-mutes for the talkgroups in its `rx_group`.

**6. Encryption** (`encrypt_key`) — privacy layered *on top*. It hides the content
from anyone without the key, but it does **not** separate groups or stop
interference (see below).

### How groups stay separate

Un-muting is decided *before* the content is played, so separation happens at
these layers — make at least one differ and two groups won't hear each other:

| Layer | Field | Same value → | Different value → |
|---|---|---|---|
| Frequency | (the channel) | share the airwaves | fully separate |
| Timeslot | `time_slot` | same slot | independent conversations |
| Color code | `color_code` | radios consider each other | radios ignore each other |
| Talkgroup | `contact` / `rx_group` | hear the group | not addressed to them |

Encryption is deliberately **not** in this table: different keys keep content
secret, but the radios still share the airwaves and may un-mute to noise. Separate
groups with frequency / color code / talkgroup; use encryption for secrecy.

### A minimal digital net (license-free PMR446)

No registration is needed or possible — pick IDs locally, just keep them
consistent within your group:

1. Give each radio a unique `radio_dmr_id` (1, 2, 3, …).
2. Define one talkgroup contact (`call_type = "group"`, e.g. `dmr_id = 9`).
3. On your digital channels set a shared `color_code`, `contact = 1`, and an
   `rx_group` whose list contains that contact. (Timeslot is already TS1 by
   default; it's an expert field you only touch for TDMA direct mode.)
4. Optionally add an `encrypt_key` for privacy.

Don't reuse amateur-network DMR IDs (e.g. 7-digit Brandmeister IDs) on PMR446 —
those belong to the ham system, not license-free use.

## Emergency & alarm

The P64 can send a DMR **emergency alarm** — a panic signal that alerts your
group and can auto-open your microphone so they hear what's happening. It's aimed
at lone-worker / safety use. Two things have to be set up: an *alarm profile* and
a *channel + trigger* that use it.

**1. The alarm profile** (`[[alarm]]`; the radio calls it a "digital alarm
system"). It defines *how* the alarm behaves:

- `alarm_type` — 0=none, 1=siren only, 2=normal (audible), 3=silent,
  4=silent + voice. "Silent" alarms with no local beep/LED, so you don't tip off
  whoever caused the emergency.
- `alarm_mode` — 0=alarm only, 1=alarm then call (you can talk), 2=alarm +
  follow-on voice (auto-opens the mic).
- `revert_channel` — where the alarm transmits: 0 = the current channel, else a
  channel number, so an emergency always goes out on a designated safety channel
  no matter where the knob is.
- `impolite_retries` / `polite_retries` — how many times it retransmits. Polite
  waits for a clear channel; impolite transmits regardless (you usually want the
  alarm to punch through, so keep impolite high).
- `hot_mic_s` — seconds the mic stays open after triggering, hands-free, so your
  group hears the situation.
- `rx_dwell_s` — how long the radio then listens for an acknowledgement.

**2. The channel reference + trigger.** Each channel points at a profile via
`emergency_system` (0 = none, else the `[[alarm]]` number). Because the P64 has
**no dedicated emergency button**, assign the **Emergency On** function (key code
`13`) to a side key — e.g. `key_side2_long = 13`. Pressing it fires the current
channel's alarm profile. (Emergency Off is code `12`; the radio auto-pairs them —
if a key's short press is Emergency On, its long press becomes Emergency Off.)

### Example: a lone-worker safety channel

```toml
[[alarm]]
index = 1
name = "SOS"
alarm_type = 2          # normal (audible)
alarm_mode = 2          # alarm + open mic
revert_channel = 0      # send on the current channel
impolite_retries = 15   # push through even on a busy channel
polite_retries = 0
hot_mic_s = 15          # 15 s of open mic after triggering
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

The **man-down** and **work-alone** settings in `[general]` can fire the same
alarm *automatically*: man-down triggers if the radio tilts past
`man_down_angle_deg` or falls; work-alone triggers if the operator doesn't
check in within `work_alone_response_s`. Both are for cases where the person
can't press a button themselves.

## Common setups

Start from `./p64tool decode backup -o radio.toml --country CH --comments`
(add `--comments` for inline explanations, `--expert` to also show frequencies).
Edit `radio.toml`, then `check` and `write`. Only changed regions are written.

**Rename channels for your team.** The P64 has no display, so names are just
labels used here (and in the Windows CPS) to keep track of channels — the radio
neither shows nor speaks them. On the radio you select channels by the knob
position; with `voice_prompt` on it announces the channel *number*. So keep the
channel order meaningful. Edit the `name` of each `[[channel]]`:
```toml
[[channel]]
index = 1
name = "Ops-1"
mode = "analog"
```

**Add analog privacy tones (CTCSS/DCS).** So you only hear your own group. Set the
same tone on RX and TX for every radio in the group:
```toml
[[channel]]
index = 1
name = "Ops-1"
mode = "analog"
rx_tone = "88.5"     # CTCSS 88.5 Hz  (or "D023N" for DCS)
tx_tone = "88.5"
```

**Set up DMR digital with a talkgroup.** Define a contact (talkgroup), then point a
digital channel at it. All radios need the same `color_code` and `time_slot`:
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
time_slot = 1
contact = 1          # -> [[contact]] #1
```

**Encrypt a channel.** Generate a key with openssl, add it, and reference it:
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
encrypt_key = 1      # -> [[enc_key]] #1
```

**Group channels into a zone = lay out the knob.** The P64 has a 16-position
channel knob and no display. A zone is an **ordered list of up to 16 channels**,
and **knob position P selects the P-th entry in the current zone's list**. The
Zone-select side key switches zones (16 zones × 16 positions = the 256 slots).
The channel *slot number* is just a reference — the zone list order is what the
knob actually visits. So this puts channel 40 on knob position 1, channel 12 on
position 2, etc.:
```toml
[[zone]]
index = 1
name = "Day trip"
channels = [40, 12, 3, 9]   # knob pos 1..4 -> these channel numbers, in order
```
(Factory layout: zone 1 = the 16 digital channels, zone 2 = the 16 analog ones.)

After editing:
```sh
./p64tool check radio.toml --country CH
./p64tool write --port /dev/ttyUSB0 --from-dump backup radio.toml --yes
```

## FIRST hardware write test (do this before trusting write)

On ONE radio, with a fresh backup, write the *unchanged* image back and let the
tool verify it read back identically. No settings change; it only proves the
write path is byte-faithful on real hardware:

```sh
./p64tool read  --port /dev/ttyUSB0 --out backup
./p64tool write --port /dev/ttyUSB0 --from-dump backup --all --yes   # identity write
# expect: "Verification OK: the radio now matches the intended image."
```

If that succeeds, try a single edit you can actually observe on-air (the P64 has
no display) — e.g. set a CTCSS tone or toggle the roger beep — write it, and
confirm the behaviour with a second radio. The tool's automatic read-back also
verifies the bytes were stored. Only then use it for real, and keep the second
radio untouched until you trust the flow.

## Getting it onto the radio machine

The radio + cable are on a different Linux box. The prebuilt binary is fully
static (musl), so just copy it over — no Rust toolchain needed there:

```sh
scp p64tool-x86_64-static  user@radiobox:~/p64tool
```

(If that box isn't x86-64, build from source instead: `cargo build --release`.)

## Usage (run on the box the radio is plugged into)

1. Plug in the cable, turn the radio on. Find the port:

   ```sh
   dmesg | tail        # look for ttyUSB0 / ttyACM0
   ls -l /dev/ttyUSB*
   ```

2. Make sure you can access the port (either add yourself to the `dialout`
   group and re-login, or use `sudo` for the commands below):

   ```sh
   sudo usermod -aG dialout "$USER"    # then log out/in
   ```

3. Liveness check:

   ```sh
   ./p64tool info --port /dev/ttyUSB0
   ```

   Expected: `Radio responded to connect (149 bytes).`

4. Full codeplug read:

   ```sh
   ./p64tool read --port /dev/ttyUSB0 --out mydump --verbose
   ```

   This writes `mydump/`:
   - `r01.bin` … `rML.bin` — one raw frame per memory region
   - `codeplug_raw.bin` — all regions concatenated
   - `manifest.txt` — region sizes + whether each header matched the protocol

## Building from source

```sh
cargo build --release                                   # native
cargo build --release --target x86_64-unknown-linux-musl # static/portable
```

## License

MIT — see [LICENSE](LICENSE). Copyright © 2026 Tobi Oetiker.

## Legal & disclaimers

- **No affiliation.** This is an independent, unofficial tool. "Retevis",
  "MateTalk" and "P64" are trademarks of their respective owners; they are used
  here only to describe compatibility. This project is **not affiliated with,
  authorised by, or endorsed by Retevis.**
- **How it was made.** The codeplug protocol and field layout were determined by
  decompiling the manufacturer's freely-distributed Windows programming software
  to obtain the interface information needed for interoperability — permitted for
  that purpose under Swiss copyright law (URG/CopA Art. 21) and the equivalent EU
  provision. **No manufacturer code, binaries, installers or data files are
  included in this repository**; all source here is original work, and the
  protocol notes are written in our own words.
- **No cryptographic algorithms.** The tool only reads/writes encryption *key
  bytes* in the codeplug; the ARC4/AES ciphers run on the radio, not here.
- **No warranty.** Provided "as is" (see LICENSE). Writing to a radio can
  misconfigure it; use at your own risk. Always keep a backup (`p64tool read`).
- **Your responsibility to operate legally.** Radio regulations vary by country.
  The built-in profiles (e.g. PMR446 for CH/CEPT) help, but **you** are
  responsible for ensuring your configuration and use comply with the rules in
  your jurisdiction and for your licence class.

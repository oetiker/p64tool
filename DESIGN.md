# p64tool — design for the read/edit/write editor

## Goal
Read the radio's codeplug, let the user edit it as a human-readable config file,
validate it against a **country regulation profile**, and write it back — all on
Linux. Target radio: Retevis MateTalk P64 / P4.

## Workflow
```
p64tool read   --port … --out radio.toml      # radio -> editable TOML (+ raw backup)
$EDITOR radio.toml                             # user edits
p64tool check  radio.toml --country CH         # validate against regulation profile
p64tool write  --port … radio.toml            # TOML -> radio (refuses if check fails)
```
- `read` also always writes a raw `.p64` backup (the exact bytes) next to the TOML,
  so we can always restore a known-good image.
- `write` does: read current image -> splice in changed fields -> write regions ->
  read back and verify. Never blind-writes.

## Config file (TOML) sketch
```toml
[radio]
name = "..."
country = "CH"          # drives regulation checks + channel-plan generator

[general]
tx_power_default = "high"   # high = 0.5 W (spec max); low = reduced
vox = false
# … (filled from the field map)

[[channel]]
index = 1
name  = "DCH 1"
mode  = "digital"          # digital | analog
rx_mhz = 446.00625
tx_mhz = 446.00625
power  = "low"
bandwidth_khz = 12.5
# analog: ctcss_tx/ctcss_rx …   digital: color_code, timeslot, contact, rx_group …
encrypt = false            # or a key name from [encryption]

[[encryption.key]]
name = "team1"
type = "aes256"            # arc4 | aes128 | aes256
key  = "hex…"
```

## Country regulation profiles
A profile clamps/validates the config. Built-in:

### `CH` (Switzerland) and general `EU`/CEPT PMR446
- Band: 446.00625 – 446.19375 MHz only (both RX and TX, i.e. simplex).
- Channel grid: 16 channels @ 12.5 kHz (6.25 kHz digital variants allowed within band).
- Max power: 0.5 W ERP. On stock P64 firmware "High" = 0.5 W (spec max), so it is
  legal and kept; "Low" is a reduced setting. (The illegal 2 W mode needs the
  separate P4 firmware upgrade, which this tool does not touch.)
- No repeater / no split TX≠RX / no fixed station -> TX must equal RX.
- Integral antenna (informational; not enforceable in software).
- `p64tool gen-pmr446` produces the canonical 16 analog + 16 digital default channels.
Sources: BAKOM pmr446, RIR0507-35, EN 300 296.

Profiles are data (a table), so adding other countries later is trivial. `check`
reports every violation with the channel index and the offending field. `write`
runs `check` first and refuses on any hard violation unless `--force` (which we
will NOT expose for regulated bands).

## The PMR446 channel grid (verified against the radio's factory defaults)
16 channels, 12.5 kHz spacing, first = 446.00625 MHz:
446.00625, 446.01875, 446.03125, 446.04375, 446.05625, 446.06875, 446.08125,
446.09375, 446.10625, 446.11875, 446.13125, 446.14375, 446.15625, 446.16875,
446.18125, 446.19375 MHz.

## Safety plan for write
1. Implement write region-by-region using the `0x44` opcode framing (from PROTOCOL.md).
2. Test on ONE radio only, after taking a raw backup.
3. First write test = read → write same bytes back → read → assert identical
   (no semantic change; proves the write path is byte-faithful).
4. Then a single change observable on-air (e.g. a CTCSS tone) → verify with a
   second radio (the P64 has no display).
5. Only then expand to full config writes.

## Status
- read: done, hardware-validated.
- decode: channel freq/name/mode done; full field map in progress.
- country profile + validation: design done (this doc).
- write: not started (careful, brick risk).

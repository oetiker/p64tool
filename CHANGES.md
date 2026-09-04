# Changelog

All notable changes to p64tool are documented here. This project follows
semantic versioning; releases are cut from the `main` branch via the Release
workflow.

## Unreleased

### New

- `[general] serial_no` exposes the CPS-editable "Serial No" (region `r01`,
  16 UTF-16LE chars). It is codeplug data, not a hardware id — it travels with a
  clone. Omit the key from a config to leave the stored value untouched.
- `[[scan]] include_selected` surfaces the CPS "Selected" pseudo-member (stored
  as member id `0`). Previously `decode` silently dropped it and `apply`
  unconditionally re-added it, so a scan list without "Selected" could not be
  represented and would have been corrupted on write.
- `read --allow-incomplete` keeps a partial dump for analysis. Without it, a
  short or malformed region is now an error and no dump is written.

### Fixed

- **Channel `bandwidth_khz` was a 1-bit read of a 2-bit field.** Bandwidth is
  channel-record byte 33 `&0x0C` with `0 = 12.5, 1 = 20, 2 = 25` kHz; the code
  tested only `&0x04`. Three consequences, all confirmed against a live P4 V1.2
  whose CPS-written 25 kHz channels store `0x88`: every 25 kHz channel decoded
  as 12.5 kHz; 20 kHz was not representable at all; and because `apply` only
  ever touched `0x04`, narrowing such a channel to 12.5 kHz wrote nothing and
  left the radio wideband while the config claimed otherwise. `roundtrip` could
  not catch this — it stayed byte-faithful precisely because `0x08` was never
  written. An unknown value 3 now decodes to `None` and leaves the bits alone,
  and an unsupported bandwidth is a hard error rather than a silent no-op.

- **Channel `power` was inverted.** Channel-record byte 33 bits `[1:0]` are
  `0 = high, 2 = low`, not the reverse taken from the CPS decompile. Confirmed on
  a live P4 V1.2: an OEM codeplug ships every channel at `0x80` and the CPS shows
  High; setting one channel to Low moved that byte to `0x82`. Anything decoded
  before this fix reported every channel's power backwards, and writing such a
  config back would have flipped it on the radio.

### Changed

- The write path is **hardware-proven**, identity and modifying, on a P4 V1.2.
  An identity write of all 11 regions re-read byte-identical to the backup; a
  one-field TOML edit changed exactly 19 bytes in one channel's name and nothing
  else, across any region. Changed content has been exercised on `r08` only, and
  on one model and firmware.
- `write` never writes `r32` or `rFF`, matching the vendor CPS. Both are
  byte-identical across every radio measured, so writing them could only ever be
  a no-op or a mistake. They are still read and preserved.
- `connect` retries up to 5 times. The first handshake after an idle port
  returns 0 bytes on real hardware; `info`, `read` and `write` previously had to
  be run twice by hand.
- `PROTOCOL.md` documents MCU-GET (`0x32`), the `0x44`→`0x54` write ACK, the
  region write frame, and corrects the DTR/RTS requirement — the old text said
  both lines are left de-asserted, which is backwards and leaves the radio silent.

### Fixed

- `decode`→`apply` is byte-faithful on P4 codeplugs. Four asymmetries broke it:
  an empty name wrote a `0x0000` terminator into an `0xFF`-filled field; blank
  records were force-filled with one convention when it varies by radio; the
  channel encryption key slot was zeroed whenever the enable bit was clear; and
  the `r02[24]` password sentinel was rewritten to a value only factory radios
  use. `roundtrip` now passes on six dumps spanning four radios.
- A truncated dump can no longer be produced or loaded. `Codeplug::from_dump_dir`
  validated only the first 18 bytes, so a short region reached
  `raw[14..14+paylen]` and panicked; it now reports the actual sizes.

## 0.2.1 - 2026-07-17

### New

- Complete codeplug decode. `decode --expert` now covers every field the vendor
  CPS binds to a control; bytes the vendor leaves reserved are preserved verbatim.
  Newly decoded channel fields: TX-admit criteria, TX-timeout + pre-alert +
  re-key, text-message confirm/format, private-call-confirm, timed-preamble
  preference, emergency alarm/reply/call-indication flags, auto-scan /
  off-network / solo-work, encryption random-key + multi-key-decrypt, direct
  dual-slot, and the analog reset-time / whisper / tail-elimination / RX-squelch
  fields. Scan lists gain reply-channel mode, designated-TX channel, probe and
  hold times, and the talk-back / nuisance-delete / scan-LED flags. General
  settings gain the master voice-encryption enable and encryption type.
### Changed

### Fixed

- The crate now builds from a clean checkout. An identity test embedded a real
  device dump via `include_bytes!("../mydump/r01.bin")` -- a git-ignored path, so
  a fresh clone failed to compile. Replaced with a synthetic, in-code r01 fixture
  that carries only the model label the test checks (no device-extracted data).
- Serial handshake reliability. The radio returns no data unless a modem-control
  line is asserted; the port is now opened with DTR and RTS raised instead of
  relying on the driver's (kernel- and re-enumeration-dependent) default. This
  also resolves the intermittent "connect handshake failed (got 0 bytes)" seen
  on a freshly-powered radio.

## 0.2.0 - 2026-07-16

### New

- Device identity + firmware-version gating. `info` now reads the radio's live
  MCU identity (model token, firmware version, build date) via the `MCU-GET`
  command and prints a gate verdict. `write` runs a **read-only identity
  pre-check first** and refuses to write to a radio whose model p64tool does not
  recognise (the MCU name must contain the `DM5` token); for a recognised model
  whose firmware/revision is outside p64tool's validated set it warns and
  proceeds, or refuses with `--require-known-version`. Raw `read`/dump is never
  gated. `decode` warns when a dump's model label is not in the validated set.
### Changed

- `info` now reports the device model/firmware/build date and the write-gate
  verdict, instead of only confirming the handshake.

### Fixed

## 0.1.0 - 2026-07-14

### New

- Read the full P64 / P4 codeplug over the serial programming cable (`read`),
  and a quick liveness check (`info`).
- Decode a dump into an editable TOML config (`decode`) and validate it against
  a country regulation profile (`check`, PMR446 for CH/CEPT).
- Write a config back to the radio (`write`) — writes only changed regions,
  with a pre-write regulation check and post-write read-back verification.
- `roundtrip` self-test proving decode→apply is byte-faithful across all regions.
- Full feature coverage: channels, contacts, RX groups, zones, scan lists,
  messages, emergency/alarm systems, one-touch, and encryption keys
  (ARC4/AES128/AES256, openssl-compatible hex).
- `--comments` annotates the config; `--expert` reveals fixed/advanced fields
  (frequencies, timeslot, bandwidth, DMR service internals).
### Changed

### Fixed

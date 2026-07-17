# Changelog

All notable changes to p64tool are documented here. This project follows
semantic versioning; releases are cut from the `main` branch via the Release
workflow.

## Unreleased

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

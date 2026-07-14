# Changelog

All notable changes to p64tool are documented here. This project follows
semantic versioning; releases are cut from the `main` branch via the Release
workflow.

## Unreleased

### New

### Changed

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

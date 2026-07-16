# p64tool — device identity & firmware-version gating (design)

**Date:** 2026-07-16 · **Status:** design, awaiting review

## Goal

Before p64tool decodes or writes a codeplug, positively identify the connected
radio and refuse to write if it is not a model whose byte layout we understand.
p64tool's field map was reverse-engineered and validated **empirically against one
radio** (a `P64 V1.1`, firmware `1.0.0.0`); applying that map to a different model
could misconfigure or brick a radio. This adds an explicit safety gate.

Hard rule: **a write must first read the device's live identity and decide whether
p64tool's map applies.** Raw reads/dumps stay unconditional (reading bytes is safe).

This is a standalone effort. **Full codeplug decode is a separate initiative** and is
not part of this spec; version-gating is orthogonal to the field map.

## Background — what the vendor CPS actually does

From the decompiled Windows CPS (`~/scratch/re-tools/decompiled/Dcps/`):

- CPS gates read/write/upgrade **only on the MODEL/MCU token**, never on firmware
  version. The default-on "model check" requires the device-reported MCU name to
  contain **`"DM5"`** (for the P64/P4). On mismatch it **blocks** with an error.
- **Firmware version is read and displayed but never compared.** There is **no
  version-conditional field layout** anywhere — all offsets are compile-time
  constants; P64 and P4 even share the same byte offsets (only the Windows form
  differs). The vendor treats codeplug layout as **fixed per model, invariant across
  firmware versions.**
- Live identity comes from a dedicated **`MCU-GET`** command (opcode `0x32`), **not**
  the 149-byte connect reply (which CPS, like p64tool, only prefix-matches as an ACK).
- `"P64 V1.1"` vs `"P64 V1.4"` is a **model-name/revision label** (stored in codeplug
  region r01), not the firmware version. Both our reference radios' firmware is
  `"1.0.0.0"`. CPS gates on neither of these — only on `"DM5"`.
- CPS writes **all 13 regions** it reads; there is no spared calibration region
  (calibration, if any, lives outside the read/write set and is never touched — same
  as p64tool). So this spec does not need special-case region handling.

### Consequence for policy

The real hazard is a **model** mismatch — that is what we hard-block, matching CPS.
But unlike CPS we do **not** own the format: our map is validated on exactly one
firmware. So we additionally keep a **validated-versions allowlist** and **warn**
(not block) on anything outside it. This is stricter than CPS in a way that fits a
reverse-engineered tool. (Chosen policy: "Option 2".)

## The MCU-GET protocol

Add to `proto.rs`. Sent inside an open session (after `CONNECT`), exactly as CPS
does during read and write.

**Command (54 bytes):**
```
5F 5F 2E 00 00 32 00 26 02 00 00 07 22 00 00 00 89 87 52 79
00 00 00 00 68 19 05 00 88 F6 19 00 90 98 43 00 30 F7 19 00
A0 62 88 76 62 0F 0A 00 FF FF 55 AA 0D 0A
```
(The 16-byte-onward middle carries fixed MCU memory addresses — this is an MCU
memory read, so the command is model/firmware-specific. It must be **validated
against the real radio** during implementation; see Open items.)

**Reply (52 bytes):** prefix `5F 5F 2E 00 00 26 00 32 02 00 07 00 22 00 00 00`
(16 bytes), then ASCII fields at these reply offsets:

| Field | Reply offset (inclusive) | Meaning | Use |
|---|---|---|---|
| MCU name | `[16..=27]` (12 B, ASCII, trimmed) | MCU family token, contains `"DM5"` | **hard model gate** |
| Firmware | `[28..=43]` (16 B, ASCII, trimmed) | firmware version, e.g. `"1.0.0.0"` | display + allowlist |
| Build date | `[44..=47]` (4 B) | BCD, printed `HEX(44)HEX(45)-HEX(46)-HEX(47)` → e.g. `2025-07-25` | display |

r01 (already read) contributes cross-display/sanity fields (UTF-16LE): model label
(`"P64 V1.1"`), firmware, serial, hardware, last-program date. r01 is the *codeplug*
copy and can lag a re-flashed radio, so **MCU-GET is authoritative; r01 is
secondary.**

## Data model & gate logic

New module `src/identity.rs` (UI-agnostic, pure where possible):

```rust
pub struct DeviceIdentity {
    pub mcu_name: String,      // from MCU-GET [16..=27]   (gate token)
    pub firmware: String,      // from MCU-GET [28..=43]
    pub build_date: String,    // from MCU-GET [44..=47]
    pub r01_model: Option<String>,  // from r01 (label, e.g. "P64 V1.1")
    pub serial: Option<String>,     // from r01
    pub hardware: Option<String>,   // from r01
}

pub enum GateOutcome {
    Ok,                        // known model + known version
    UnknownVersion { .. },     // known model, version not in allowlist -> WARN
    WrongModel { .. },         // model token missing -> BLOCK
}
```

- **Model gate (hard):** `mcu_name` must contain `MODEL_TOKEN = "DM5"`. Else
  `WrongModel` → refuse to write.
- **Version allowlist (soft):** a static table of validated `(r01_model, firmware)`
  pairs — seeded with `("P64 V1.1", "1.0.0.0")`. If the observed pair is absent →
  `UnknownVersion` → **warn**, still allow. A `--require-known-version` flag escalates
  this to a block.

Parsing lives in `proto.rs` (frame → raw fields) + `identity.rs` (raw → typed +
gate); the allowlist and token are constants there. Pure gate logic is unit-tested.

## Protocol integration

- `proto::mcu_get(port, verbose) -> Result<RawMcuInfo>` — assumes an open session;
  `transact(port, MCU_GET, 52, verbose)`, prefix-check, slice the fields.
- **`write`**: run MCU-GET **inside the write session, immediately after CONNECT and
  before any region frame** (so we gate the exact radio we're about to write). Apply
  the gate: `WrongModel` → abort with a clear message; `UnknownVersion` → warn (or
  abort under `--require-known-version`). Then proceed with the existing
  apply/write/verify. (`write_all` gains an identity+gate step; the r01 read for
  cross-display can reuse the base image already in hand.)
- **`info`**: becomes a real identity probe — CONNECT → MCU-GET → DISCONNECT, print
  model/firmware/date/serial/hardware and the gate verdict.
- **`decode`** (from a dump, no live radio): read identity from the dump's r01 only;
  warn if outside the allowlist. No MCU-GET (no radio).
- **`read`/dump**: unchanged and never gated.

## Safety properties

- Writing is blocked on wrong model; unknown-but-P64 firmware warns (escalatable).
- Raw read/dump is always allowed.
- **Note:** the existing `roundtrip` proof does NOT catch a version/layout mismatch
  (it reads and rewrites the same offsets, so it is byte-identical regardless). This
  gate is a separate, necessary guard — that is the whole point.

## Testing

- Unit: MCU-GET reply parser (model/firmware/date extraction, trimming, BCD date);
  gate logic (Ok / UnknownVersion / WrongModel) over crafted `DeviceIdentity`.
- Unit: r01 identity field extraction against `mydump/r01.bin` (expects
  `P64 V1.1` / firmware / serial / hardware).
- Hardware (manual, user's radio): `info` prints correct identity; MCU-GET command
  validated to elicit the 52-byte reply; a `write` on the V1.1 radio passes the gate.

## Open items / risks

1. **MCU-GET command validation.** The command embeds fixed MCU addresses; confirm it
   returns the 52-byte reply on the real radio before relying on it. If the reply
   differs, capture the real bytes and adjust offsets. Until validated on hardware,
   keep a fallback: if MCU-GET fails, fall back to **r01-only** identity with a
   prominent warning (still hard-block on a non-P64 r01 label).
2. **Exact `Dstringall` semantics** (inclusive end vs length) — verify against the
   real reply; offsets above assume inclusive-end per the decompiled source.
3. Confirm the P64 MCU name actually contains `"DM5"` on this radio (expected, but
   verify and record the full string).

## Out of scope

- Full codeplug decode (separate initiative).
- The parked TUI (see `docs/tui-design-notes.md`).
- `MCU-GET`-based firmware **upgrade** / ISP (we only read identity).

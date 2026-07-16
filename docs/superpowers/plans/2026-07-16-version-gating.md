# Device Identity & Firmware-Version Gating — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Before p64tool writes a codeplug, read the radio's live identity via the `MCU-GET` command and refuse to write unless it is a P64 whose byte layout we understand.

**Architecture:** A new pure `identity` module holds the `DeviceIdentity` type, the model-token gate, and the validated-versions allowlist. `proto.rs` gains the `MCU-GET` command + reply parser and session helpers. `main.rs` wires the gate into `write` (hard block / soft warn), makes `info` an identity probe, and adds a warn to `decode`. Raw `read`/dump is never gated.

**Tech Stack:** Rust 2021, `anyhow`, existing `serial`/`proto`/`codeplug` modules. Turbo Vision / TUI is NOT part of this work.

## Global Constraints

- **Cap build/test parallelism at 4 cores:** always `cargo test -j4` / `cargo build -j4` / `cargo clippy -j4` (shared machine).
- **Gate is safety-critical:** raw `read`/dump must remain unconditional; only `write` may hard-block; `decode` only warns.
- **Authoritative identity = `MCU-GET`** (live MCU value); r01 is secondary/display.
- **Model token:** `"DM5"` (P64/P4 MCU family). **Validated allowlist seed:** `("P64 V1.1", "1.0.0.0")`.
- **MCU-GET offsets are inclusive** per the decompiled source: model `[16..=27]`, firmware `[28..=43]`, date `[44..=47]`.
- Keep every module UI-agnostic (no TUI deps).
- Commit messages end with a trailer line: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Spec: `docs/superpowers/specs/2026-07-16-version-gating-design.md`.

## File Structure

- `src/proto.rs` (modify) — `MCU_GET` command bytes, `MCU_GET_REPLY_PREFIX`/`_LEN`, `RawMcuInfo`, `parse_mcu_reply()`, session helpers `mcu_get()` / `read_region()` / `probe_identity()`, and a `gate` param on `write_all()`.
- `src/identity.rs` (create) — `DeviceIdentity`, `from_probe()`, r01 model-label extraction, `MODEL_TOKEN`, `VALIDATED`, `GateOutcome`, `gate()`, `write_decision()`, `unknown_model_note()`.
- `src/main.rs` (modify) — `mod identity;`; wire `info`, `write` (+ `--require-known-version` flag), `decode`.

---

### Task 1: MCU-GET command + reply parser (pure)

**Files:**
- Modify: `src/proto.rs`
- Test: inline `#[cfg(test)]` module in `src/proto.rs`

**Interfaces:**
- Produces: `proto::MCU_GET: &[u8]`, `proto::MCU_GET_REPLY_LEN: usize`, `proto::RawMcuInfo { mcu_name: String, firmware: String, build_date: String }`, `proto::parse_mcu_reply(reply: &[u8]) -> anyhow::Result<RawMcuInfo>`.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/proto.rs`:

```rust
#[cfg(test)]
mod mcu_tests {
    use super::*;

    fn sample_reply() -> Vec<u8> {
        let mut r = vec![0u8; MCU_GET_REPLY_LEN];
        r[..MCU_GET_REPLY_PREFIX.len()].copy_from_slice(MCU_GET_REPLY_PREFIX);
        r[16..16 + 6].copy_from_slice(b"DM5abc");      // model [16..=27]
        r[28..28 + 7].copy_from_slice(b"1.0.0.0");     // firmware [28..=43]
        r[44..=47].copy_from_slice(&[0x20, 0x25, 0x07, 0x25]); // date -> 2025-07-25
        r
    }

    #[test]
    fn parses_model_firmware_and_date() {
        let info = parse_mcu_reply(&sample_reply()).unwrap();
        assert_eq!(info.mcu_name, "DM5abc");
        assert_eq!(info.firmware, "1.0.0.0");
        assert_eq!(info.build_date, "2025-07-25");
    }

    #[test]
    fn rejects_short_or_wrong_reply() {
        assert!(parse_mcu_reply(&[0u8; 10]).is_err());
        let mut bad = sample_reply();
        bad[0] = 0x00; // break prefix
        assert!(parse_mcu_reply(&bad).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j4 --lib mcu_tests`
Expected: FAIL — `MCU_GET`, `MCU_GET_REPLY_LEN`, `parse_mcu_reply` not found (won't compile).

- [ ] **Step 3: Write minimal implementation**

Add near the other command constants in `src/proto.rs` (after `DISCONNECT_REPLY_PREFIX`):

```rust
/// MCU-GET: reads the live MCU model/firmware/date. Opcode 0x32; carries fixed
/// MCU memory addresses. Reply is 52 bytes. (Recovered from the decompiled CPS.)
pub const MCU_GET: &[u8] = &[
    95, 95, 46, 0, 0, 50, 0, 38, 2, 0, 0, 7, 34, 0, 0, 0, 137, 135, 82, 121, 0, 0, 0,
    0, 104, 25, 5, 0, 136, 246, 25, 0, 144, 152, 67, 0, 48, 247, 25, 0, 160, 98, 136,
    118, 98, 15, 10, 0, 255, 255, 85, 170, 13, 10,
];
pub const MCU_GET_REPLY_PREFIX: &[u8] = &[
    0x5F, 0x5F, 0x2E, 0x00, 0x00, 0x26, 0x00, 0x32, 0x02, 0x00, 0x07, 0x00, 0x22, 0x00,
    0x00, 0x00,
];
pub const MCU_GET_REPLY_LEN: usize = 52;

/// Live identity fields from an MCU-GET reply (ASCII, trimmed).
#[derive(Debug, Clone)]
pub struct RawMcuInfo {
    pub mcu_name: String,   // reply[16..=27] — contains the model token, e.g. "DM5..."
    pub firmware: String,   // reply[28..=43] — e.g. "1.0.0.0"
    pub build_date: String, // reply[44..=47] BCD -> "YYYY-MM-DD"
}

fn ascii_field(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0x00 || c == 0xFF).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// Parse a 52-byte MCU-GET reply. Offsets are inclusive per the decompiled CPS.
pub fn parse_mcu_reply(reply: &[u8]) -> Result<RawMcuInfo> {
    if reply.len() < MCU_GET_REPLY_LEN {
        bail!(
            "MCU-GET reply too short: {} bytes (want {})",
            reply.len(),
            MCU_GET_REPLY_LEN
        );
    }
    if !reply.starts_with(MCU_GET_REPLY_PREFIX) {
        bail!(
            "MCU-GET reply has unexpected header: {}",
            hex(&reply[..reply.len().min(16)])
        );
    }
    let d = &reply[44..=47];
    Ok(RawMcuInfo {
        mcu_name: ascii_field(&reply[16..=27]),
        firmware: ascii_field(&reply[28..=43]),
        build_date: format!("{:02X}{:02X}-{:02X}-{:02X}", d[0], d[1], d[2], d[3]),
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j4 --lib mcu_tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/proto.rs
git commit -m "feat(proto): add MCU-GET command and reply parser

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: identity module + r01 model-label extraction (pure)

**Files:**
- Create: `src/identity.rs`
- Modify: `src/main.rs` (add `mod identity;`)
- Test: inline `#[cfg(test)]` in `src/identity.rs` (uses `include_bytes!` of `mydump/r01.bin`)

**Interfaces:**
- Consumes: `proto::RawMcuInfo`, `codeplug::{Region, get_name}`.
- Produces: `identity::DeviceIdentity { mcu_name, firmware, build_date, model_label: Option<String> }`, `identity::r01_model_label(r01_payload: &[u8]) -> String`, `identity::from_probe(mcu: proto::RawMcuInfo, r01_raw: &[u8]) -> DeviceIdentity`.

Note: only the **model label** is extracted from r01 (verified: UTF-16LE at payload offset 1). Serial/hardware need a proper r01 decode and are deferred to the separate full-decode effort.

- [ ] **Step 1: Write the failing test**

Create `src/identity.rs`:

```rust
//! Device identity + firmware-version gating (see
//! docs/superpowers/specs/2026-07-16-version-gating-design.md).

use crate::codeplug::{self, get_name, Region};
use crate::proto::RawMcuInfo;

#[cfg(test)]
mod tests {
    use super::*;

    // A real P64 V1.1 r01 region frame (full framed reply).
    const R01: &[u8] = include_bytes!("../mydump/r01.bin");

    fn r01_payload() -> Vec<u8> {
        Region { name: "r01".into(), raw: R01.to_vec() }.payload().to_vec()
    }

    #[test]
    fn extracts_model_label_from_real_r01() {
        assert_eq!(r01_model_label(&r01_payload()), "P64 V1.1");
    }

    #[test]
    fn from_probe_combines_mcu_and_r01() {
        let mcu = RawMcuInfo {
            mcu_name: "DM5abc".into(),
            firmware: "1.0.0.0".into(),
            build_date: "2025-07-25".into(),
        };
        let id = from_probe(mcu, R01);
        assert_eq!(id.mcu_name, "DM5abc");
        assert_eq!(id.firmware, "1.0.0.0");
        assert_eq!(id.model_label.as_deref(), Some("P64 V1.1"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

First add `mod identity;` to `src/main.rs` (next to the other `mod` lines, after `mod config;`).
Run: `cargo test -j4 --lib identity`
Expected: FAIL — `r01_model_label` / `from_probe` / `DeviceIdentity` not found (won't compile).

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in `src/identity.rs`:

```rust
/// Combined device identity: MCU-GET (authoritative) + r01 model label (display).
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub mcu_name: String,
    pub firmware: String,
    pub build_date: String,
    pub model_label: Option<String>,
}

/// The codeplug model label, e.g. "P64 V1.1". UTF-16LE at r01 payload offset 1
/// (CPS WW01 offset 16; payload = WW - codeplug::WW_TO_PAYLOAD), up to 16 chars.
pub fn r01_model_label(r01_payload: &[u8]) -> String {
    let off = 16 - codeplug::WW_TO_PAYLOAD; // = 1
    get_name(r01_payload, off, 16)
}

/// Build a DeviceIdentity from an MCU-GET result plus a raw (framed) r01 region.
pub fn from_probe(mcu: RawMcuInfo, r01_raw: &[u8]) -> DeviceIdentity {
    let region = Region { name: "r01".into(), raw: r01_raw.to_vec() };
    let label = r01_model_label(region.payload());
    DeviceIdentity {
        mcu_name: mcu.mcu_name,
        firmware: mcu.firmware,
        build_date: mcu.build_date,
        model_label: (!label.is_empty()).then_some(label),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j4 --lib identity`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/identity.rs src/main.rs
git commit -m "feat(identity): DeviceIdentity + r01 model-label extraction

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Gate policy — model gate + version allowlist (pure)

**Files:**
- Modify: `src/identity.rs`
- Test: inline `#[cfg(test)]` in `src/identity.rs`

**Interfaces:**
- Consumes: `identity::DeviceIdentity`.
- Produces: `identity::MODEL_TOKEN: &str`, `identity::VALIDATED: &[(&str, &str)]`, `identity::GateOutcome`, `identity::gate(&DeviceIdentity) -> GateOutcome`, `identity::WriteDecision`, `identity::write_decision(&DeviceIdentity, require_known: bool) -> WriteDecision`, `identity::unknown_model_note(&str) -> Option<String>`.

- [ ] **Step 1: Write the failing test**

Add these tests inside the existing `mod tests` in `src/identity.rs`:

```rust
    fn id(mcu: &str, fw: &str, label: Option<&str>) -> DeviceIdentity {
        DeviceIdentity {
            mcu_name: mcu.into(),
            firmware: fw.into(),
            build_date: "2025-07-25".into(),
            model_label: label.map(|s| s.into()),
        }
    }

    #[test]
    fn gate_ok_for_known_p64() {
        assert!(matches!(gate(&id("DM5abc", "1.0.0.0", Some("P64 V1.1"))), GateOutcome::Ok));
    }

    #[test]
    fn gate_wrong_model_when_token_missing() {
        assert!(matches!(gate(&id("XYZ", "1.0.0.0", Some("P64 V1.1"))), GateOutcome::WrongModel { .. }));
    }

    #[test]
    fn gate_unknown_version_for_p64_off_allowlist() {
        assert!(matches!(gate(&id("DM5abc", "9.9.9.9", Some("P64 V9.9"))), GateOutcome::UnknownVersion { .. }));
    }

    #[test]
    fn write_decision_blocks_wrong_model() {
        assert!(matches!(write_decision(&id("XYZ", "1.0.0.0", None), false), WriteDecision::Block(_)));
    }

    #[test]
    fn write_decision_warns_then_blocks_on_unknown_version() {
        let d = id("DM5abc", "9.9.9.9", Some("P64 V9.9"));
        assert!(matches!(write_decision(&d, false), WriteDecision::Proceed(Some(_))));
        assert!(matches!(write_decision(&d, true), WriteDecision::Block(_)));
    }

    #[test]
    fn write_decision_proceeds_clean_for_known() {
        assert!(matches!(write_decision(&id("DM5abc", "1.0.0.0", Some("P64 V1.1")), false), WriteDecision::Proceed(None)));
    }

    #[test]
    fn unknown_model_note_is_none_for_validated() {
        assert!(unknown_model_note("P64 V1.1").is_none());
        assert!(unknown_model_note("P64 V9.9").is_some());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j4 --lib identity`
Expected: FAIL — `gate` / `write_decision` / `unknown_model_note` / types not found.

- [ ] **Step 3: Write minimal implementation**

Add to `src/identity.rs` (above the test module):

```rust
/// MCU family token the P64 must report (from the decompiled CPS model check).
pub const MODEL_TOKEN: &str = "DM5";

/// (r01 model label, firmware) pairs p64tool's field map is validated against.
pub const VALIDATED: &[(&str, &str)] = &[("P64 V1.1", "1.0.0.0")];

#[derive(Debug)]
pub enum GateOutcome {
    Ok,
    UnknownVersion { model_label: String, firmware: String },
    WrongModel { mcu_name: String },
}

pub fn gate(id: &DeviceIdentity) -> GateOutcome {
    if !id.mcu_name.contains(MODEL_TOKEN) {
        return GateOutcome::WrongModel { mcu_name: id.mcu_name.clone() };
    }
    let label = id.model_label.clone().unwrap_or_default();
    if VALIDATED.iter().any(|(m, f)| *m == label && *f == id.firmware) {
        GateOutcome::Ok
    } else {
        GateOutcome::UnknownVersion { model_label: label, firmware: id.firmware.clone() }
    }
}

/// A write-time decision: proceed (optionally with a warning to print) or block.
#[derive(Debug)]
pub enum WriteDecision {
    Proceed(Option<String>),
    Block(String),
}

pub fn write_decision(id: &DeviceIdentity, require_known: bool) -> WriteDecision {
    match gate(id) {
        GateOutcome::Ok => WriteDecision::Proceed(None),
        GateOutcome::WrongModel { mcu_name } => WriteDecision::Block(format!(
            "device model {mcu_name:?} is not a P64 (missing token {MODEL_TOKEN:?})"
        )),
        GateOutcome::UnknownVersion { model_label, firmware } => {
            let msg = format!(
                "device {model_label:?}/{firmware:?} is not in p64tool's validated set {VALIDATED:?}"
            );
            if require_known {
                WriteDecision::Block(msg)
            } else {
                WriteDecision::Proceed(Some(msg))
            }
        }
    }
}

/// For decode-from-dump: a note if the codeplug model label is unrecognised.
pub fn unknown_model_note(model_label: &str) -> Option<String> {
    if VALIDATED.iter().any(|(m, _)| *m == model_label) {
        None
    } else {
        Some(format!(
            "codeplug model label {model_label:?} is not in p64tool's validated set {VALIDATED:?}; decoded fields may not match"
        ))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -j4 --lib identity`
Expected: PASS (all identity tests).

- [ ] **Step 5: Commit**

```bash
git add src/identity.rs
git commit -m "feat(identity): model gate + validated-version allowlist policy

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: proto session helpers (mcu_get / read_region / probe_identity)

**Files:**
- Modify: `src/proto.rs`

**Interfaces:**
- Consumes: `serial::Serial`, `transact()`, `REGIONS`, `CONNECT`/`DISCONNECT` constants, `parse_mcu_reply()`.
- Produces: `proto::mcu_get(port, verbose) -> Result<RawMcuInfo>`, `proto::read_region(port, name, verbose) -> Result<Vec<u8>>`, `proto::probe_identity(port, verbose) -> Result<(RawMcuInfo, Vec<u8>)>` (returns MCU info + the raw framed r01).

These drive hardware, so behaviour is validated manually (Task 5). The pure parsing they call is already covered by Task 1.

- [ ] **Step 1: Add the helpers**

Add to `src/proto.rs` (after `read_all`):

```rust
/// Send MCU-GET within an already-open session and parse the reply.
pub fn mcu_get(port: &Serial, verbose: bool) -> Result<RawMcuInfo> {
    let reply = transact(port, MCU_GET, MCU_GET_REPLY_LEN, verbose)?;
    parse_mcu_reply(&reply)
}

/// Read a single region by name within an already-open session (raw framed reply).
pub fn read_region(port: &Serial, name: &str, verbose: bool) -> Result<Vec<u8>> {
    let r = REGIONS
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown region {name}"))?;
    transact(port, &r.command(), r.expected, verbose)
}

/// Open a session, read the live MCU identity + region r01, then disconnect.
pub fn probe_identity(port: &Serial, verbose: bool) -> Result<(RawMcuInfo, Vec<u8>)> {
    let reply = transact(port, CONNECT, CONNECT_REPLY_LEN, verbose)?;
    if !reply.starts_with(CONNECT_REPLY_PREFIX) {
        bail!(
            "connect handshake failed (got {} bytes). Radio on? Right --port?",
            reply.len()
        );
    }
    let result = (|| -> Result<(RawMcuInfo, Vec<u8>)> {
        let mcu = mcu_get(port, verbose)?;
        let r01 = read_region(port, "r01", verbose)?;
        Ok((mcu, r01))
    })();
    let _ = transact(port, DISCONNECT, DISCONNECT_REPLY_PREFIX.len() + 4, verbose);
    result
}
```

- [ ] **Step 2: Verify it compiles cleanly**

Run: `cargo build -j4 && cargo clippy -j4 --all-targets -- -D warnings`
Expected: builds, no warnings. (If `REGIONS` items expose `name`/`command()`/`expected` under different names, match the existing `read_all` usage in `proto.rs`.)

- [ ] **Step 3: Commit**

```bash
git add src/proto.rs
git commit -m "feat(proto): mcu_get / read_region / probe_identity session helpers

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Wire `info` into an identity probe

**Files:**
- Modify: `src/main.rs` (the `info` function, ~line 361)

**Interfaces:**
- Consumes: `proto::probe_identity()`, `identity::{from_probe, gate, GateOutcome}`.

- [ ] **Step 1: Replace the `info` body**

Replace the current `info` function in `src/main.rs` with:

```rust
fn info(port: &str, verbose: bool) -> Result<()> {
    let s = serial::Serial::open(port)?;
    let (mcu, r01) = proto::probe_identity(&s, verbose)?;
    let id = identity::from_probe(mcu, &r01);
    println!("MCU name : {}", id.mcu_name);
    println!("Firmware : {}", id.firmware);
    println!("Built    : {}", id.build_date);
    println!(
        "Model    : {}",
        id.model_label.as_deref().unwrap_or("(unknown)")
    );
    match identity::gate(&id) {
        identity::GateOutcome::Ok => println!("Gate     : OK (known P64 layout)"),
        identity::GateOutcome::UnknownVersion { model_label, firmware } => println!(
            "Gate     : WARNING — {model_label}/{firmware} not in p64tool's validated set"
        ),
        identity::GateOutcome::WrongModel { mcu_name } => println!(
            "Gate     : REFUSE writes — model {mcu_name:?} is not a P64"
        ),
    }
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -j4 && cargo clippy -j4 --all-targets -- -D warnings`
Expected: builds, no warnings.

- [ ] **Step 3: Manual hardware validation (needs the radio)**

Run: `cargo run -j4 -- info --port /dev/ttyUSB0 --verbose`
Expected: prints an MCU name containing `DM5`, firmware `1.0.0.0`, model `P64 V1.1`, and `Gate : OK`.
**If the MCU-GET reply is not 52 bytes or the header differs:** capture the raw bytes from `--verbose`, record them, and reconcile `MCU_GET`/offsets against the real reply before proceeding (see spec Open items). Do not continue to Task 6 until `info` reports a correct identity.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(info): report live device identity and gate verdict

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Gate `write` on identity (read-only pre-check) + flag

**Design note (supersedes an earlier draft):** the identity gate is a **read-only
pre-check at the very start of `write_radio`, before the `--yes` confirmation** — NOT
inside `write_all` (which only runs after `--yes`). This way a wrong model is refused
immediately, the verdict prints on a dry run, and it can be validated with no write.
`proto::probe_identity`, `identity::from_probe`, and `identity::write_decision`
already exist (Tasks 3–5); `write_all` is unchanged.

**Files:**
- Modify: `src/main.rs` (`Write` subcommand: add flag; `write_radio`: add the pre-check + a param), `src/identity.rs` (remove now-unnecessary dead_code allows).

**Interfaces:**
- Consumes: `proto::probe_identity`, `identity::{from_probe, write_decision, WriteDecision}`.

- [ ] **Step 1: Add the `--require-known-version` flag and thread it through**

In `src/main.rs`, in the `Write` variant of `enum Cmd`, add after the `no_verify` field:

```rust
        /// Refuse to write if the device firmware is not in p64tool's validated set
        #[arg(long)]
        require_known_version: bool,
```

Add `require_known_version` to the `Cmd::Write { .. }` destructure in `main()` and
pass it to `write_radio(...)`. Add a `require_known_version: bool` parameter to the
`write_radio(...)` signature (place it after `no_verify: bool`).

- [ ] **Step 2: Add the read-only identity pre-check at the top of `write_radio`**

In `src/main.rs`, insert this as the FIRST statements inside `write_radio`, before the
existing `// 1. Obtain the base codeplug` block:

```rust
    // 0. Identity pre-check (read-only): confirm this radio is one whose codeplug
    //    layout p64tool understands, BEFORE reading/applying/writing anything.
    {
        let s = serial::Serial::open(port)?;
        let (mcu, r01) = proto::probe_identity(&s, verbose)?;
        let id = identity::from_probe(mcu, &r01);
        match identity::write_decision(&id, require_known_version) {
            identity::WriteDecision::Proceed(None) => println!(
                "Device identity OK: {} fw {} ({})",
                id.mcu_name,
                id.firmware,
                id.model_label.as_deref().unwrap_or("?")
            ),
            identity::WriteDecision::Proceed(Some(warn)) => println!(
                "WARNING: {warn} — proceeding (use --require-known-version to refuse)."
            ),
            identity::WriteDecision::Block(msg) => anyhow::bail!("refusing to write: {msg}"),
        }
    }
```

The rest of `write_radio` (obtain base, apply, build frames, `--yes` gate, `write_all`,
verify) is unchanged.

- [ ] **Step 3: Remove now-unnecessary dead_code allows**

Wiring the pre-check makes `identity::write_decision` and `identity::WriteDecision`
used. Remove their `#[allow(dead_code)]` attributes in `src/identity.rs`. After this
task NO `#[allow(dead_code)]` should remain anywhere in `src/identity.rs` or
`src/proto.rs` — verify by building; an allow on a now-used item fails `-D warnings`.

- [ ] **Step 4: Verify it compiles and unit tests pass**

Run: `cargo test -j4 && cargo clippy -j4 --all-targets -- -D warnings && cargo fmt --check`
Expected: builds, all tests pass (the decision logic is covered by Task 3), no warnings.

- [ ] **Step 5: Manual hardware validation (controller runs against the real radio)**

The controller (not the implementer) runs this against the P64:
`p64tool write --port /dev/ttyUSB0 --all` **(dry-run: no `--yes`)**
Expected: prints `Device identity OK: DM5 fw 1.0.0.0 (P64 V1.1)` (the read-only
pre-check), then the base read + `About to write … Re-run with --yes to proceed.`, and
exits WITHOUT writing. This is safe — no `--yes`, so no write occurs. Do NOT pass
`--yes` (that would reprogram the radio) unless the user explicitly authorises it.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/identity.rs
git commit -m "feat(write): read-only identity pre-check before writing

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Warn in `decode` on an unrecognised codeplug

**Files:**
- Modify: `src/main.rs` (the `decode` function, ~line 313)

**Interfaces:**
- Consumes: `identity::{r01_model_label, unknown_model_note}`.

- [ ] **Step 1: Add the warning**

In `src/main.rs`, in `decode()`, right after `let cp = codeplug::Codeplug::from_dump_dir(&dump)?;`, add:

```rust
    let label = identity::r01_model_label(cp.region("r01")?.payload());
    if let Some(note) = identity::unknown_model_note(&label) {
        eprintln!("NOTE: {note}");
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -j4 && cargo clippy -j4 --all-targets -- -D warnings`
Expected: builds, no warnings.

- [ ] **Step 3: Manual check against the validated dump**

Run: `cargo run -j4 -- decode mydump --out /tmp/radio.toml`
Expected: NO note printed (mydump is `P64 V1.1`, which is in `VALIDATED`).

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(decode): warn when a dump's model is not in the validated set

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final verification

- [ ] `cargo test -j4` — all pass.
- [ ] `cargo clippy -j4 --all-targets -- -D warnings` — clean.
- [ ] `cargo fmt --check` — clean.
- [ ] Hardware (user): `info` shows the correct identity; `write --all` prints `Identity OK` before the `--yes` gate; a deliberately wrong device (if available) is refused.
- [ ] Update `CHANGES.md` with a user-visible entry for the new gate + `info` behaviour + `--require-known-version` flag.

## Notes for the implementer

- **MCU-GET is unvalidated on hardware until Task 5.** If the reply differs from the assumed 52-byte / prefix / offset layout, STOP and reconcile against the captured bytes before wiring `write` (Task 6). The r01 model-label path is already hardware-verified (Task 2 uses `mydump/r01.bin`), so a fallback of "gate on r01 label only" is available if MCU-GET proves troublesome — raise it for a decision rather than guessing.
- Do not touch the field map / decode logic — this work is orthogonal to full decode.

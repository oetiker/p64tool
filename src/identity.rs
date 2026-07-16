//! Device identity + firmware-version gating (see
//! docs/superpowers/specs/2026-07-16-version-gating-design.md).

use crate::codeplug::{self, get_name, Region};
use crate::proto::RawMcuInfo;

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
    let region = Region {
        name: "r01".into(),
        raw: r01_raw.to_vec(),
    };
    let label = r01_model_label(region.payload());
    DeviceIdentity {
        mcu_name: mcu.mcu_name,
        firmware: mcu.firmware,
        build_date: mcu.build_date,
        model_label: (!label.is_empty()).then_some(label),
    }
}

/// MCU family token the P64 must report (from the decompiled CPS model check).
pub const MODEL_TOKEN: &str = "DM5";

/// (r01 model label, firmware) pairs p64tool's field map is validated against.
pub const VALIDATED: &[(&str, &str)] = &[("P64 V1.1", "1.0.0.0")];

#[derive(Debug)]
pub enum GateOutcome {
    Ok,
    UnknownVersion {
        model_label: String,
        firmware: String,
    },
    WrongModel {
        mcu_name: String,
    },
}

pub fn gate(id: &DeviceIdentity) -> GateOutcome {
    if !id.mcu_name.contains(MODEL_TOKEN) {
        return GateOutcome::WrongModel {
            mcu_name: id.mcu_name.clone(),
        };
    }
    let label = id.model_label.clone().unwrap_or_default();
    if VALIDATED
        .iter()
        .any(|(m, f)| *m == label && *f == id.firmware)
    {
        GateOutcome::Ok
    } else {
        GateOutcome::UnknownVersion {
            model_label: label,
            firmware: id.firmware.clone(),
        }
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
        GateOutcome::UnknownVersion {
            model_label,
            firmware,
        } => {
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

#[cfg(test)]
mod tests {
    use super::*;

    // A real P64 V1.1 r01 region frame (full framed reply).
    const R01: &[u8] = include_bytes!("../mydump/r01.bin");

    fn r01_payload() -> Vec<u8> {
        Region {
            name: "r01".into(),
            raw: R01.to_vec(),
        }
        .payload()
        .to_vec()
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
        assert!(matches!(
            gate(&id("DM5abc", "1.0.0.0", Some("P64 V1.1"))),
            GateOutcome::Ok
        ));
    }

    #[test]
    fn gate_wrong_model_when_token_missing() {
        assert!(matches!(
            gate(&id("XYZ", "1.0.0.0", Some("P64 V1.1"))),
            GateOutcome::WrongModel { .. }
        ));
    }

    #[test]
    fn gate_unknown_version_for_p64_off_allowlist() {
        assert!(matches!(
            gate(&id("DM5abc", "9.9.9.9", Some("P64 V9.9"))),
            GateOutcome::UnknownVersion { .. }
        ));
    }

    #[test]
    fn gate_unknown_version_when_only_label_matches() {
        // label "P64 V1.1" is validated, but firmware "9.9.9.9" is not.
        assert!(matches!(
            gate(&id("DM5abc", "9.9.9.9", Some("P64 V1.1"))),
            GateOutcome::UnknownVersion { .. }
        ));
    }

    #[test]
    fn gate_unknown_version_when_model_label_none() {
        match gate(&id("DM5abc", "1.0.0.0", None)) {
            GateOutcome::UnknownVersion { model_label, .. } => assert!(model_label.is_empty()),
            other => panic!("expected UnknownVersion, got {other:?}"),
        }
    }

    #[test]
    fn write_decision_blocks_wrong_model() {
        assert!(matches!(
            write_decision(&id("XYZ", "1.0.0.0", None), false),
            WriteDecision::Block(_)
        ));
    }

    #[test]
    fn write_decision_warns_then_blocks_on_unknown_version() {
        let d = id("DM5abc", "9.9.9.9", Some("P64 V9.9"));
        assert!(matches!(
            write_decision(&d, false),
            WriteDecision::Proceed(Some(_))
        ));
        assert!(matches!(write_decision(&d, true), WriteDecision::Block(_)));
    }

    #[test]
    fn write_decision_proceeds_clean_for_known() {
        assert!(matches!(
            write_decision(&id("DM5abc", "1.0.0.0", Some("P64 V1.1")), false),
            WriteDecision::Proceed(None)
        ));
    }

    #[test]
    fn unknown_model_note_is_none_for_validated() {
        assert!(unknown_model_note("P64 V1.1").is_none());
        assert!(unknown_model_note("P64 V9.9").is_some());
    }
}

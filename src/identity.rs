//! Device identity + firmware-version gating (see
//! docs/superpowers/specs/2026-07-16-version-gating-design.md).

use crate::codeplug::{self, get_name, Region};
use crate::proto::RawMcuInfo;

/// Combined device identity: MCU-GET (authoritative) + r01 model label (display).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub mcu_name: String,
    pub firmware: String,
    pub build_date: String,
    pub model_label: Option<String>,
}

/// The codeplug model label, e.g. "P64 V1.1". UTF-16LE at r01 payload offset 1
/// (CPS WW01 offset 16; payload = WW - codeplug::WW_TO_PAYLOAD), up to 16 chars.
#[allow(dead_code)]
pub fn r01_model_label(r01_payload: &[u8]) -> String {
    let off = 16 - codeplug::WW_TO_PAYLOAD; // = 1
    get_name(r01_payload, off, 16)
}

/// Build a DeviceIdentity from an MCU-GET result plus a raw (framed) r01 region.
#[allow(dead_code)]
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
}

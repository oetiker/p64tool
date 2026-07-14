//! Country regulation profiles. A profile validates a RadioConfig; `check`
//! returns a list of findings. Frequencies/power that break a hard rule are
//! Errors (write refuses); softer concerns are Warnings.

use crate::config::{Mode, Power, RadioConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub channel: Option<u16>,
    pub message: String,
}

pub struct Profile {
    pub name: &'static str,
    /// inclusive band [lo, hi] in MHz that every RX and TX must fall in
    pub band_mhz: (f64, f64),
    /// require TX == RX (no repeater / duplex)
    pub simplex_only: bool,
    /// max legal channel bandwidth in kHz (wider => error)
    pub max_bandwidth_khz: f64,
    /// if true, "High" power is flagged (only relevant if High could exceed 0.5 W ERP)
    pub power_limited: bool,
}

/// PMR446 (CEPT-harmonised; applies to CH and most of Europe).
/// power_limited=false: on stock P64 firmware "High" = 0.5 W (per the spec sheet),
/// which is the legal PMR446 limit; the illegal 2 W mode needs the separate P4
/// firmware upgrade, which this tool does not touch.
pub const PMR446: Profile = Profile {
    name: "PMR446 (CH/CEPT)",
    band_mhz: (446.00625, 446.19375),
    simplex_only: true,
    max_bandwidth_khz: 12.5,
    power_limited: false,
};

pub fn profile_for(country: &str) -> Option<&'static Profile> {
    match country.to_ascii_uppercase().as_str() {
        // CEPT PMR446 members share the same rule; extend as needed.
        "CH" | "EU" | "CEPT" | "DE" | "AT" | "FR" | "IT" | "LI" => Some(&PMR446),
        "" => None,
        _ => None,
    }
}

const EPS: f64 = 0.0005; // 0.5 kHz tolerance for float compares

pub fn check(cfg: &RadioConfig, profile: &Profile) -> Vec<Finding> {
    let mut out = Vec::new();
    let (lo, hi) = profile.band_mhz;
    let in_band = |f: f64| f >= lo - EPS && f <= hi + EPS;

    for ch in &cfg.channel {
        let c = Some(ch.index);
        let err = |m: String| Finding {
            severity: Severity::Error,
            channel: c,
            message: m,
        };
        let warn = |m: String| Finding {
            severity: Severity::Warning,
            channel: c,
            message: m,
        };

        // Frequencies are only checkable when present (expert mode). When hidden
        // they are preserved unchanged from the radio, so there is nothing new
        // to validate.
        if let Some(rx) = ch.rx_mhz {
            if !in_band(rx) {
                out.push(err(format!(
                    "RX {:.5} MHz outside {} band {:.5}-{:.5}",
                    rx, profile.name, lo, hi
                )));
            }
        }
        if let Some(tx) = ch.tx_mhz {
            if !in_band(tx) {
                out.push(err(format!(
                    "TX {:.5} MHz outside {} band {:.5}-{:.5}",
                    tx, profile.name, lo, hi
                )));
            }
        }
        if let (true, Some(rx), Some(tx)) = (profile.simplex_only, ch.rx_mhz, ch.tx_mhz) {
            if (rx - tx).abs() > EPS {
                out.push(err(format!(
                    "TX {tx:.5} != RX {rx:.5} MHz (repeater/duplex not allowed)"
                )));
            }
        }
        if let Some(bw) = ch.bandwidth_khz {
            if bw > profile.max_bandwidth_khz + 0.01 {
                out.push(err(format!(
                    "bandwidth {bw} kHz exceeds max {} kHz",
                    profile.max_bandwidth_khz
                )));
            }
        }
        if profile.power_limited && ch.power == Power::High {
            out.push(warn(
                "power = High exceeds the 0.5 W PMR446 limit on this profile - set power = \"low\""
                    .to_string(),
            ));
        }
        // analog TX with 25 kHz already covered; digital fine at 12.5.
        let _ = ch.mode;
        let _ = Mode::Digital;
    }
    out
}

pub fn print_findings(findings: &[Finding]) -> (usize, usize) {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    for f in findings {
        let tag = match f.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "warn ",
        };
        match f.channel {
            Some(ch) => println!("  [{tag}] ch {ch}: {}", f.message),
            None => println!("  [{tag}] {}", f.message),
        }
    }
    (errors, warnings)
}

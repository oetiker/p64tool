//! Human-editable configuration model (TOML) and conversion to/from the raw
//! codeplug. `decode` reads fields out of a Codeplug; `apply` writes edited
//! fields back onto an existing Codeplug (leaving unmapped bytes untouched, so
//! we never lose settings we don't understand).

use crate::codeplug::*;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RadioConfig {
    pub radio: RadioInfo,
    pub general: General,
    #[serde(default)]
    pub channel: Vec<Channel>,
    #[serde(default)]
    pub contact: Vec<Contact>,
    #[serde(default)]
    pub rx_group: Vec<RxGroup>,
    #[serde(default)]
    pub zone: Vec<Zone>,
    #[serde(default)]
    pub scan: Vec<ScanList>,
    #[serde(default)]
    pub message: Vec<Message>,
    #[serde(default)]
    pub alarm: Vec<Alarm>,
    #[serde(default)]
    pub one_touch: Vec<OneTouch>,
    #[serde(default)]
    pub enc_key: Vec<EncKey>,
}

// ---- list-table records ---------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Contact {
    pub index: u16,
    pub name: String,
    pub dmr_id: u32,
    /// "private" | "group" | "all"
    pub call_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RxGroup {
    pub index: u16,
    pub name: String,
    /// 1-based contact indices
    pub contacts: Vec<u16>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Zone {
    pub index: u16,
    pub name: String,
    /// 1-based channel indices
    pub channels: Vec<u16>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScanList {
    pub index: u16,
    pub name: String,
    /// 1-based channel indices (0 = "selected/current channel")
    pub channels: Vec<u16>,
    pub priority1: u16, // 0=selected, 0xFFFF=off, else 1-based channel
    pub priority2: u16,
    /// Reply-channel mode 0..2 (rec[33]; 1 = use designated channel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_mode: Option<u8>,
    /// Designated / revert TX channel, 1-based (rec[34..35]); 0xFFFF = none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designated_channel: Option<u16>,
    /// Probe / sample time (rec[40]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_time: Option<u8>,
    /// Signalling hold time (rec[42]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_time: Option<u8>,
    /// Scan behaviour flags (rec[44]): talk-back, nuisance-delete, scan LED.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub talkback: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nuisance_delete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_led: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Message {
    pub index: u16,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Alarm {
    pub index: u16,
    pub name: String,
    pub revert_channel: u16, // 0 = current channel
    pub alarm_type: u8,      // 0..4
    pub alarm_mode: u8,      // 0..2
    pub impolite_retries: u8,
    pub polite_retries: u8,
    pub hot_mic_s: u8,
    pub rx_dwell_s: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OneTouch {
    pub index: u8,
    pub mode: u8, // 0 = none, 1 = digital
    pub contact: u8,
    /// "call" | "message"
    pub action: String,
    pub message_index: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EncKey {
    pub index: u8,
    pub name: String,
    /// "arc4" | "aes128" | "aes256"
    pub key_type: String,
    /// The key as a plain hex string, same order the Windows CPS shows and what
    /// `openssl rand -hex <n>` produces: 32 hex chars (16 bytes) for AES128,
    /// 64 (32 bytes) for AES256, variable for ARC4.
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RadioInfo {
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct General {
    // --- core (region r02) ---
    /// 0 = digital only, 1 = analog only, 2 = analog+digital
    pub channel_mode: u8,
    pub squelch: u8,
    pub vox_enable: bool,
    pub vox_level: u8,
    pub volume_max: u8,
    pub volume_min: u8,
    pub password_enable: bool,
    // --- audio (r02) ---
    /// internal mic gain: "low"|"mid"|"high" (stored 0/6/12)
    pub mic_gain_internal: String,
    pub mic_agc: bool,
    /// analog mic gain, -42..+20 (stored raw+42)
    pub mic_gain_analog: i8,
    pub tx_end_tone: bool,
    /// voice prompt: "off"|"english"|"chinese" (0/1/2)
    pub voice_prompt: String,
    /// RX sensitivity: false = normal, true = enhanced
    pub rf_sensitivity_enhanced: bool,
    /// scan method: false = time-operated, true = carrier-operated
    pub scan_carrier_operated: bool,
    /// power-save mode 0=off,1=1:1,2=1:2,3=1:4
    pub power_save_mode: u8,
    /// power-save delay seconds (5..60 step 5)
    pub power_save_delay_s: u8,
    // --- DMR service (region r03) ---
    pub radio_dmr_id: u32,
    /// Advanced DMR timing/decode internals; emitted only with --expert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dmr_service: Option<DmrService>,
    // --- man-down (region r0A) ---
    pub man_down_enable: bool,
    pub man_down_prealarm_s: u8,
    pub man_down_delay_s: u8,
    pub man_down_angle_deg: u8,
    // --- work-alone (region r05) ---
    pub work_alone_enable: bool,
    pub work_alone_prealarm_tone: bool,
    pub work_alone_response_s: u8,
    pub work_alone_prealarm_s: u8,
    // --- encryption globals (region r02) ---
    /// Master voice-encryption enable (r02[140]: 'C'=on, 'B'=off).
    pub voice_encrypt_enable: bool,
    /// Encryption type / mode, 0..1 (r02[141]).
    pub encrypt_type: u8,
    // --- buttons (region r02) ---
    /// long-press time seconds (0.5..5.0 step 0.5)
    pub key_long_press_s: f64,
    pub key_top_short: u8,
    pub key_top_long: u8,
    pub key_side1_short: u8,
    pub key_side1_long: u8,
    pub key_side2_short: u8,
    pub key_side2_long: u8,
    // --- tones ---
    pub tones: Tones,
}

/// Advanced DMR timing / decode settings (region r03). Rarely changed, so this
/// whole group is emitted only with --expert; hidden = preserved unchanged.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DmrService {
    pub preamble: u8,            // head-frame count 1..10
    pub group_call_hang_ms: u16, // 0..7000 step 500
    pub private_call_hang_ms: u16,
    pub priority_hang_ms: u16,
    pub remote_monitor_s: u8, // 10..120 step 10
    pub decode_call_alert: bool,
    pub decode_radio_check: bool,
    pub decode_remote_stun: bool,
    pub decode_remote_monitor: bool,
    pub remote_kill_id: u32,
}

/// All the audible-alert toggles. Master `all_tones_off` (r02[100]==0xCD) mutes
/// everything; `talk_permit` is 0=prohibit,1=analog,2=digital,3=both.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Tones {
    pub all_tones_off: bool,
    pub talk_permit: u8,
    pub fixed_volume: u8, // 1..10 (r0A[22])
    pub remote_stun: bool,
    pub remote_activate: bool,
    pub wrong_operation: bool,
    pub tx_busy_lockout: bool,
    pub talk_timeout: bool,
    pub talk_timeout_prealert: bool,
    pub call_out: bool,
    pub private_call: bool,
    pub group_call: bool,
    pub all_call: bool,
    pub low_battery: bool,
    pub free_channel: bool,
    pub scan_list_empty: bool,
    pub contact_list_empty: bool,
    pub alarm_dwell: bool,
    pub power_on: bool,
    pub alarm: bool,
    pub call_alert: bool,
    pub priority_channel: bool,
    pub send_fail: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Channel {
    pub index: u16, // 1-based
    pub name: String,
    pub mode: Mode,
    /// RX/TX frequency in MHz. Optional: omitted in non-expert output (the
    /// PMR446 grid is fixed); when absent, apply keeps the radio's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_mhz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_mhz: Option<f64>,
    pub power: Power,
    /// Channel bandwidth in kHz. Expert-only: PMR446 is fixed at 12.5 kHz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_khz: Option<f64>,
    #[serde(default)]
    pub rx_only: bool,
    // analog
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_tone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_tone: Option<String>,
    // digital
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_code: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_slot: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_group: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypt_key: Option<u8>, // 0/None = off, 1..30 = key slot
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_system: Option<u8>, // 0 = none, else 1-based alarm-system slot
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_list: Option<u8>, // 0 = none, else 1-based scan-list slot
    // --- expert: TX admit / timeout (both modes) ---
    /// TX admit criteria: 0=always, 1=channel-free, 2=CTCSS/CC correct (rec[34] bits 0x30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_admit: Option<u8>,
    /// TX timeout timer, seconds (0=off) (rec[45]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tot_s: Option<u8>,
    /// TOT pre-alert time, seconds (rec[46]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tot_prealert_s: Option<u8>,
    /// TOT re-key/rekey time, seconds (rec[47]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tot_rekey_s: Option<u8>,
    /// Auto-start scan on channel select (D rec[60]&1 / A rec[56]&1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_scan: Option<bool>,
    /// Off-network / direct mode (脱网) (D rec[60]&2 / A rec[56]&2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_network: Option<bool>,
    // --- expert: digital-only ---
    /// Text-message delivery confirmation (rec[35]&0x80).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sms_confirm: Option<bool>,
    /// Text-message format 0..3 (rec[35]&0x03).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sms_format: Option<u8>,
    /// Private-call confirmation (rec[53]&0x10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_call_confirm: Option<bool>,
    /// Timed-preamble preference 0..3 (rec[55]&0x03).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preamble_pref: Option<u8>,
    /// Emergency: alarm indication / alarm reply / call indication (rec[58] bits 1/2/4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_alarm_ind: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_alarm_reply: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_call_ind: Option<bool>,
    /// Solo / lone-worker mode (rec[60]&0x08).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo_work: Option<bool>,
    /// Encryption: random key / multi-key decrypt (rec[61] bits 2/4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enc_random_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enc_multi_key: Option<bool>,
    /// Direct dual-slot / 直通双时隙 (rec[63]&0x08).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_slot: Option<bool>,
    // --- expert: analog-only ---
    /// Signalling reset time, seconds (rec[48]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_time_s: Option<u8>,
    /// Whisper mode / 耳语 (rec[56]&0x10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper: Option<bool>,
    /// Sub-audio tail elimination: kind 0..3 (rec[57] bits 0x06), enable (rec[57]&0x10),
    /// and 120°/180° phase / tail-HZ (rec[57]&0x01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_elim: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_hz: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subaudio_tail_elim: Option<u8>,
    /// RX squelch mode (rec[63]&0x01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_squelch_mode: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Digital,
    Analog,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Power {
    Low,
    High,
}

// ---- CTCSS / DCS sub-tone -------------------------------------------------
// TOML representation: "off", a CTCSS frequency like "114.8", or DCS like
// "D026N" / "D026I" (octal code + Normal/Inverted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    Off,
    Ctcss(u16), // frequency * 10
    Dcs { code: u16, invert: bool },
}

impl Tone {
    fn decode(lo: u8, hi: u8) -> Tone {
        if lo == 0xFF && (hi == 0xFF || hi == 0x3F) {
            return Tone::Off;
        }
        let val = ((hi & 0x0F) as u16) * 256 + lo as u16;
        match hi & 0xC0 {
            0x00 => Tone::Ctcss(val),
            0x80 => Tone::Dcs {
                code: val,
                invert: false,
            },
            _ => Tone::Dcs {
                code: val,
                invert: true,
            },
        }
    }
    fn encode(self) -> (u8, u8) {
        match self {
            Tone::Off => (0xFF, 0xFF),
            Tone::Ctcss(v) => ((v & 0xFF) as u8, ((v >> 8) & 0x0F) as u8),
            Tone::Dcs { code, invert } => {
                let base = if invert { 0xC0 } else { 0x80 };
                ((code & 0xFF) as u8, base | ((code >> 8) & 0x0F) as u8)
            }
        }
    }
    fn to_toml(self) -> Option<String> {
        match self {
            Tone::Off => None,
            Tone::Ctcss(v) => Some(format!("{:.1}", v as f64 / 10.0)),
            Tone::Dcs { code, invert } => {
                Some(format!("D{:03o}{}", code, if invert { "I" } else { "N" }))
            }
        }
    }
}

impl FromStr for Tone {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Tone> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("off") {
            return Ok(Tone::Off);
        }
        if let Some(rest) = s.strip_prefix(['D', 'd']) {
            let (digits, inv) = match rest.chars().last() {
                Some('N') | Some('n') => (&rest[..rest.len() - 1], false),
                Some('I') | Some('i') => (&rest[..rest.len() - 1], true),
                _ => (rest, false),
            };
            let code = u16::from_str_radix(digits.trim(), 8)?;
            return Ok(Tone::Dcs { code, invert: inv });
        }
        let f: f64 = s.parse()?;
        Ok(Tone::Ctcss((f * 10.0).round() as u16))
    }
}

// ---- decode: Codeplug -> RadioConfig --------------------------------------

/// Scalar setting access: CPS index WWxx[i] -> region payload[i-15].
fn g(pl: &[u8], ww: usize) -> u8 {
    pl[ww - 15]
}
fn bit(pl: &[u8], ww: usize, mask: u8) -> bool {
    pl[ww - 15] & mask != 0
}

fn decode_general(cp: &Codeplug, expert: bool) -> Result<General> {
    let r02 = cp.region("r02")?.payload();
    let r03 = cp.region("r03")?.payload();
    let r0a = cp.region("r0A")?.payload();
    let r05 = cp.region("r05")?.payload();
    let vox = g(r02, 86);
    let ps = g(r02, 94);
    let m100 = g(r02, 100);
    let tones = Tones {
        all_tones_off: m100 == 0xCD,
        talk_permit: (m100 & 0x0C) >> 2,
        fixed_volume: g(r0a, 22),
        remote_stun: g(r0a, 23) != 0,
        remote_activate: g(r0a, 24) != 0,
        wrong_operation: g(r0a, 25) != 0,
        tx_busy_lockout: g(r0a, 26) != 0,
        talk_timeout: g(r0a, 27) != 0,
        talk_timeout_prealert: g(r0a, 28) != 0,
        call_out: g(r0a, 29) != 0,
        private_call: g(r0a, 30) != 0,
        group_call: g(r0a, 31) != 0,
        all_call: g(r0a, 32) != 0,
        low_battery: g(r0a, 33) != 0,
        free_channel: g(r0a, 34) != 0,
        scan_list_empty: g(r0a, 35) != 0,
        contact_list_empty: g(r0a, 36) != 0,
        alarm_dwell: g(r0a, 37) != 0,
        power_on: g(r0a, 38) != 0,
        alarm: g(r0a, 39) != 0,
        call_alert: g(r0a, 40) != 0,
        priority_channel: g(r0a, 41) != 0,
        send_fail: g(r0a, 42) != 0,
    };
    Ok(General {
        channel_mode: g(r02, 92),
        squelch: g(r02, 84),
        vox_enable: vox & 0x80 != 0,
        vox_level: vox & 0x0F,
        volume_max: g(r02, 76),
        volume_min: g(r02, 77),
        password_enable: g(r02, 24) == 2,
        mic_gain_internal: match g(r02, 17) {
            0 => "low",
            6 => "mid",
            _ => "high",
        }
        .to_string(),
        mic_agc: bit(r02, 18, 0x01),
        mic_gain_analog: g(r02, 19) as i8 - 42,
        tx_end_tone: g(r02, 87) != 0,
        voice_prompt: match g(r02, 95) {
            0 => "off",
            1 => "english",
            _ => "chinese",
        }
        .to_string(),
        rf_sensitivity_enhanced: bit(r02, 93, 0x01),
        scan_carrier_operated: g(r02, 85) != 0,
        power_save_mode: ps & 0x0F,
        power_save_delay_s: 5 + (ps >> 4) * 5,
        radio_dmr_id: get_dmr_id(r03, 16 - 15),
        dmr_service: expert.then(|| DmrService {
            preamble: g(r03, 35),
            group_call_hang_ms: g(r03, 37) as u16 * 500,
            private_call_hang_ms: g(r03, 38) as u16 * 500,
            priority_hang_ms: g(r03, 43) as u16 * 500,
            remote_monitor_s: (g(r03, 41) as u16 * 10).min(255) as u8,
            decode_call_alert: bit(r03, 40, 0x01),
            decode_radio_check: bit(r03, 40, 0x02),
            decode_remote_stun: bit(r03, 40, 0x04),
            decode_remote_monitor: bit(r03, 40, 0x08),
            remote_kill_id: get_dmr_id(r03, 44 - 15),
        }),
        man_down_enable: g(r0a, 18) != 0,
        man_down_prealarm_s: g(r0a, 19),
        man_down_delay_s: g(r0a, 20),
        man_down_angle_deg: g(r0a, 21),
        work_alone_enable: bit(r05, 784, 0x01),
        work_alone_prealarm_tone: bit(r05, 784, 0x04),
        work_alone_response_s: g(r05, 785),
        work_alone_prealarm_s: g(r05, 786),
        voice_encrypt_enable: g(r02, 140) == 67,
        encrypt_type: g(r02, 141) & 0x01,
        key_long_press_s: 0.5 + g(r02, 104) as f64 * 0.5,
        key_top_short: g(r02, 106),
        key_top_long: g(r02, 107),
        key_side1_short: g(r02, 108),
        key_side1_long: g(r02, 109),
        key_side2_short: g(r02, 110),
        key_side2_long: g(r02, 111),
        tones,
    })
}

pub fn decode(cp: &Codeplug, country: &str, expert: bool) -> Result<RadioConfig> {
    let general = decode_general(cp, expert)?;

    let r08 = cp.region("r08")?.payload();
    let mut channel = Vec::new();
    for l in 0..CHANNEL_COUNT {
        let base = l * CHANNEL_STRIDE;
        if base + CHANNEL_STRIDE > r08.len() {
            break;
        }
        let rec = &r08[base..base + CHANNEL_STRIDE];
        let rx = u32le(rec, 37);
        let mode_b = rec[33];
        if rx == 0xFFFFFFFF || rx == 0 || mode_b > 1 {
            continue; // empty slot
        }
        let pb = rec[34];
        let power = if pb & 0x03 == 2 {
            Power::High
        } else {
            Power::Low
        };
        let mut ch = Channel {
            index: (l + 1) as u16,
            name: get_name(rec, 1, 16),
            mode: if mode_b == 0 {
                Mode::Digital
            } else {
                Mode::Analog
            },
            rx_mhz: expert.then(|| rx as f64 / 1e6),
            tx_mhz: expert.then(|| u32le(rec, 41) as f64 / 1e6),
            power,
            bandwidth_khz: expert.then_some({
                if mode_b == 1 && pb & 0x04 != 0 {
                    25.0
                } else {
                    12.5
                }
            }),
            rx_only: pb & 0x40 != 0,
            rx_tone: None,
            tx_tone: None,
            color_code: None,
            time_slot: None,
            contact: None,
            rx_group: None,
            encrypt_key: None,
            emergency_system: None,
            scan_list: None,
            tx_admit: None,
            tot_s: None,
            tot_prealert_s: None,
            tot_rekey_s: None,
            auto_scan: None,
            off_network: None,
            sms_confirm: None,
            sms_format: None,
            private_call_confirm: None,
            preamble_pref: None,
            emergency_alarm_ind: None,
            emergency_alarm_reply: None,
            emergency_call_ind: None,
            solo_work: None,
            enc_random_key: None,
            enc_multi_key: None,
            dual_slot: None,
            reset_time_s: None,
            whisper: None,
            tail_elim: None,
            tail_hz: None,
            subaudio_tail_elim: None,
            rx_squelch_mode: None,
        };
        // Shared expert fields (both modes).
        ch.tx_admit = expert.then(|| (rec[34] & 0x30) >> 4);
        ch.tot_s = expert.then_some(rec[45]);
        ch.tot_prealert_s = expert.then_some(rec[46]);
        ch.tot_rekey_s = expert.then_some(rec[47]);
        if mode_b == 0 {
            ch.color_code = Some(rec[53] & 0x0F);
            // Timeslot is expert-only: on stock PMR446 simplex it must stay TS1.
            ch.time_slot = expert.then(|| if rec[53] & 0x20 != 0 { 2 } else { 1 });
            ch.contact = Some(u16le(rec, 49));
            ch.rx_group = Some(u16le(rec, 51));
            ch.encrypt_key = Some(if rec[61] & 0x01 != 0 { rec[62] } else { 0 });
            ch.emergency_system = Some(rec[57]);
            ch.scan_list = Some(rec[59]);
            ch.auto_scan = expert.then_some(rec[60] & 0x01 != 0);
            ch.off_network = expert.then_some(rec[60] & 0x02 != 0);
            ch.sms_confirm = expert.then_some(rec[35] & 0x80 != 0);
            ch.sms_format = expert.then_some(rec[35] & 0x03);
            ch.private_call_confirm = expert.then_some(rec[53] & 0x10 != 0);
            ch.preamble_pref = expert.then_some(rec[55] & 0x03);
            ch.emergency_alarm_ind = expert.then_some(rec[58] & 0x01 != 0);
            ch.emergency_alarm_reply = expert.then_some(rec[58] & 0x02 != 0);
            ch.emergency_call_ind = expert.then_some(rec[58] & 0x04 != 0);
            ch.solo_work = expert.then_some(rec[60] & 0x08 != 0);
            ch.enc_random_key = expert.then_some(rec[61] & 0x02 != 0);
            ch.enc_multi_key = expert.then_some(rec[61] & 0x04 != 0);
            ch.dual_slot = expert.then_some(rec[63] & 0x08 != 0);
        } else {
            ch.rx_tone = Tone::decode(rec[59], rec[60]).to_toml();
            ch.tx_tone = Tone::decode(rec[61], rec[62]).to_toml();
            ch.auto_scan = expert.then_some(rec[56] & 0x01 != 0);
            ch.off_network = expert.then_some(rec[56] & 0x02 != 0);
            ch.reset_time_s = expert.then_some(rec[48]);
            ch.whisper = expert.then_some(rec[56] & 0x10 != 0);
            ch.tail_elim = expert.then_some(rec[57] & 0x10 != 0);
            ch.tail_hz = expert.then_some(rec[57] & 0x01 != 0);
            ch.subaudio_tail_elim = expert.then_some((rec[57] & 0x06) >> 1);
            ch.rx_squelch_mode = expert.then_some(rec[63] & 0x01);
        }
        channel.push(ch);
    }

    let (contact, rx_group, zone, scan, message, alarm, one_touch, enc_key) =
        decode_tables(cp, expert)?;

    Ok(RadioConfig {
        radio: RadioInfo {
            country: country.to_string(),
            note: String::new(),
        },
        general,
        channel,
        contact,
        rx_group,
        zone,
        scan,
        message,
        alarm,
        one_touch,
        enc_key,
    })
}

// ---- apply: RadioConfig -> existing Codeplug ------------------------------

/// Scalar setting mutable access: WWxx[i] -> payload[i-15].
fn gm(pl: &mut [u8], ww: usize, v: u8) {
    pl[ww - 15] = v;
}
fn gm_bit(pl: &mut [u8], ww: usize, mask: u8, on: bool) {
    let i = ww - 15;
    if on {
        pl[i] |= mask;
    } else {
        pl[i] &= !mask;
    }
}

fn apply_general(cp: &mut Codeplug, gen: &General) -> Result<()> {
    {
        let r02 = cp.region_mut("r02")?.payload_mut();
        gm(r02, 92, gen.channel_mode);
        gm(r02, 84, gen.squelch);
        gm(
            r02,
            86,
            (if gen.vox_enable { 0x80 } else { 0 }) | (gen.vox_level & 0x0F),
        );
        gm(r02, 76, gen.volume_max);
        gm(r02, 77, gen.volume_min);
        gm(r02, 24, if gen.password_enable { 2 } else { 0xF8 });
        gm(
            r02,
            17,
            match gen.mic_gain_internal.as_str() {
                "low" => 0,
                "mid" => 6,
                _ => 12,
            },
        );
        gm_bit(r02, 18, 0x01, gen.mic_agc);
        gm(
            r02,
            19,
            (gen.mic_gain_analog as i16 + 42).clamp(0, 62) as u8,
        );
        gm(r02, 87, gen.tx_end_tone as u8);
        gm(
            r02,
            95,
            match gen.voice_prompt.as_str() {
                "off" => 0,
                "english" => 1,
                _ => 2,
            },
        );
        gm_bit(r02, 93, 0x01, gen.rf_sensitivity_enhanced);
        gm(r02, 85, gen.scan_carrier_operated as u8);
        let delay_idx = (gen.power_save_delay_s.saturating_sub(5) / 5).min(11);
        gm(r02, 94, (gen.power_save_mode & 0x0F) | (delay_idx << 4));
        // buttons
        gm(
            r02,
            104,
            ((gen.key_long_press_s / 0.5 - 1.0).round().clamp(0.0, 9.0)) as u8,
        );
        gm(r02, 106, gen.key_top_short);
        gm(r02, 107, gen.key_top_long);
        gm(r02, 108, gen.key_side1_short);
        gm(r02, 109, gen.key_side1_long);
        gm(r02, 110, gen.key_side2_short);
        gm(r02, 111, gen.key_side2_long);
        // master tone byte + talk-permit (read-modify-write)
        if gen.tones.all_tones_off {
            gm(r02, 100, 0xCD);
        } else {
            let cur = g(r02, 100);
            gm(
                r02,
                100,
                (cur & !0x0C) | ((gen.tones.talk_permit & 0x03) << 2),
            );
        }
    }
    {
        let r03 = cp.region_mut("r03")?.payload_mut();
        set_dmr_id(r03, 16 - 15, gen.radio_dmr_id);
        // Advanced DMR-service fields: only written when present (expert),
        // otherwise the radio's existing values are preserved.
        if let Some(ds) = &gen.dmr_service {
            gm(r03, 35, ds.preamble);
            gm(r03, 37, (ds.group_call_hang_ms / 500) as u8);
            gm(r03, 38, (ds.private_call_hang_ms / 500) as u8);
            gm(r03, 43, (ds.priority_hang_ms / 500) as u8);
            gm(r03, 41, (ds.remote_monitor_s / 10).max(1));
            gm_bit(r03, 40, 0x01, ds.decode_call_alert);
            gm_bit(r03, 40, 0x02, ds.decode_radio_check);
            gm_bit(r03, 40, 0x04, ds.decode_remote_stun);
            gm_bit(r03, 40, 0x08, ds.decode_remote_monitor);
            set_dmr_id(r03, 44 - 15, ds.remote_kill_id);
        }
    }
    {
        let r0a = cp.region_mut("r0A")?.payload_mut();
        gm(r0a, 18, gen.man_down_enable as u8);
        gm(r0a, 19, gen.man_down_prealarm_s);
        gm(r0a, 20, gen.man_down_delay_s);
        gm(r0a, 21, gen.man_down_angle_deg);
        let t = &gen.tones;
        gm(r0a, 22, t.fixed_volume);
        for (ww, on) in [
            (23, t.remote_stun),
            (24, t.remote_activate),
            (25, t.wrong_operation),
            (26, t.tx_busy_lockout),
            (27, t.talk_timeout),
            (28, t.talk_timeout_prealert),
            (29, t.call_out),
            (30, t.private_call),
            (31, t.group_call),
            (32, t.all_call),
            (33, t.low_battery),
            (34, t.free_channel),
            (35, t.scan_list_empty),
            (36, t.contact_list_empty),
            (37, t.alarm_dwell),
            (38, t.power_on),
            (39, t.alarm),
            (40, t.call_alert),
            (41, t.priority_channel),
            (42, t.send_fail),
        ] {
            gm(r0a, ww, on as u8);
        }
    }
    {
        let r05 = cp.region_mut("r05")?.payload_mut();
        gm_bit(r05, 784, 0x01, gen.work_alone_enable);
        gm_bit(r05, 784, 0x04, gen.work_alone_prealarm_tone);
        gm(r05, 785, gen.work_alone_response_s);
        gm(r05, 786, gen.work_alone_prealarm_s);
    }
    {
        let r02 = cp.region_mut("r02")?.payload_mut();
        gm(r02, 140, if gen.voice_encrypt_enable { 67 } else { 66 });
        gm_bit(r02, 141, 0x01, gen.encrypt_type != 0);
    }
    Ok(())
}

pub fn apply(cp: &mut Codeplug, cfg: &RadioConfig) -> Result<()> {
    apply_general(cp, &cfg.general)?;
    let r08 = cp.region_mut("r08")?.payload_mut();
    for ch in &cfg.channel {
        let l = (ch.index as usize).saturating_sub(1);
        let base = l * CHANNEL_STRIDE;
        if base + CHANNEL_STRIDE > r08.len() {
            bail!(
                "channel index {} out of range (max {})",
                ch.index,
                CHANNEL_COUNT
            );
        }
        let rec = &mut r08[base..base + CHANNEL_STRIDE];
        set_name(rec, 1, 16, &ch.name);
        rec[33] = match ch.mode {
            Mode::Digital => 0,
            Mode::Analog => 1,
        };
        // power bits [1:0], preserve other bits of the byte
        let mut pb = rec[34] & !0x03;
        pb |= match ch.power {
            Power::Low => 0,
            Power::High => 2,
        };
        pb &= !0x40;
        if ch.rx_only {
            pb |= 0x40;
        }
        if ch.mode == Mode::Analog {
            if let Some(bw) = ch.bandwidth_khz {
                pb &= !0x04;
                if bw >= 20.0 {
                    pb |= 0x04;
                }
            }
        }
        if let Some(a) = ch.tx_admit {
            pb = (pb & !0x30) | ((a & 0x03) << 4);
        }
        rec[34] = pb;
        // Shared TX-timeout fields (both modes).
        if let Some(v) = ch.tot_s {
            rec[45] = v;
        }
        if let Some(v) = ch.tot_prealert_s {
            rec[46] = v;
        }
        if let Some(v) = ch.tot_rekey_s {
            rec[47] = v;
        }
        if let Some(rx) = ch.rx_mhz {
            put_u32le(rec, 37, (rx * 1e6).round() as u32);
        }
        if let Some(tx) = ch.tx_mhz {
            put_u32le(rec, 41, (tx * 1e6).round() as u32);
        }
        match ch.mode {
            Mode::Digital => {
                // Only touch a field's bits when it is present, so hidden
                // (expert) fields keep the radio's existing value.
                if let Some(cc) = ch.color_code {
                    rec[53] = (rec[53] & !0x0F) | (cc & 0x0F);
                }
                if let Some(ts) = ch.time_slot {
                    rec[53] = (rec[53] & !0x20) | if ts == 2 { 0x20 } else { 0 };
                }
                put_u16le(rec, 49, ch.contact.unwrap_or(0));
                put_u16le(rec, 51, ch.rx_group.unwrap_or(0));
                let key = ch.encrypt_key.unwrap_or(0);
                rec[61] = (rec[61] & !0x01) | if key > 0 { 1 } else { 0 };
                rec[62] = key;
                if let Some(e) = ch.emergency_system {
                    rec[57] = e;
                }
                if let Some(s) = ch.scan_list {
                    rec[59] = s;
                }
                if let Some(b) = ch.auto_scan {
                    rec[60] = (rec[60] & !0x01) | b as u8;
                }
                if let Some(b) = ch.off_network {
                    rec[60] = (rec[60] & !0x02) | ((b as u8) << 1);
                }
                if let Some(b) = ch.solo_work {
                    rec[60] = (rec[60] & !0x08) | ((b as u8) << 3);
                }
                if let Some(b) = ch.sms_confirm {
                    rec[35] = (rec[35] & !0x80) | ((b as u8) << 7);
                }
                if let Some(v) = ch.sms_format {
                    rec[35] = (rec[35] & !0x03) | (v & 0x03);
                }
                if let Some(b) = ch.private_call_confirm {
                    rec[53] = (rec[53] & !0x10) | ((b as u8) << 4);
                }
                if let Some(v) = ch.preamble_pref {
                    rec[55] = (rec[55] & !0x03) | (v & 0x03);
                }
                if let Some(b) = ch.emergency_alarm_ind {
                    rec[58] = (rec[58] & !0x01) | b as u8;
                }
                if let Some(b) = ch.emergency_alarm_reply {
                    rec[58] = (rec[58] & !0x02) | ((b as u8) << 1);
                }
                if let Some(b) = ch.emergency_call_ind {
                    rec[58] = (rec[58] & !0x04) | ((b as u8) << 2);
                }
                if let Some(b) = ch.enc_random_key {
                    rec[61] = (rec[61] & !0x02) | ((b as u8) << 1);
                }
                if let Some(b) = ch.enc_multi_key {
                    rec[61] = (rec[61] & !0x04) | ((b as u8) << 2);
                }
                if let Some(b) = ch.dual_slot {
                    rec[63] = (rec[63] & !0x08) | ((b as u8) << 3);
                }
            }
            Mode::Analog => {
                let (lo, hi) = ch
                    .rx_tone
                    .as_deref()
                    .unwrap_or("off")
                    .parse::<Tone>()?
                    .encode();
                rec[59] = lo;
                rec[60] = hi;
                let (lo, hi) = ch
                    .tx_tone
                    .as_deref()
                    .unwrap_or("off")
                    .parse::<Tone>()?
                    .encode();
                rec[61] = lo;
                rec[62] = hi;
                if let Some(b) = ch.auto_scan {
                    rec[56] = (rec[56] & !0x01) | b as u8;
                }
                if let Some(b) = ch.off_network {
                    rec[56] = (rec[56] & !0x02) | ((b as u8) << 1);
                }
                if let Some(v) = ch.reset_time_s {
                    rec[48] = v;
                }
                if let Some(b) = ch.whisper {
                    rec[56] = (rec[56] & !0x10) | ((b as u8) << 4);
                }
                if let Some(b) = ch.tail_elim {
                    rec[57] = (rec[57] & !0x10) | ((b as u8) << 4);
                }
                if let Some(b) = ch.tail_hz {
                    rec[57] = (rec[57] & !0x01) | b as u8;
                }
                if let Some(v) = ch.subaudio_tail_elim {
                    rec[57] = (rec[57] & !0x06) | ((v & 0x03) << 1);
                }
                if let Some(v) = ch.rx_squelch_mode {
                    rec[63] = (rec[63] & !0x01) | (v & 0x01);
                }
            }
        }
    }
    apply_tables(cp, cfg)?;
    Ok(())
}

// ---- list tables ----------------------------------------------------------

const T_CONTACT: Table = Table {
    region: "r04",
    base_ww: 16,
    stride: 40,
    count: 200,
};
const T_RXGROUP: Table = Table {
    region: "r04",
    base_ww: 8016,
    stride: 72,
    count: 32,
};
const T_ZONE: Table = Table {
    region: "r07",
    base_ww: 16,
    stride: 68,
    count: 16,
};
const T_SCAN: Table = Table {
    region: "r06",
    base_ww: 16,
    stride: 90,
    count: 32,
};
const T_MSG: Table = Table {
    region: "rML",
    base_ww: 16,
    stride: 516,
    count: 32,
};
const T_ALARM: Table = Table {
    region: "r05",
    base_ww: 16,
    stride: 48,
    count: 16,
};
const T_ONETOUCH: Table = Table {
    region: "rKL",
    base_ww: 16,
    stride: 4,
    count: 6,
};
const T_ENCKEY: Table = Table {
    region: "r02",
    base_ww: 144,
    stride: 68,
    count: 30,
};

fn is_fill(rec: &[u8], fill: u8) -> bool {
    rec.iter().all(|&b| b == fill)
}
fn clear(rec: &mut [u8], fill: u8) {
    rec.iter_mut().for_each(|b| *b = fill);
}
/// Encryption-key material lives in 32 bytes at record offset 36. The CPS reads
/// it as 8 little-endian u32 words shown big-endian, so canonical byte `j`
/// (0..31) maps to stored byte `36 + (j/4)*4 + 3 - (j%4)`. Returns the full
/// 64-char canonical hex string.
fn key_canonical_hex(rec: &[u8]) -> String {
    (0..32)
        .map(|j| format!("{:02x}", rec[36 + (j / 4) * 4 + 3 - (j % 4)]))
        .collect()
}

/// Inverse of `key_canonical_hex`: write a canonical key (any length) into the
/// 32-byte material, padding unused nibbles with 0xF (matches the radio's fill).
fn set_key_material(rec: &mut [u8], key_hex: &str) {
    let mut c: String = key_hex
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .collect();
    c.truncate(64);
    while c.len() < 64 {
        c.push('f');
    }
    for j in 0..32 {
        let b = u8::from_str_radix(&c[j * 2..j * 2 + 2], 16).unwrap_or(0xFF);
        rec[36 + (j / 4) * 4 + 3 - (j % 4)] = b;
    }
}

#[allow(clippy::type_complexity)]
fn decode_tables(
    cp: &Codeplug,
    expert: bool,
) -> Result<(
    Vec<Contact>,
    Vec<RxGroup>,
    Vec<Zone>,
    Vec<ScanList>,
    Vec<Message>,
    Vec<Alarm>,
    Vec<OneTouch>,
    Vec<EncKey>,
)> {
    let mut contacts = Vec::new();
    for i in 0..T_CONTACT.count {
        let rec = T_CONTACT.record(cp, i)?;
        if is_fill(rec, 0xFF) {
            continue;
        }
        contacts.push(Contact {
            index: (i + 1) as u16,
            name: get_name(rec, 0, 16),
            dmr_id: get_dmr_id(rec, 32),
            call_type: match rec[36] {
                0 => "private",
                1 => "group",
                _ => "all",
            }
            .to_string(),
        });
    }

    let mut rx_group = Vec::new();
    for i in 0..T_RXGROUP.count {
        let rec = T_RXGROUP.record(cp, i)?;
        if is_fill(rec, 0xFF) {
            continue;
        }
        let n = rec[64] as usize;
        rx_group.push(RxGroup {
            index: (i + 1) as u16,
            name: get_name(rec, 0, 16),
            contacts: get_members(rec, 32, n.min(16)),
        });
    }

    let mut zone = Vec::new();
    for i in 0..T_ZONE.count {
        let rec = T_ZONE.record(cp, i)?;
        if is_fill(rec, 0xFF) {
            continue;
        }
        let n = u16le(rec, 34) as usize;
        zone.push(Zone {
            index: (i + 1) as u16,
            // zone sort byte lives at record offset 30, so the name is 15 chars max
            name: get_name(rec, 0, 15),
            channels: get_members(rec, 36, n.min(16)),
        });
    }

    let mut scan = Vec::new();
    for i in 0..T_SCAN.count {
        let rec = T_SCAN.record(cp, i)?;
        if is_fill(rec, 0xFF) {
            continue;
        }
        let n = u16le(rec, 56) as usize;
        // members start at +58; slot 0 is the "selected channel" (0) marker.
        let all = get_members(rec, 58, n.min(16));
        scan.push(ScanList {
            index: (i + 1) as u16,
            name: get_name(rec, 0, 16),
            channels: all.into_iter().filter(|&v| v != 0).collect(),
            priority1: u16le(rec, 36),
            priority2: u16le(rec, 38),
            reply_mode: expert.then_some(rec[33]),
            designated_channel: expert.then(|| u16le(rec, 34)),
            probe_time: expert.then_some(rec[40]),
            hold_time: expert.then_some(rec[42]),
            talkback: expert.then_some(rec[44] & 0x02 != 0),
            nuisance_delete: expert.then_some(rec[44] & 0x20 != 0),
            scan_led: expert.then_some(rec[44] & 0x80 != 0),
        });
    }

    let mut message = Vec::new();
    for i in 0..T_MSG.count {
        let rec = T_MSG.record(cp, i)?;
        if is_fill(rec, 0x00) {
            continue;
        }
        let len = u16le(rec, 2) as usize;
        message.push(Message {
            index: (i + 1) as u16,
            text: get_name(rec, 4, len.min(256)),
        });
    }

    let mut alarm = Vec::new();
    for i in 0..T_ALARM.count {
        let rec = T_ALARM.record(cp, i)?;
        if is_fill(rec, 0xFF) {
            continue;
        }
        alarm.push(Alarm {
            index: (i + 1) as u16,
            name: get_name(rec, 0, 16),
            revert_channel: u16le(rec, 32),
            alarm_type: rec[34] & 0x07,
            alarm_mode: (rec[34] & 0x30) >> 4,
            impolite_retries: rec[35],
            polite_retries: rec[36],
            hot_mic_s: rec[38],
            rx_dwell_s: rec[39],
        });
    }

    let mut one_touch = Vec::new();
    for i in 0..T_ONETOUCH.count {
        let rec = T_ONETOUCH.record(cp, i)?;
        one_touch.push(OneTouch {
            index: (i + 1) as u8,
            mode: rec[0],
            contact: rec[1],
            action: if rec[2] == 1 { "message" } else { "call" }.to_string(),
            message_index: rec[3],
        });
    }

    let mut enc_key = Vec::new();
    for i in 0..T_ENCKEY.count {
        let rec = T_ENCKEY.record(cp, i)?;
        if is_fill(rec, 0xFF) || !(1..=3).contains(&rec[2]) {
            continue;
        }
        let key_len = rec[1] as usize; // key length in hex chars
        let canonical = key_canonical_hex(rec);
        enc_key.push(EncKey {
            index: (i + 1) as u8,
            name: get_name(rec, 4, 16),
            key_type: match rec[2] {
                1 => "arc4",
                2 => "aes128",
                _ => "aes256",
            }
            .to_string(),
            key: canonical[..key_len.min(64)].to_string(),
        });
    }

    Ok((
        contacts, rx_group, zone, scan, message, alarm, one_touch, enc_key,
    ))
}

fn find<T>(list: &[T], slot: usize, index_of: impl Fn(&T) -> usize) -> Option<&T> {
    list.iter().find(|e| index_of(e) == slot + 1)
}

fn apply_tables(cp: &mut Codeplug, cfg: &RadioConfig) -> Result<()> {
    for i in 0..T_CONTACT.count {
        let rec = T_CONTACT.record_mut(cp, i)?;
        match find(&cfg.contact, i, |c| c.index as usize) {
            Some(c) => {
                set_name(rec, 0, 16, &c.name);
                set_dmr_id(rec, 32, c.dmr_id);
                rec[36] = match c.call_type.as_str() {
                    "private" => 0,
                    "group" => 1,
                    _ => 2,
                };
                put_u16le(rec, 38, (i + 1) as u16);
            }
            None => clear(rec, 0xFF),
        }
    }
    for i in 0..T_RXGROUP.count {
        let rec = T_RXGROUP.record_mut(cp, i)?;
        match find(&cfg.rx_group, i, |g| g.index as usize) {
            Some(g) => {
                set_name(rec, 0, 16, &g.name);
                set_members(rec, 32, &g.contacts, 16);
                rec[64] = g.contacts.len() as u8;
                rec[65] = (i + 1) as u8;
            }
            None => clear(rec, 0xFF),
        }
    }
    for i in 0..T_ZONE.count {
        let rec = T_ZONE.record_mut(cp, i)?;
        match find(&cfg.zone, i, |z| z.index as usize) {
            Some(z) => {
                set_name(rec, 0, 15, &z.name);
                rec[30] = (i + 1) as u8; // sort byte
                put_u16le(rec, 32, (i + 1) as u16);
                put_u16le(rec, 34, z.channels.len() as u16);
                set_members(rec, 36, &z.channels, 16);
            }
            None => clear(rec, 0xFF),
        }
    }
    for i in 0..T_SCAN.count {
        let rec = T_SCAN.record_mut(cp, i)?;
        match find(&cfg.scan, i, |s| s.index as usize) {
            Some(s) => {
                set_name(rec, 0, 16, &s.name);
                put_u16le(rec, 36, s.priority1);
                put_u16le(rec, 38, s.priority2);
                if let Some(m) = s.reply_mode {
                    rec[33] = m;
                }
                if let Some(c) = s.designated_channel {
                    put_u16le(rec, 34, c);
                }
                if let Some(v) = s.probe_time {
                    rec[40] = v;
                }
                if let Some(v) = s.hold_time {
                    rec[42] = v;
                }
                if let Some(b) = s.talkback {
                    rec[44] = (rec[44] & !0x02) | ((b as u8) << 1);
                }
                if let Some(b) = s.nuisance_delete {
                    rec[44] = (rec[44] & !0x20) | ((b as u8) << 5);
                }
                if let Some(b) = s.scan_led {
                    rec[44] = (rec[44] & !0x80) | ((b as u8) << 7);
                }
                let mut members = vec![0u16]; // slot 0 = selected-channel marker
                members.extend_from_slice(&s.channels);
                put_u16le(rec, 54, (i + 1) as u16);
                put_u16le(rec, 56, members.len() as u16);
                set_members(rec, 58, &members, 16);
            }
            None => clear(rec, 0xFF),
        }
    }
    for i in 0..T_MSG.count {
        let rec = T_MSG.record_mut(cp, i)?;
        match find(&cfg.message, i, |m| m.index as usize) {
            Some(m) => {
                let chars: Vec<u16> = m.text.encode_utf16().take(256).collect();
                put_u16le(rec, 0, (i + 1) as u16);
                put_u16le(rec, 2, chars.len() as u16);
                for k in 0..256 {
                    let v = chars.get(k).copied().unwrap_or(0xFFFF);
                    put_u16le(rec, 4 + k * 2, v);
                }
            }
            None => clear(rec, 0x00),
        }
    }
    for i in 0..T_ALARM.count {
        let rec = T_ALARM.record_mut(cp, i)?;
        match find(&cfg.alarm, i, |a| a.index as usize) {
            Some(a) => {
                set_name(rec, 0, 16, &a.name);
                put_u16le(rec, 32, a.revert_channel);
                rec[34] = (rec[34] & !0x37) | (a.alarm_type & 0x07) | ((a.alarm_mode & 0x03) << 4);
                rec[35] = a.impolite_retries;
                rec[36] = a.polite_retries;
                rec[38] = a.hot_mic_s;
                rec[39] = a.rx_dwell_s;
                rec[45] = (i + 1) as u8;
            }
            None => clear(rec, 0xFF),
        }
    }
    for i in 0..T_ONETOUCH.count {
        let rec = T_ONETOUCH.record_mut(cp, i)?;
        if let Some(o) = find(&cfg.one_touch, i, |o| o.index as usize) {
            rec[0] = o.mode;
            rec[1] = o.contact;
            rec[2] = if o.action == "message" { 1 } else { 0 };
            rec[3] = o.message_index;
        }
    }
    for i in 0..T_ENCKEY.count {
        let rec = T_ENCKEY.record_mut(cp, i)?;
        match find(&cfg.enc_key, i, |k| k.index as usize) {
            Some(k) => {
                let key_hex: String = k.key.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                // length validation for the fixed-size AES types
                let want = match k.key_type.as_str() {
                    "aes128" => Some(32),
                    "aes256" => Some(64),
                    _ => None,
                };
                if let Some(w) = want {
                    if key_hex.len() != w {
                        bail!(
                            "enc_key {}: {} needs a {}-hex-char ({}-byte) key, got {} chars",
                            k.index,
                            k.key_type,
                            w,
                            w / 2,
                            key_hex.len()
                        );
                    }
                }
                rec[0] = (i + 1) as u8;
                rec[1] = key_hex.len() as u8;
                rec[2] = match k.key_type.as_str() {
                    "arc4" => 1,
                    "aes128" => 2,
                    _ => 3,
                };
                set_name(rec, 4, 16, &k.name);
                set_key_material(rec, &key_hex);
            }
            None => clear(rec, 0xFF),
        }
    }
    Ok(())
}

pub fn to_toml(cfg: &RadioConfig) -> Result<String> {
    Ok(toml::to_string_pretty(cfg)?)
}

/// One-line explanation for a setting key, shown as a trailing `# comment`
/// when the user asks for an annotated config.
fn comment_for(key: &str) -> Option<&'static str> {
    Some(match key {
        // general
        "channel_mode" => "0=digital only, 1=analog only, 2=both",
        "squelch" => "0 (open) .. 9 (tight)",
        "vox_enable" => "hands-free voice-activated TX",
        "vox_level" => "VOX sensitivity 1..9",
        "volume_max" => "0..127",
        "volume_min" => "0..127",
        "password_enable" => "require a password to re-program the radio",
        "mic_gain_internal" => "low | mid | high",
        "mic_agc" => "automatic mic gain control",
        "mic_gain_analog" => "-42..+20",
        "tx_end_tone" => "roger beep at end of transmission",
        "voice_prompt" => {
            "off | english | chinese (announces the channel NUMBER; the P64 has no display)"
        }
        "rf_sensitivity_enhanced" => "false=normal, true=enhanced RX sensitivity",
        "scan_carrier_operated" => "false=time-operated scan, true=carrier-operated",
        "power_save_mode" => "0=off, 1=1:1, 2=1:2, 3=1:4",
        "power_save_delay_s" => "seconds before power-save kicks in",
        "radio_dmr_id" => "this radio's DMR ID (1..16776415)",
        "preamble" => "DMR preamble/head-frame count 1..10",
        "group_call_hang_ms" => "0..7000 ms, step 500",
        "private_call_hang_ms" => "0..7000 ms, step 500",
        "priority_hang_ms" => "0..7000 ms, step 500",
        "remote_monitor_s" => "remote-monitor duration, 10..120 s",
        "remote_kill_id" => "DMR ID allowed to remote-kill this radio",
        "man_down_enable" => "fall/tilt alarm",
        "man_down_angle_deg" => "tilt angle that triggers, 10..60",
        "work_alone_enable" => "lone-worker check-in alarm",
        "key_long_press_s" => "long-press threshold, 0.5..5.0 s",
        "key_top_short" | "key_top_long" | "key_side1_short" | "key_side1_long"
        | "key_side2_short" | "key_side2_long" => {
            "key-function code (see PROTOCOL.md button table)"
        }
        "talk_permit" => "0=off, 1=analog, 2=digital, 3=both",
        "all_tones_off" => "master mute for every alert tone",
        // channel
        "mode" => "digital | analog",
        "rx_mhz" | "tx_mhz" => "frequency in MHz (expert; PMR446 grid is fixed)",
        "power" => "high = 0.5 W (spec max, legal PMR446); low = reduced power",
        "bandwidth_khz" => "12.5 or 25 (analog); digital is always 12.5",
        "rx_only" => "listen-only, no transmit",
        "color_code" => "DMR color code 0..15 (must match the other radios)",
        "time_slot" => "DMR timeslot 1 or 2",
        "contact" => "digital TX target: the [[contact]] (talkgroup) you transmit to",
        "rx_group" => "extra talkgroups you also hear: the [[rx_group]] list number (0=none)",
        "encrypt_key" => "encryption on/off: 0=clear, else the [[enc_key]] slot number",
        "emergency_system" => "panic-button behaviour: 0=none, else the [[alarm]] number",
        "scan_list" => "multi-channel monitoring: 0=none, else the [[scan]] number",
        "rx_tone" | "tx_tone" => "off | CTCSS Hz (e.g. 88.5) | DCS (e.g. D023N)",
        // tables
        "dmr_id" => "DMR ID; 16777215 = All Call",
        "call_type" => "private | group | all",
        "key_type" => "arc4 | aes128 | aes256",
        "key" => "hex key; openssl rand -hex 16 (AES128) / 32 (AES256)",
        "channels" => "1-based channel numbers (0 = current/selected)",
        "contacts" => "1-based contact numbers",
        "revert_channel" => "0=current channel, else channel number to alarm on",
        "alarm_type" => "0=none, 1=siren, 2=normal, 3=silent, 4=silent+voice",
        "alarm_mode" => "0=alarm only, 1=alarm+call, 2=alarm+open-mic",
        "impolite_retries" => "retransmits ignoring a busy channel (1..15)",
        "polite_retries" => "retransmits only on a clear channel (1..15)",
        "hot_mic_s" => "seconds the mic stays open after triggering",
        "rx_dwell_s" => "seconds to listen for an acknowledgement",
        "action" => "call | message",
        _ => return None,
    })
}

/// Add trailing `# explanation` comments to a serialized TOML string.
pub fn annotate(toml_str: &str) -> String {
    let mut out = String::with_capacity(toml_str.len() * 2);
    for line in toml_str.lines() {
        let trimmed = line.trim_start();
        if let Some(eq) = trimmed.find(" = ") {
            let key = &trimmed[..eq];
            if let Some(c) = comment_for(key) {
                out.push_str(line);
                out.push_str("  # ");
                out.push_str(c);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
pub fn from_toml(s: &str) -> Result<RadioConfig> {
    Ok(toml::from_str(s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmr_id_bcd_roundtrip() {
        for id in [1u32, 9, 1234567, 16_776_415, 16_777_215] {
            let mut b = [0u8; 4];
            set_dmr_id(&mut b, 0, id);
            assert_eq!(get_dmr_id(&b, 0), id, "dmr id {id}");
        }
    }

    #[test]
    fn tone_roundtrip() {
        let norm = |t: Tone| t.to_toml().unwrap_or_else(|| "off".into());
        for s in ["off", "88.5", "127.3", "D023N", "D754I"] {
            let t: Tone = s.parse().unwrap();
            let (lo, hi) = t.encode();
            assert_eq!(norm(t), norm(Tone::decode(lo, hi)), "tone {s}");
        }
        // sanity on exact string forms
        assert_eq!(norm("88.5".parse().unwrap()), "88.5");
        assert_eq!(norm("D023N".parse().unwrap()), "D023N");
    }

    #[test]
    fn key_material_canonical_roundtrip() {
        let mut rec = [0u8; 68];
        let key = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        set_key_material(&mut rec, key);
        assert_eq!(key_canonical_hex(&rec), key);
    }

    #[test]
    fn name_roundtrip() {
        let mut b = [0xAAu8; 40];
        set_name(&mut b, 0, 16, "Ops-1");
        assert_eq!(get_name(&b, 0, 16), "Ops-1");
        // truncation to max chars
        set_name(&mut b, 0, 4, "ABCDEFG");
        assert_eq!(get_name(&b, 0, 4), "ABCD");
    }
}

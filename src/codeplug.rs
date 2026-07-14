//! Raw codeplug container: the 13 memory regions exactly as read from the radio,
//! with helpers to reach the payload bytes and the validated field offsets.
//!
//! Offset conventions (validated against a real radio dump):
//!  - A region file is a full frame: `5F 5F LEN(2) 00 26 00 23 02 00 55 11 PAYLEN(2)`
//!    then `payload` then `FF FF 55 AA`. Payload starts at byte 14, length = u16 at [12].
//!  - Channel table = region "r08": records of 72 bytes; record L begins at payload
//!    byte 72*L. Field offsets below are within that 72-byte record.
//!  - General settings = region "r02": the CPS "WW02[x]" maps to payload[x - 15].

use anyhow::{bail, Context, Result};
use std::path::Path;

pub const REGION_ORDER: &[&str] = &[
    "r01", "r02", "r03", "r04", "r05", "r06", "r07", "r08", "rFF", "r32", "r0A", "rKL", "rML",
];

pub const CHANNEL_STRIDE: usize = 72;
pub const CHANNEL_COUNT: usize = 256;

pub struct Region {
    pub name: String,
    pub raw: Vec<u8>,
}

impl Region {
    fn paylen(&self) -> usize {
        u16::from_le_bytes([self.raw[12], self.raw[13]]) as usize
    }
    pub fn payload(&self) -> &[u8] {
        let n = self.paylen();
        &self.raw[14..14 + n]
    }
    pub fn payload_mut(&mut self) -> &mut [u8] {
        let n = self.paylen();
        &mut self.raw[14..14 + n]
    }
}

pub struct Codeplug {
    pub regions: Vec<Region>,
}

impl Codeplug {
    /// Load from a directory of `<region>.bin` files produced by `p64tool read`.
    pub fn from_dump_dir(dir: &Path) -> Result<Codeplug> {
        let mut regions = Vec::new();
        for name in REGION_ORDER {
            let p = dir.join(format!("{name}.bin"));
            let raw = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
            if raw.len() < 18 || &raw[0..2] != b"\x5f\x5f" {
                bail!("{} is not a valid region frame", p.display());
            }
            regions.push(Region {
                name: name.to_string(),
                raw,
            });
        }
        Ok(Codeplug { regions })
    }

    pub fn region(&self, name: &str) -> Result<&Region> {
        self.regions
            .iter()
            .find(|r| r.name == name)
            .with_context(|| format!("region {name} missing"))
    }
    pub fn region_mut(&mut self, name: &str) -> Result<&mut Region> {
        self.regions
            .iter_mut()
            .find(|r| r.name == name)
            .with_context(|| format!("region {name} missing"))
    }
}

/// A list table stored in one region: `count` fixed-size records of `stride`
/// bytes, the first starting at the CPS array index `base_ww`. A CPS field at
/// WW offset `j` within a record is at record-slice index `j` here.
///
/// Payload mapping: `payload_index = WW_index - 15`, so record `i` occupies
/// payload bytes `[base_ww - 15 + i*stride .. + stride]`.
#[derive(Clone, Copy)]
pub struct Table {
    pub region: &'static str,
    pub base_ww: usize,
    pub stride: usize,
    pub count: usize,
}

pub const WW_TO_PAYLOAD: usize = 15;

impl Table {
    fn base_pl(&self) -> usize {
        self.base_ww - WW_TO_PAYLOAD
    }
    /// Immutable slice of record `i` (panics only on internal misuse).
    pub fn record<'a>(&self, cp: &'a Codeplug, i: usize) -> Result<&'a [u8]> {
        let pl = cp.region(self.region)?.payload();
        let off = self.base_pl() + i * self.stride;
        Ok(&pl[off..off + self.stride])
    }
    pub fn record_mut<'a>(&self, cp: &'a mut Codeplug, i: usize) -> Result<&'a mut [u8]> {
        let stride = self.stride;
        let off = self.base_pl() + i * stride;
        let pl = cp.region_mut(self.region)?.payload_mut();
        Ok(&mut pl[off..off + stride])
    }
}

// ---- little helpers -------------------------------------------------------

pub fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
pub fn put_u32le(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
pub fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
pub fn put_u16le(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

/// Decode a UTF-16LE name field of `max_chars` chars starting at `off`.
pub fn get_name(b: &[u8], off: usize, max_chars: usize) -> String {
    let mut s = String::new();
    for k in 0..max_chars {
        let lo = b[off + 2 * k];
        let hi = b[off + 2 * k + 1];
        if (lo == 0 && hi == 0) || lo == 0xFF {
            break;
        }
        if let Some(c) = char::from_u32(u16::from_le_bytes([lo, hi]) as u32) {
            s.push(c);
        }
    }
    s
}

/// Decode a DMR call ID stored as 4-byte little-endian BCD (pair order:
/// least-significant digit-pair first). All-Call = low 3 bytes 0xFF -> 16777215.
pub fn get_dmr_id(b: &[u8], off: usize) -> u32 {
    if b[off] == 0xFF && b[off + 1] == 0xFF && b[off + 2] == 0xFF {
        return 16_777_215;
    }
    let pair = |x: u8| (x >> 4) as u32 * 10 + (x & 0x0F) as u32;
    pair(b[off]) + pair(b[off + 1]) * 100 + pair(b[off + 2]) * 10_000 + pair(b[off + 3]) * 1_000_000
}

/// Encode a DMR call ID as 4-byte little-endian BCD. 16777215 = All-Call.
pub fn set_dmr_id(b: &mut [u8], off: usize, id: u32) {
    if id == 16_777_215 {
        b[off..off + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x00]);
        return;
    }
    let bcd = |p: u32| ((p / 10) << 4) as u8 | (p % 10) as u8;
    b[off] = bcd(id % 100);
    b[off + 1] = bcd((id / 100) % 100);
    b[off + 2] = bcd((id / 10_000) % 100);
    b[off + 3] = bcd((id / 1_000_000) % 100);
}

/// Read a member list: `count` u16-LE values starting at `off`.
pub fn get_members(rec: &[u8], off: usize, count: usize) -> Vec<u16> {
    (0..count).map(|i| u16le(rec, off + i * 2)).collect()
}

/// Write a member list: the values as u16-LE at `off`, then 0xFF fill up to
/// `max` slots. Returns the number written.
pub fn set_members(rec: &mut [u8], off: usize, members: &[u16], max: usize) {
    for i in 0..max {
        if i < members.len() {
            put_u16le(rec, off + i * 2, members[i]);
        } else {
            rec[off + i * 2] = 0xFF;
            rec[off + i * 2 + 1] = 0xFF;
        }
    }
}

/// Write a UTF-16LE name using the radio's convention: the characters, then a
/// single 0x0000 terminator (if it fits), then 0xFFFF padding for the rest.
pub fn set_name(b: &mut [u8], off: usize, max_chars: usize, name: &str) {
    let chars: Vec<u16> = name.encode_utf16().take(max_chars).collect();
    for k in 0..max_chars {
        let v = if k < chars.len() {
            chars[k]
        } else if k == chars.len() {
            0x0000
        } else {
            0xFFFF
        };
        b[off + 2 * k..off + 2 * k + 2].copy_from_slice(&v.to_le_bytes());
    }
}

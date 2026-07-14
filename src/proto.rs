//! MateTalk P64 / Retevis P4 serial protocol.
//!
//! Recovered by decompiling the Windows CPS (a .NET assembly). All frames look
//! like: `5F 5F | LEN(2, little-endian) | 00 | TYPE | ...body... | FF FF 55 AA`
//! where LEN = (frame length in bytes) - 6, TYPE 0x23 = PC->radio.

use crate::serial::Serial;
use anyhow::{bail, Result};
use std::time::Duration;

/// Connect / open a programming session. Reply starts `5F 5F 8F 00 00 26 00 23 02 00 50 11`.
pub const CONNECT: &[u8] = &[
    0x5F, 0x5F, 0x1E, 0x00, 0x00, 0x23, 0x00, 0x26, 0x02, 0x00, 0x40, 0x11, 0x12, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0x55, 0xAA,
];
pub const CONNECT_REPLY_PREFIX: &[u8] = &[
    0x5F, 0x5F, 0x8F, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x50, 0x11,
];
pub const CONNECT_REPLY_LEN: usize = 149;

/// Disconnect / close the session.
pub const DISCONNECT: &[u8] = &[
    0x5F, 0x5F, 0x10, 0x00, 0x00, 0x23, 0x00, 0x26, 0x02, 0x00, 0x41, 0x11, 0x04, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xFF, 0xFF, 0x55, 0xAA,
];
pub const DISCONNECT_REPLY_PREFIX: &[u8] = &[
    0x5F, 0x5F, 0x0D, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x51, 0x11, 0x01, 0x00, 0x00, 0xFF,
    0xFF, 0x55,
];

/// Template for a read command. Bytes [14],[15] carry the 2-byte region selector.
const READ_TEMPLATE: [u8; 20] = [
    0x5F, 0x5F, 0x0E, 0x00, 0x00, 0x23, 0x00, 0x26, 0x02, 0x00, 0x4D, 0x11, 0x02, 0x00, 0x01, 0x00,
    0xFF, 0xFF, 0x55, 0xAA,
];

/// A single memory region to fetch. `expected` is the full framed reply size.
pub struct Region {
    pub name: &'static str,
    pub sel: [u8; 2],
    pub expected: usize,
    pub reply_prefix: &'static [u8],
}

impl Region {
    pub fn command(&self) -> [u8; 20] {
        let mut c = READ_TEMPLATE;
        c[14] = self.sel[0];
        c[15] = self.sel[1];
        c
    }
}

/// The full read sequence, in the order the CPS issues it (connect first,
/// disconnect last, these 13 regions in between).
pub const REGIONS: &[Region] = &[
    Region {
        name: "r01",
        sel: [0x01, 0x00],
        expected: 275,
        reply_prefix: &[
            0x5F, 0x5F, 0x0D, 0x01, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x01, 0x01,
        ],
    },
    Region {
        name: "r02",
        sel: [0x02, 0x00],
        expected: 2187,
        reply_prefix: &[
            0x5F, 0x5F, 0x85, 0x08, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x79, 0x08,
        ],
    },
    Region {
        name: "r03",
        sel: [0x03, 0x00],
        expected: 51,
        reply_prefix: &[
            0x5F, 0x5F, 0x2D, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x21, 0x00,
            0x00,
        ],
    },
    Region {
        name: "r04",
        sel: [0x04, 0x00],
        expected: 10323,
        reply_prefix: &[
            0x5F, 0x5F, 0x4D, 0x28, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x41, 0x28,
        ],
    },
    Region {
        name: "r05",
        sel: [0x05, 0x00],
        expected: 791,
        reply_prefix: &[
            0x5F, 0x5F, 0x11, 0x03, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x05, 0x03,
            0x00,
        ],
    },
    Region {
        name: "r06",
        sel: [0x06, 0x00],
        expected: 2899,
        reply_prefix: &[
            0x5F, 0x5F, 0x4D, 0x0B, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x41, 0x0B,
            0x00,
        ],
    },
    Region {
        name: "r07",
        sel: [0x07, 0x00],
        expected: 1107,
        reply_prefix: &[
            0x5F, 0x5F, 0x4D, 0x04, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x41, 0x04,
            0x00,
        ],
    },
    Region {
        name: "r08",
        sel: [0x08, 0x00],
        expected: 18451,
        reply_prefix: &[
            0x5F, 0x5F, 0x0D, 0x48, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x01, 0x48,
            0x00,
        ],
    },
    Region {
        name: "rFF",
        sel: [0xFF, 0xFF],
        expected: 619,
        reply_prefix: &[
            0x5F, 0x5F, 0x65, 0x02, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x59, 0x02,
        ],
    },
    Region {
        name: "r32",
        sel: [0x32, 0x00],
        expected: 51,
        reply_prefix: &[
            0x5F, 0x5F, 0x2D, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x21, 0x00,
            0x00,
        ],
    },
    Region {
        name: "r0A",
        sel: [0x0A, 0x00],
        expected: 53,
        reply_prefix: &[
            0x5F, 0x5F, 0x2F, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x23, 0x00,
            0x00,
        ],
    },
    Region {
        name: "rKL",
        sel: [0x00, 0x01],
        expected: 43,
        reply_prefix: &[
            0x5F, 0x5F, 0x25, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x19, 0x00,
            0x00,
        ],
    },
    Region {
        name: "rML",
        sel: [0x01, 0x01],
        expected: 16531,
        reply_prefix: &[
            0x5F, 0x5F, 0x8D, 0x40, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x55, 0x11, 0x81, 0x40,
            0x00,
        ],
    },
];

// ---- write path ----------------------------------------------------------

/// Region write order and 16-bit region ID, from `Form初始化.WW数组初始化0` /
/// `Run写频`. Matches the read order. IDs are the values `WW_init` stamps at
/// frame bytes [14..15] (note rFF uses 0x00FF here, unlike the read selector).
pub const WRITE_REGIONS: &[(&str, u16)] = &[
    ("r01", 1),
    ("r02", 2),
    ("r03", 3),
    ("r04", 4),
    ("r05", 5),
    ("r06", 6),
    ("r07", 7),
    ("r08", 8),
    ("rFF", 255),
    ("r32", 50),
    ("r0A", 10),
    ("rKL", 256),
    ("rML", 257),
];

/// Expected 19-byte write acknowledgement (opcode 0x54).
pub const WRITE_ACK_PREFIX: &[u8] = &[
    0x5F, 0x5F, 0x0D, 0x00, 0x00, 0x26, 0x00, 0x23, 0x02, 0x00, 0x54, 0x11,
];

/// Build a write frame for region `id` from a region's read payload
/// (`payload` = raw[14..14+paylen]). The write data is `payload[1..]` — the
/// CPS drops the payload's first byte when converting a read frame to a write
/// frame (`W[i+1]=R[i]`, loop starting at R index 15).
pub fn build_write_frame(id: u16, payload: &[u8]) -> Vec<u8> {
    let data = &payload[1..];
    let data_len = data.len();
    let total = 16 + data_len + 4;
    let l1 = (total - 6) as u16;
    let l2 = (data_len + 2) as u16;
    let mut f = vec![0u8; total];
    f[0] = 0x5F;
    f[1] = 0x5F;
    f[2..4].copy_from_slice(&l1.to_le_bytes());
    f[4] = 0x00;
    f[5] = 0x23;
    f[6] = 0x00;
    f[7] = 0x26;
    f[8] = 0x02;
    f[9] = 0x00;
    f[10] = 0x44;
    f[11] = 0x11;
    f[12..14].copy_from_slice(&l2.to_le_bytes());
    f[14..16].copy_from_slice(&id.to_le_bytes());
    f[16..16 + data_len].copy_from_slice(data);
    f[total - 4..].copy_from_slice(&[0xFF, 0xFF, 0x55, 0xAA]);
    f
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Send one command and read its reply, tolerating a slightly short/long frame.
pub fn transact(port: &Serial, cmd: &[u8], expected: usize, verbose: bool) -> Result<Vec<u8>> {
    port.flush_input()?;
    if verbose {
        eprintln!("  -> {}", hex(cmd));
    }
    port.write_all(cmd)?;
    // Generous per-region deadline: 18 KB at 115200 baud is ~1.6 s of wire time,
    // plus radio think-time. gap = end-of-frame detection.
    let deadline = Duration::from_millis((expected as u64) / 8 + 4000);
    let reply = port.read_response(expected, Duration::from_millis(250), deadline)?;
    if verbose {
        let head = &reply[..reply.len().min(16)];
        eprintln!("  <- {} bytes, head {}", reply.len(), hex(head));
    }
    Ok(reply)
}

/// Result of reading one region.
pub struct RegionData {
    pub name: String,
    pub selector: [u8; 2],
    pub requested: usize,
    pub reply: Vec<u8>,
    pub prefix_ok: bool,
}

/// Open a session and write the given pre-built region frames, in order.
/// `frames` = (label, id, full_write_frame). Verifies the 19-byte ACK after
/// each region. Always attempts to disconnect at the end.
pub fn write_all(port: &Serial, frames: &[(String, Vec<u8>)], verbose: bool) -> Result<()> {
    eprintln!("Connecting...");
    let reply = transact(port, CONNECT, CONNECT_REPLY_LEN, verbose)?;
    if !reply.starts_with(CONNECT_REPLY_PREFIX) {
        bail!(
            "connect handshake failed (got {} bytes). Radio on? Right --port?",
            reply.len()
        );
    }
    eprintln!("Connected. Writing {} regions...", frames.len());

    let result = (|| -> Result<()> {
        for (label, frame) in frames {
            eprint!("  {label} ({} bytes) ... ", frame.len());
            port.flush_input()?;
            if verbose {
                eprintln!();
                eprintln!("    -> head {}", hex(&frame[..frame.len().min(16)]));
            }
            // Stream in chunks, pacing via tcdrain, then await the ACK.
            for chunk in frame.chunks(1024) {
                port.write_all(chunk)?;
            }
            let ack = port.read_response(
                19,
                std::time::Duration::from_millis(300),
                std::time::Duration::from_millis(4000),
            )?;
            if !ack.starts_with(WRITE_ACK_PREFIX) {
                bail!(
                    "no write ACK for {label} (got {} bytes: {})",
                    ack.len(),
                    hex(&ack[..ack.len().min(19)])
                );
            }
            eprintln!("ACK");
        }
        Ok(())
    })();

    eprintln!("Disconnecting...");
    let _ = transact(port, DISCONNECT, DISCONNECT_REPLY_PREFIX.len() + 4, verbose);
    result
}

/// Open a session and read every region. Always disconnects at the end.
pub fn read_all(port: &Serial, verbose: bool) -> Result<Vec<RegionData>> {
    eprintln!("Connecting...");
    let reply = transact(port, CONNECT, CONNECT_REPLY_LEN, verbose)?;
    if !reply.starts_with(CONNECT_REPLY_PREFIX) {
        bail!(
            "connect handshake failed (got {} bytes, head: {}). \
             Is the radio on, cable seated, and the right --port selected?",
            reply.len(),
            hex(&reply[..reply.len().min(16)])
        );
    }
    eprintln!("Connected. Reading {} regions...", REGIONS.len());

    let mut out = Vec::new();
    let result = (|| -> Result<()> {
        for r in REGIONS {
            eprint!(
                "  {} (sel {}, expect {} B) ... ",
                r.name,
                hex(&r.sel),
                r.expected
            );
            let reply = transact(port, &r.command(), r.expected, verbose)?;
            let prefix_ok = reply.starts_with(r.reply_prefix);
            eprintln!(
                "{} bytes{}",
                reply.len(),
                if prefix_ok {
                    ""
                } else {
                    "  [!! unexpected header]"
                }
            );
            out.push(RegionData {
                name: r.name.to_string(),
                selector: r.sel,
                requested: r.expected,
                reply,
                prefix_ok,
            });
        }
        Ok(())
    })();

    eprintln!("Disconnecting...");
    let _ = transact(port, DISCONNECT, DISCONNECT_REPLY_PREFIX.len() + 4, verbose);

    result?;
    Ok(out)
}

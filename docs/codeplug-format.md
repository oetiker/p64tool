# P64 / P4 codeplug format

Reverse-engineered reference for the Retevis MateTalk **P64 / P4** DMR radio
codeplug, as decoded by p64tool. It documents the on-the-wire region framing, the
storage model, and the field↔offset map for every record type.

**Authoritative source.** The offsets here come from the decompiled vendor CPS
(a C# WinForms program), cross-checked against live hardware dumps. Where the CPS
and a dump disagreed, the dump won. The firmware image was analysed too but is
*not* a source of offsets (see [Firmware notes](#firmware-notes)).

> Scope: this describes the P64 V1.1 / firmware 1.0.0.0 codeplug validated by
> p64tool. The vendor treats codeplug layout as fixed per model (P64 and P4 share
> the same byte offsets); it is not known to vary across firmware versions, but
> p64tool's map is empirical, so treat other models/versions as unverified.

## Region frame

The radio stores the codeplug as ~13 **regions**, each read/written as one framed
serial reply:

```
offset  bytes  field
0       2      magic          0x5F 0x5F
2       2      frame length   u16 LE = (total frame length) - 6
4       6      fixed header   00 26 00 23 02 00
10      2      marker         0x55 0x11
12      2      payload length u16 LE  (= N)
14      N      payload        the region body
14+N    4      trailer        0xFF 0xFF 0x55 0xAA
```

p64tool stores each region as its full frame and exposes `payload()` = bytes
`[14 .. 14+N]`. All multi-byte scalars in the payload are **little-endian**.
Names are **UTF-16LE**, NUL-terminated, `0xFF`-padded.

## The regions

Records within a region are fixed-size. "Size" is the payload length `N`.

⚠️ **The `@N` figures below are region-header-relative, not `payload()` offsets.**
They match the vendor CPS's own `Adim.*` copy arrays, which is why they are kept in
that form. To index a `payload()` buffer, subtract **15**:

```python
# table starting at doc offset @N, record i, from payload() bytes
base = N - 15
rec  = payload[base + i*stride : base + (i+1)*stride]
```

For every table below `N == 16`, so the first record begins at payload offset **1**.

An earlier revision said records "begin at region offset 16" and applied that
directly to `payload()`. That conflated the two coordinate systems and decodes to
garbage — see [Storage model and offset
conventions](#storage-model-and-offset-conventions).

⚠️ **The `Size` column below is the full frame length, not the payload length.**
`payload()` is 18 bytes shorter in every region (14-byte frame header + 4-byte
trailer). E.g. `r08` is listed as 18451 but `payload()` is 18433. Capacity maths
must use the payload figure: `(18433 - 1) // 72 == 256` channels.

| Region | Purpose | Size | Record layout |
|--------|---------|------|---------------|
| `r01` | Device identity: model labels, serial no., fw/CPS/date strings | 275 | scalar strings |
| `r02` | General/menu settings **+ encryption-key table** (30×68 @144) | 2187 | scalar + table |
| `r03` | DMR network/service, radio DMR ID, advanced timing | 51 | scalar |
| `r04` | **Contacts** (200×40 @16) **+ RX-group lists** (32×72 @8016) | 10323 | two tables |
| `r05` | **Emergency / digital-alarm systems** (16×48 @16) + work-alone scalar tail | 791 | table + scalar |
| `r06` | **Scan lists** (32×90 @16) | 2899 | table |
| `r07` | **Zones** (16×68 @16) | 1107 | table |
| `r08` | **Channel table** (256×72 @16) | 18451 | table |
| `r0A` | Alerts / man-down settings | 53 | scalar |
| `r32` | Unknown — all zero on every radio observed | 51 | — |
| `rFF` | Mostly-erased flash; **not** per-unit calibration | 619 | — |
| `rKL` | **One-touch / quick-call buttons** (6×4 @16) | 43 | table |
| `rML` | **Quick-text messages** (32×516 @16) | 16531 | table |

The "E-number" mnemonic in the CPS record IDs is a list-type id independent of the
region number: E0=zones(r07), E1=scan(r06), E4=contacts(r04), E5=RX-groups(r04),
E7=encryption(r02), EM=emergency(r05), CH=channels(r08).

## Storage model and offset conventions

Two coordinate systems appear in the RE work; keep them straight:

- **CPS record-relative offset** (0-based from the start of a record). This is the
  canonical format used throughout this document, and matches the vendor's own
  `Adim.*[rec, N]` copy arrays.
- **p64tool record offset** — what `src/config.rs` indexes as `rec[N]`. p64tool
  frames each record slightly differently, so a translation applies:

  | Record family | table base in `payload()` | Translation |
  |---------------|---------------------------|-------------|
  | Channel table (`r08`, doc `@16`) | `1 + l*72` | `payload = @N - 15` |
  | Tables with `base_ww = 16` (contacts, zones, scan, messages, emergency) | `1 + i*stride` | `payload = @N - 15` |
  | RX-groups (doc `@8016`) | `8001 + i*72` | `payload = @N - 15` |
  | Encryption keys (doc `@144`) | `129 + i*68` | `payload = @N - 15` |

  **One rule for every table: `payload_offset = @N - 15`.** Offsets *within* a
  record are unchanged — the channel/contact/zone field maps below are correct as
  written.

  ⚠️ An earlier revision listed the `base_ww = 16` and RX-group/enc-key families as
  "shift 0" against a `@16` base applied directly to `payload()`. That decodes to
  garbage. It also described the fix as a bare `+1` on CPS record offsets, which is
  a coincidence of `16 - 15 == 1` and does not generalise to the `@8016` and `@144`
  tables.

### Verified against live dumps

Two independent hardware reads (factory baseline and a flashed family codeplug),
both agreeing:

| Region | needle | found at payload | `@N - 15` predicts |
|--------|--------|------------------|--------------------|
| `r07` zones | `Family` (rec off 0) | `1` | `16 - 15 + 0 = 1` ✓ |
| `r06` scan | `Family` (rec off 0) | `1` | `16 - 15 + 0 = 1` ✓ |
| `r08` channels | `FAM ALL D` (rec off 0) | `1` | `16 - 15 + 0 = 1` ✓ |
| `rML` messages | `HELLO` (rec off 4) | `5` | `16 - 15 + 4 = 5` ✓ |

Decoding `r07` at base `16` instead yields a garbage name, member count `512`, and
members `[768, 1024, 1280, 65280]`. At base `1`: name `Family`, count `11`, members
`[9, 10, 11, 12, 13, 14, 1, 2, 3, 4, 5]`.

`r08` at base `1` decodes 11 named channels with every field landing where the map
says (type @32, RX @36–39, TX @40–43, record number @70–71), including one duplex
pair at `462.7000` / `467.7000`.

Corroboration: an OEM CPS write capture shows
`readback_payload == b'\x00' + written_payload` byte-for-byte across all 11 written
regions — independent confirmation of the single leading byte. This `-15` is the
same constant derived independently for the CPS `.dat` container in
[`Chicago-Offline/retevis_matetalk_p4`](https://github.com/Chicago-Offline/retevis_matetalk_p4)
→ `CODEPLUG.md` (`-16 + 1`).

⚠️ Contacts (`r04 @16`), RX-groups (`r04 @8016`) and encryption keys (`r02 @144`)
are **empty on both available dumps** (`r04` is 10223/10305 bytes of `0xFF`), so
their bases are derived from the `-15` rule rather than confirmed against populated
records. Confirm with a codeplug that actually programs them.

The CPS also uses a "WW index" for scalar regions: `WWxx[i]` addresses payload byte
`i - 15`. So e.g. general-setting `WW02[92]` is `r02` payload byte 77.

Every record round-trips byte-for-byte regardless of decode: the vendor (and
p64tool) copy the whole record and only bind a subset of bytes to controls. A
**RESERVED** byte below is one that no handler reads — preserved, never decoded.

## Channel record — `r08`, 72 bytes

Offsets are CPS record-relative (p64tool = these `+ 1`). Many bytes mean different
things for **D**igital (`type==0`) vs **A**nalog (`type==1`) channels — a decoder
**must branch on byte 32** before interpreting the type-specific bytes.

| Off | Field |
|-----|-------|
| 0–31 | Name (UTF-16LE, ≤16 chars, NUL-term, FF-pad) |
| 32 | Channel type: 0=digital, 1=analog |
| 33 | `&0x03` power (**0=high, 2=low**); `&0x30>>4` TX-admit criteria; `&0x40` RX-only; `&0x80` set on every observed record, meaning unknown. **A** also: `&0x0C>>2` bandwidth (**0=12.5, 1=20, 2=25 kHz**; 3 unobserved) |
| 34 | **D**: `&0x80` SMS delivery-confirm, `&0x03` SMS format. **A**: RESERVED |
| 35 | RESERVED |
| 36–39 | RX frequency, u32 LE, Hz |
| 40–43 | TX frequency, u32 LE, Hz |
| 44 | TX-timeout timer (TOT). DP4-class models: UI index = value − 3 |
| 45 | TOT pre-alert time |
| 46 | TOT re-key time |
| 47 | **A**: signalling reset time. **D**: RESERVED |
| 48–49 | **D**: TX contact index, u16 LE (into contacts). **A**: RESERVED |
| 50–51 | **D**: RX-group list index, u16 LE. **A**: RESERVED |
| 52 | **D**: `&0x0F` color code, `&0x10` private-call-confirm, `&0x20` time-slot (0=TS1,1=TS2). **A**: emergency-alarm-system index |
| 53 | RESERVED (constant 1) |
| 54 | **D**: `&0x03` timed-preamble preference. **A**: scan-list index |
| 55 | **A**: `&0x01` auto-start-scan, `&0x02` off-network (脱网), `&0x10` whisper (耳语). **D**: RESERVED |
| 56 | **D**: emergency-alarm-system index. **A**: `&0x06>>1` sub-audio tail-elim kind, `&0x10` tail-elim enable, `&0x01` tail-HZ |
| 57 | **D**: `&0x01` emergency alarm-indication, `&0x02` alarm-reply, `&0x04` call-indication. **A**: RESERVED |
| 58–59 | **D**: 58 = scan-list index; 59 flags `&0x01` auto-scan, `&0x02` off-network, `&0x08` solo/lone-worker. **A**: sub-audio DECODE tone (CTCSS/DCS), u16 LE |
| 60–61 | **D**: 60 enc flags `&0x01` enable, `&0x02` random-key, `&0x04` multi-key-decrypt; 61 = enc-key index. **A**: sub-audio ENCODE tone, u16 LE |
| 62 | **D**: `&0x08` direct dual-slot (直通双时隙). **A**: `&0x01` RX-squelch mode |
| 63–65 | RESERVED |
| 66–67 | BOOKKEEPING: per-type ordering/group index, u16 LE |
| 68–69 | RESERVED |
| 70–71 | BOOKKEEPING: record number (1-based), u16 LE |

**Reserved set** (never bound): universal `35, 53, 63, 64, 65, 68, 69`; digital
also `47, 55`; analog also `34, 48, 49, 50, 51, 57`.

### Sub-tone (CTCSS/DCS) encoding

A tone is two bytes (`lo`, `hi`):

- `0xFF 0xFF` → off.
- CTCSS: `value = (hi & 0x0F)*256 + lo` is the frequency ×10 (e.g. 1148 = 114.8 Hz),
  when `hi & 0xC0 == 0x00`.
- DCS: `code = (hi & 0x0F)*256 + lo` (octal on the radio); `hi & 0xC0 == 0x80`
  normal, `0xC0` inverted.

## Other record tables

Offsets are CPS record-relative. Only the bound/bookkeeping bytes are listed;
everything else in the record is RESERVED.

### Contacts — `r04` @16, 40 bytes, ×200
- 0–31 name; 32–35 DMR ID (u32 LE, ID ≤ 0xFFFFFF so byte 35 is a flag/high byte);
  36 call type (0=private, 1=group, 2=all); 38–39 record number (bookkeeping).
- RESERVED: 37.

### RX-group lists — `r04` @8016, 72 bytes, ×32
- 0–31 name; 32–63 member channel numbers (up to 16 × u16 LE); 64 member count;
  65 record number (both bookkeeping).
- RESERVED: 66–71.

### Zones — `r07` @16, 68 bytes, ×16
- 0–31 name; 34–35 member count (u16 LE); 36–67 member channel numbers
  (16 × u16 LE). A zone/record number and a sort byte also live low in the record.

### Scan lists — `r06` @16, 90 bytes, ×32
- 0–31 name; 32 priority-presence flags (`&0x03` gate PCH1, `&0x0C` gate PCH2);
  33 reply-channel mode (0..2; 1 = use designated channel); 34–35 designated /
  revert TX channel (u16 LE, 0xFFFF = none); **36–37 priority channel 1** (u16 LE);
  38–39 priority channel 2 (u16 LE); 40 probe/sample time; 42 signalling hold time;
  44 behaviour flags (`&0x02` talk-back, `&0x20` nuisance-delete, `&0x80` scan LED);
  56–57 member count (u16 LE); 58–89 member channel numbers (16 × u16 LE).
- RESERVED: 41, 43, 45, 46–55.
- Member id **0** is not a channel: it is the CPS "Selected" pseudo-channel
  (scan whatever the knob is on). OEM codeplugs ship it as the sole member.
- **Note:** priority-channel-1 is at offset **36**, not 34 — offset 34 is the
  separate designated/revert-TX channel. (Two earlier analyses conflated them.)

### Emergency / alarm systems — `r05` @16, 48 bytes, ×16
- 0–31 name; 34 alarm mode/type bitfield; 35 impolite-TX count; 36 polite-TX count;
  38 hot-mic duration; 39 RX duration; 45 record number (bookkeeping).
- RESERVED: 32–33, 37, 40–44, 46–47.

### Encryption keys — `r02` @144, 68 bytes, ×30
- 0 record number (bookkeeping); 1 key length (in hex chars); 2 key type
  (1=ARC4, 2=AES128, 3=AES256); 4–35 name (UTF-16LE); 36–67 key material (32 bytes).
- RESERVED: 3.

### Quick-text messages — `rML` @16, 516 bytes, ×32
- 0 record number; 2–3 name/text length (chars); 4… message text (UTF-16LE).

## Scalar regions

Bound byte offsets (payload-relative). Everything else is reserved. Multi-byte IDs
are BCD, little-endian byte order.

- **`r01` device identity** — the CPS "Basic Information" screen, field for field.
  Strings use the usual name convention (chars, `0000` terminator, `FFFF` pad).

  | Payload | Enc | CPS label | Example |
  |---------|-----|-----------|---------|
  | 1 | UTF-16LE ×16 | Model Name, as written at factory programming | `P4 V1.2` |
  | 49 | ASCII 14 | Last Programmed Time (`YYYYMMDDhhmmss`) | `20260813232207` |
  | 81 | UTF-16LE | CPS Software Version, first half | `T020` |
  | 91 | ASCII | CPS Software Version, date half | `2026-04-20` |
  | 113 | UTF-16LE ×16 | band/tuning data file (CPS shows "Frequency Range") | `400 435 469.dat` |
  | 145 | UTF-16LE ×16 | Model Name, as written by the last CPS to program it | `P4 V1.5` |
  | 177 | UTF-16LE ×16 | MCU Version | `1.0.0.0` |
  | 193 | ASCII | MCU Updata Date *(sic)* | `2025-06-23` |
  | 209 | UTF-16LE ×16 | **Serial No.** (CPS-editable) | `50716RP041101784` |
  | 241 | UTF-16LE | unidentified, constant across dumps | `v1.01` |

  ⚠️ **"Model Name" is a CPS/codeplug version, not a hardware revision, and it
  lives in two places.** On one unit: offset 1 held `P4 V1.2` and offset 145 was
  all `FF`; the CPS displayed `P4 V1.2`. After a save from CPS `T020 2026-04-20`,
  offset 145 became `P4 V1.5`, offset 1 was **unchanged**, and the CPS displayed
  `P4 V1.5`. Best reading: offset 1 is stamped at factory programming, offset 145
  by every subsequent CPS save, and the CPS shows 145 when set, else 1. So the
  `V1.x` tracks the programming software, and two radios with identical hardware
  and firmware can carry different labels purely by programming history.

  This matters for `identity::VALIDATED`, which gates writes on the offset-1
  label. Offset 1 surviving a CPS save is what keeps that gate stable — but a
  unit factory-programmed by a different CPS build will report a different label
  and fall out of the validated set despite being the same radio. Treat an
  unknown label as "unverified field map", not "wrong hardware"; the
  authoritative model check is MCU-GET (`DM5`), see PROTOCOL.md.
  *(Offset-145 behaviour observed on one write from one CPS build; a second
  read-back after another CPS session would confirm it.)*

  The serial at 209 is a full 16 chars in the observed unit, so it has **no NUL
  terminator** and runs straight into the field at 241 — read it length-bounded,
  not NUL-bounded. It is codeplug data and travels with a clone, so it is not a
  hardware id.

  Offset 49 is rewritten by the CPS on every **write**; two `p64tool read` runs
  either side of a CPS session left it unchanged, so reads do not touch it.
- **`r02` general settings** — bound `WW02` indices (payload byte = index − 15):
  `17, 18, 19, 24, 76, 77, 84, 85, 86, 87, 92, 93, 94, 95, 100, 101, 104, 106,
  107, 108, 109, 110, 111, 140, 141`. Notables: `100/101` talk-permit-tone &
  all-tones-off (`WW02[100]==0xCD` mutes everything); `104` key long-press time;
  `106–111` programmable-key assignments; `140` master voice-encryption enable
  (`'C'`/67 on, `'B'`/66 off); `141` encryption type. The 30×68 encryption-key
  table lives at `WW02` byte ≥144.
- **`r03` DMR service** — bound `WW03` indices `16–19` (this radio's DMR ID, 4-byte
  BCD LE), `35, 37, 38, 40, 41` (hang times / preamble / decode toggles), `42`
  (remote-kill decode enable, 0xF9/0xF8), `43–47` (remote-kill source ID, 4-byte
  BCD LE).
- **`r0A` alerts / man-down** — bound `WW0A` indices `18` (man-down enable) through
  `42` (the audible-alert toggle block).
- **`r05` scalar tail** — beyond the emergency table, only `784–786` (work-alone
  enable/tone/times).

## Unknown / reserved regions

`r32` (51 B) and `rKL` (43 B) payloads are all-zero and `rFF` (619 B) is ~99 %
`0xFF` erased flash, on every radio measured.

`rKL` is **not** unknown: it is the one-touch / quick-call table, 6 records of 4
bytes at offset 16 (`0` mode, `1` contact, `2` action, `3` message index). It
reads all-zero simply because no one-touch button has been configured. The CPS
`.dat` shows the same six `KL0001`–`KL0006` records.

`rFF` carries only a 4-byte header (`01 00 02 00`) and a single `0x01` at region
offset 400. 🔴 **It is commonly assumed to be factory/calibration storage, and it
cannot be** — it is byte-identical across four different radios in both factory
and CPS-written states, whereas per-unit calibration necessarily varies. Whatever
trim data the radio holds lives outside these region selectors.

`r32` remains genuinely unmapped, but there is nothing in it to map.

p64tool reads and preserves all three verbatim, and **never writes `r32` or
`rFF`** — matching the vendor CPS, and safe because they never differ.

## Firmware notes

The firmware (`dmr_dm5_1_0_0_0_..._Iap.bin`) is an **Artery AT32** Cortex-M image
in an IAP container: a **4096-byte header** then the raw image. Header offset 0 is
the payload length (u32 LE); the image loads at **flash address `0x08000000`**
(so `file_offset = flash_addr − 0x08000000 + 0x1000`). It is unencrypted and
shares kernel code with model `md440`; components include LittleFS, EasyLogger, the
full DMR stack, and `platform/db.c` (the codeplug/database logic).

**The firmware is not a shortcut to codeplug offsets.** It embeds no default
codeplug template, so offsets are not recoverable from strings; extracting them
would require disassembling `db.c` (Cortex-M/thumb). Its value is confirming field
*semantics* (e.g. the analog/digital config log formats, encryption algorithms) and
cross-checking, not the byte map. The decompiled CPS remains authoritative.

## RE sources and method

- **Decompiled vendor CPS** — the field↔offset map. Trust the form code and the
  `File.cs` / `File_Save.cs` load/save paths; the `C_DataLoad.cs` validators are
  stale and disagree with the live paths (e.g. channel type at 0 vs the correct 32).
- **Live hardware dumps** (`p64tool read`) — ground truth for region sizes, the
  offset translation, and per-field value sanity.
- **Verification without differential dumps**: p64tool's `roundtrip` self-test
  proves decode→apply is byte-faithful, and decoded values were cross-checked
  against raw bytes. Differential dumps (change one setting, re-read, diff) can
  resolve the remaining unknown bytes and the empty regions if ever needed.

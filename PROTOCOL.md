# MateTalk P64 / Retevis P4 serial protocol

Recovered by decompiling the Windows CPS (`P64 V1.4.exe`, a VB.NET assembly).
Everything here comes from the classes `SC`, `SC1`, `SC握手` (handshake),
`AF读频` (read), `AF写频` (write), `Uart` in that binary.

Cross-checked against live hardware and against OEM CPS **v1.5** serial captures
(the handshake is unchanged between v1.4 and v1.5). Where the decompiled code and
a capture disagreed, the capture won.

## Physical layer

- USB-to-serial cable (FTDI / CH340 / Prolific) → appears as `/dev/ttyUSB*`
  (`/dev/cu.*` on macOS).
- **115200 baud, 8 data bits, no parity, 1 stop bit** (8N1).
- No flow control, but **DTR and RTS must both be asserted**. With both modem
  lines low the radio stays silent and returns 0 bytes to `CONNECT`. The .NET
  CPS only ever sets `BaudRate`, so it inherits `SerialPort`'s defaults, which
  raise both — the lines are required, not merely incidental. Driver power-on
  defaults vary by kernel and across USB re-enumeration, so p64tool raises both
  explicitly rather than relying on them.

  On macOS the Prolific PL2303G DriverKit extension must be **enabled** under
  System Settings → General → Login Items & Extensions → Driver Extensions. A
  dext left in `[activated waiting for user]` enumerates a normal-looking
  `/dev/cu.*` node, but `TIOCMSET` silently fails, so the lines never assert and
  the radio looks dead. Check with `systemextensionsctl list`.

## Frame format

```
5F 5F | LEN(2, LE) | 00 | SRC | 00 | DST | 02 00 | OP | 11 | PAYLEN(2, LE) | …body… | FF FF 55 AA
  0 1     2  3       4     5     6     7     8  9    10   11    12   13        14…
```

- Magic: `5F 5F` (ASCII `__`).
- `LEN` = total frame length in bytes − 6 (i.e. counts bytes from index 6 to the
  end, including the `FF FF 55 AA` trailer).
- Index 5 and index 7 are an **address pair**: index 5 = sender, index 7 =
  recipient. `0x23` = PC, `0x26` = radio. So a PC→radio frame carries
  `23 … 26` and its reply comes back `26 … 23`. MCU-GET uses a third address,
  `0x32`, in place of the PC's `0x23` (see below).
- Opcode at index 10: connect `0x40`→reply `0x50`; disconnect `0x41`→`0x51`;
  read `0x4D` (`M`)→reply `0x55` (`U`); write `0x44` (`D`)→reply `0x54` (`T`).
- Index 12–13: length of the inner payload (little-endian).

The CPS does **not** verify any per-frame checksum on receive — it only checks
that the reply begins with an expected byte prefix and has the expected length.

## Session

1. **Connect** (open programming session):
   `5F5F 1E00 00 23 00 26 02 00 40 11 12 00` + 20×`00` + `FF FF 55 AA`
   Reply (149 bytes) starts `5F 5F 8F 00 00 26 00 23 02 00 50 11`.
2. Optionally **MCU-GET** to identify the radio (see below).
3. **Read** and/or **write** one or more regions (see below).
4. **Disconnect**:
   `5F5F 1000 00 23 00 26 02 00 41 11 04 00 00 00 00 00 FF FF 55 AA`
   Reply starts `5F 5F 0D 00 00 26 00 23 02 00 51 11 01 00 00 FF FF 55`.

### ⚠️ The first connect after an idle port returns 0 bytes

Reproducible on multiple radios and cables: the first `CONNECT` sent after the
port has been sitting idle gets no reply at all. Re-sending on the **same open
port** succeeds, and the link is then stable for the rest of the session.

It is not a baud or modem-line problem — a 16-way sweep of baud × line states
returned 0 bytes on every combination, then a plain retry at 115200 with DTR+RTS
worked. p64tool therefore retries `CONNECT` up to 5 times, 250 ms apart, before
giving up.

A *full-length* reply with the wrong prefix is a different fault: something
answered but it is not a P64/P4, and retrying will not change that.

## MCU-GET — live model / firmware identity

Opcode `0x00` at index 10 with `0x07` at index 11, addressed `32 … 26`. Carries
fixed MCU memory addresses recovered from the decompiled CPS. Sent inside an
open session.

```
5F 5F 2E 00 00 32 00 26 02 00 00 07 22 00 00 00
89 87 52 79 00 00 00 00 68 19 05 00 88 F6 19 00
90 98 43 00 30 F7 19 00 A0 62 88 76 62 0F 0A 00
FF FF 55 AA 0D 0A
```

⚠️ Two oddities, both required to match the CPS byte-for-byte:

- The command is **54 bytes but `LEN` says 46**, i.e. a 52-byte frame plus a
  trailing `0D 0A` the CPS appends after the trailer.
- Index 5 is `0x32`, not the usual `0x23`.

Reply is **52 bytes**, prefixed
`5F 5F 2E 00 00 26 00 32 02 00 07 00 22 00 00 00`, with three ASCII/BCD fields:

| bytes | field |
|-------|-------|
| 16–27 | MCU name, ASCII — carries the model token (`DM5`) |
| 28–43 | firmware version, ASCII (e.g. `1.0.0.0`) |
| 44–47 | build date, BCD → `YYYY-MM-DD` |

Strings end at the first `0x00` or `0xFF`.

This is the authoritative model check. The human-readable model label
(e.g. `P64 V1.1`, `P4 V1.2`) is *codeplug* data, at `r01` payload offset 1 as
UTF-16LE — see `docs/codeplug-format.md`.

⚠️ The 149-byte connect reply also contains a 15-digit string at bytes 81–110
(UTF-16LE), e.g. `428734460100152`.

**It is not a per-unit serial, and not the CPS-editable "Serial No" either.**
Two radios confirmed distinct — different codeplugs, different DMR IDs (439 vs
3207125), different channel counts, physically swapped between reads — returned
**byte-identical 149-byte connect replies**, this string included. Treat it as a
model/firmware constant.

The CPS-editable "Serial No" is a separate field: codeplug data at `r01` payload
offset 209, 16 UTF-16LE chars. It is worth reading, but it is *not* a hardware
id either — it travels with a cloned codeplug. One radio in the sample set
carries a neighbour's serial for exactly that reason.

**Nothing in this protocol distinguishes one P64/P4 from another.** Track units
by case marking or an external register.

## Region read

Command (20 bytes), 2-byte selector at indices 14–15:

```
5F 5F 0E 00 00 23 00 26 02 00 4D 11 02 00 <SEL_LO> <SEL_HI> FF FF 55 AA
```

The radio replies with a full frame of a fixed size per region. Read order used
by the CPS and the expected reply size (bytes, including framing):

| region | selector | reply size | reply header prefix |
|--------|----------|-----------:|---------------------|
| r01 | `01 00` |   275 | `5F 5F 0D 01 00 26 00 23 02 00 55 11 01 01` |
| r02 | `02 00` |  2187 | `5F 5F 85 08 00 26 00 23 02 00 55 11 79 08` |
| r03 | `03 00` |    51 | `5F 5F 2D 00 00 26 00 23 02 00 55 11 21 00 00` |
| r04 | `04 00` | 10323 | `5F 5F 4D 28 00 26 00 23 02 00 55 11 41 28` |
| r05 | `05 00` |   791 | `5F 5F 11 03 00 26 00 23 02 00 55 11 05 03 00` |
| r06 | `06 00` |  2899 | `5F 5F 4D 0B 00 26 00 23 02 00 55 11 41 0B 00` |
| r07 | `07 00` |  1107 | `5F 5F 4D 04 00 26 00 23 02 00 55 11 41 04 00` |
| r08 | `08 00` | 18451 | `5F 5F 0D 48 00 26 00 23 02 00 55 11 01 48 00` |
| rFF | `FF FF` |   619 | `5F 5F 65 02 00 26 00 23 02 00 55 11 59 02` |
| r32 | `32 00` |    51 | `5F 5F 2D 00 00 26 00 23 02 00 55 11 21 00 00` |
| r0A | `0A 00` |    53 | `5F 5F 2F 00 00 26 00 23 02 00 55 11 23 00 00` |
| rKL | `00 01` |    43 | `5F 5F 25 00 00 26 00 23 02 00 55 11 19 00 00` |
| rML | `01 01` | 16531 | `5F 5F 8D 40 00 26 00 23 02 00 55 11 81 40 00` |

The reply body (after the `... 55 11 PAYLEN PAYLEN` header, before the
`FF FF 55 AA` trailer) is the region's raw memory contents.

⚠️ **The payload carries one leading byte ahead of the region proper.** A region
read back after a write satisfies `readback_payload == b"\x00" + written_payload`
byte-for-byte, on every region. So record tables that the CPS places at region
offset 16 start at **payload offset 1** in a read reply, and the write path has
to drop that byte again (below).

## Region write

Opcode `0x44` (`D`), acknowledged with `0x54` (`T`).

✅ **Proven on hardware.** On a P4 V1.2 / fw 1.0.0.0:

- **Identity write** — all 11 regions written from the radio's own backup, every
  region ACKed, and an independent re-read byte-identical to the backup.
- **Modifying write** — one channel name changed via a TOML edit. Exactly one
  region (`r08`) was sent, and the re-read showed **19 changed bytes, all inside
  channel record 0's name field**. The rest of that record (frequencies, flags,
  bookkeeping), all 255 other channel records and all 12 other regions were
  untouched. The radio was then restored byte-for-byte from the backup.
- The frame layout was validated offline first: all 11 write frames from an OEM
  CPS capture reproduce byte-for-byte from p64tool's builder.

⚠️ Scope: changed *content* has been exercised on `r08` only; the other regions
have so far been written with identical content. One model and one firmware.

⚠️ p64tool's write session goes straight from `CONNECT` to the first write. The
CPS additionally sends MCU-GET and reads `r02` first — most likely its password
check (`RR02[23]`), since skipping it caused no observable difference across
three successful write sessions.

```
5F 5F <L1(2)> 00 23 00 26 02 00 44 11 <L2(2)> <ID_LO> <ID_HI> <data…> FF FF 55 AA
```

- `L1` = total frame length − 6, little-endian.
- `L2` = `len(data) + 2`, little-endian.
- `ID` is a **16-bit region id**, little-endian, at indices 14–15. It is not
  always the same as the read selector — see the table.
- `data` is the region payload **with its leading byte removed**
  (`payload[1..]`), mirroring the extra byte reads prepend.

Every write is answered by a **19-byte** frame beginning
`5F 5F 0D 00 00 26 00 23 02 00 54 11`.

### Region ids and write order

The CPS writes regions in this order. Note `rFF` uses `0x00FF` here, unlike its
`FF FF` read selector, and `rKL`/`rML` exceed one byte.

| region | write id | read selector |
|--------|---------:|---------------|
| r01 | 1 | `01 00` |
| r02 | 2 | `02 00` |
| r03 | 3 | `03 00` |
| r04 | 4 | `04 00` |
| r05 | 5 | `05 00` |
| r06 | 6 | `06 00` |
| r07 | 7 | `07 00` |
| r08 | 8 | `08 00` |
| r0A | 10 | `0A 00` |
| rKL | 256 | `00 01` |
| rML | 257 | `01 01` |

⚠️ **`r32` and `rFF` are read but never written**, by the CPS or by p64tool.
Both are byte-identical across every radio observed — four units, factory and
CPS-written — so writing them can only ever be a no-op or a mistake. (The CPS
assigns them ids 50 and 255 respectively; those ids are unused in practice.)

Measured contents, identical everywhere:

- `r32`: 33 bytes, all zero.
- `rFF`: 601 bytes — a 4-byte header `01 00 02 00`, a single `0x01` at region
  offset 400, and `0xFF` erased flash for the rest.

⚠️ `rFF` is often assumed to be factory/calibration storage. **It cannot be
per-unit calibration** — power and frequency trim vary between units and these
bytes do not vary at all. Whatever calibration the radio holds is not reachable
through these region selectors.

### Safety

p64tool defaults to writing only the regions whose bytes actually changed, and
verifies by reading everything back afterwards. Before writing it also runs a
read-only identity pre-check (MCU-GET + `r01`) and refuses outright on a
non-P64/P4 MCU.

A prerequisite for trusting any of this is that decode→apply is byte-faithful:
run `p64tool roundtrip <dump>` and expect zero diffs.

## Password-protected radios

`RR02[23] == 2` means a programming password is enabled. Reading still works;
the CPS additionally sends a `MiMa_enter` step (found in class `SC_密码`). Not
needed for a plain read.

⚠️ The "password disabled" value is **not a single constant**: a factory radio
holds `0xF8` there and a radio the OEM CPS has written holds `0x8F`. Both mean
disabled. Treat any value other than `2` as "off" and preserve it rather than
normalising it, or the byte will flip on every write.

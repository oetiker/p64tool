# MateTalk P64 / Retevis P4 serial protocol

Recovered by decompiling the Windows CPS (`P64 V1.4.exe`, a VB.NET assembly).
Everything here comes from the classes `SC`, `SC1`, `SC握手` (handshake),
`AF读频` (read), `AF写频` (write), `Uart` in that binary.

## Physical layer

- USB-to-serial cable (FTDI / CH340 / Prolific) → appears as `/dev/ttyUSB*`.
- **115200 baud, 8 data bits, no parity, 1 stop bit** (8N1).
- No flow control. DTR and RTS are left de-asserted (.NET `SerialPort` defaults;
  the CPS only sets `BaudRate = 115200`).

## Frame format

```
5F 5F | LEN(2 bytes, little-endian) | 00 | TYPE | ... body ... | FF FF 55 AA
```

- Magic: `5F 5F` (ASCII `__`).
- `LEN` = total frame length in bytes − 6 (i.e. counts bytes from index 6 to the
  end, including the `FF FF 55 AA` trailer).
- `TYPE` at index 5: `0x23` = PC→radio, `0x26` = radio→PC.
- Opcode at index 10: connect `0x40`→reply `0x50`; disconnect `0x41`→`0x51`;
  read `0x4D` (`M`)→reply `0x55` (`U`); write `0x44` (`D`).
- Index 12–13: length of the inner payload (little-endian).

The CPS does **not** verify any per-frame checksum on receive — it only checks
that the reply begins with an expected byte prefix and has the expected length.

## Session

1. **Connect** (open programming session):
   `5F5F 1E00 00 23 00 26 02 00 40 11 12 00` + 20×`00` + `FF FF 55 AA`
   Reply (149 bytes) starts `5F 5F 8F 00 00 26 00 23 02 00 50 11`.
2. **Read** one or more regions (see below).
3. **Disconnect**:
   `5F5F 1000 00 23 00 26 02 00 41 11 04 00 00 00 00 00 FF FF 55 AA`
   Reply starts `5F 5F 0D 00 00 26 00 23 02 00 51 11 01 00 00 FF FF 55`.

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

## Password-protected radios

`RR02[23] == 2` means a programming password is enabled. Reading still works;
the CPS additionally sends a `MiMa_enter` step (found in class `SC_密码`). Not
needed for a plain read.

## Write (not yet implemented)

Connect → send the `WW*` region blocks (opcode `0x44`, chunked by `SC1.SendW`)
→ disconnect. Held back until read is validated on hardware and the field layout
is mapped, because a wrong write risks bricking the radio.

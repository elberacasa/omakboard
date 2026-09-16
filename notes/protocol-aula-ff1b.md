# AULA Hall-effect protocol (vendor page 0xFF1B)

Recorded 2026-09-16 from the WIN 60 HE (2e3c:c365), firmware V3.13.06, identity string
`W669,34,KB,SI,SI2825KZHEARGB,V3.13.06`.

Sources, and how far to trust each line:

- **captured**: seen on the wire between AULA's web driver (`hed.aulacn.com`) and this
  keyboard, with `research/tools/webhid-tap.js`. The traces themselves are kept out of this repository.
- **driver**: read from the web driver's JavaScript (`agreement.min.js`), not yet seen on the
  wire. Layout is likely right; values and edge cases still need a capture before any write.
- **verified**: written by Omakboard and read back from the keyboard (2026-09-16).

## Transport  (captured)

One HID report ID, `0x01`, 63 payload bytes in both directions, on usage page `0xFF1B`
usage `0x91`. Output reports only; no feature reports. Bytes below are the 63 payload bytes
(the report ID is not shown). Unused bytes are zero.

Every frame shares one header:

| Byte | Meaning |
|---|---|
| 0 | command |
| 1 | sub-command, or a table selector (`0x80`, `0x81`, `0x82`, ...) |
| 2-3 | page number, big-endian, for data that spans several frames |
| 4 | payload length |
| 5.. | payload |

Reads are a request frame with the command (and selector) set and the rest zero; the reply
comes back as input reports with the same command byte, one per page. Many writes get no
reply at all.

## Identifying the model  (driver, confirmed on the WIN 60 HE)

The web driver is a white-label build: the site's hostname picks a brand, and several boards
share `2e3c:c365`. **The USB id alone does not identify the model.** The driver's model table
(`/config/device.json`) keys models by the product field of the `0d` identity string:

| Product | Model |
|---|---|
| `SI2825KZHEARGB`, `SI2825HEARGB` | WIN 60 HE |
| `SI2828KZHEARGB`, `SI2828HEARGB` | WIN 68 HE |
| `SI2851UKKZHEARGB` | KP-TE153 |

Each product's key layout, with on-screen positions, is `/config/keys/<product>.json`.
Omakboard checks the product before reading anything else.

## Commands

### `01`  status / heartbeat  (captured)

Request `01`. The driver sends it about once a second. Reply bytes 5-22 carry capability
flags (driver): byte 5+9 = 1 SOCD ("PRCS") supported; byte 5+12 bit0 per-key calibration, bit1 custom per-key light, bit2 switch
selection, bit3 polling rate, bit4 dead band, bit5 music rhythm; byte 5+13 bit0 high
precision (0.001 mm units instead of 0.01), bit2 gamepad, bit3 "RS", bit4 "RKRT".

WIN 60 HE reply (constant): `01 00 00 00 11 00 00 00 00 00 00 00 00 00 01 00 00 3f 00 01 01`.

### `0d`  identity  (captured)

Request `0d`. Two replies, page 0 and page 1, each with an ASCII string starting at byte 5:

- page 0: `W669,34,KB,SI,SI2825KZHEARGB,V3.13.06` (platform, ?, kind, ?, chip, firmware)
- page 1: `Sep 22 2025,14:11:51` (firmware build time)

### `0a`  lighting mode list  (captured)

Request `0a`. Payload: supported mode ids, a two-byte marker `a5 5a`, then max speed and max
brightness. The driver adds mode 10 (per-key custom) and, with music rhythm, 100.

WIN 60 HE: modes `02 01 03 09 04 00 0e 0f 06 07 0b 0c 10 08`, max speed 4, max brightness 4.
Only 0 (Static) and 1 (Breath) are named from a capture.

### `07`  keyboard lighting  (captured)

Read: request `07 01`. Reply payload is 12 bytes (length `0c`).
Write: `07 00 00 00 0e` + 14 payload bytes. No reply.

| Payload byte | Field | Notes |
|---|---|---|
| 0 | mode | 0 Static, 1 Breath (captured); others from `0a` |
| 1 | brightness | 0-4 on the WIN 60 HE (captured 2 and 4) |
| 2 | speed | 0-4 |
| 3-5 | foreground R, G, B | captured |
| 6-8 | background R, G, B | |
| 9 | direction | pairs of opposites; some modes keep only one pair (below) |
| 10 | full colour (rainbow) | Static keeps it off |
| 11 | power | |

Each mode keeps its own colour: switching Static -> Breath -> Static restored Static's earlier
colour (captured). Only the active mode's settings can be read. The driver sends a write on
every keystroke in the colour fields.

**Verified by Omakboard**: every offered mode, brightness 0-4, speed 0-4, colours, and full colour
on and off, each written and read back. The keyboard does not acknowledge lighting writes, and every
read-back matched on the first read.

**What the firmware stores** (measured with `research/tools/lighting_probe.py`, sending direction 0-8
and full colour 0-1 to every mode): some values come back in the firmware's own form, so a writer
must expect that form or its read-back will not match.

| Mode | Direction kept when sent 0, 1, 2 … 8 | Full colour |
|---|---|---|
| Radar (4), Striation (15) | 6, 7, 6, 7, 6, 7, 6, 7, 6 | as sent |
| Aurora (7) | 4, 5, 4, 5, 4, 5, 4, 5, 4 | as sent |
| Speed Respond (12) | 2, 3, 2, 3, 2, 3, 2, 3, 2 | as sent |
| Static (0) | 0-7, 8 wraps to 0 | always off |
| every other mode, Custom (10) included | 0-7, 8 wraps to 0 | as sent |

AULA's driver reads a stored direction of 6 or more as 0; Omakboard keeps the raw value.

**Custom (10)** is not in the `0a` list, but the driver offers it and the keyboard stores it like any
other mode. **Music rhythm (100)** follows audio streamed from the host; Omakboard does not switch
into it.

`08` is the same layout for side lighting; the WIN 60 HE has none.

### `17`  lighting helper  (captured)

`17 00 00 00 02 xx yy` is sent before a mode change; the keyboard replies `17` and keeps its
lighting. The driver uses it to enter (`01 01`) and leave (`00 00`) music-rhythm mode. Omakboard
sends the leave form before every mode change, as the driver does (verified: acknowledged, no
lighting change).

### `09`  per-key custom colours  (driver)

Read: `09 80` (set 1) or `09 81` (set 2). Replies are pages 0-7, payload = RGB triplets,
key index = 18 x page + i.
Write: `09 <set 0|1> <page> <len>` + up to 54 bytes (18 keys x RGB), 5 ms apart. The buffer is
396 bytes, index x 3.

### `21`  Hall-effect family

Sub-command in **byte 5**, not byte 1.

| Frame | Meaning | Source |
|---|---|---|
| `21 00 00 00 18 04` | read max travel and trigger units | captured, verified |
| `21 00 00 00 18 05 <row> <col>` | read one key's trigger | captured, verified |
| `21 00 00 00 18 <mode> <22 col masks> <8 value bytes>` | set trigger | captured, verified |
| `21 00 00 00 18 07 <row> <col>` | calibrate one key | driver |
| `21 00 00 00 18 06` | reset triggers | driver |
| `21 00 00 00 18 02 <22 col masks>` / `.. 03` | start / stop live travel test | driver |
| `21 00 00 00 01 09` | read polling rate: reply byte 6 = 1, 2, 4, 8 kHz | captured |
| `21 00 00 00 00 09 01 <rate>` | set polling rate: 1, 2, 4, 8 (kHz) | driver |
| `21 00 00 00 00 11` | read active profile; the WIN 60 HE did not reply | captured |
| `21 00 00 00 00 12 <n>` | select profile | driver |

**Key addressing.** Keys sit on a 22-column matrix: index = 22 x row + column.
The set-trigger frame selects keys with one byte per column (22 bytes, payload 1-22), each
bit a row. W (index 46, row 2, column 2) is `00 00 04 00 ...` (captured).

**Set trigger** payload, after the 22 column masks:

| Payload byte | Field |
|---|---|
| 0 | mode |
| 23, 24 | travel, low byte (repeated) |
| 25 | interval 1 (press sensitivity), low byte |
| 26 | interval 2 (release sensitivity), low byte |
| 27, 28 | travel, high byte (repeated) |
| 29 | interval 1, high byte |
| 30 | interval 2, high byte |

Units: 0.01 mm on the WIN 60 HE (`triggerUnit` 0.01, not high precision). The keyboard
acknowledges with `21 00 00 00 01`.

Limits the web driver applies (driver), all in travel steps:

| Value | Range | WIN 60 HE |
|---|---|---|
| travel | min-max from the max-travel reply, capped per switch type | 8-340 (0.08-3.40 mm) |
| press, release sensitivity | 1 to ceil(max / 2) | 1-170 (0.01-1.70 mm), default 50 |
| dead band, top and bottom | 0 to ceil(1 mm / unit) | 0-100 |

Switch types cap travel at 3.4 or 3.9 mm, never below the WIN 60 HE's 3.40 mm, so on this board
the max-travel reply is the binding limit.

Modes (captured on W):

| Mode | Meaning | Captured frame tail |
|---|---|---|
| `00` | fixed actuation point | `78 78 01 01` (1.20 mm) |
| `0c` | rapid trigger, one sensitivity | `78 78 32 32` (1.20 mm, 0.50 mm) |
| `0d` | rapid trigger, separate press / release | `78 78 32 32` |

Turning rapid trigger off keeps the last intervals (`32 32`) instead of the factory `01 01`;
fixed mode ignores them.

**Verified by Omakboard** on W: fixed 1.20 and 1.70 mm; rapid trigger 0.30 mm; separate
0.20 / 0.40 mm; off; and fixed with intervals `01 01`. One frame for W, A, S and D together
(`.. 00 00 0c 08 08 ..`: W and A share column 2) changed exactly those four keys. Every write was
acknowledged and read back as sent.

**Read trigger** reply (captured, verified): byte 6 mode, travel = byte11 << 8 | byte7, interval 1 =
byte13 << 8 | byte9, interval 2 = byte14 << 8 | byte10.

**Max travel** reply (captured `21 00 00 00 06 04 54 01 01 01 08`): max = byte9 << 8 | byte6; unit = byte7 (0 means 10) / 100
(or / 1000 on high-precision boards); byte 10 is the minimum.

### `26`  dead band  (driver)

Read `26 00`: pages 0-4, 29 keys per page, two bytes per key (top, bottom).
Write `26 01 <page> <len>` + 58 bytes, a 264-byte buffer (index x 2).
The driver's "reset dead band" sends the same `26 00` frame as the read, so it only re-reads.

### `18`  keymap  (captured read, driver write)

Read `18 80` (base layer) or `18 82` (Fn layer): pages 0-9, 14 keys per page, 4 bytes per key
(`code1 hid code3 code4`). Write `18 00` (base) or `18 02` (Fn), 56 bytes per page from a
528-byte buffer (index x 4).

code1 selects the kind: 0 plain HID usage, 0x10 macro, 0x12 and 0x13 combinations, 0x17
advanced key, 0x18/0x19 gamepad.

Omakboard leaves remapping to keyd, so it has no reason to write these.

### `19`  macros  (captured read, driver write)

Read `19 80`..`19 89` (ten macros). Write `19 <n> <page>`, 58 bytes per page from a 256-byte
buffer: header (index | used flag, type, _, steps x 4, repeat count u16 BE), then 4-byte steps
(HID usage, state << 4 | type, delay u16 BE).

### `22`  advanced keys  (driver)

Read `22 80`: one reply per slot, pages 0-39. Write `22 <slot> 00 00 32` + payload, 40 slots.
Payload byte 0 (frame byte 5) is the type:

| Type | Name | Payload |
|---|---|---|
| 1 | DKS | 4 route bytes, then 4 x (4-byte key + 4 step bytes) |
| 2 | mod-tap | key, key, duration |
| 3 | toggle | key |
| 4 | "RS" | key + row/col, key + row/col |
| 5 | "RKRT" | key, key |

### `24`  "PRCS" (SOCD)  (driver)

`24 02` read power, `24 03 00 00 01 <on>` set power, `24 01` read pairs,
`24 00 <page> 28` + 10 x (`01 <model> <hid key 1> <hid key 2>`) write pairs, 2 pages.

### `25`  switch type  (driver)

`25 00` read the list of supported switches; `25 02` read per-key switch (58 keys per page);
`25 01 <page>` write, 132-byte buffer.

### `14`  device settings  (driver)

| Frame | Meaning |
|---|---|
| `14 00 00 00 01 01` | **factory reset: never send** |
| `14 00 00 00 01 00 ... flags at 12-15, 18` | performance flags |
| `14 01 ... bytes 8-11` | sleep timers, u16 BE each |
| `14 02` | read sleep timers |
| `14 03 00 00 01 <mode>` | gamepad mode |

## Firmware

The web driver updates firmware by offering a file download, not over this interface.
Nothing in the protocol flashes firmware.

## Safety rules for Omakboard

1. Refuse `14 00` with payload byte 0 non-zero (factory reset), in every build.
2. Refuse the trigger reset (`21 .. 18 06`) unless the user asked for that reset.
3. Refuse the calibration sub-command (`21 .. 18 07`) unless the user started calibration.
4. Before any write, read and store what it replaces; read it back after.
5. Only send frames whose layout is marked **captured** above. A **driver** layout gets a
   capture first.
6. Check the `0d` identity string, not just the USB id, before using this layout.

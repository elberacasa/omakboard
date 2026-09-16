# AULA WIN 60 HE — hardware

Recorded 2026-09-16 on Omarchy 4.0.3 (Linux 7.2), firmware V3.13.06.

## Device

    USB id      2e3c:c365   (shared by other AULA boards; see protocol-aula-ff1b.md)
    Product     "WIN 60 HE"  (no manufacturer string)
    Identity    W669,34,KB,SI,SI2825KZHEARGB,V3.13.06
    Switches    Hall effect (magnetic), 61 keys, ANSI 60%

## USB interfaces

| Interface | Class | Purpose | Report descriptor |
|---|---|---|---|
| 0 | 03/01/01 | Boot keyboard | usage page 0x01, usage 0x06; LED output |
| 1 | 03/00/00 | Mouse, consumer, system, NKRO | report IDs 2-5; NKRO bitmap is ID 5 (160 bits) |
| 2 | 03/00/00 | **Vendor configuration** | **usage page 0xFF1B, usage 0x91** |

The configuration interface, in full:

    06 1b ff    Usage Page (Vendor 0xFF1B)
    09 91       Usage (0x91)
    a1 01       Collection (Application)
    85 01         Report ID (1)
    19 00 2a ff 00  Usage Min 0, Usage Max 255
    15 00 26 ff 00  Logical Min 0, Logical Max 255
    75 08 95 3f     Report Size 8, Report Count 63
    81 02           Input (Data,Var,Abs)
    19 00 29 ff     Usage Min 0, Usage Max 255
    91 02           Output (Data,Var,Abs)
    c0          End Collection

Report ID 1, 63 payload bytes each way (64 on the wire), no feature reports.

0xFF1B is not the QMK/VIA vendor page (0xFF60), so VIA, Vial and QMK tooling cannot see this
board.

## Official software

AULA ships a browser driver over WebHID (`hed.aulacn.com` for the WIN 60 HE; the Pro and Max
use `magnet.aulastar.com`). It works in Chromium on Linux once the user can open the
configuration interface.

## Permissions

hidraw nodes are root-only by default, which blocks Omakboard and Chromium's WebHID alike.
`packaging/udev/70-omakboard.rules` tags interface 2 with `uaccess`, matching on
`ID_USB_INTERFACE_NUM=="02"`. The interfaces used for typing keep their defaults.

## What the firmware owns

Remapping, layers and macros are better done at the OS level on Linux (keyd, kanata). The
firmware owns what they cannot reach, because it happens before USB: actuation depth, rapid
trigger, SOCD, dynamic keystrokes, dead bands, polling rate, switch calibration and lighting.
These live in onboard memory and persist without any software running.

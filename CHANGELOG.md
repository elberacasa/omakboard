# Changelog

## 0.1.0 (2026-09-16)

### Added

- Omarchy overlay, opened from the bar: the keyboard drawn from its real layout, with click,
  shift-click and drag selection; actuation depth, rapid trigger and its sensitivities per key;
  lighting effects by name, brightness, speed and colour with a live preview. Changes save as
  you make them, are read back from the keyboard, and Undo puts back what a save replaced.
- `omakboard serve`: the overlay's long-lived connection, JSON lines over stdin and stdout.
- A device lock, so the overlay and commands never talk to the keyboard at the same time.
- Lighting modes by name: `omakboard lighting --mode auto-ripple`.
- AULA WIN 60 HE (2e3c:c365, SI2825KZHEARGB), verified on firmware 3.13.06.
- `omakboard info`: product, firmware, polling rate, actuation limits, lighting and the
  lighting modes the keyboard advertises.
- `omakboard triggers`: every key's actuation point and rapid trigger, laid out by row.
- `omakboard actuation`, `omakboard rapid-trigger on|off` (one sensitivity, or separate press
  and release) and `omakboard lighting` (mode, brightness, speed, colour).
- `omakboard backup` and `omakboard restore`. Every change saves a backup first, writes only
  what differs, waits for the keyboard's acknowledgement, and reads the result back.
- `--dry-run` on every change: the plan and the exact frames, with nothing sent.
- Models are confirmed from the product name the keyboard reports, since AULA reuses one USB id
  across several boards.
- Frames that factory-reset the keyboard, reset all triggers or start calibration are refused.
- Finds supported keyboards by their configuration interface's HID report descriptor, not by
  hidraw or interface number.
- `omakboard devices`, `omakboard devices --json` and `omakboard doctor`.
- Omarchy bar widget: shows a plugged-in keyboard, offers the installer while the command is
  missing, and opens diagnostics on click.
- udev rule granting the seat user access to the WIN 60 HE's configuration interface only.
- `notes/protocol-aula-ff1b.md`: AULA's Hall-effect protocol, marking what was captured on the
  wire and what was read from the web driver.

#!/usr/bin/env python3
"""Lighting read-back probe for AULA keyboards on vendor page 0xFF1B.

Research tooling, not project code. For each lighting change it sends the same `07` frame
Omakboard sends, then reads the lighting back after several delays and prints which fields
differ from what was written. It restores the lighting it found when it finishes, even on error.

  research/tools/lighting_probe.py [--sweep modes|levels|colours|all]

It takes Omakboard's device lock, so it never talks over the overlay or a command.
"""

import argparse, fcntl, os, select, sys, time, glob

FIELDS = ["mode", "brightness", "speed", "fg_r", "fg_g", "fg_b", "bg_r", "bg_g", "bg_b", "direction", "full_color", "power"]


def find_node():
    for d in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            uevent = open(f"{d}/device/uevent").read()
            desc = open(f"{d}/device/report_descriptor", "rb").read()
        except OSError:
            continue
        if "HID_ID=0003:00002E3C:0000C365" in uevent and desc.startswith(b"\x06\x1b\xff"):
            return "/dev/" + os.path.basename(d)
    sys.exit("no WIN 60 HE configuration interface found")


class Keyboard:
    def __init__(self, path):
        runtime = os.environ.get("XDG_RUNTIME_DIR", "/tmp")
        os.makedirs(f"{runtime}/omakboard", exist_ok=True)
        self.lock = open(f"{runtime}/omakboard/device.lock", "w")
        fcntl.flock(self.lock, fcntl.LOCK_EX)
        self.fd = os.open(path, os.O_RDWR | os.O_NONBLOCK)

    def send(self, payload):
        report = bytes([1]) + bytes(payload) + bytes(63 - len(payload))
        while select.select([self.fd], [], [], 0)[0]:
            os.read(self.fd, 64)
        os.write(self.fd, report)

    def reply(self, command, timeout=0.5):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if select.select([self.fd], [], [], max(0, end - time.monotonic()))[0]:
                r = os.read(self.fd, 64)
                p = r[1:] if len(r) == 64 else r
                if p and p[0] == command:
                    return p
        return None

    def lighting(self):
        self.send([0x07, 0x01])
        p = self.reply(0x07)
        return list(p[5:17]) if p else None

    def write_lighting(self, values):
        self.send([0x07, 0, 0, 0, 0x0E] + list(values))

    def modes(self):
        self.send([0x0A])
        p = self.reply(0x0A)
        n = p[4]
        body = p[5:5 + n]
        return list(body[: n - 4]), body[n - 2], body[n - 1]


def diff(wanted, got):
    if got is None:
        return "no reply"
    changed = [f"{FIELDS[i]} {wanted[i]}->{got[i]}" for i in range(12) if wanted[i] != got[i]]
    return "ok" if not changed else ", ".join(changed)


def trial(kb, label, values):
    kb.write_lighting(values)
    results = []
    start = time.monotonic()
    for delay in (0.03, 0.15, 0.6):
        time.sleep(max(0, start + delay - time.monotonic()))
        results.append(f"{int(delay * 1000)}ms: {diff(values, kb.lighting())}")
    print(f"{label:<28} " + " | ".join(results), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sweep", default="all", choices=["modes", "levels", "colours", "directions", "all"])
    args = parser.parse_args()
    kb = Keyboard(find_node())
    original = kb.lighting()
    print("original:", dict(zip(FIELDS, original)))
    modes, max_speed, max_brightness = kb.modes()
    print("modes:", modes, "max speed", max_speed, "max brightness", max_brightness)
    try:
        if args.sweep in ("modes", "all"):
            for m in modes:
                v = list(original); v[0] = m
                trial(kb, f"mode {m}", v)
        if args.sweep in ("levels", "all"):
            for m in (0, 1):
                for b in range(max_brightness + 1):
                    v = list(original); v[0] = m; v[1] = b
                    trial(kb, f"mode {m} brightness {b}", v)
                for s in range(max_speed + 1):
                    v = list(original); v[0] = m; v[2] = s
                    trial(kb, f"mode {m} speed {s}", v)
        if args.sweep in ("colours", "all"):
            for m in (0, 1, 2):
                for rgb in ((255, 0, 0), (0, 160, 255), (60, 199, 181), (255, 255, 255), (1, 2, 3)):
                    v = list(original); v[0] = m; v[3:6] = rgb
                    trial(kb, f"mode {m} colour {rgb}", v)
        if args.sweep in ("directions", "all"):
            # What each mode stores for each direction, and for the full-colour flag.
            for m in modes:
                stored = []
                for d in range(9):
                    v = list(original); v[0] = m; v[9] = d
                    kb.write_lighting(v)
                    time.sleep(0.05)
                    stored.append(kb.lighting()[9])
                flags = []
                for f in (0, 1):
                    v = list(original); v[0] = m; v[10] = f
                    kb.write_lighting(v)
                    time.sleep(0.05)
                    flags.append(kb.lighting()[10])
                print(f"mode {m:<3} direction 0-8 stored as {stored}   full_color 0,1 stored as {flags}", flush=True)
    finally:
        kb.write_lighting(original)
        time.sleep(0.2)
        print("restored:", diff(original, kb.lighting()))


if __name__ == "__main__":
    main()

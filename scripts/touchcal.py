#!/usr/bin/env python3
"""9-Punkt-Touchkalibrierung fuer den PocketCHIP (libinput Calibration Matrix).

Liest die ROHEN Touch-Werte direkt vom evdev-Geraet (unabhaengig von X und der
aktuellen Matrix), zeigt Fadenkreuze an bekannten Bildschirmpositionen und
berechnet per Least-Squares die Matrix M mit  bildschirm_norm = M * roh_norm.
Ergebnis: Ausgabe und /tmp/touchcal.out. Es wird NICHTS angewendet.
"""
import fcntl
import os
import statistics
import struct
import sys
import threading
import tkinter as tk

DEVICE_NAME = "1c25000.rtp"
FRACS = [0.1, 0.5, 0.9]
EV_KEY, EV_ABS = 0x01, 0x03
ABS_X, ABS_Y, BTN_TOUCH = 0x00, 0x01, 0x14A
EVENT_FMT = "llHHi"
EVENT_SIZE = struct.calcsize(EVENT_FMT)


def find_event():
    name = None
    with open("/proc/bus/input/devices") as fh:
        for line in fh:
            if line.startswith("N: Name="):
                name = line.strip()
            elif line.startswith("H: Handlers=") and name and DEVICE_NAME in name:
                for tok in line.split("=", 1)[1].split():
                    if tok.startswith("event"):
                        return "/dev/input/" + tok
    raise SystemExit("Touch-Geraet nicht gefunden")


def absinfo(fd, code):
    buf = bytearray(24)
    fcntl.ioctl(fd, 0x80184540 + code, buf)  # EVIOCGABS(code)
    _val, mn, mx, _fuzz, _flat, _res = struct.unpack("6i", buf)
    return mn, mx


def solve3(a, b):
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for c in range(3):
        p = max(range(c, 3), key=lambda r: abs(m[r][c]))
        m[c], m[p] = m[p], m[c]
        for r in range(3):
            if r != c:
                f = m[r][c] / m[c][c]
                m[r] = [x - f * y for x, y in zip(m[r], m[c])]
    return [m[i][3] / m[i][i] for i in range(3)]


def fit_row(src, dst):
    ata = [[0.0] * 3 for _ in range(3)]
    atb = [0.0] * 3
    for (sx, sy), d in zip(src, dst):
        v = (sx, sy, 1.0)
        for i in range(3):
            atb[i] += v[i] * d
            for j in range(3):
                ata[i][j] += v[i] * v[j]
    return solve3(ata, atb)


def main():
    path = find_event()
    fd = os.open(path, os.O_RDONLY)
    xmin, xmax = absinfo(fd, ABS_X)
    ymin, ymax = absinfo(fd, ABS_Y)

    root = tk.Tk()
    root.attributes("-fullscreen", True)
    root.configure(cursor="none")
    w, h = root.winfo_screenwidth(), root.winfo_screenheight()
    cv = tk.Canvas(root, width=w, height=h, bg="black", highlightthickness=0)
    cv.pack(fill="both", expand=True)

    targets = [(fx * w, fy * h) for fy in FRACS for fx in FRACS]
    raw_points = []
    state = {"i": 0}

    def draw():
        cv.delete("all")
        i = state["i"]
        tx, ty = targets[i]
        cv.create_line(tx - 15, ty, tx + 15, ty, fill="white", width=2)
        cv.create_line(tx, ty - 15, tx, ty + 15, fill="white", width=2)
        cv.create_oval(tx - 4, ty - 4, tx + 4, ty + 4, outline="red", width=2)
        cv.create_text(w / 2, h / 2 + (40 if ty < h / 2 else -40),
                       text=f"Punkt {i + 1}/9: Mitte antippen, kurz halten, loslassen",
                       fill="gray70", font=("Sans", 10))

    def on_touch(rx, ry):
        raw_points.append((rx, ry))
        state["i"] += 1
        if state["i"] >= len(targets):
            root.quit()
        else:
            draw()

    def reader():
        cur = {"x": None, "y": None}
        samples = []
        down = False
        while True:
            data = os.read(fd, EVENT_SIZE)
            _s, _us, typ, code, val = struct.unpack(EVENT_FMT, data)
            if typ == EV_ABS and code == ABS_X:
                cur["x"] = val
            elif typ == EV_ABS and code == ABS_Y:
                cur["y"] = val
            elif typ == EV_KEY and code == BTN_TOUCH:
                if val == 1:
                    down = True
                    samples = []
                else:
                    down = False
                    if len(samples) >= 3:
                        pts = samples[len(samples) // 3:]
                        rx = statistics.median(p[0] for p in pts)
                        ry = statistics.median(p[1] for p in pts)
                        root.after(0, on_touch, rx, ry)
                continue
            if down and typ == EV_ABS and cur["x"] is not None and cur["y"] is not None:
                samples.append((cur["x"], cur["y"]))

    threading.Thread(target=reader, daemon=True).start()
    root.bind("<Escape>", lambda e: sys.exit("abgebrochen"))
    draw()
    root.mainloop()
    root.destroy()

    if len(raw_points) != len(targets):
        sys.exit("unvollstaendig")

    src = [((rx - xmin) / (xmax - xmin), (ry - ymin) / (ymax - ymin)) for rx, ry in raw_points]
    ax = fit_row(src, [tx / w for tx, _ in targets])
    ay = fit_row(src, [ty / h for _, ty in targets])

    lines = [f"geraet {path} x {xmin}..{xmax} y {ymin}..{ymax} bildschirm {w}x{h}",
             "ziel_px  roh  vorhersage_px  rest_px"]
    for (tx, ty), (rx, ry), (sx, sy) in zip(targets, raw_points, src):
        px = (ax[0] * sx + ax[1] * sy + ax[2]) * w
        py = (ay[0] * sx + ay[1] * sy + ay[2]) * h
        lines.append(f"{tx:4.0f},{ty:4.0f}  {rx:5.0f},{ry:5.0f}  {px:5.0f},{py:5.0f}  {px - tx:+4.0f},{py - ty:+4.0f}")
    matrix = " ".join(f"{v:.5f}" for v in ax + ay + [0.0, 0.0, 1.0])
    lines.append("NEU " + matrix)
    text = "\n".join(lines)
    print(text)
    with open("/tmp/touchcal.out", "w") as fh:
        fh.write(text + "\n")


if __name__ == "__main__":
    main()

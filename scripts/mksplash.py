#!/usr/bin/env python3
"""Rechnet ein Logo (PNG mit Transparenz) in ein Framebuffer-Rohbild fuer
das PocketCHIP-Display um: 480x272, 32 bpp, Byte-Reihenfolge B G R X
(= DRM XRGB8888 little endian, wie /dev/fb0 von sun4i-drm).

Dunkler Hintergrund; sehr dunkle Logo-Teile (z. B. der schwarze Schriftzug
"CHIP") werden hell gezeichnet, damit sie auf Schwarz sichtbar bleiben.

    python3 mksplash.py logo.png splash.fb [--preview vorschau.png]

Benoetigt Pillow (auf dem Mac: pip3 install pillow).
"""
import argparse

from PIL import Image

W, H = 480, 272


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("logo")
    ap.add_argument("out")
    ap.add_argument("--preview")
    ap.add_argument("--scale", type=float, default=0.80,
                    help="Anteil der Displaybreite/-hoehe (Standard 0.80)")
    ap.add_argument("--bg", default="000000", help="Hintergrund, hex RGB")
    ap.add_argument("--dark-to", default="f0f0f0",
                    help="Farbe fuer sehr dunkle Logo-Teile, hex RGB")
    a = ap.parse_args()

    bg = tuple(int(a.bg[i:i + 2], 16) for i in (0, 2, 4))
    light = tuple(int(a.dark_to[i:i + 2], 16) for i in (0, 2, 4))

    logo = Image.open(a.logo).convert("RGBA")
    px = logo.load()
    for y in range(logo.height):
        for x in range(logo.width):
            r, g, b, al = px[x, y]
            if al and max(r, g, b) < 60:
                px[x, y] = light + (al,)

    f = min(W * a.scale / logo.width, H * a.scale / logo.height)
    size = (round(logo.width * f), round(logo.height * f))
    logo = logo.resize(size, Image.LANCZOS)

    canvas = Image.new("RGBA", (W, H), bg + (255,))
    canvas.alpha_composite(logo, ((W - size[0]) // 2, (H - size[1]) // 2))
    rgb = canvas.convert("RGB")
    if a.preview:
        rgb.save(a.preview)

    r, g, b = rgb.split()
    x = Image.new("L", (W, H), 0)
    raw = Image.merge("RGBA", (b, g, r, x)).tobytes()
    assert len(raw) == W * H * 4
    with open(a.out, "wb") as fh:
        fh.write(raw)
    print(f"{a.out}: {len(raw)} Bytes, Logo {size[0]}x{size[1]}")


if __name__ == "__main__":
    main()

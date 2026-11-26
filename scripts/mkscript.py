#!/usr/bin/env python3
"""Erzeugt ein U-Boot-Legacy-Skriptimage (wie 'mkimage -A arm -T script -C none')."""
import struct
import sys
import time
import zlib

src, dst = sys.argv[1], sys.argv[2]
text = open(src, "rb").read()
payload = struct.pack(">II", len(text), 0) + text
name = b"flash hynix usbboot".ljust(32, b"\0")
# magic, hcrc, time, size, load, ep, dcrc, os=Linux(5), arch=ARM(2), type=script(6), comp=none(0)
fields = [0x27051956, 0, int(time.time()), len(payload), 0, 0,
          zlib.crc32(payload) & 0xFFFFFFFF, 5, 2, 6, 0]
hdr = struct.pack(">IIIIIIIBBBB", *fields) + name
fields[1] = zlib.crc32(hdr) & 0xFFFFFFFF
hdr = struct.pack(">IIIIIIIBBBB", *fields) + name
open(dst, "wb").write(hdr + payload)
print(f"{dst}: {len(hdr) + len(payload)} bytes")

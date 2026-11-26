#!/usr/bin/env python3
"""Extrahiert U-Boot proper aus einem sunxi 'u-boot-sunxi-with-spl.bin'.

Das PocketTRIX-Artefakt 'boot-pocketchip' enthaelt u-boot-sunxi-with-spl.bin:
  0x0000  mainline SPL (eGON-Header)
  0x8000  U-Boot proper als Legacy-uImage (64-Byte-Header, Load/Entry 0x4A000000)

Der NTC-SPL 2016.01 laedt U-Boot aus dem NAND als Rohbinaerdatei an 0x4A000000.
Erzeugt werden:
  <out>/u-boot-dtb-mainline.bin       Rohdaten (fuer den FEL-Test)
  <out>/uboot-mainline-400000.bin     mit Nullen auf einen 4-MiB-Eraseblock aufgefuellt
                                      (so wie CHIP-tools uboot-400000.bin erzeugt)

Aufruf: extract-mainline-uboot.py u-boot-sunxi-with-spl.bin <out-verzeichnis>
"""
import os
import struct
import sys
import zlib

UIMAGE_OFFSET = 0x8000
ERASEBLOCK = 0x400000


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    src, out = sys.argv[1], sys.argv[2]
    data = open(src, "rb").read()
    if data[4:12] != b"eGON.BT0":
        sys.exit("kein sunxi-SPL-Header gefunden")
    hdr = data[UIMAGE_OFFSET:UIMAGE_OFFSET + 64]
    (magic, _hcrc, _ts, size, load, entry, dcrc,
     _os, _arch, typ, comp) = struct.unpack(">IIIIIIIBBBB", hdr[:32])
    name = hdr[32:64].rstrip(b"\0").decode(errors="replace")
    if magic != 0x27051956:
        sys.exit(f"kein Legacy-uImage bei 0x{UIMAGE_OFFSET:x} (FIT-Image wird nicht unterstuetzt)")
    payload = data[UIMAGE_OFFSET + 64:UIMAGE_OFFSET + 64 + size]
    if zlib.crc32(payload) & 0xFFFFFFFF != dcrc:
        sys.exit("CRC der U-Boot-Daten stimmt nicht")
    if load != 0x4A000000 or entry != 0x4A000000:
        sys.exit(f"unerwartete Adresse load=0x{load:x} entry=0x{entry:x}")
    if comp != 0 or typ not in (2, 5):
        sys.exit("unerwarteter Image-Typ oder Kompression")
    if size > 0xC0000:
        sys.exit(f"U-Boot zu gross ({size} Bytes); der NTC-SPL laedt hoechstens 768 KiB")

    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "u-boot-dtb-mainline.bin"), "wb") as fh:
        fh.write(payload)
    with open(os.path.join(out, "uboot-mainline-400000.bin"), "wb") as fh:
        fh.write(payload + b"\0" * (ERASEBLOCK - len(payload)))
    print(f"{name}: {size} Bytes, Load/Entry 0x{load:08X}, CRC ok -> {out}/")


if __name__ == "__main__":
    main()

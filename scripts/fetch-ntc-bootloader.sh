#!/bin/sh
# Laedt die benoetigten NTC-Bootloader-Dateien (stable-pocketchip-b126) einzeln
# aus der "C.H.I.P. Flash Collection" auf archive.org und prueft die SHA256-Summen.
# Ergebnis: ./firmware/ntc-b126/
set -eu
cd "$(dirname "$0")/.."

BASE="https://archive.org/download/C.h.i.p.FlashCollection/stable-pocketchip-b126.zip/stable-pocketchip-b126%2F"
DIR=firmware/ntc-b126
mkdir -p "$DIR"

for f in sunxi-spl.bin u-boot-dtb.bin spl-400000-4000-680.bin; do
  if [ ! -f "$DIR/$f" ]; then
    echo "Lade $f ..."
    curl -fSL --retry 3 -o "$DIR/$f.part" "$BASE$f"
    mv "$DIR/$f.part" "$DIR/$f"
  fi
done

(cd "$DIR" && shasum -a 256 -c ../../checksums/ntc-b126.sha256)
echo "NTC-Dateien geprueft: $DIR"

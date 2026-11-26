#!/bin/sh
# Schreibt den USB-Start-Bootloader in einen Hynix-8G-MLC-NAND (CHIP / PocketCHIP):
#   0x000000  NTC-SPL (BROM-Format, raw)
#   0x400000  NTC-SPL (Backup)
#   0x800000  U-Boot 2026.04 (PocketTRIX-Build, roh, auf 4 MiB aufgefuellt)
#
# Voraussetzungen:
#   - CHIP im FEL-Modus (leerer NAND oder FEL-Pin an GND), per Micro-USB verbunden
#   - firmware/ntc-b126/ (scripts/fetch-ntc-bootloader.sh)
#   - firmware/mainline/uboot-mainline-400000.bin (scripts/extract-mainline-uboot.py)
#   - serielle Konsole empfohlen: dort erscheint das Ergebnis
#
# NUR fuer Hynix 8G MLC (Seite 16 KiB, OOB 1664, Eraseblock 4 MiB)!
set -eu
cd "$(dirname "$0")/.."

FEL="${FEL:-./tools/sunxi-fel}"
NTC=firmware/ntc-b126
UBOOT=firmware/mainline/uboot-mainline-400000.bin
SCR=work/flash-bootloader.scr

for f in "$NTC/sunxi-spl.bin" "$NTC/u-boot-dtb.bin" "$NTC/spl-400000-4000-680.bin" "$UBOOT"; do
  [ -f "$f" ] || { echo "fehlt: $f" >&2; exit 1; }
done
(cd "$NTC" && shasum -a 256 -c -s ../../checksums/ntc-b126.sha256) || { echo "NTC-Pruefsummen falsch" >&2; exit 1; }

if [ "${CONFIRM_NAND_WRITE:-}" != YES ]; then
  echo "Loescht die ersten 16 MiB des NAND und schreibt den Bootloader."
  echo "Zum Ausfuehren: CONFIRM_NAND_WRITE=YES $0"
  exit 2
fi

mkdir -p work
python3 -I scripts/mkscript.py uboot-script/flash-bootloader.txt "$SCR"

$FEL version
$FEL spl "$NTC/sunxi-spl.bin"                        # NTC-SPL: DRAM-Init, zurueck in FEL
sleep 1
$FEL -p write 0x4a000000 "$NTC/u-boot-dtb.bin"       # NTC-U-Boot 2016.01 (fuehrt das Skript aus)
$FEL -p write 0x45000000 "$NTC/spl-400000-4000-680.bin"
$FEL -p write 0x48000000 "$UBOOT"
$FEL write 0x43100000 "$SCR"                         # scriptaddr -> wird per (FEL boot) ausgefuehrt
$FEL exe 0x4a000000

echo
echo "NTC-U-Boot laeuft. Auf der seriellen Konsole muss erscheinen:"
echo "  === ERGEBNIS: erase=1 spl=1 spl_backup=1 uboot=1 vergleich=1 ==="

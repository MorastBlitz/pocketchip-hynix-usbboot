#!/bin/sh
# Test OHNE Schreiben: NTC-SPL richtet per FEL den DRAM ein, danach wird
# U-Boot 2026.04 gestartet. Bootet Debian vom USB-Stick, passt die Kombination.
set -eu
cd "$(dirname "$0")/.."
FEL="${FEL:-./tools/sunxi-fel}"

$FEL version
$FEL spl firmware/ntc-b126/sunxi-spl.bin
sleep 1
$FEL -p write 0x4a000000 firmware/mainline/u-boot-dtb-mainline.bin
$FEL exe 0x4a000000
echo "U-Boot 2026.04 gestartet (Ausgabe auf der seriellen Konsole)."

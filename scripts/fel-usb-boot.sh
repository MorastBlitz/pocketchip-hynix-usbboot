#!/bin/sh
# Startet den PocketCHIP per FEL vom USB-Stick (ohne den NAND zu benutzen).
# Benoetigt: u-boot-sunxi-with-spl.bin aus dem PocketTRIX-Artefakt "boot-pocketchip".
set -eu
cd "$(dirname "$0")/.."
FEL="${FEL:-./tools/sunxi-fel}"
IMG="${1:-firmware/mainline/u-boot-sunxi-with-spl.bin}"

$FEL version
$FEL -v -p uboot "$IMG"
echo "U-Boot gestartet; es sucht nun den USB-Stick."

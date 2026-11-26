#!/bin/sh
# USB-Stick unter macOS beschreiben, sichern oder klonen.
#
#   usb-stick.sh write  <image.img.zst> <diskN>   Image auf den Stick schreiben
#   usb-stick.sh backup <diskN> <ziel.img.zst>    die ersten 4 GiB sichern
#
# Das PocketTRIX-Image belegt genau die ersten 4 GiB (Partition "pocketroot").
# Eine Sicherung passt daher auf jeden Stick ab 8 GB.
# Achtung: Nie zwei geklonte Sticks gleichzeitig einstecken (gleiches Label/UUID).
set -eu

die() { echo "$*" >&2; exit 1; }

check_disk() {
  disk="$1"
  case "$disk" in disk[0-9]*) ;; *) die "Bitte z.B. disk4 angeben (siehe: diskutil list external physical)";; esac
  info="$(diskutil info "$disk")" || die "$disk nicht gefunden"
  echo "$info" | grep -q "Device Location:.*External" || die "$disk ist kein externes Laufwerk"
  echo "$info" | grep -q "Removable Media:.*Removable" || die "$disk ist kein Wechseldatentraeger"
  echo "$info" | grep -E "Media Name|Disk Size"
}

confirm() {
  printf '%s [ja/NEIN] ' "$1"
  read -r a
  [ "$a" = ja ] || die "abgebrochen"
}

cmd="${1:-}"
case "$cmd" in
  write)
    img="${2:?Image fehlt}"; disk="${3:?Disk fehlt}"
    [ -f "$img" ] || die "$img nicht gefunden"
    check_disk "$disk"
    confirm "ALLES auf $disk wird geloescht. Fortfahren?"
    diskutil unmountDisk "/dev/$disk"
    zstd -dc "$img" | sudo dd of="/dev/r$disk" bs=4m
    sync
    diskutil eject "/dev/$disk"
    echo "Fertig."
    ;;
  backup)
    disk="${2:?Disk fehlt}"; out="${3:?Zieldatei fehlt}"
    check_disk "$disk"
    diskutil unmountDisk "/dev/$disk"
    sudo dd if="/dev/r$disk" bs=4m count=1024 | zstd -T0 -o "$out"
    shasum -a 256 "$out" > "$out.sha256"
    diskutil eject "/dev/$disk"
    echo "Sicherung: $out (enthaelt WLAN-Passwoerter und SSH-Schluessel - privat halten!)"
    ;;
  *)
    sed -n '2,10p' "$0"; exit 2 ;;
esac

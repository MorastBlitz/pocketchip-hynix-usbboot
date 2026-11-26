# 07 – Fehlersuche

| Symptom | Ursache / Lösung |
|---|---|
| Schwarzer Bildschirm, grüne + orange LED | NAND-Bootloader defekt → CHIP am Mac: erscheint `1f3a:efe8` = FEL ([01](01-diagnose.md)) |
| `brew install sunxi-tools` schlägt fehl | Gibt es nicht → [`build-sunxi-fel-macos.sh`](../scripts/build-sunxi-fel-macos.sh) |
| Keine PocketTRIX-Releases | Image in eigenem Fork per GitHub Actions bauen ([02](02-debian-usb.md)) |
| `dd: /dev/rdiskN: Resource busy` | macOS hat eine Partition eingehängt → `diskutil unmountDisk /dev/diskN` |
| `dd: … Device not configured`, sehr langsam | Stick defekt → anderen Stick nehmen |
| Nach FEL-Start nichts mehr am Mac-USB | Normal – U-Boot hat übernommen; Ausgabe über UART oder Display |
| U-Boot: `0 Storage Device(s) found`, dann `BOOTP broadcast …` | Kein USB-Stick im CHIP gefunden |
| `sunxi SPL version mismatch: expected 3, got 1` | Harmlos (NTC-SPL ist älter als mainline U-Boot) |
| Touch in der Mitte richtig, außen immer weiter daneben | Skalierungsfehler → [05](05-touch-calibration.md) |
| `tca8418_keypad … error: -6` im Log | CHIP steckt nicht im PocketCHIP (Tastatur fehlt) |
| `sudo: fdisk: command not found` | Debian hat fdisk nicht vorinstalliert; Partition über `/sys/block/sda/sda1/{start,size}` oder `lsblk` |
| PocketCHIP per SSH plötzlich nicht erreichbar | Akku leer/aus oder WLAN getrennt |
| Gar nichts startet mehr | FEL-Pin an GND → FEL-Modus ist immer erreichbar → per FEL starten oder Bootloader neu schreiben |

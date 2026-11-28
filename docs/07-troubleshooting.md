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
| PocketCHIP per SSH plötzlich nicht erreichbar | WLAN-Treiber hängt nach AP-Wechsel (`Association request to the driver failed`) → [08](08-optimization.md#wlan-stabilisieren); sonst Akku leer |
| Kurzer Druck auf Power-Taste tut nichts | PocketTRIX setzt `HandlePowerKey=ignore` → [08](08-optimization.md#power-taste-sauber-herunterfahren) |
| `apt autoremove` will openbox löschen | Ghostscript/poppler-data wurden entfernt → wieder installieren, nie pauschal autoremove (nur relevant, solange LXDE installiert ist) |
| Beim Entfernen von LXDE will apt lightdm, xscreensaver oder GNOME installieren | Nebenpakete `lxlock`, `nm-connection-editor`, `notification-daemon` mit entfernen, `--no-install-recommends` → [08](08-optimization.md#x-und-lxde-entfernen) |
| Nach Treiber-Neuladen nur noch `Authentication … timed out` | Chip hängt weiter → SDIO-Controller neu binden (pwrseq-Reset), `pocketchip-wifi-watchdog --reset-now` ([08](08-optimization.md#wlan-stabilisieren)) |
| Display beim Start ~38 s schwarz | Display-Treiber erst am Ende geladen → [09](09-boot-logo.md) (`sun4i_backend` ins initramfs, `fw_devlink=permissive`) |
| Nach Splash-Einbau startet die Oberfläche nicht | `Conflicts=` zwischen Splash und Oberfläche → Drop-in statt Conflicts ([09](09-boot-logo.md#stolpersteine)) |
| Nach Kernel-Update startet das System nicht | Über die serielle Konsole im U-Boot-Menü `previous` oder `most` wählen ([08](08-optimization.md#neue-kernel-automatisch-eintragen)); die alte Datei liegt als `extlinux.conf.bak` daneben |
| Gar nichts startet mehr | FEL-Pin an GND → FEL-Modus ist immer erreichbar → per FEL starten oder Bootloader neu schreiben |

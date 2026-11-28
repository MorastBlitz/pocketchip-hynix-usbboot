# PocketCHIP: Debian 13 vom USB-Stick, eigene Touch-Oberfläche, Emulatoren

> **English summary:** Revives a PocketCHIP whose NAND is corrupt. Debian 13
> ([PocketTRIX](https://github.com/RedElectricity/pocketchip-debian-builder),
> kernel 6.12) runs from a USB stick. For boards with **SK hynix 8 GB MLC NAND**
> (unsupported by PocketTRIX's NAND path), only a tiny bootloader is put into
> NAND: the original **NTC SPL 2016.01** (which handles Hynix MLC) loads
> **mainline U-Boot 2026.04**, which boots the USB stick. Linux never writes to
> the NAND. Everything works from **macOS** (FEL via `sunxi-fel`). Tested on one
> CHIP v1.0 with H27UCG8T2ETR-BC. On top runs a custom touch console in Rust/Slint
> directly on DRM/KMS (no X11), with emulators (ScummVM) started from it.
> Documentation is in German.

Ein PocketCHIP (Next Thing Co., 2016) bootete nicht mehr: schwarzer Bildschirm,
grüne und orange LED an. Ursache war – wie bei sehr vielen CHIPs – ein defekter
MLC-NAND. Dieses Repository enthält alles für den Weg zurück und darüber hinaus:
das System, die Oberfläche, die Emulatoren und künftige Erweiterungen.

1. **Diagnose** per FEL-Modus vom Mac aus
2. **Debian 13 „Trixie“ mit Kernel 6.12** (PocketTRIX) auf einem USB-Stick
3. **Touch-Kalibrierung** mit 9 Messpunkten
4. **Dauerhafter Start ohne Rechner**: nur der Bootloader liegt im NAND –
   auch für Geräte mit **SK hynix 8 GB MLC**, für die PocketTRIX keinen NAND-Weg hat
5. **Eigene Touch-Oberfläche** ([ui/](ui/README.md)) im Retro-Terminal-Stil, direkt
   auf DRM/KMS ohne X11, mit Einstellungen, Akku-Schutz und **Emulatoren** (ScummVM)

```
Boot-ROM ─► NTC-SPL 2016.01 (NAND 0x0) ─► U-Boot 2026.04 (NAND 0x800000) ─► USB-Stick ─► Debian 13
```

## Status

| | |
|---|---|
| Getestet auf | CHIP v1.0 (Allwinner R8), NAND **SK hynix H27UCG8T2ETR-BC** (8 GB MLC), PocketCHIP v1.0 |
| Host | macOS (Apple Silicon), alles ohne Linux-VM |
| Funktioniert | Start ohne Rechner, Display, Touch, Tastatur, WLAN, SSH, Akku/Laden |
| Nicht getestet | Toshiba-4G- oder 512M-SLC-NAND (dafür gibt es den NAND-Weg von PocketTRIX) |

> ⚠️ **Auf eigene Gefahr.** Das Schreiben löscht die ersten 16 MiB des NAND.
> Ein „Brick“ ist praktisch ausgeschlossen: Der FEL-Modus steckt im Boot-ROM des
> Prozessors und ist mit FEL-Pin an GND immer erreichbar.

## Schnellstart

```sh
./scripts/build-sunxi-fel-macos.sh        # sunxi-fel fuer macOS bauen
./scripts/fetch-ntc-bootloader.sh         # NTC-SPL/U-Boot von archive.org (SHA256-geprueft)
# PocketTRIX-Artefakte (image-pocketchip, boot-pocketchip) besorgen -> docs/02
python3 scripts/extract-mainline-uboot.py firmware/mainline/u-boot-sunxi-with-spl.bin firmware/mainline
./scripts/usb-stick.sh write pocketchip-debian-trixie-armhf.img.zst disk4
./scripts/fel-test-ntc-spl.sh             # Test ohne Schreiben
CONFIRM_NAND_WRITE=YES ./scripts/flash-bootloader.sh
```

Ausführlich – bitte vorher lesen:

| Kapitel | Inhalt |
|---|---|
| [01 Diagnose](docs/01-diagnose.md) | LEDs, FEL-Modus, `sunxi-fel` unter macOS |
| [02 Debian vom USB-Stick](docs/02-debian-usb.md) | PocketTRIX per GitHub Actions bauen, Stick schreiben, FEL-Start |
| [03 Bootloader im Hynix-NAND](docs/03-hynix-nand-bootloader.md) | Konzept, NAND-Layout, Flashen, Protokolle |
| [04 Serielle Konsole](docs/04-serial-console.md) | UART-Anschluss, Mitschnitt unter macOS |
| [05 Touch-Kalibrierung](docs/05-touch-calibration.md) | 9-Punkt-Kalibrierung mit Rohwerten |
| [06 Betrieb & Wartung](docs/06-maintenance.md) | SSH, Stick sichern/klonen, Akku, Reparatur |
| [07 Fehlersuche](docs/07-troubleshooting.md) | Probleme, die unterwegs aufgetreten sind |
| [08 Optimierung](docs/08-optimization.md) | Aufräumen, WLAN-Stabilität, Power-Taste, Startzeit |
| [09 Boot-Logo](docs/09-boot-logo.md) | Logo nach ~10 s statt 38 s schwarz, Taste zeigt Startmeldungen |
| [Touch-Oberfläche](ui/README.md) | Funktionen, Bauen unter macOS, Installation, Emulatoren |
| [Rückschale mit Steckern](hardware/back-shell/README.md) | Ersatz-Rückschale mit UART- und I²C-Stecker und Kabeldurchführung (Entwurf) |

## Verzeichnisse

```
scripts/        Hilfsskripte (macOS)
uboot-script/   U-Boot-Skript, das der NTC-U-Boot beim Flashen ausfuehrt
rootfs/         Dateien fuer den PocketCHIP (WLAN-Waechter, Power-Taste, Boot-Logo)
ui/             Touch-Oberflaeche (Rust/Slint) samt Emulator-Start, eigenes Install-Skript
hardware/       Druckteile, z. B. Rueckschale mit UART- und I2C-Stecker
checksums/      SHA256 der NTC-Dateien
docs/           Dokumentation
firmware/       (nicht im Repo) heruntergeladene/extrahierte Binaerdateien
tools/, work/   (nicht im Repo) gebautes sunxi-fel, Arbeitsdateien
```

Binärdateien (NTC-Bootloader, U-Boot, Debian-Image) sind bewusst **nicht**
enthalten; die Skripte laden bzw. erzeugen sie aus den Originalquellen.

## Autoren

- **MorastBlitz** – Hardware, Tests, Idee
- **Claude** (Anthropic, Modell Claude Opus 5.5) – Analyse, Konzept, Skripte, Dokumentation

## Dank und Quellen

- [PocketTRIX / pocketchip-debian-builder](https://github.com/RedElectricity/pocketchip-debian-builder)
  (RedElectricity, Fork von m4xx3d0ut) – Debian-13-Image und mainline U-Boot
- [linux-sunxi / sunxi-tools](https://github.com/linux-sunxi/sunxi-tools) – `sunxi-fel`
- [Project-chip-crumbs / CHIP-tools](https://github.com/Project-chip-crumbs/CHIP-tools) –
  ursprünglicher NTC-Flash-Ablauf (NAND-Layout, Skript-per-FEL-Prinzip)
- [C.H.I.P. Flash Collection auf archive.org](https://archive.org/details/C.h.i.p.FlashCollection) –
  NTC-Images (stable-pocketchip-b126)
- Next Thing Co. – für CHIP und PocketCHIP

## Lizenz

Skripte, Oberfläche und Dokumentation: [MIT](LICENSE). Die Druckteile unter
`hardware/` sind von CAD-Daten von Next Thing Co. abgeleitet und stehen unter CC BY-SA 3.0. Die heruntergeladenen NTC-Bootloader
und U-Boot stehen unter der GPL-2.0 (Quellen: CHIP-u-boot bzw. U-Boot).

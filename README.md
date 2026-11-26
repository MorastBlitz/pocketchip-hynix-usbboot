# PocketCHIP: Debian 13 vom USB-Stick – dauerhaft, auch mit Hynix-8G-MLC-NAND

> **English summary:** Revives a PocketCHIP whose NAND is corrupt. Debian 13
> ([PocketTRIX](https://github.com/RedElectricity/pocketchip-debian-builder),
> kernel 6.12) runs from a USB stick. For boards with **SK hynix 8 GB MLC NAND**
> (unsupported by PocketTRIX's NAND path), only a tiny bootloader is put into
> NAND: the original **NTC SPL 2016.01** (which handles Hynix MLC) loads
> **mainline U-Boot 2026.04**, which boots the USB stick. Linux never writes to
> the NAND. Everything works from **macOS** (FEL via `sunxi-fel`). Tested on one
> CHIP v1.0 with H27UCG8T2ETR-BC. Documentation is in German.

Ein PocketCHIP (Next Thing Co., 2016) bootete nicht mehr: schwarzer Bildschirm,
grüne und orange LED an. Ursache war – wie bei sehr vielen CHIPs – ein defekter
MLC-NAND. Dieses Repository dokumentiert den kompletten Weg zurück:

1. **Diagnose** per FEL-Modus vom Mac aus
2. **Debian 13 „Trixie“ mit Kernel 6.12** (PocketTRIX) auf einem USB-Stick
3. **Touch-Kalibrierung** mit 9 Messpunkten
4. **Dauerhafter Start ohne Rechner**: nur der Bootloader liegt im NAND –
   auch für Geräte mit **SK hynix 8 GB MLC**, für die PocketTRIX keinen NAND-Weg hat

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

## Verzeichnisse

```
scripts/        Hilfsskripte (macOS)
uboot-script/   U-Boot-Skript, das der NTC-U-Boot beim Flashen ausfuehrt
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

Skripte und Dokumentation: [MIT](LICENSE). Die heruntergeladenen NTC-Bootloader
und U-Boot stehen unter der GPL-2.0 (Quellen: CHIP-u-boot bzw. U-Boot).

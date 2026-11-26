# 01 – Diagnose

## Symptom

- PocketCHIP bleibt schwarz, auch nach Laden und langem Druck auf den Power-Knopf.
- **Grüne LED** (Strom) und **orange LED** (Akku lädt) am CHIP leuchten.

Die Stromversorgung (AXP209) arbeitet also – das Betriebssystem startet nicht.

## CHIP allein am Mac testen

CHIP aus dem PocketCHIP nehmen und per Micro-USB-**Datenkabel** an den Mac:

| Was der Mac sieht | Bedeutung |
|---|---|
| `/dev/tty.usbmodem…` | Linux läuft (USB-Seriell-Gadget) → Problem liegt am PocketCHIP/Display |
| USB-Gerät `1f3a:efe8` (Allwinner) | **FEL-Modus** – der Boot-ROM hat keinen gültigen Bootloader im NAND gefunden |
| nichts | Kabel (nur Laden?), Strom, Hardware prüfen |

```sh
ioreg -p IOUSB -l -w0 | grep -E '"idVendor"|"idProduct"'
# idVendor 7994 = 0x1f3a, idProduct 61416 = 0xefe8  -> FEL
```

Landet der CHIP **ohne** FEL-Draht im FEL-Modus, ist der Bootloader im NAND
zerstört. Prozessor, DRAM und USB funktionieren aber – das Gerät ist zu retten.

## sunxi-fel unter macOS

Homebrew hat kein `sunxi-tools`-Paket; [`scripts/build-sunxi-fel-macos.sh`](../scripts/build-sunxi-fel-macos.sh)
baut `sunxi-fel` aus den Quellen (libusb, dtc/libfdt über Homebrew).

```sh
./tools/sunxi-fel version
# AWUSBFEX soc=00001625(A13) 00000001 ver=0001 44 08 scratchpad=00007e00 ...
```

Der R8 meldet sich als **A13** (sun5i) – das ist korrekt.

## NAND-Typ bestimmen

Der NAND sitzt unter dem silbernen Aufkleber auf der Oberseite des CHIP
(TSOP-48-Gehäuse neben dem WLAN-Modul). Aufkleber vorsichtig ablösen:

| Aufdruck | Typ | Weg |
|---|---|---|
| `H27UCG8T2ETR` (SK hynix) | 8 GB MLC, Seite 16 KiB, OOB 1664 | **dieses Repository** |
| Toshiba (`TC58…`) | 4 GB MLC, Seite 16 KiB, OOB 1280 | NAND-Weg von PocketTRIX möglich |
| Toshiba 512 MB SLC (CHIP Pro/GR8) | SLC | anderer Fall |

Im getesteten Gerät: **`H27UCG8T2ETR BC 439A`** → Hynix 8 GB MLC.
Der NTC-U-Boot bestätigt es später: `NAND: 8192 MiB`, `Page size 16384 b`,
`OOB size 1664 b`, `Erase size 4194304 b`.

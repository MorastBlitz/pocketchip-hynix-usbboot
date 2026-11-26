# 03 – Bootloader im Hynix-8G-MLC-NAND (Start ohne Rechner)

## Idee

Nur der **Bootloader** kommt in den NAND, das System bleibt auf dem USB-Stick.

```
Boot-ROM (A13/R8)
  └─► NTC-SPL 2016.01            NAND 0x000000 (Backup 0x400000)   DRAM, AXP209, liest NAND
        └─► U-Boot 2026.04       NAND 0x800000                     aus dem PocketTRIX-Build
              └─► usb start → /boot/extlinux/extlinux.conf (Stick) → Debian 13, Kernel 6.12
```

Warum diese Kombination:

- **Der NTC-SPL kann Hynix-MLC.** Genau dieser SPL (stable-pocketchip-b126) wurde
  von NTC mit CHIP-tools (`Hynix_8G_MLC`) auf ausgelieferte Geräte geflasht.
  PocketTRIX hat für den NAND-Weg einen eigenen, nur auf **Toshiba** angepassten
  SPL – deshalb funktioniert dessen NAND-Weg mit Hynix nicht.
- **Mainline U-Boot braucht den NAND nicht.** Es wird vom NTC-SPL wie früher der
  NTC-U-Boot als Rohbinärdatei nach `0x4A000000` geladen und bootet dann den
  USB-Stick genauso wie beim FEL-Start.
- **Linux schreibt nie in den NAND** (im USB-Image ist der NAND-Controller
  deaktiviert). Die ~13 MiB Bootloader werden einmal geschrieben und nur gelesen –
  der Hauptgrund für tote CHIPs (Schreibverschleiß, Stromausfall beim Schreiben
  auf MLC-Paired-Pages) entfällt.

## NAND-Layout

Wie bei CHIP-tools (`mtdparts=sunxi-nand.0:4m(spl),4m(spl-backup),4m(uboot),4m(env),-(UBI)`):

| Offset | Inhalt | Quelle | Schreibart |
|---|---|---|---|
| `0x000000` | SPL (BROM-Format, 256 Seiten raw inkl. OOB, 4.620.288 B) | `spl-400000-4000-680.bin` (NTC) | `nand write.raw.noverify` |
| `0x400000` | SPL-Backup | dto. | `nand write.raw.noverify` |
| `0x800000` | U-Boot 2026.04, roh, mit Nullen auf 4 MiB aufgefüllt | `uboot-mainline-400000.bin` | `nand write` (ECC) |
| `0xC00000` | (Env, gelöscht – mainline U-Boot nutzt `ENV_IS_NOWHERE`) | – | – |
| ab `0x1000000` | unbenutzt | – | – |

## Benötigte Dateien

```sh
./scripts/fetch-ntc-bootloader.sh
python3 scripts/extract-mainline-uboot.py \
    firmware/mainline/u-boot-sunxi-with-spl.bin firmware/mainline
```

| Datei | Zweck |
|---|---|
| `firmware/ntc-b126/sunxi-spl.bin` | NTC-SPL für FEL (DRAM-Init) |
| `firmware/ntc-b126/u-boot-dtb.bin` | NTC-U-Boot 2016.01 – führt das Flash-Skript aus (`nand write.raw` gibt es nur dort) |
| `firmware/ntc-b126/spl-400000-4000-680.bin` | NTC-SPL im NAND-Format für Hynix 8G |
| `firmware/mainline/u-boot-dtb-mainline.bin` | U-Boot 2026.04 roh (551.016 B), Load/Entry `0x4A000000` |
| `firmware/mainline/uboot-mainline-400000.bin` | dto., auf einen Eraseblock aufgefüllt |

`u-boot-sunxi-with-spl.bin` von PocketTRIX enthält bei Offset `0x8000` ein
Legacy-uImage (`0x27051956`, „U-Boot 2026.04 … for sunxi board“);
`extract-mainline-uboot.py` prüft Header, CRC und Adressen.

## Schritt 1: Test ohne Schreiben

```sh
./scripts/fel-test-ntc-spl.sh
```

Serielle Ausgabe (gekürzt):

```
U-Boot SPL 2016.01-00115-g5f814bb (Dec 09 2016 - 23:00:24)
DRAM: 512 MiB
CPU: 1008000000Hz, AXI/AHB/APB: 3/2/2
U-Boot 2026.04-dirty (Nov 25 2026 - 14:07:35 +0000) Allwinner Technology
Model: NextThing C.H.I.P.
DRAM:  sunxi SPL version mismatch: expected 3, got 1
512 MiB
...
starting USB...
```

`sunxi SPL version mismatch` ist harmlos: Mainline U-Boot liest den
SPL-Header des älteren NTC-SPL und ignoriert die fehlenden Felder.

## Schritt 2: Schreiben

CHIP im FEL-Modus, serielle Konsole mitlaufen lassen ([Kapitel 04](04-serial-console.md)):

```sh
CONFIRM_NAND_WRITE=YES ./scripts/flash-bootloader.sh
```

Ablauf (wie CHIP-tools, aber ohne fastboot/UBI):

1. `sunxi-fel spl sunxi-spl.bin` – NTC-SPL initialisiert DRAM, zurück in FEL
2. Per FEL in den RAM: NTC-U-Boot `0x4A000000`, NAND-SPL `0x45000000`,
   mainline U-Boot `0x48000000`, Skript `0x43100000`
3. `sunxi-fel exe 0x4a000000` – der NTC-U-Boot erkennt den FEL-Start
   (`bootcmd_fel`) und führt [`uboot-script/flash-bootloader.txt`](../uboot-script/flash-bootloader.txt) aus:
   löschen, SPL ×2 raw schreiben, U-Boot mit ECC schreiben, zurücklesen, vergleichen.

Protokoll vom getesteten Gerät:

```
NAND:  8192 MiB
(FEL boot)
## Executing script at 43100000
Device 0: nand0, sector size 4096 KiB
  Page size      16384 b
  OOB size        1664 b
  Erase size   4194304 b
NAND erase: device 0 offset 0x0, size 0x1000000 ... OK
NAND write:  4620288 bytes written: OK
NAND write:  4620288 bytes written: OK
NAND write: device 0 offset 0x800000, size 0x400000
 4194304 bytes written: OK
NAND read: device 0 offset 0x800000, size 0x400000
 4194304 bytes read: OK
Total of 4194304 byte(s) were the same
=== ERGEBNIS: erase=1 spl=1 spl_backup=1 uboot=1 vergleich=1 ===
```

## Schritt 3: Start ohne Rechner

FEL-Draht entfernen, Stick einstecken, Strom kurz trennen:

```
U-Boot SPL 2016.01-00115-g5f814bb (Dec 09 2016 - 23:00:24)
Trying to boot from NAND
U-Boot 2026.04-dirty (Nov 25 2026 - 14:07:35 +0000) Allwinner Technology
Loading Environment from nowhere... OK
starting USB...
       scanning usb for storage devices... 1 Storage Device(s) found
Scanning usb 0:1...
Found /boot/extlinux/extlinux.conf
Debian trixie PocketCHIP
Retrieving file: /boot/vmlinuz-6.12.118+deb13-armmp
Retrieving file: /boot/initrd.img-6.12.118+deb13-armmp
append: console=ttyS0,115200 root=LABEL=pocketroot rootwait rw loglevel=4
Retrieving file: /boot/sun5i-r8-pocketchip.dtb
Starting kernel ...
```

## Was, wenn der NAND trotzdem nachgibt?

- **Kein Brick:** Findet der Boot-ROM keinen gültigen SPL, geht er von selbst in
  FEL. Hängt der Start später, erzwingt der FEL-Pin an GND den FEL-Modus.
- Dann entweder per FEL vom Rechner starten ([Kapitel 02](02-debian-usb.md)) oder
  den Bootloader in zwei Minuten neu schreiben (Schritt 2).
- Vorsorglich kann man den Bootloader alle paar Jahre neu schreiben
  (frischt die Zellladung auf).

## Grenzen

- Getestet auf **einem** Gerät. Der NTC-SPL muss zur NAND-Geometrie passen
  (`spl-400000-4000-680.bin` = Hynix 8G). Für Toshiba 4G gäbe es
  `spl-400000-4000-500.bin` im selben NTC-Archiv – **nicht getestet**.
- Der NTC-SPL lädt U-Boot als Rohdatei mit festem Fenster; U-Boot darf laut
  PocketTRIX-Doku höchstens 768 KiB groß sein (2026.04: 551 KiB).
- Ohne USB-Stick findet U-Boot kein System und fällt auf PXE/DHCP über
  USB-Ethernet zurück.

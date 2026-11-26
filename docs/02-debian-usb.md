# 02 – Debian 13 vom USB-Stick (PocketTRIX)

[PocketTRIX](https://github.com/RedElectricity/pocketchip-debian-builder) baut ein
Debian-13-Image für den PocketCHIP mit Debians eigenem Kernel **6.12 LTS**
(`linux-image-armmp`) und mainline **U-Boot 2026.04**. Display (sun4i-drm,
480×272), Tastatur (TCA8418), Touch, WLAN (RTL8723BS, nur 2,4 GHz) und Bluetooth
funktionieren.

## Warum nicht einfach den alten NAND neu flashen?

Der CHIP nutzt **MLC-NAND**. Der Mainline-Kernel weigert sich, UBI auf MLC zu
benutzen (`ubi: refuse attaching mtd4 - MLC NAND is not supported`). Das alte
NTC-System läuft nur mit dem NTC-Kernel 4.4. PocketTRIX umgeht das mit einem
SLC-Modus – aber nur für **Toshiba 4G**. Für Hynix bleibt der USB-Stick.

## Image bauen (GitHub Actions, ohne Linux)

Es gibt keine fertigen Releases; das Image wird in einem eigenen Fork gebaut:

1. [pocketchip-debian-builder](https://github.com/RedElectricity/pocketchip-debian-builder)
   forken – Haken bei *„Copy the main branch only“* **entfernen** (Build liegt in `dev`).
2. Optional: *Settings → Secrets and variables → Actions → Variables*:
   `POCKETCHIP_TIMEZONE` = `Europe/Berlin` (Standard ist Asia/Shanghai).
3. *Actions* aktivieren → Workflow **„build images“** → *Run workflow*:
   Branch `dev`, variant `pocketchip`, `with_nand` aus.
4. Nach ca. einer Stunde unter *Artifacts* herunterladen (14 Tage verfügbar):
   - `image-pocketchip` → `pocketchip-debian-trixie-armhf.img.zst` (+ `.sha256`)
   - `boot-pocketchip` → `u-boot-mainline/u-boot-sunxi-with-spl.bin`, DTBs

```sh
unzip image-pocketchip.zip && shasum -a 256 -c pocketchip-debian-trixie-armhf.img.zst.sha256
mkdir -p firmware/mainline
unzip -j boot-pocketchip.zip 'u-boot-mainline/u-boot-sunxi-with-spl.bin' -d firmware/mainline
```

Das Image ist entpackt genau **4 GiB** groß (eine ext4-Partition mit Label
`pocketroot`) → Stick mit **mindestens 8 GB**.

## Stick schreiben (macOS)

```sh
diskutil list external physical          # Stick finden, z.B. disk4
./scripts/usb-stick.sh write pocketchip-debian-trixie-armhf.img.zst disk4
```

Das Skript prüft, dass es ein externer Wechseldatenträger ist, fragt nach,
hängt aus und schreibt auf das schnellere `/dev/rdiskN`.

> Erfahrung: Ein alter Stick schrieb mit 0,6 MB/s und meldete sich nach 1,5 GB ab
> (`dd: /dev/rdisk4: Device not configured`) – defekt. Der Ersatzstick schaffte
> 14 MB/s (5 Minuten). `Resource busy` heißt: macOS hat eine Partition
> automatisch eingehängt → vorher `diskutil unmountDisk`.

## Starten per FEL

Stick in den USB-A-Port des CHIP (oben am PocketCHIP), Micro-USB an den Mac.
Mit leerem NAND startet der CHIP automatisch im FEL-Modus, sonst FEL-Pin an GND.

```sh
./scripts/fel-usb-boot.sh firmware/mainline/u-boot-sunxi-with-spl.bin
```

`sunxi-fel` lädt SPL und U-Boot in den RAM; U-Boot (`preboot=usb start`,
`boot_targets=fel usb0 …`) findet `/boot/extlinux/extlinux.conf` auf dem Stick.
Der CHIP verschwindet danach vom Mac-USB – das ist normal.

Erster Start: Benutzer `chip` **ohne Passwort**, `sudo` ohne Passwort, SSH aktiv.
Sofort `passwd` ausführen und WLAN über das Netzwerk-Symbol verbinden.

Ohne [Kapitel 03](03-hynix-nand-bootloader.md) muss dieser FEL-Start bei jedem
Einschalten wiederholt werden.

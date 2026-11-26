# 09 – Boot-Logo statt schwarzem Bildschirm

Ohne Anpassung bleibt das Display beim Start rund 38 s dunkel: U-Boot gibt
(mit `CHIP_defconfig`) nur auf den Composite-Ausgang aus, und Linux bindet das
LCD erst ganz am Ende ein. Danach erscheint das Logo **etwa 10 s nach dem
Einschalten** und bleibt stehen, bis die Oberfläche startet. Eine **Taste
oder Berührung** schaltet auf die laufenden Startmeldungen (`journalctl -f`) um.

## Warum das Display so spät kam

1. `sun4i-backend` fehlte im initramfs → das Display konnte dort nicht starten.
2. Selbst mit dem Modul wartete das Backend auf seinen „Lieferanten“
   `dram-controller@1c01000` (`allwinner,sun5i-a13-mbus`), für den es **keinen
   Treiber gibt**. Der Kernel (`fw_devlink`) gibt erst nach einem Timeout auf, der
   sich mit jedem geladenen Modul verlängert – erst bei ~22 s.
   `/sys/kernel/debug/devices_deferred` zeigte:
   `1e60000.display-backend  platform: supplier 1c01000.dram-controller not ready`

Abhilfe: Kernel-Parameter **`fw_devlink=permissive`** (das Verhalten vor
Linux 5.13). Danach ist `/dev/fb0` nach ~10 s da.

## Bestandteile ([`rootfs/`](../rootfs))

| Datei | Aufgabe |
|---|---|
| `etc/initramfs-tools/hooks/pocketchip-splash` | Display-, Touch- und Tastaturtreiber sowie das Logo ins initramfs |
| `etc/initramfs-tools/scripts/init-top/pocketchip-splash` | zeichnet das Logo, sobald `/dev/fb0` existiert; beobachtet Eingaben schon im initramfs |
| `usr/local/sbin/pocketchip-bootsplash` + `pocketchip-bootsplash.service` | hält das Logo (übermalt jede Sekunde, falls die Textkonsole es löscht), schaltet bei Taste/Touch auf die Startmeldungen |
| `etc/systemd/system/pocketchip-console.service.d/stop-bootsplash.conf` | beendet das Logo, wenn die Oberfläche startet |
| `/usr/share/pocketchip/splash.fb` | das Bild: 480×272, 32 bpp, B‑G‑R‑X (522 240 Byte) |

Das PocketCHIP-Logo selbst liegt **nicht** im Repo (Marke von Next Thing Co.).
Erzeugen mit [`scripts/mksplash.py`](../scripts/mksplash.py) (braucht Pillow):

```sh
curl -LO https://www.home-assistant.io/images/blog/2016-07-pocketchip/pocketchip-logo.png
python3 scripts/mksplash.py pocketchip-logo.png splash.fb --preview vorschau.png
```

Dunkle Logo-Teile (der schwarze Schriftzug „CHIP“) werden dabei hell gezeichnet,
weil der Hintergrund schwarz ist.

## Installation auf dem PocketCHIP

```sh
sudo install -D -m644 splash.fb /usr/share/pocketchip/splash.fb
# Dateien aus rootfs/ an dieselben Pfade kopieren, dann:
sudo update-initramfs -u
sudo systemctl daemon-reload
sudo systemctl enable pocketchip-bootsplash.service
```

In `/boot/extlinux/extlinux.conf` an die `APPEND`-Zeile anhängen:

```
vt.global_cursor_default=0 fw_devlink=permissive
```

Vorher Sicherungen anlegen und einen Ersatzeintrag behalten (über die serielle
Konsole im extlinux-Menü wählbar):

```sh
sudo cp /boot/initrd.img-$(uname -r) /boot/initrd.img-$(uname -r).orig
```

```
LABEL orig
  MENU LABEL Debian (altes initramfs, ohne Logo)
  LINUX /boot/vmlinuz-<version>
  INITRD /boot/initrd.img-<version>.orig
  FDT /boot/sun5i-r8-pocketchip.dtb
  APPEND console=ttyS0,115200 root=LABEL=pocketroot rootwait rw loglevel=4
```

## Stolpersteine

- **Kein `Conflicts=` zwischen Splash und Oberfläche:** Beide stehen in derselben
  Boot-Transaktion; systemd verwirft dann einen der beiden – die Oberfläche
  startete nicht. Deshalb beendet ein Drop-in der Oberfläche den Splash.
- **Eingabegerät offen halten:** Öffnet man `/dev/input/eventN` für jedes
  Ereignis neu (`dd if=… count=1`), kommt von einem Tastendruck nur das erste
  Ereignis (`MSC_SCAN`) an; das `EV_KEY` geht verloren.
- **Python ist zu langsam:** Unter der Bootlast brauchte ein Python-Skript ~10 s
  bis zur ersten Zeile. Die Dienste sind deshalb `/bin/sh` bzw. busybox.
- U-Boot selbst zeigt nichts auf dem LCD (nur Composite) und kann die
  PocketCHIP-Tastatur (I²C) nicht lesen. Die ersten ~10 s bleiben daher dunkel,
  solange man U-Boot nicht neu baut und ins NAND schreibt.

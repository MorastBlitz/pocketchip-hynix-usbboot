# 08 – Optimierung: Aufräumen, WLAN, Power-Taste, Startzeit

Messungen auf dem getesteten Gerät (Debian 13, LXDE, nach dem Login):

| | vorher | nachher |
|---|---|---|
| RAM belegt (`free -m`, used) | 144 MB | 108 MB |
| RAM verfügbar | 311 MB | 346 MB |
| Startzeit (`systemd-analyze`) | 42,5 s | 36,6 s |
| davon Userspace | 29,2 s | 22,2 s |
| installierte Pakete | 731 | 690 |

Größte Speicherverbraucher danach: Xorg (68 MB), lxsession (57 MB),
lxpanel (30 MB), nm-applet (24 MB), pcmanfm (20 MB). Eine eigene Oberfläche
direkt auf KMS/DRM statt X11+LXDE würde den Großteil davon einsparen.

## Sicherheitsupdates

```sh
sudo apt-get update && sudo apt-get upgrade
```

## Entfernte Pakete

```sh
sudo apt-get purge blueman bluez-obexd udisks2 zathura zathura-pdf-poppler \
    linux-image-6.12.117+deb13-armmp        # alter Kernel, 6.12.118 bleibt
```

Dazu die verwaisten Bibliotheken (libblockdev*, libudisks2-0, gvfs-*, libpoppler147,
libgirara …) gezielt per Name – **nicht** pauschal mit `apt autoremove`.

> ⚠️ **Ghostscript (`libgs10`) und `poppler-data` nicht entfernen!** Über
> `libspectre1 → libimlib2 → openbox` hängt der Fenstermanager daran.
> Immer erst mit `apt-get -s purge …` simulieren.

Bluetooth selbst (`bluez`, `bluetooth.service`) bleibt; nur die grafischen
Blueman-Teile sind weg. Steuerung per `bluetoothctl`.

## WLAN stabilisieren

Problem: Bei mehreren Access Points mit gleicher SSID (Mesh/Repeater) versucht
`wpa_supplicant` zu roamen; der Staging-Treiber `r8723bs` bleibt danach hängen:

```
wpa_supplicant: wlan0: CTRL-EVENT-ASSOC-REJECT status_code=1
wpa_supplicant: wlan0: Association request to the driver failed   (endlos)
```

Das Gerät bleibt offline bis zum Neustart. Abhilfe ([Dateien unter `rootfs/`](../rootfs)):

| Datei | Wirkung |
|---|---|
| `/etc/modprobe.d/r8723bs.conf` | `rtw_power_mgnt=0 rtw_ips_mode=0` – Stromsparen im Treiber aus |
| `/etc/NetworkManager/conf.d/wifi-powersave-off.conf` | `wifi.powersave = 2` – Stromsparen in NetworkManager aus |
| `/usr/local/sbin/pocketchip-wifi-watchdog` + `pocketchip-wifi-watchdog.service` | Prüft jede Minute; ist `wlan0` nicht verbunden und meldet `wpa_supplicant` ≥ 3× „Association request to the driver failed“ in 3 Minuten, wird `r8723bs` neu geladen (höchstens alle 5 Minuten) |

```sh
sudo systemctl enable --now pocketchip-wifi-watchdog.service
journalctl -u pocketchip-wifi-watchdog      # Neuladungen werden protokolliert
```

Wer nur einen festen Access Point nutzt, kann zusätzlich die BSSID festlegen:
`nmcli con modify <verbindung> 802-11-wireless.bssid <ap-mac>`.

## Power-Taste: sauber herunterfahren

PocketTRIX setzt `HandlePowerKey=ignore` (`/etc/systemd/logind.conf.d/pocketchip.conf`);
ein kurzer Druck tat nichts, ein langer schaltet hart über den AXP209 ab.
`/etc/systemd/logind.conf.d/zz-pocketchip-powerkey.conf` (wird später gelesen und gewinnt):

```ini
[Login]
HandlePowerKey=poweroff
HandlePowerKeyLongPress=ignore
```

Jetzt fährt ein **kurzer Druck** Debian sauber herunter. Der lange Druck bleibt
als Notaus erhalten.

## Startzeit verkürzen

| Maßnahme | Wirkung |
|---|---|
| `TIMEOUT 5` statt `30` in `/boot/extlinux/extlinux.conf` | Das Menü (nur über UART bedienbar) wartet 0,5 s statt 3 s |
| `MODULES=dep` statt `most` in `/etc/initramfs-tools/initramfs.conf` | initramfs 10,5 statt 25,6 MB (42 statt 836 Module): U-Boot lädt weniger vom Stick, Root ist ~1 s früher eingehängt |
| Oberfläche wartet nicht mehr auf NetworkManager | startet direkt nach logind |
| Display-Treiber früh laden ([09](09-boot-logo.md)) | Logo nach ~10 s |

Ergebnis auf dem getesteten Gerät: Oberfläche bei **33,7 s** statt 38,3 s
Kernelzeit, zusätzlich ~2,5 s weniger in U-Boot. Mit `MODULES=dep` bestimmt
`update-initramfs` die Module am laufenden System; der Splash-Hook ergänzt
Display, Touch, Tastatur und Power-Taste. Vorher das große initramfs als
Ersatzeintrag behalten:

```sh
sudo cp /boot/initrd.img-$(uname -r) /boot/initrd.img-$(uname -r).most
# + LABEL most ... INITRD /boot/initrd.img-<version>.most in extlinux.conf
```

Der verbleibende Großteil ist systemd selbst auf dem 1-GHz-Einkern-R8
(`basic.target` erst ~14 s nach dem Wechsel ins Root-Dateisystem) und die
2 s Countdown in U-Boot (nur mit neu gebautem U-Boot im NAND änderbar).

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
direkt auf KMS/DRM statt X11+LXDE spart den Großteil davon ein – siehe
[X und LXDE entfernen](#x-und-lxde-entfernen).

## X und LXDE entfernen

Seit die eigene Touch-Oberfläche ([ui/](../ui/README.md))
direkt auf DRM/KMS läuft und auch ScummVM ohne X startet (SDL2 `kmsdrm`),
braucht das System keinen X-Server mehr. Ergebnis:

| | mit LXDE | ohne X |
|---|---|---|
| RAM belegt (`free -m`, used) | 108 MB | 60 MB |
| installierte Pakete | 719 | 532 |
| belegt auf dem Stick | 1,7 GB | 1,6 GB |

Größter Prozess ist danach NetworkManager mit 14 MB.

Paketliste sichern, dann mit `--no-install-recommends` und erst als
Simulation (`-s`) entfernen:

```sh
dpkg --get-selections > ~/pakete-vorher.txt
sudo systemctl set-default multi-user.target
sudo systemctl disable pocketchip-one.service     # PocketTRIX-Akku-Daemon, braucht X
sudo apt-get -s --no-install-recommends purge \
    lxappearance lxde-common lxinput lxmenu-data lxpanel lxpanel-data lxpolkit \
    lxrandr lxsession lxsession-data lxsession-edit lxsession-logout lxtask lxlock \
    openbox pcmanfm libfm4t64 libfm-gtk3-4t64 libfm-modules libfm-data libfm-gtk-data \
    gpicview mousepad libmousepad0 xarchiver galculator xpad \
    i3-wm i3lock i3status xss-lock rxvt-unicode suckless-tools lisgd unclutter scrot \
    wmctrl xdotool xinput xwallpaper x11-utils x11-xserver-utils x11-xkb-utils \
    xinit xserver-xorg-core xserver-xorg-input-libinput xserver-common \
    network-manager-applet nm-connection-editor dbus-x11 python3-tk notification-daemon \
    scummvm+ libinput-tools+
# passt die Liste: ohne -s wiederholen, danach
sudo apt-get autoremove --purge     # erst mit -s prüfen
```

> ⚠️ **Die Nebenpakete müssen mit in die Liste.** Fehlen `lxlock`,
> `nm-connection-editor` oder `notification-daemon`, erfüllt apt deren
> Abhängigkeiten neu: Statt 50 Pakete zu entfernen, installiert es über 160,
> darunter lightdm, xscreensaver, nautilus und gnome-flashback.
> `scummvm+` verhindert, dass ScummVM mitgeht, denn es hängt an `libgtk-3-0t64`.
> `dbus-x11` wird durch `dbus-user-session` ersetzt.

Danach von Hand entfernen, was nicht aus Paketen stammt: die PocketTRIX-X-Skripte
in `/usr/local/bin` (`pocketchip-startx`, `startlxde`, `pocketchip-terminal`,
`pocketchip-menu`, `pocketchip-browser`, `pocketchip-gestures`,
`pocketchip-touch-calibrate` …), `/usr/sbin/pocketchip-one`,
`/etc/xdg/lxsession`, `~/.xinitrc`, `~/.Xresources` und den Start von X in
`~/.zprofile`. PICO-8 und SunVox laufen nur unter X und sind ebenfalls entfernt.

Die Konsolenwerkzeuge `pocketchip-connect`, `pocketchip-control` und
`pocketchip-status` bleiben erhalten.

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

> ⚠️ **Solange LXDE installiert ist: Ghostscript (`libgs10`) und
> `poppler-data` nicht entfernen!** Über `libspectre1 → libimlib2 → openbox`
> hängt der Fenstermanager daran. Ohne X entfällt das.
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
| `/etc/NetworkManager/conf.d/wifi-no-scan-rand-mac.conf` | Keine Zufalls-MAC: `wifi.scan-rand-mac-address=no` (Scannen) und `wifi.cloned-mac-address=permanent` (Verbindungen) – immer die Hardware-Adresse; direkt vor dem Hänger stand jedes Mal `set-hw-addr: … not successfully set (scanning)` |
| `/usr/local/sbin/pocketchip-wifi-watchdog` + `pocketchip-wifi-watchdog.service` | Prüft jede Minute; ist `wlan0` nicht verbunden und meldet `wpa_supplicant` ≥ 3× „Association request to the driver failed“ oder „Authentication … timed out“ in 3 Minuten, wird der **Chip zurückgesetzt** (höchstens alle 5 Minuten) |

**Reset statt Modul-Neuladen:** Nur `modprobe -r/modprobe r8723bs` reichte nicht –
danach lief jede Anmeldung in „Authentication … timed out“. Der Wächter trennt
deshalb zusätzlich den SDIO-Controller (`1c0f000.mmc`) vom Treiber `sunxi-mmc`
und bindet ihn neu; dabei schaltet `mmc-pwrseq-simple` den Chip aus und ein.
Manuell (unterbricht SSH über WLAN kurz, Wiederverbindung nach ~1 min):

```sh
sudo pocketchip-wifi-watchdog --reset-now
```

```sh
sudo systemctl enable --now pocketchip-wifi-watchdog.service
journalctl -u pocketchip-wifi-watchdog      # Resets werden protokolliert
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

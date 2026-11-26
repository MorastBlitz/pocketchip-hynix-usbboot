# pocketchip-console

Touch- und Tastatur-Oberfläche für den PocketCHIP (CHIP v1.0, 480×272, resistiver Touch)
im Stil eines Retro-Terminals, grün auf schwarz. Rust + [Slint](https://slint.dev),
Software-Renderer direkt auf DRM/KMS (kein X11 nötig), libinput für Touch und Tastatur.

## Funktionen

Startseite: Kachel ⚙ EINSTELLUNGEN (weitere Kacheln folgen). Darin:

- **01 SYSTEMSTATUS**: Host, Modell, Kernel, CPU, Takt, Auslastung, Load, RAM, Swap, Speicher, Uptime, IP, Temperaturen
- **02 AKKU**: AXP209 Ladung, Status, Spannung, Strom, Leistung, Zustand, USB/AC-Eingang
- **03 ENERGIE**: Ausschalten, Neustart, Standby, jeweils mit Sicherheitsabfrage. Standby ist ein leichter Standby (Display aus, CPU gedrosselt), weil s2idle auf dem R8 nicht wieder aufwacht
- **04 ANZEIGE**: Helligkeit, Display-Timeout, Standby-Timeout (optional nur im Akkubetrieb), Scanlines, LED im Betrieb an/aus, LED-Blinken im Standby, Display aus, **Touch-Kalibrierung**
- **05 AUDIO**: Lautstärke und Stumm für die 3,5-mm-Klinke (kein eingebauter Lautsprecher), Testton; wird beim Start gesetzt
- **06 WLAN** (nmcli): Funk an/aus, Netze suchen, verbinden (Passwort mit Anzeigen-Knopf), trennen, Passwort ändern, vergessen
- **07 BLUETOOTH** (bluetoothctl): an/aus, suchen, Detailseite je Gerät, koppeln, verbinden, trennen, entfernen. UART mit 115200 Baud (Override), damit BT auch nach Warmstarts geht; zu langsam für Audio
- **08 DATUM / ZEIT** (timedatectl): NTP, Zeitzone, manuelles Stellen
- **09 DESKTOP**: wechselt zu LXDE; zurück über Panel-Knopf, Desktop-Symbol oder Menü „PocketCHIP Konsole“
- **Power-Taste** öffnet einen Ausschalt-Dialog (logind-Inhibitor, solange die Konsole läuft)

Bedienung: Touch überall; Tastatur ↑↓ Auswahl, ◀▶ Wert ändern, Enter öffnen/umschalten, Esc zurück.

## Bauen (macOS, Apple Silicon)

```bash
brew install zig cargo-zigbuild
rustup target add armv7-unknown-linux-gnueabihf
scripts/make-sysroot.sh        # holt libinput/libudev/libxkbcommon/libfontconfig vom Gerät
cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.41
scripts/install.sh             # kopiert per SSH, richtet systemd-Dienst ein
```

Tipp: Auf einem Netzlaufwerk `CARGO_TARGET_DIR=$HOME/.cache/pocketchip-console-target` setzen (die Skripte nutzen diesen Pfad als Vorgabe).

Desktop-Vorschau: `cargo run --no-default-features --features desktop`.

## Aufbau auf dem Gerät

| Datei | Zweck |
|---|---|
| `/usr/local/bin/pocketchip-console` | Programm |
| `/etc/systemd/system/pocketchip-console.service` | Dienst auf tty1, `Conflicts=getty@tty1` |
| `/usr/local/bin/pocketchip-console-start` | Rückkehr aus LXDE |
| `/etc/udev/rules.d/90-pocketchip-touch-libinput.rules` | Kalibriermatrix für libinput ohne X |
| `/var/lib/pocketchip-console/settings.conf` | Einstellungen |

Die Touch-Kalibrierung schreibt die Matrix in udev-Regel, `/etc/default/pocketchip-touch`
und `/etc/X11/xorg.conf.d/40-pocketchip-touch.conf`.

## Entwicklung

`scripts/dev-restart.sh` startet einen Testbuild von Hand, `scripts/devcmd.sh OUT.png "key down" "tap 200 113"`
schickt Tasten/Taps an die laufende Konsole (Steuerdatei `/run/pocketchip-console/cmd`, nur root)
und holt ein Bildschirmfoto.

Lizenz: MIT. Schrift: DejaVu Sans Mono (freie Lizenz, siehe assets/fonts/LICENSE).

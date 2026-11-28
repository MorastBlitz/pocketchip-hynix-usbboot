# pocketchip-console

Touch- und Tastatur-Oberfläche für den PocketCHIP (CHIP v1.0, 480×272, resistiver Touch)
im Stil eines Retro-Terminals, grün auf schwarz. Rust + [Slint](https://slint.dev),
Software-Renderer direkt auf DRM/KMS (kein X11 nötig), libinput für Touch und Tastatur.

## Funktionen

Startseite: Kacheln ⚙ EINSTELLUNGEN und 🎮 EMULATOREN.

**EMULATOREN**: startet ScummVM im Vollbild direkt auf DRM/KMS (SDL2 `kmsdrm`, GPU über lima, ohne X11). Die Konsole beendet sich dafür und kommt zurück, sobald ScummVM endet (STRG+Q oder „Beenden“ im Startfenster). Spiele nach `~/games/scummvm` kopieren und in ScummVM mit „Spiel hinzufügen“ eintragen. Touch: SDL 2.32 verliert Single-Touch-Displays (Finger-ID −1, keine Mausereignisse ohne Fenster). Deshalb übernimmt `pocketchip-emu-helper.service` den Touchscreen, wendet die Kalibriermatrix an und meldet ihn als absolute Maus (uinput): Tippen = Linksklick, Ziehen = Bewegen mit gedrückter Taste. ScummVM läuft im OpenGL-Modus, damit die Menüs die vollen 480×272 nutzen. Spiele in 4:3 bekommen bei „Fit to window“ schmale schwarze Ränder; „Stretch to window“ unter Global Options › Graphics füllt den Schirm, verzerrt aber. **Pausenmenü**: Die Power-Taste friert ScummVM ein und zeigt das Pausenmenü der Konsole (`pocketchip-console --overlay`): WEITER SPIELEN, BEENDEN, Akku, Uhrzeit, WLAN, IP, CPU, RAM, Helligkeit, Lautstärke, Standby, Neustart, Ausschalten. Power-Taste oder ESC kehren ins Spiel zurück. Der Helfer übernimmt dafür über eine Kopie des Dateideskriptors (`pidfd_getfd`) die DRM-Master-Rolle von ScummVM, gibt sie danach samt Bild zurück und verwirft die Eingaben aus dem Menü, damit sie nicht im Spiel landen.

**EINSTELLUNGEN**:

- **01 SYSTEMSTATUS**: Host, Modell, Kernel, CPU, Takt, Auslastung, Load, RAM, Swap, Speicher, Uptime, IP, Temperaturen
- **02 AKKU**: AXP209 Ladung, Status, Spannung, Strom, Leistung, Zustand, USB/AC-Eingang
- **03 ENERGIE**: Ausschalten, Neustart, Standby, jeweils mit Sicherheitsabfrage. Akku: Warnung bei 10 % (Display geht an), bei 5 % 30-s-Countdown und Ausschalten (Ladegerät bricht ab); beide Schwellen einstellbar. `pocketchip-battery-guard.service` fährt auch ohne laufende Konsole herunter (nach ca. 2 min unter der Schwelle). Standby ist ein leichter Standby (Display aus, CPU gedrosselt), weil s2idle auf dem R8 nicht wieder aufwacht
- **04 ANZEIGE**: Helligkeit, Display-Timeout, Standby-Timeout (optional nur im Akkubetrieb), Scanlines, LED im Betrieb an/aus, LED-Blinken im Standby, Display aus, **Touch-Kalibrierung**
- **05 AUDIO**: Lautstärke und Stumm für die 3,5-mm-Klinke (kein eingebauter Lautsprecher), Testton; wird beim Start gesetzt
- **06 WLAN** (nmcli): Funk an/aus, Netze suchen, verbinden (Passwort mit Anzeigen-Knopf), trennen, Passwort ändern, vergessen
- **07 BLUETOOTH** (bluetoothctl): an/aus, suchen, Detailseite je Gerät, koppeln, verbinden, trennen, entfernen. UART mit 115200 Baud (Override), damit BT auch nach Warmstarts geht; zu langsam für Audio
- **08 DATUM / ZEIT** (timedatectl): NTP, Zeitzone, manuelles Stellen
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
| `/etc/systemd/system/pocketchip-console.service` | Dienst auf tty1, `Conflicts=getty@tty1` (Autologin-Shell nur bei Bedarf) |
| `/etc/udev/rules.d/90-pocketchip-touch-libinput.rules` | Kalibriermatrix für libinput ohne X |
| `/var/lib/pocketchip-console/settings.conf` | Einstellungen |
| `/usr/local/sbin/pocketchip-battery-guard` | Fährt bei leerem Akku herunter, auch ohne Konsole |
| `/etc/systemd/system/pocketchip-emu@.service` | Emulator auf tty1, `Conflicts=pocketchip-console`, startet die Konsole danach wieder |
| `/usr/local/lib/pocketchip-console/pocketchip-emu-run` | Startet den Emulator, legt beim ersten Start `~/.config/scummvm/scummvm.ini` an |
| `/etc/systemd/system/pocketchip-emu-helper.service` | Helfer, läuft nur zusammen mit einem Emulator, hält die Power-Taste (systemd-inhibit) |
| `/usr/local/lib/pocketchip-console/pocketchip-emu-helper` | Touch → kalibrierte absolute Maus (uinput), Power-Taste → Pausenmenü |

Die Touch-Kalibrierung schreibt die Matrix in die udev-Regel und nach `/etc/default/pocketchip-touch`.

## Entwicklung

`scripts/dev-restart.sh` startet einen Testbuild von Hand, `scripts/devcmd.sh OUT.png "key down" "tap 200 113"`
schickt Tasten/Taps an die laufende Konsole (Steuerdatei `/run/pocketchip-console/cmd`, nur root)
und holt ein Bildschirmfoto.

Lizenz: MIT. Schrift: DejaVu Sans Mono (freie Lizenz, siehe assets/fonts/LICENSE).

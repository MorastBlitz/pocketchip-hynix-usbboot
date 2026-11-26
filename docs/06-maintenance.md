# 06 – Betrieb und Wartung

## Erste Einrichtung

- `passwd` – der Benutzer `chip` hat anfangs **kein Passwort**, SSH läuft aber.
- WLAN über das Netzwerk-Symbol (nur 2,4 GHz).
- Zeitzone: `sudo timedatectl set-timezone Europe/Berlin`
- SSH-Schlüssel vom Mac: `ssh-copy-id chip@<ip-adresse>` (Hostname `pocketchip`)

## USB-Stick sichern und klonen

Das System belegt nur die ersten **4 GiB** des Sticks (Partition `pocketroot`,
Sektor 2048 – 8388607). Sicherung und Klon passen deshalb auf jeden Stick ab 8 GB.

```sh
sudo poweroff                                          # auf dem PocketCHIP
./scripts/usb-stick.sh backup disk4 pocketchip-backup.img.zst
./scripts/usb-stick.sh write pocketchip-backup.img.zst disk5
```

- **Nie zwei Klone gleichzeitig einstecken** – gleiches Label und gleiche UUID.
- Wer die Partition später vergrößert, sollte unter der Größe des kleinsten
  geplanten Sticks bleiben.
- Die Sicherung enthält WLAN-Passwörter und SSH-Hostkeys – privat halten.

Der Stick ist jetzt das empfindlichste Teil. Eine aktuelle Sicherung macht einen
Ausfall zur Fünf-Minuten-Sache.

## Akku

```sh
cat /sys/class/power_supply/axp20x-battery/{capacity,status,voltage_now,current_now}
upower -i $(upower -e | grep -i bat)
```

Der Treiber schätzt den Ladestand nur aus der Spannung; die tatsächliche
Kapazität meldet er nicht (`energy-full: 0 Wh`). Messbar wäre sie über den
Coulomb-Zähler des AXP209 (Register `0xB0`–`0xB7`) mit einem vollen Entladezyklus.

Originalakku: `LWN11.1Wh3.7V 904360` → 1S-LiPo, ca. 3000 mAh, Zelle 9,0 × 43 × 60 mm,
Stecker **JST-PH 2,0 mm, 2-polig**. Für einen Ersatz:

- LiPo 1S 3,7 V (4,2 V Ladeschluss), mit Schutzschaltung (PCM)
- höchstens so groß wie das Original (Platz vorher nachmessen)
- **JST-PH 2,0** – nicht „JST 1,25“/Micro-JST
- **Polarität vor dem Einstecken prüfen** – bei JST-PH-Akkus nicht einheitlich
- Ladestrom ist auf 1,2 A eingestellt (`constant_charge_current`); bei deutlich
  kleineren Akkus (< 2000 mAh) reduzieren
- Aufgeblähte Akkus sofort ersetzen

## Bootloader auffrischen oder reparieren

Siehe [Kapitel 03, Schritt 2](03-hynix-nand-bootloader.md#schritt-2-schreiben).
FEL-Pin an GND, serielle Konsole an, `CONFIRM_NAND_WRITE=YES ./scripts/flash-bootloader.sh`.

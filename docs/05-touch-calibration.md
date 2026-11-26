# 05 – Touch-Kalibrierung

## Problem

Mit der in PocketTRIX mitgelieferten Matrix stimmte der Touch in der Mitte, lief
aber nach außen immer weiter davon (nach oben → zu weit oben, nach rechts → zu
weit rechts). Das ist ein **Skalierungsfehler**, kein Versatz – eine reine
Verschiebung hilft nicht.

## Lösung: 9-Punkt-Kalibrierung mit Rohwerten

[`scripts/touchcal.py`](../scripts/touchcal.py) zeigt nacheinander neun
Fadenkreuze (10 %/50 %/90 % in X und Y), liest die **rohen** Werte direkt vom
evdev-Gerät `1c25000.rtp` (unabhängig von X und der aktuellen Matrix) und
berechnet per Least-Squares die `libinput Calibration Matrix`:

```
bildschirm_norm = M · roh_norm      (roh_norm = (roh − min) / (max − min))
```

Auf dem PocketCHIP (Benutzer `chip` ist in der Gruppe `input`):

```sh
sudo apt install python3-tk
DISPLAY=:0 python3 touchcal.py
```

Jedes Kreuz mittig antippen, kurz halten, loslassen. Das Skript gibt die
Restfehler je Punkt und die neue Matrix aus (und schreibt sie nach
`/tmp/touchcal.out`) – angewendet wird nichts.

> Ein erster Versuch über Tk-Mausereignisse lieferte unbrauchbare Koordinaten
> (fast immer x≈230). Deshalb liest das Skript evdev direkt.

## Ergebnis (getestetes Gerät)

```
ziel_px  roh  vorhersage_px  rest_px
  48,  27   3550, 3308     49,   27    +1,  +0
 240,  27   2043, 3308    238,   26    -2,  -2
 432,  27    486, 3243    434,   29    +2,  +1
  48, 136   3600, 1940     45,  135    -3,  -1
 240, 136   2030, 1908    243,  136    +3,  -0
 432, 136    531, 1872    431,  137    -1,  +1
  48, 245   3592,  535     49,  246    +1,  +1
 240, 245   2060,  530    241,  245    +1,  -0
 432, 245    558,  520    430,  243    -2,  -1
NEU -1.07169 -0.01700 1.04440 0.01932 -1.18735 1.04330 0 0 1
```

Restfehler ≤ 3 px. Vorher: Skalierung X 1,209 / Y 1,417 – um 13 % bzw. 19 % zu stark.

Jedes Panel ist etwas anders – bitte selbst messen statt diese Werte zu übernehmen.

## Testen und dauerhaft speichern

```sh
M="-1.07169 -0.01700 1.04440 0.01932 -1.18735 1.04330 0 0 1"
DISPLAY=:0 xinput set-prop 1c25000.rtp "libinput Calibration Matrix" $M   # sofort, bis Neustart

sudo sed -i "s/^POCKETCHIP_TOUCH_MATRIX=.*/POCKETCHIP_TOUCH_MATRIX='$M'/" /etc/default/pocketchip-touch
sudo sed -i "s/Option \"CalibrationMatrix\" \".*\"/Option \"CalibrationMatrix\" \"$M\"/" \
    /etc/X11/xorg.conf.d/40-pocketchip-touch.conf
DISPLAY=:0 pocketchip-touch-calibrate apply
```

Beide Stellen werden beim Start gelesen: Xorg (`40-pocketchip-touch.conf`) und
das PocketTRIX-Skript `pocketchip-touch-calibrate` (`/etc/default/pocketchip-touch`).
Nach einem Neustart geprüft – die Matrix bleibt erhalten.

# 04 – Serielle Konsole (UART)

Für die NAND-Arbeiten dringend empfohlen: Ohne Konsole sieht man nicht, ob
SPL, U-Boot oder Kernel hängen.

## Anschluss

USB-Seriell-Adapter mit **3,3 V** (z. B. FT232R). 5 V kann den R8 beschädigen.
Steckleiste **U14** des CHIP (siehe Beschriftung auf der PocketCHIP-Platine):

| Adapter | CHIP U14 |
|---|---|
| GND | Pin 1 – GND |
| RX | Pin 3 – UART1-TX |
| TX | Pin 5 – UART1-RX |
| VCC | **nicht verbinden** |

FEL-Modus erzwingen: Pin 7 (FEL) mit Pin 1 (GND) verbinden.

Parameter: **115200 Baud, 8N1**, keine Flusskontrolle.

## Mitschnitt unter macOS

```sh
ls /dev/cu.usbserial-*
( stty 115200 cs8 -cstopb -parenb raw -echo -crtscts clocal; exec cat ) \
    < /dev/cu.usbserial-XXXXXXXX > serial.log
```

`stty` wirkt auf macOS nur, solange das Gerät geöffnet ist – deshalb öffnet die
Subshell das Gerät einmal als stdin und `cat` liest daraus. Interaktiv geht es
auch mit `screen /dev/cu.usbserial-XXXXXXXX 115200` (Beenden: Ctrl-A, K).

Ohne eingebauten PocketCHIP erscheinen im Kernel-Log viele
`tca8418_keypad … error: -6` – die Tastatur fehlt einfach, das ist normal.

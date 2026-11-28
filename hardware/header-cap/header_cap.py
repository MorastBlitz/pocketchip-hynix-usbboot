# PocketCHIP: Kappe fuer die GPIO-Leiste mit Steckern fuer UART und I2C
#
# Ausfuehren mit FreeCAD 1.x (ohne Oberflaeche):
#   /Applications/FreeCAD.app/Contents/Resources/bin/freecadcmd -c \
#     "exec(open('hardware/header-cap/header_cap.py').read())"
# Erzeugt header_cap.step und header_cap.stl neben diesem Skript.
#
# Koordinaten wie in den NTC-STEP-Dateien der Rueckschale
# (NextThingCo/PocketCHIP-Mechanical, NTC-100092):
#   X  Breite (von vorn gesehen zeigt +X nach links)
#   Y  Dicke: Platine liegt bei Y = 0 ... 1.5, die Rueckschale bei Y < 0
#   Z  Hoehe: Oberkante der Rueckschale bei Z = 114.5
# Platine -> Gehaeuse: X_case = -x_pcb, Z_case = y_pcb - 33.1
# (abgeleitet aus NextThingCo/PocketCHIP-PCB, PocketCHIP-v1.0.brd:
#  Unterkante der Platine an den unteren Clips der Schale, oberer Rand der
#  Rastschlitze an der Oberkante der Schale)
#
# Die Kappe sitzt auf dem freien Platinenstreifen ueber der Rueckschale, in dem
# die Leiste JP1 liegt. Die Kabel werden von hinten in die Leiste geloetet und
# laufen im Kasten der Kappe zu zwei Steckern, die nach oben zeigen:
#   UART: JST-PH 2,0 mm, 3-polig, stehend (z. B. B3B-PH-K-S): GND, RX, TX
#   I2C:  JST-SH 1,0 mm, 4-polig (Qwiic/STEMMA QT) auf kleiner Adapterplatine
# Abgeleitet von CC-BY-SA-3.0-Daten von Next Thing Co.; daher ebenfalls
# CC BY-SA 3.0 (siehe README.md in diesem Ordner).

import FreeCAD as App
import Part
import os

V = App.Vector
HERE = os.path.dirname(os.path.abspath(__file__)) if "__file__" in dir() else \
    os.path.join(os.getcwd(), "hardware", "header-cap")

# ---- Masse aus den NTC-Daten --------------------------------------------
PCB_T = 1.5            # Platinendicke laut STEP (Spalt wird groesser gewaehlt)
SHELL_TOP = 114.53     # Oberkante Rueckschale
PCB_TOP = 161.70 - 33.1  # Oberkante Platine = 128.6
HDR_Z = 157.976 - 33.1   # Mitte der Loecher von JP1 = 124.88
# Loecher JP1 (Raster 2,54 mm) im Gehaeuse-X: Pin 21 bei -15.24 ... Pin 25 bei +45.72

# ---- Einstellbare Masse --------------------------------------------------
GAP = 0.25             # Spiel rundum
WALL = 1.6             # Wandstaerke
TOP = 2.0              # Deckel ueber der Platinenkante
X_MIN, X_MAX = -18.0, 49.0   # Breite der Kappe (deckt alle 25 Pins ab)
DEPTH = 13.0           # Tiefe des Kastens hinter der Platine (Y); die Schale
                       # selbst ist an ihrer Oberkante nur ~4,6 mm tief
FRONT_T = 1.4          # Dicke der vorderen Lippe
FRONT_DOWN = 2.2       # wie weit die Lippe vorn ueber die Platine greift
# (bleibt oberhalb der Loetstellen der Leiste: Pads reichen bis ~Z 125.9)

# Stecker (Mitte X, Ausschnitt im Deckel, Auflage darunter)
UART_X = -6.0
UART_CUT = (8.2, 4.8)        # JST-PH 3-polig stehend: Koerper 7,95 x 4,5 mm
UART_BODY_H = 6.0
I2C_X = 24.0
I2C_CUT = (6.4, 4.6)         # JST-SH 4-polig stehend: Koerper ca. 6,0 x 4,25 mm
I2C_BOARD = (12.0, 10.5)     # Tasche fuer die Adapterplatine (X, Y)
I2C_BOARD_T = 1.0            # Tiefe der Tasche unter dem Deckel

z0 = SHELL_TOP + 0.3                 # Unterkante: knapp ueber der Schale
z_top = PCB_TOP + GAP + TOP          # Oberkante der Kappe
w = X_MAX - X_MIN

# Grundkoerper: Kasten hinten + Steg ueber der Platinenkante + vordere Lippe
back = Part.makeBox(w, DEPTH, z_top - z0, V(X_MIN, -DEPTH, z0))
bridge = Part.makeBox(w, PCB_T + 2 * GAP + FRONT_T, TOP,
                      V(X_MIN, 0, PCB_TOP + GAP))
lip = Part.makeBox(w, FRONT_T, FRONT_DOWN + GAP + TOP,
                   V(X_MIN, PCB_T + 2 * GAP, PCB_TOP - FRONT_DOWN))
body = back.fuse(bridge).fuse(lip)

# Innenraum des Kastens (unten offen, damit die Kabel von der Leiste kommen)
inner = Part.makeBox(w - 2 * WALL, DEPTH - WALL, z_top - TOP - z0 + 1,
                     V(X_MIN + WALL, -DEPTH + WALL, z0 - 1))
body = body.cut(inner)

# Schlitz fuer die Platine (vorne bis zur Lippe)
slot = Part.makeBox(w + 2, PCB_T + 2 * GAP, PCB_TOP + GAP - z0 + 1,
                    V(X_MIN - 1, -GAP, z0 - 1))
body = body.cut(slot)

# UART: Ausschnitt im Deckel, Kasten traegt den Stecker an den Seitenwaenden
ux, uy = UART_CUT
uy0 = -DEPTH / 2 - uy / 2
body = body.cut(Part.makeBox(ux, uy, TOP + 2, V(UART_X - ux / 2, uy0, z_top - TOP - 1)))
# Auflage: zwei Stege unter dem Stecker, damit er nicht durchrutscht. Sie enden
# WIRE_GAP vor der Platine: dort laufen die Kabel von der Leiste nach oben.
WIRE_GAP = 3.0
shelf_z = z_top - UART_BODY_H
for sx in (UART_X - ux / 2 - 1.2, UART_X + ux / 2):
    body = body.fuse(Part.makeBox(1.2, DEPTH - WALL - WIRE_GAP, 1.0,
                                  V(sx, -DEPTH + WALL, shelf_z - 1.0)))

# I2C: Tasche fuer die Adapterplatine unter dem Deckel + Ausschnitt fuer den Stecker
bx, by = I2C_BOARD
cx, cy = I2C_CUT
body = body.cut(Part.makeBox(cx, cy, TOP + 2, V(I2C_X - cx / 2, -DEPTH / 2 - cy / 2, z_top - TOP - 1)))
pocket = Part.makeBox(bx, min(by, DEPTH - WALL), I2C_BOARD_T,
                      V(I2C_X - bx / 2, -DEPTH + WALL, z_top - TOP - I2C_BOARD_T + 0.01))
body = body.cut(pocket)

# Beschriftung als Vertiefung im Deckel waere schoen; fuer den Druck reicht
# zunaechst ein Markierungspunkt an Pin 1 der Stecker (Kerbe hinten)
for x in (UART_X - ux / 2 - 0.2, I2C_X - cx / 2 - 0.2):
    body = body.cut(Part.makeCylinder(0.6, 0.6, V(x - 0.8, -DEPTH / 2 + 2.6, z_top - 0.6)))

body = body.removeSplitter()
assert body.isValid(), "Koerper ungueltig"

step = os.path.join(HERE, "header_cap.step")
stl = os.path.join(HERE, "header_cap.stl")
body.exportStep(step)
mesh = body.tessellate(0.05)
import Mesh
Mesh.Mesh(mesh).write(stl)
bb = body.BoundBox
print("header_cap: %.1f x %.1f x %.1f mm, Volumen %.1f cm3" % (bb.XLength, bb.YLength, bb.ZLength, body.Volume / 1000))
print("geschrieben:", step, stl)
CAP = body

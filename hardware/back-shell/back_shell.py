# PocketCHIP: geaenderte Rueckschale mit Kabeldurchfuehrung und Steckern
# fuer UART und I2C
#
# Ausgangspunkt ist die Original-Rueckschale von Next Thing Co.
# (NextThingCo/PocketCHIP-Mechanical, NTC-100092_PocketCHIP_Back_r1.1,
# CC BY-SA 3.0 US). Das Skript laedt die STEP-Datei bei Bedarf herunter und
# aendert sie:
#   1. Kabeldurchfuehrung in der Oberkante, neben dem Ausschnitt fuer den
#      USB-Port des CHIP. Die Kabel kommen von der Leiste JP1 auf dem
#      Platinenstreifen ueber der Schale.
#   2. Zwei Fenster in der Schraegflaeche an der Seite, mit Haltern innen:
#      UART (JST-PH, 3-polig, liegend) und I2C (JST-SH/Qwiic, 4-polig, liegend).
#      Die Stecker zeigen zur Seite (-X), der Stecker wird seitlich eingesteckt.
#
# Ausfuehren mit FreeCAD 1.x (ohne Oberflaeche), im Repo-Hauptordner:
#   /Applications/FreeCAD.app/Contents/Resources/bin/freecadcmd -c \
#     "exec(open('hardware/back-shell/back_shell.py').read())"
# Ergebnis: back_shell.step und back_shell.stl neben diesem Skript.
#
# Koordinaten wie in der NTC-STEP-Datei:
#   X  Breite; von hinten gesehen zeigt +X nach rechts, von vorn nach links
#   Y  Dicke; die Platine liegt bei Y = 0 ... 1.5, die Schale bei Y < 0
#   Z  Hoehe; Oberkante der Schale bei Z = 114.5, Unterkante bei Z = -35.1
# Platine -> Gehaeuse: X = -x_pcb, Z = y_pcb - 33.1 (aus PocketCHIP-v1.0.brd)

import FreeCAD as App
import Part
import Mesh
import os
import urllib.request

V = App.Vector
HERE = os.path.join(os.getcwd(), "hardware", "back-shell")
CACHE = os.path.join(HERE, ".cache")
SRC_NAME = "NTC-100092_PocketCHIP_Back_r1.1_06-14-2016.STEP"
SRC_URL = ("https://raw.githubusercontent.com/NextThingCo/PocketCHIP-Mechanical/"
           "master/v1.0/3D_Model/STEP_files/" + SRC_NAME)

# ---- Kabeldurchfuehrung oben --------------------------------------------
# Die Oberkante ist dort 4,6 mm tief (Y -4.6 ... 0). Der USB-Ausschnitt des
# CHIP reicht von X -20.8 bis +20.8. Die Signal-Pins (UART, I2C, GND) liegen
# bei X -15 ... -2.5; die Kabel laufen auf der Platinenrueckseite zur Seite.
SLOT_X = (-30.0, -23.0)      # Breite der Durchfuehrung
SLOT_Y = (-3.6, -0.6)        # Tiefe (laesst vorn und hinten Wand stehen)
SLOT_Z = (100.0, 116.0)      # von innen bis durch die Oberkante

# ---- Stecker in der seitlichen Schraegflaeche ----------------------------
# Jeder Stecker: Mitte (Y, Z), Fenster (Hoehe in Y, Breite in Z),
# Laenge des Halters nach innen (+X). Die Fenster sind grosszuegig; vor dem
# Druck die gekauften Teile messen und hier eintragen.
WALL = 1.6                   # Wandstaerke der Halter
GAP = 0.3                    # Spiel um den Steckerkoerper
CONNECTORS = {
    #          Y-Mitte  Z-Mitte  Fenster Y  Fenster Z  Halterlaenge
    "UART": (-6.6,     82.0,    6.2,       8.2,       9.0),   # JST-PH S3B-PH-K, liegend
    "I2C":  (-6.0,     69.0,    3.4,       6.4,       9.0),   # JST-SH SM04B, liegend, auf Adapter
}
PCB_CLEAR = -1.8             # Halter bleiben hinter dieser Y-Ebene (Platine + Bauteile)


def load_source():
    os.makedirs(CACHE, exist_ok=True)
    path = os.path.join(CACHE, SRC_NAME)
    if not os.path.exists(path):
        print("lade", SRC_URL)
        urllib.request.urlretrieve(SRC_URL, path)
    s = Part.Shape()
    s.read(path)
    return s


def outer_x(shell, y, z):
    """X der Aussenwand (linke Seite, X < 0) auf Hoehe z bei Tiefe y."""
    line = Part.makeLine(V(-70, y, z), V(-20, y, z))
    pts = [v.Point.x for v in shell.section(line).Vertexes]
    return min(pts) if pts else None


def envelope(shell, za, zb, step=1.0):
    """Volumen innerhalb der Aussenhaut zwischen za und zb (Schale + Innenraum),
    als Loft ueber zwei Schnitte. Dient dazu, Halter innen abzuschneiden, damit
    nichts aussen ueber die Schale ragt."""
    wires = []
    for z in (za, zb):
        pts = []
        x = -56.5
        while x <= 56.5:
            line = Part.makeLine(V(x, -30, z), V(x, 0.0, z))
            ys = [v.Point.y for v in shell.section(line).Vertexes]
            if ys:
                pts.append(V(x, min(ys), z))
            x += step
        poly = [V(pts[0].x, 0, z)] + pts + [V(pts[-1].x, 0, z), V(pts[0].x, 0, z)]
        wires.append(Part.makePolygon(poly))
    return Part.makeLoft(wires, True, True)


shell = load_source()
assert shell.isValid()

# 1. Kabeldurchfuehrung
x0, x1 = SLOT_X
y0, y1 = SLOT_Y
z0, z1 = SLOT_Z
slot = Part.makeBox(x1 - x0, y1 - y0, z1 - z0, V(x0, y0, z0))
# Kanten verrunden, damit die Isolierung nicht scheuert
try:
    slot = slot.makeFillet(1.0, [e for e in slot.Edges
                                 if abs(e.Vertexes[0].Point.z - e.Vertexes[-1].Point.z) > 1])
except Exception:
    pass
shell = shell.cut(slot)

# 2. Stecker
for name, (yc, zc, wy, wz, length) in CONNECTORS.items():
    xo = outer_x(shell, yc, zc)
    assert xo is not None, name + ": keine Aussenwand gefunden"
    # Innenseite der Wand: erste Flaeche nach der Aussenwand (+X)
    line = Part.makeLine(V(xo + 0.05, yc, zc), V(xo + 15, yc, zc))
    xs = sorted(v.Point.x for v in shell.section(line).Vertexes)
    xi = xs[0] if xs else xo + 2.0
    py, pz = wy + 2 * GAP, wz + 2 * GAP
    # Halter: Huelse von der Innenwand nach innen, hinten offen fuer die Kabel
    oy, oz = py + 2 * WALL, pz + 2 * WALL
    sleeve = Part.makeBox(length + (xi - xo), oy, oz,
                          V(xo, yc - oy / 2, zc - oz / 2))
    sleeve = sleeve.common(Part.makeBox(200, 200, 300, V(-100, PCB_CLEAR - 200, -100)))
    sleeve = sleeve.common(envelope(shell, zc - oz / 2 - 0.5, zc + oz / 2 + 0.5))
    shell = shell.fuse(sleeve)
    # Fenster durch Wand und Huelse
    window = Part.makeBox(length + (xi - xo) + 10, py, pz,
                          V(xo - 5, yc - py / 2, zc - pz / 2))
    shell = shell.cut(window)
    print("%s: Fenster %.1f x %.1f mm, Aussenwand bei X=%.2f, Innenwand X=%.2f"
          % (name, py, pz, xo, xi))

shell = shell.removeSplitter()
assert shell.isValid(), "Ergebnis ungueltig"

shell.exportStep(os.path.join(HERE, "back_shell.step"))
Mesh.Mesh(shell.tessellate(0.05)).write(os.path.join(HERE, "back_shell.stl"))
bb = shell.BoundBox
print("back_shell: %.1f x %.1f x %.1f mm" % (bb.XLength, bb.YLength, bb.ZLength))
SHELL = shell

#!/bin/sh
# Copy the shared libraries the linker needs from the device into ./sysroot.
set -e
HOST="${POCKETCHIP_HOST:-chip@pocketchip.local}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
D="$ROOT/sysroot/usr/lib/arm-linux-gnueabihf"
mkdir -p "$D" "$ROOT/sysroot/pkgconfig"
for l in libinput.so.10 libudev.so.1 libxkbcommon.so.0 libfontconfig.so.1; do
  scp -q "$HOST:/usr/lib/arm-linux-gnueabihf/$l" "$D/"
  ln -sf "$l" "$D/${l%%.so*}.so"
done
for n in libudev:udev libinput:input xkbcommon:xkbcommon; do
  pc="${n%%:*}"; lib="${n##*:}"
  printf 'libdir=${pcfiledir}/../usr/lib/arm-linux-gnueabihf\nName: %s\nDescription: device copy\nVersion: 1.0\nLibs: -L${libdir} -l%s\nCflags:\n' "$pc" "$lib" > "$ROOT/sysroot/pkgconfig/$pc.pc"
done
echo "sysroot ready"

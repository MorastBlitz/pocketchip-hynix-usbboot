#!/bin/sh
# Install pocketchip-console on the device over SSH and make it the default UI.
# usage: scripts/install.sh [--no-enable]
set -e
HOST="${POCKETCHIP_HOST:-chip@pocketchip.local}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$HOME/.cache/pocketchip-console-target}/armv7-unknown-linux-gnueabihf/release/pocketchip-console"
[ -x "$BIN" ] || { echo "build first: cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.41" >&2; exit 1; }

ssh "$HOST" 'mkdir -p /tmp/pcc-install'
scp -q "$BIN" "$ROOT/system/pocketchip-console.service" "$ROOT/system/90-pocketchip-touch-libinput.rules" \
    "$ROOT/system/pocketchip-bt-fix" "$ROOT/system/pocketchip-bt-fix.service" \
    "$ROOT/system/pocketchip-battery-guard" "$ROOT/system/pocketchip-battery-guard.service" \
    "$ROOT/system/pocketchip-update" "$ROOT/system/pocketchip-upload" \
    "$ROOT/system/firmware/rtl8723bs_config.bin" \
    "$ROOT/system/pocketchip-emu@.service" "$ROOT/system/pocketchip-emu-helper.service" "$ROOT/system/pocketchip-emu-run" "$ROOT/system/pocketchip-emu-helper" \
    "$HOST:/tmp/pcc-install/"
ssh "$HOST" ENABLE="${1:-}" POCKETCHIP_USER="${POCKETCHIP_USER:-}" 'set -e; cd /tmp/pcc-install
sudo install -m 0755 pocketchip-console /usr/local/bin/pocketchip-console.new
sudo mv /usr/local/bin/pocketchip-console.new /usr/local/bin/pocketchip-console
sudo install -m 0644 pocketchip-console.service /etc/systemd/system/
# X11/LXDE is gone: remove the old way back from the desktop
u="${POCKETCHIP_USER:-chip}"; h="$(getent passwd "$u" | cut -d: -f6)"
sudo rm -f /usr/local/bin/pocketchip-console-start /usr/share/applications/pocketchip-console.desktop
[ -n "$h" ] && sudo rm -f "$h/Desktop/pocketchip-console.desktop"
# keep a calibration the console already wrote
[ -e /etc/udev/rules.d/90-pocketchip-touch-libinput.rules ] || sudo install -m 0644 90-pocketchip-touch-libinput.rules /etc/udev/rules.d/
sudo udevadm control --reload
# Bluetooth: 115200 baud config override and re-probe after warm reboots
sudo install -D -m 0644 rtl8723bs_config.bin /lib/firmware/updates/rtl_bt/rtl8723bs_config.bin
sudo install -m 0755 pocketchip-bt-fix /usr/local/sbin/
sudo install -m 0644 pocketchip-bt-fix.service /etc/systemd/system/
# EINSTELLUNGEN > UPDATE (apt) and the UPLOAD file server
sudo install -m 0755 pocketchip-update /usr/local/sbin/
sudo install -d /usr/local/lib/pocketchip-console
sudo install -m 0755 pocketchip-upload /usr/local/lib/pocketchip-console/
# new kernels are added to extlinux.conf by pocketchip-update-extlinux (rootfs);
# lift the hold that older versions of this script set
if dpkg -s linux-image-armmp >/dev/null 2>&1; then sudo apt-mark unhold linux-image-armmp >/dev/null; fi
# low battery: power off even when the console is not running
sudo install -m 0755 pocketchip-battery-guard /usr/local/sbin/
sudo install -m 0644 pocketchip-battery-guard.service /etc/systemd/system/
# emulators (EMULATOREN tile): full screen on DRM/KMS, console comes back on exit
u="${POCKETCHIP_USER:-chip}"
sudo install -d /usr/local/lib/pocketchip-console
sudo install -m 0755 pocketchip-emu-run pocketchip-emu-helper /usr/local/lib/pocketchip-console/
sudo install -m 0644 pocketchip-emu-helper.service /etc/systemd/system/
# replaced by pocketchip-emu-helper
sudo rm -f /etc/systemd/system/pocketchip-emu-touch.service /usr/local/lib/pocketchip-console/pocketchip-emu-touch
sed "s/^User=chip$/User=$u/" pocketchip-emu@.service | sudo tee /etc/systemd/system/pocketchip-emu@.service >/dev/null
dpkg -s scummvm >/dev/null 2>&1 || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends scummvm
# BT-RADAR: vendor names of public Bluetooth addresses (IEEE OUI list)
dpkg -s ieee-data >/dev/null 2>&1 || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ieee-data
sudo systemctl daemon-reload
if [ "$ENABLE" != "--no-enable" ]; then
  # boot into the console; the getty@tty1 autologin shell only on demand
  sudo systemctl disable getty@tty1.service 2>/dev/null || true
  sudo systemctl enable pocketchip-console.service
  sudo systemctl enable pocketchip-bt-fix.service
  sudo systemctl enable pocketchip-battery-guard.service
fi
if [ "$ENABLE" != "--no-enable" ]; then
  sudo systemctl restart pocketchip-battery-guard.service
fi
if systemctl is-active -q getty@tty1.service; then
  echo "Shell auf tty1 aktiv: Konsole nicht neu gestartet (systemctl start pocketchip-console)"
else
  sudo systemctl restart pocketchip-console.service
fi
rm -rf /tmp/pcc-install
systemctl --no-pager --lines=3 status pocketchip-console.service | head -5'

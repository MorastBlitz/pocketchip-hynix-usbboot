#!/bin/sh
# Install pocketchip-console on the device over SSH and make it the default UI.
# usage: scripts/install.sh [--no-enable]
set -e
HOST="${POCKETCHIP_HOST:-chip@pocketchip.local}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$HOME/.cache/pocketchip-console-target}/armv7-unknown-linux-gnueabihf/release/pocketchip-console"
[ -x "$BIN" ] || { echo "build first: cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.41" >&2; exit 1; }

ssh "$HOST" 'mkdir -p /tmp/pcc-install'
scp -q "$BIN" "$ROOT/system/pocketchip-console.service" "$ROOT/system/pocketchip-console-start" \
    "$ROOT/system/pocketchip-console.desktop" "$ROOT/system/90-pocketchip-touch-libinput.rules" \
    "$ROOT/system/pocketchip-bt-fix" "$ROOT/system/pocketchip-bt-fix.service" \
    "$ROOT/system/firmware/rtl8723bs_config.bin" \
    "$HOST:/tmp/pcc-install/"
ssh "$HOST" ENABLE="${1:-}" 'set -e; cd /tmp/pcc-install
sudo install -m 0755 pocketchip-console /usr/local/bin/pocketchip-console.new
sudo mv /usr/local/bin/pocketchip-console.new /usr/local/bin/pocketchip-console
sudo install -m 0755 pocketchip-console-start /usr/local/bin/
sudo install -m 0644 pocketchip-console.service /etc/systemd/system/
sudo install -m 0644 pocketchip-console.desktop /usr/share/applications/
# keep a calibration the console already wrote
[ -e /etc/udev/rules.d/90-pocketchip-touch-libinput.rules ] || sudo install -m 0644 90-pocketchip-touch-libinput.rules /etc/udev/rules.d/
sudo udevadm control --reload
# Bluetooth: 115200 baud config override and re-probe after warm reboots
sudo install -D -m 0644 rtl8723bs_config.bin /lib/firmware/updates/rtl_bt/rtl8723bs_config.bin
sudo install -m 0755 pocketchip-bt-fix /usr/local/sbin/
sudo install -m 0644 pocketchip-bt-fix.service /etc/systemd/system/
# way back from LXDE: desktop icon and first button in the panel launchbar
u="${POCKETCHIP_USER:-chip}"; h="$(getent passwd "$u" | cut -d: -f6)"
if [ -n "$h" ]; then
  sudo -u "$u" mkdir -p "$h/Desktop"
  sudo -u "$u" install -m 0755 pocketchip-console.desktop "$h/Desktop/"
  p="$h/.config/lxpanel/LXDE/panels/panel"
  if [ -f "$p" ] && ! grep -q pocketchip-console.desktop "$p"; then
    sudo -u "$u" cp "$p" "$p.bak-pcc"
    sudo -u "$u" sed -i "/type=launchbar/,/Config {/{/Config {/a\\    Button {\\n      id=pocketchip-console.desktop\\n    }
}" "$p"
  fi
fi
sudo systemctl daemon-reload
if [ "$ENABLE" != "--no-enable" ]; then
  # boot into the console; getty@tty1 (autologin + LXDE) only on demand
  sudo systemctl disable getty@tty1.service 2>/dev/null || true
  sudo systemctl enable pocketchip-console.service
  sudo systemctl enable pocketchip-bt-fix.service
fi
if systemctl is-active -q getty@tty1.service; then
  echo "LXDE laeuft: Konsole startet beim naechsten Wechsel zurueck"
else
  sudo systemctl restart pocketchip-console.service
fi
rm -rf /tmp/pcc-install
systemctl --no-pager --lines=3 status pocketchip-console.service | head -5'

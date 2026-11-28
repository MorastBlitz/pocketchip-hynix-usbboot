#!/bin/sh
# Copy the freshly built binary to the device and (re)start it on tty1 by hand.
set -e
HOST="${POCKETCHIP_HOST:-chip@pocketchip.local}"
BIN="${CARGO_TARGET_DIR:-$HOME/.cache/pocketchip-console-target}/armv7-unknown-linux-gnueabihf/release/pocketchip-console"
scp -q "$BIN" "$HOST:/tmp/pocketchip-console.new"
ssh "$HOST" 'sudo systemctl stop pocketchip-console 2>/dev/null || true; sudo pkill -x pocketchip-cons || true; sleep 1; mv /tmp/pocketchip-console.new /tmp/pocketchip-console; sudo sh -c "setsid /tmp/pocketchip-console > /tmp/pcc.log 2>&1 < /dev/tty1 &"; sleep 4; cat /tmp/pcc.log'

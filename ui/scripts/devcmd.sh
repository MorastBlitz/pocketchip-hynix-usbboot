#!/bin/sh
# Send debug commands to the running console and fetch a snapshot.
# usage: scripts/devcmd.sh OUT.png "key down" "key enter" ...
set -e
HOST="${POCKETCHIP_HOST:-chip@pocketchip.local}"
out="$1"; shift
cmds=""
for c in "$@"; do cmds="$cmds$c
"; done
cmds="${cmds}snap /tmp/pcc-snap.png"
printf '%s\n' "$cmds" | ssh "$HOST" 'sudo tee /run/pocketchip-console/cmd.tmp >/dev/null && sudo mv /run/pocketchip-console/cmd.tmp /run/pocketchip-console/cmd; for i in 1 2 3 4 5 6 7 8 9 10; do sleep 0.5; [ -e /run/pocketchip-console/cmd ] || break; done; sleep 0.3; sudo cat /tmp/pcc-snap.png' > "$out"

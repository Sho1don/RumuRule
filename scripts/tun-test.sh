#!/bin/sh
# Run a TUN session with a dead-man's switch. For development and debugging.
#
#   ./scripts/tun-test.sh ~/.config/slop.rumurule/profiles.json 2 [seconds]
#
# Layered protection:
#   1. --owner      the helper stops when this script exits, even on a crash
#   2. --max-runtime the helper stops itself after the time limit
#   3. a detached watchdog touches the stop file if this script is SIGKILLed
#   4. an EXIT trap requests a stop on every normal or signal exit
#   5. a traffic check stops the helper when the tunnel cannot carry traffic
#
# The helper also removes the interface itself when it exits, so a session can
# never outlive the process that asked for it.
#
# The polkit prompt is shown once; cached authorization covers the next minutes.
set -eu

store=${1:?usage: tun-test.sh <profiles.json> <index> [seconds]}
index=${2:-0}
limit=${3:-120}

root=$(cd "$(dirname "$0")/.." && pwd)
work=${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}
config="$work/rumurule-tun.json"
stop="$work/rumurule-tun.stop"
xray=${RUMURULE_XRAY:-"$HOME/.local/share/slop.rumurule/bin/xray"}
helper="$root/target/debug/rumurule-helper"

if [ ! -x "$helper" ]; then
    echo "build the helper first: cargo build -p rumurule-helper" >&2
    exit 1
fi
if [ ! -x "$xray" ]; then
    echo "xray not found: $xray" >&2
    exit 1
fi

cargo run -q -p rumurule-core --example dump_tun -- "$store" "$index" > "$config"
chmod 644 "$config"
rm -f "$stop"

stop_now() {
    : > "$stop"
}
trap stop_now EXIT INT TERM HUP

# Survives this shell: stops the tunnel if everything above is killed.
setsid sh -c 'sleep "$1"; : > "$2"' sh "$limit" "$stop" >/dev/null 2>&1 &

# Set RUMURULE_HEALTH=off to keep the tunnel up without a traffic check.
health=${RUMURULE_HEALTH:-1.1.1.1:80}

echo "starting TUN for ${limit}s; stop with: touch $stop"
set +e
pkexec "$helper" run \
    --config "$config" \
    --xray "$xray" \
    --owner $$ \
    --stop-file "$stop" \
    --max-runtime "$limit" \
    --health "$health"
code=$?
set -e

# Xray removes its own routes and link; report anything left behind.
if [ -e /sys/class/net/rumurule0 ]; then
    echo "warning: rumurule0 is still up" >&2
fi
if resolvectl status rumurule0 >/dev/null 2>&1; then
    echo "warning: system DNS still points at rumurule0" >&2
fi
exit "$code"

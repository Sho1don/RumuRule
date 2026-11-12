#!/bin/sh
# Install the privileged TUN helper and its polkit action. Run as root.
#
#   ./scripts/install-helper.sh [path/to/rumurule-helper]
#
# Packages install the same two files; this script is for development builds.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
binary=${1:-"$root/target/release/rumurule-helper"}

if [ ! -x "$binary" ]; then
    echo "missing $binary" >&2
    echo "build it first: cargo build --release -p rumurule-helper" >&2
    exit 1
fi

install -Dm755 "$binary" /usr/lib/rumurule/rumurule-helper
install -Dm644 \
    "$root/packaging/slop.rumurule.tunnel.policy" \
    /usr/share/polkit-1/actions/slop.rumurule.tunnel.policy

echo "installed /usr/lib/rumurule/rumurule-helper"
echo "installed /usr/share/polkit-1/actions/slop.rumurule.tunnel.policy"
echo "remove both files to undo; TUN mode also works through pkexec without them"

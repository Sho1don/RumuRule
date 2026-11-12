#!/bin/sh
# Fetch the pinned Xray-core release and install it for RumuRule.
#
# Usage: scripts/fetch-xray.sh [destination]
# Default destination: $XDG_DATA_HOME/slop.rumurule/bin/xray
set -eu

VERSION="v26.11.9"

case "$(uname -m)" in
    x86_64)       ASSET="Xray-linux-64.zip";        SHA256="b651f7e1d9966ee2c1b1ee1bf8b52c72771824d3065dce96bbc8bcc19ed95f82" ;;
    aarch64|arm64) ASSET="Xray-linux-arm64-v8a.zip"; SHA256="64ed6858982a7f5eb85d88b811089063bd4f17561f4c8f44e0f1dc8e6f6430be" ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

URL="https://github.com/XTLS/Xray-core/releases/download/${VERSION}/${ASSET}"
DEST="${1:-${XDG_DATA_HOME:-$HOME/.local/share}/slop.rumurule}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading ${ASSET} (${VERSION})"
curl -fsSL -o "$TMP/xray.zip" "$URL"

ACTUAL="$(sha256sum "$TMP/xray.zip" | cut -d' ' -f1)"
if [ "$ACTUAL" != "$SHA256" ]; then
    echo "checksum mismatch: expected ${SHA256}, got ${ACTUAL}" >&2
    exit 1
fi

unzip -oq "$TMP/xray.zip" xray -d "$TMP"
mkdir -p "$DEST/bin"
install -m 755 "$TMP/xray" "$DEST/bin/xray"
"$DEST/bin/xray" version | head -1
echo "Installed to $DEST/bin/xray"

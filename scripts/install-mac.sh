#!/bin/sh
# Chargestrip installer for macOS: downloads the latest .dmg, copies the app to /Applications and
# opens it. Usage (Terminal):
#   curl -fsSL https://raw.githubusercontent.com/erogluuefe/chargestrip/main/scripts/install-mac.sh | sh
#
# Files downloaded with curl don't get the quarantine flag browsers set, so the "Apple could not
# verify" warning does not appear. Later versions arrive through the app's own "Update" menu entry.
set -eu

RELEASES=https://github.com/erogluuefe/chargestrip/releases
# releases/latest redirects to .../releases/tag/v1.2.3 (no API rate limit).
VERSION=$(curl -fsSI "$RELEASES/latest" | tr -d '\r' | sed -n 's#^[Ll]ocation: .*/tag/v\(.*\)$#\1#p')
[ -n "$VERSION" ] || { echo "Could not find the latest release." >&2; exit 1; }
URL="$RELEASES/download/v$VERSION/Chargestrip-macOS-$VERSION.dmg"

TMP=$(mktemp -d)
trap 'hdiutil detach -quiet "$TMP/mnt" 2>/dev/null || true; rm -rf "$TMP"' EXIT

echo "Downloading Chargestrip $VERSION..."
curl -fsSL -o "$TMP/chargestrip.dmg" "$URL"
mkdir "$TMP/mnt"
hdiutil attach -nobrowse -quiet -mountpoint "$TMP/mnt" "$TMP/chargestrip.dmg"

pkill -x chargestrip 2>/dev/null || true # quit a running copy so it can be replaced
rm -rf /Applications/Chargestrip.app
ditto "$TMP/mnt/Chargestrip.app" /Applications/Chargestrip.app
xattr -dr com.apple.quarantine /Applications/Chargestrip.app 2>/dev/null || true # left over from a browser install

open /Applications/Chargestrip.app
echo "Chargestrip $VERSION installed; look for it in the menu bar."

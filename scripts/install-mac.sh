#!/bin/sh
# Chargestrip macOS kurulumu: son sürümün .dmg'sini indirir, uygulamayı /Applications'a kopyalar
# ve açar. Kullanım (Terminal):
#   curl -fsSL https://raw.githubusercontent.com/erogluuefe/chargestrip/main/scripts/install-mac.sh | sh
#
# curl ile indirilen dosya tarayıcıların koyduğu karantina işaretini almaz; bu yüzden
# "Apple doğrulayamadı" uyarısı çıkmaz. Sonraki sürümler uygulamanın "Güncelle" menüsünden gelir.
set -eu

API=https://api.github.com/repos/erogluuefe/chargestrip/releases/latest
URL=$(curl -fsSL "$API" | grep -o '"browser_download_url": *"[^"]*\.dmg"' | sed 's/.*"\(https[^"]*\)"$/\1/')
[ -n "$URL" ] || { echo "Son sürümde .dmg bulunamadı." >&2; exit 1; }

TMP=$(mktemp -d)
trap 'hdiutil detach -quiet "$TMP/mnt" 2>/dev/null || true; rm -rf "$TMP"' EXIT

echo "İndiriliyor: $URL"
curl -fsSL -o "$TMP/chargestrip.dmg" "$URL"
mkdir "$TMP/mnt"
hdiutil attach -nobrowse -quiet -mountpoint "$TMP/mnt" "$TMP/chargestrip.dmg"

pkill -x chargestrip 2>/dev/null || true # açıksa kapat ki dosya değiştirilebilsin
rm -rf /Applications/Chargestrip.app
ditto "$TMP/mnt/Chargestrip.app" /Applications/Chargestrip.app
xattr -dr com.apple.quarantine /Applications/Chargestrip.app 2>/dev/null || true # eski kurulumdan kalmışsa

open /Applications/Chargestrip.app
echo "Chargestrip kuruldu; menü çubuğunda görünür."

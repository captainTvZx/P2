#!/bin/bash
# P2 Client — macOS paketi (P2.app + P2-Client-<sürüm>-mac.dmg). Bir Mac'te çalıştırılır
# (Xcode Command Line Tools + rustup gerekir). GPU arayüzü Metal (GPUI macOS katmanı).
#
#   bash packaging/build-macos.sh                 → evrensel ikili (Apple Silicon + Intel), imza: ad-hoc
#   bash packaging/build-macos.sh --arch arm64    → yalnız Apple Silicon (daha hızlı derleme)
#   P2_MAC_SIGN="Developer ID Application: ..." bash packaging/build-macos.sh   → Developer ID ile imzala
#   P2_MAC_NOTARY_PROFILE=<keychain profili> ...  → imzadan sonra notarize + staple (xcrun notarytool)
#
# Çıktı: $CARGO_TARGET_DIR/dist/macos/{P2.app, P2-Client-<sürüm>-mac.dmg}
# Ad-hoc imzalı paket başka Mac'lerde ilk açılışta "doğrulanamayan geliştirici" uyarısı verir
# (sağ tık → Aç). Dağıtım için Developer ID + notarization gerekir.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$(pwd)"
ARCH="universal"
[ "${1:-}" = "--arch" ] && ARCH="${2:-universal}"

VERSION="${P2_PRODUCT_VERSION:-$(awk '/^\[workspace.package\]/{f=1} f && /^version *=/{gsub(/[" ]/,"",$0); split($0,a,"="); print a[2]; exit}' Cargo.toml)}"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/target}"
OUT="$TARGET_DIR/dist/macos"
APP="$OUT/P2.app"
export MACOSX_DEPLOYMENT_TARGET=11.0
export P2_PRODUCT_VERSION="$VERSION"

case "$ARCH" in
  universal) TRIPLES="aarch64-apple-darwin x86_64-apple-darwin" ;;
  arm64) TRIPLES="aarch64-apple-darwin" ;;
  x86_64) TRIPLES="x86_64-apple-darwin" ;;
  *) echo "bilinmeyen mimari: $ARCH (universal|arm64|x86_64)"; exit 1 ;;
esac

echo "P2 Client $VERSION · macOS ($ARCH)"
BINS=()
for t in $TRIPLES; do
  rustup target add "$t" >/dev/null
  echo "cargo build --release --target $t -p p2-client"
  cargo build --release --target "$t" --target-dir "$TARGET_DIR" -p p2-client
  BINS+=("$TARGET_DIR/$t/release/p2-client")
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
if [ ${#BINS[@]} -gt 1 ]; then
  lipo -create -output "$APP/Contents/MacOS/P2" "${BINS[@]}"
else
  cp "${BINS[0]}" "$APP/Contents/MacOS/P2"
fi
sed "s/{{VERSION}}/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# Simge: assets/icon.png → P2.icns (tüm Retina boyutları).
ICONSET="$OUT/P2.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/P2.icns"
rm -rf "$ICONSET"

# İmza: Developer ID verilmişse sertleştirilmiş çalışma zamanıyla, yoksa ad-hoc ("-").
SIGN="${P2_MAC_SIGN:--}"
if [ "$SIGN" = "-" ]; then
  codesign --force --deep --sign - "$APP"
else
  codesign --force --deep --options runtime --timestamp --entitlements packaging/macos/entitlements.plist --sign "$SIGN" "$APP"
fi
codesign --verify --deep --strict "$APP"

DMG="$OUT/P2-Client-$VERSION-mac.dmg"
STAGE="$OUT/dmg"
rm -rf "$STAGE" "$DMG" && mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "P2 $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
[ "$SIGN" != "-" ] && codesign --sign "$SIGN" --timestamp "$DMG"

if [ -n "${P2_MAC_NOTARY_PROFILE:-}" ] && [ "$SIGN" != "-" ]; then
  xcrun notarytool submit "$DMG" --keychain-profile "$P2_MAC_NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

echo
echo "Tamam: $APP"
echo "       $DMG ($(du -h "$DMG" | cut -f1))"
echo "Deneme: open \"$APP\""

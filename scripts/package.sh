#!/usr/bin/env bash
# Package Kestrel: tar.gz release + .deb package.
set -euo pipefail

BIN="${1:-target/release/kestrel}"
PACKAGING_DIR="${2:-packaging}"
OUT="${3:-dist}"
mkdir -p "$OUT"

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')"
echo "packaging Kestrel v$VERSION"

# ---------- tar.gz ----------
STAGE="$OUT/kestrel-$VERSION"
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp "$BIN" "$STAGE/kestrel"
cp README.md "$STAGE/" 2>/dev/null || true
cp LICENSE "$STAGE/" 2>/dev/null || true
cp "$PACKAGING_DIR/io.kestrel.Browser.desktop" "$STAGE/"
strip "$STAGE/kestrel" 2>/dev/null || true
tar -czf "$OUT/kestrel-$VERSION-linux-amd64.tar.gz" -C "$OUT" "kestrel-$VERSION"
rm -rf "$STAGE"

# ---------- .deb ----------
DEBROOT="$OUT/debroot"
rm -rf "$DEBROOT"
mkdir -p "$DEBROOT/DEBIAN" \
         "$DEBROOT/usr/bin" \
         "$DEBROOT/usr/share/applications" \
         "$DEBROOT/usr/share/icons/hicolor/256x256/apps" \
         "$DEBROOT/usr/share/icons/hicolor/scalable/apps"
cp "$BIN" "$DEBROOT/usr/bin/kestrel"
strip "$DEBROOT/usr/bin/kestrel" 2>/dev/null || true
cp "$PACKAGING_DIR/debian/control" "$DEBROOT/DEBIAN/control"
cp "$PACKAGING_DIR/io.kestrel.Browser.desktop" "$DEBROOT/usr/share/applications/io.kestrel.Browser.desktop"
ASSETS="$PACKAGING_DIR/../assets/icons"
if [ -f "$ASSETS/io.kestrel.Browser.png" ]; then
  cp "$ASSETS/io.kestrel.Browser.png" "$DEBROOT/usr/share/icons/hicolor/256x256/apps/"
fi
if [ -f "$ASSETS/io.kestrel.Browser.svg" ]; then
  cp "$ASSETS/io.kestrel.Browser.svg" "$DEBROOT/usr/share/icons/hicolor/scalable/apps/"
fi
dpkg-deb --build --root-owner-group "$DEBROOT" "$OUT/kestrel-$VERSION-amd64.deb"
rm -rf "$DEBROOT"

echo "== dist =="
ls -la "$OUT"

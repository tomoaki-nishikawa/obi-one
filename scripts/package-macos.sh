#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/obi-one.app}"
DIST_DIR="${2:-$ROOT_DIR/dist/macos}"
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS packaging requires macOS." >&2
  exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH" >&2
  exit 1
fi

PACKAGE_STAGE="$(mktemp -d)"
trap 'rm -rf "$PACKAGE_STAGE"' EXIT
CLEAN_APP="$PACKAGE_STAGE/obi-one.app"
DMG_ROOT="$PACKAGE_STAGE/dmg-root"
EXECUTABLE="$CLEAN_APP/Contents/MacOS/obi-one"
PDFIUM="$CLEAN_APP/Contents/Resources/resources/pdfium/libpdfium.dylib"

# Sign a metadata-free copy. Synced folders can attach Finder/resource-fork
# attributes that codesign correctly rejects.
ditto --norsrc --noextattr --noqtn --noacl "$APP_PATH" "$CLEAN_APP"
xattr -cr "$CLEAN_APP"
codesign --force --sign - "$EXECUTABLE"
codesign --force --sign - "$PDFIUM"
codesign --force --sign - "$CLEAN_APP"

lipo "$EXECUTABLE" -verify_arch arm64 x86_64
lipo "$PDFIUM" -verify_arch arm64 x86_64
codesign --verify --strict "$PDFIUM"
codesign --verify --deep --strict "$CLEAN_APP"
APP_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$CLEAN_APP/Contents/Info.plist")"

mkdir -p "$DIST_DIR"
ditto -c -k --norsrc --noextattr --noqtn --keepParent \
  "$CLEAN_APP" \
  "$DIST_DIR/obi-one-macos-universal.zip"

mkdir -p "$DMG_ROOT"
ditto --norsrc --noextattr --noqtn --noacl "$CLEAN_APP" "$DMG_ROOT/obi-one.app"
ln -s /Applications "$DMG_ROOT/Applications"
hdiutil create \
  -volname "obi-one" \
  -srcfolder "$DMG_ROOT" \
  -ov \
  -format UDZO \
  "$DIST_DIR/obi-one_${APP_VERSION}_universal.dmg"

cp "$ROOT_DIR/docs/macos-installation.md" "$DIST_DIR/README-macOS.md"

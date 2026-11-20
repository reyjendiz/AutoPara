#!/usr/bin/env bash
# Build native macOS Apple Silicon (M1/M2/M3/M4) AutoPara.app and drag-and-drop DMG.
# Usage:
#   chmod +x scripts/build_macos_arm64.sh
#   ./scripts/build_macos_arm64.sh
# Optional env:
#   AUTOPARA_GITHUB_REPO="owner/repo" ./scripts/build_macos_arm64.sh

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: this script must be run on macOS." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' crates/autopara-app/Cargo.toml | head -1)"
if [[ -z "$VERSION" ]]; then
  echo "Error: could not read version from crates/autopara-app/Cargo.toml" >&2
  exit 1
fi

TARGET="aarch64-apple-darwin"
APP_NAME="AutoPara"
APP_DIR="$ROOT/dist/${APP_NAME}.app"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"
DMG_PATH="$ROOT/dist/${DMG_NAME}"

echo "==> 1. Ensuring Rust target $TARGET is installed..."
rustup target add "$TARGET" >/dev/null 2>&1 || true

echo "==> 2. Building pure Rust release binary ($TARGET)..."
cargo build --release --target "$TARGET" -p autopara

BIN_PATH="$ROOT/target/$TARGET/release/autopara"
if [[ ! -f "$BIN_PATH" ]]; then
  echo "Error: binary not found at $BIN_PATH" >&2
  exit 1
fi

echo "==> 3. Generating Retina macOS icon (.icns) from assets/logo.png..."
ICONSET="$ROOT/build/AutoPara.iconset"
ICNS_PATH="$ROOT/build/AutoPara.icns"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"

for sz in 16 32 128 256 512; do
  sips -z "$sz" "$sz" "$ROOT/assets/logo.png" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null
  dbl=$((sz * 2))
  sips -z "$dbl" "$dbl" "$ROOT/assets/logo.png" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$ICNS_PATH"

echo "==> 4. Assembling $APP_DIR..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/$APP_NAME"
chmod +x "$APP_DIR/Contents/MacOS/$APP_NAME"
cp "$ICNS_PATH" "$APP_DIR/Contents/Resources/AutoPara.icns"

cat > "$APP_DIR/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>uk</string>
    <key>CFBundleDisplayName</key>
    <string>AutoPara</string>
    <key>CFBundleExecutable</key>
    <string>AutoPara</string>
    <key>CFBundleIconFile</key>
    <string>AutoPara.icns</string>
    <key>CFBundleIdentifier</key>
    <string>com.autopara.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>AutoPara</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
EOF

echo "==> 5. Ad-hoc signing for Apple Silicon..."
codesign --force --deep --sign - "$APP_DIR"

echo "==> 6. Creating drag-and-drop DMG ($DMG_PATH)..."
STAGING="$ROOT/build/dmg_staging"
rm -rf "$STAGING" "$DMG_PATH"
mkdir -p "$STAGING"
cp -R "$APP_DIR" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$STAGING" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null

echo ""
echo "Done:"
echo "  App: $APP_DIR"
echo "  DMG: $DMG_PATH ($(du -h "$DMG_PATH" | awk '{print $1}'))"

#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this packaging script only runs on macOS" >&2
  exit 1
fi

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool: $tool" >&2
    exit 1
  fi
}

require_tool cargo
require_tool sips
require_tool iconutil
require_tool hdiutil
require_tool plutil
require_tool awk

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT_DIR/target"
PACKAGE_DIR="$TARGET_DIR/macos-package"
APP_NAME="AIWallpaper"
BINARY_NAME="ai-wallpaper"
BUNDLE_ID="com.liyulingyue.aiwallpaper"
ICON_SOURCE="$ROOT_DIR/assets/app_icon.png"

if [[ ! -f "$ICON_SOURCE" ]]; then
  echo "error: missing icon source: $ICON_SOURCE" >&2
  exit 1
fi

VERSION="$(
  awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$ROOT_DIR/Cargo.toml"
)"

if [[ -z "$VERSION" ]]; then
  echo "error: failed to read package version from Cargo.toml" >&2
  exit 1
fi

ARCH="$(uname -m)"
case "$ARCH" in
  arm64|x86_64) ;;
  *)
    echo "warning: unrecognized architecture '$ARCH', using raw uname output" >&2
    ;;
esac

RELEASE_BINARY="$TARGET_DIR/release/$BINARY_NAME"
APP_DIR="$PACKAGE_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
ICONSET_DIR="$PACKAGE_DIR/AppIcon.iconset"
DMG_STAGING_DIR="$PACKAGE_DIR/dmg-root"
DMG_NAME="$APP_NAME-$VERSION-macos-$ARCH.dmg"
DMG_PATH="$TARGET_DIR/release/$DMG_NAME"

generate_icns_fallback() {
  local output_path="$1"

  if [[ ! -x /usr/bin/python3 ]]; then
    echo "error: iconutil rejected the iconset and /usr/bin/python3 is unavailable for fallback icns generation" >&2
    exit 1
  fi

  /usr/bin/python3 - "$ICONSET_DIR" "$output_path" <<'PY'
import struct
import sys
from pathlib import Path

iconset_dir = Path(sys.argv[1])
output_path = Path(sys.argv[2])
entries = [
    ("icp4", iconset_dir / "icon_16x16.png"),
    ("icp5", iconset_dir / "icon_16x16@2x.png"),
    ("icp6", iconset_dir / "icon_32x32@2x.png"),
    ("ic07", iconset_dir / "icon_128x128.png"),
    ("ic08", iconset_dir / "icon_128x128@2x.png"),
    ("ic09", iconset_dir / "icon_256x256@2x.png"),
    ("ic10", iconset_dir / "icon_512x512@2x.png"),
]

chunks = []
for icon_type, path in entries:
    data = path.read_bytes()
    chunks.append(icon_type.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

payload = b"".join(chunks)
output_path.write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)
PY
}

echo "==> Building release binary"
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

if [[ ! -f "$RELEASE_BINARY" ]]; then
  echo "error: expected release binary not found: $RELEASE_BINARY" >&2
  exit 1
fi

echo "==> Preparing app bundle structure"
rm -rf "$APP_DIR" "$ICONSET_DIR" "$DMG_STAGING_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$ICONSET_DIR" "$DMG_STAGING_DIR"

echo "==> Generating AppIcon.icns"
for size in 16 32 128 256 512; do
  sips -s format png -z "$size" "$size" "$ICON_SOURCE" --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null
  doubled_size=$((size * 2))
  sips -s format png -z "$doubled_size" "$doubled_size" "$ICON_SOURCE" --out "$ICONSET_DIR/icon_${size}x${size}@2x.png" >/dev/null
done

ICONUTIL_LOG="$PACKAGE_DIR/iconutil.log"
if ! iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/AppIcon.icns" 2>"$ICONUTIL_LOG"; then
  echo "warning: iconutil rejected the generated iconset, using fallback icns assembly" >&2
  generate_icns_fallback "$RESOURCES_DIR/AppIcon.icns"
fi
rm -f "$ICONUTIL_LOG"

echo "==> Creating Info.plist"
cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleExecutable</key>
  <string>$BINARY_NAME</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

echo "==> Copying binary into app bundle"
cp "$RELEASE_BINARY" "$MACOS_DIR/$BINARY_NAME"
chmod 755 "$MACOS_DIR/$BINARY_NAME"
plutil -lint "$CONTENTS_DIR/Info.plist" >/dev/null

echo "==> Assembling DMG contents"
cp -R "$APP_DIR" "$DMG_STAGING_DIR/"
ln -sfn /Applications "$DMG_STAGING_DIR/Applications"
rm -f "$DMG_PATH"

echo "==> Creating DMG"
hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$DMG_STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null

echo "App bundle: $APP_DIR"
echo "DMG package: $DMG_PATH"

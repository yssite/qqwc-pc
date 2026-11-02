#!/bin/bash
# 在无 GUI 会话（SSH）下制作 macOS dmg：
#   1) ad-hoc 签名（Apple Silicon 上未签名的二进制无法执行）
#   2) hdiutil 直接封装（避开 Tauri 的 Finder AppleScript）
set -e
export PATH="$HOME/.cargo/bin:$HOME/.local/node/bin:$PATH"

APP_DIR="$HOME/gst/app/src-tauri/target/aarch64-apple-darwin/release/bundle/macos"
APP="$APP_DIR/GlobalSpeedTest.app"
OUT_DIR="$HOME/gst/out"
VER="1.2.2"
STAGE="$HOME/gst/stage"

echo "=== 1. 检查 .app"
[ -d "$APP" ] || { echo "缺少 $APP"; exit 1; }
file "$APP/Contents/MacOS/GlobalSpeedTest" | sed 's/^/    /'

echo "=== 2. ad-hoc 签名"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -3

echo "=== 3. 制作 dmg"
rm -rf "$STAGE" "$OUT_DIR"
mkdir -p "$STAGE" "$OUT_DIR"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "全球网测" -srcfolder "$STAGE" -ov -format UDZO \
  "$OUT_DIR/GlobalSpeedTest_${VER}_aarch64.dmg" -quiet

echo "=== 4. 校验"
ls -lh "$OUT_DIR"/*.dmg
hdiutil attach "$OUT_DIR/GlobalSpeedTest_${VER}_aarch64.dmg" -nobrowse -quiet -mountpoint /tmp/gstmnt
ls /tmp/gstmnt
codesign --verify --deep --strict /tmp/gstmnt/GlobalSpeedTest.app && echo "    签名校验通过"
hdiutil detach /tmp/gstmnt -quiet
echo "DMG_OK"

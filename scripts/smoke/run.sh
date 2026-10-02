#!/usr/bin/env bash
# Real-app smoke test on Linux: builds the actual Tauri app, runs it on a
# virtual display (Xvfb) under tauri-driver (WebDriver) and drives the UI with
# scripts/smoke/smoke.mjs against live u.gg / Data Dragon data. League is not
# needed (and not running). See docs/TESTING.md.
#
# Needs: xvfb, imagemagick (import), webkit2gtk-driver (WebKitWebDriver),
#        tauri-driver (cargo install tauri-driver --locked), node, internet.
# Env:   SKIP_BUILD=1  reuse src-tauri/target/debug/champ-select-helper
#        SHOTS=dir     screenshots (default docs/screenshots)
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

for tool in Xvfb import WebKitWebDriver tauri-driver node; do
  command -v "$tool" >/dev/null || { echo "missing: $tool (see docs/TESTING.md)"; exit 2; }
done

if [ "${SKIP_BUILD:-0}" != 1 ]; then
  npm run tauri build -- --debug --no-bundle
fi
APP="$ROOT/src-tauri/target/debug/champ-select-helper"
SHOTS="${SHOTS:-$ROOT/docs/screenshots}"
LOG="${LOG:-$ROOT/src-tauri/target/smoke-app.log}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/com.bsieben9.champselecthelper"

# Start from default settings; put the user's settings back afterwards.
mkdir -p "$CONFIG_DIR"
if [ -f "$CONFIG_DIR/settings.json" ]; then
  mv "$CONFIG_DIR/settings.json" "$CONFIG_DIR/settings.json.smoke-backup"
fi

DISPLAY_NUM=:${SMOKE_DISPLAY:-97}
Xvfb "$DISPLAY_NUM" -screen 0 1280x900x24 -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
cleanup() {
  kill "${DRIVER:-}" 2>/dev/null || true
  pkill -x WebKitWebDriver 2>/dev/null || true
  kill "$XVFB" 2>/dev/null || true
  rm -f "$CONFIG_DIR/settings.json"
  if [ -f "$CONFIG_DIR/settings.json.smoke-backup" ]; then
    mv "$CONFIG_DIR/settings.json.smoke-backup" "$CONFIG_DIR/settings.json"
  fi
}
trap cleanup EXIT
sleep 1

# Software rendering so an X screenshot of the window shows the page too.
DISPLAY="$DISPLAY_NUM" WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  tauri-driver >"$LOG" 2>&1 &
DRIVER=$!
for _ in $(seq 50); do
  curl -s http://127.0.0.1:4444/status >/dev/null 2>&1 && break
  sleep 0.2
done

status=0
APP="$APP" SHOTS="$SHOTS" CONFIG_DIR="$CONFIG_DIR" XSHOT_DISPLAY="$DISPLAY_NUM" \
  node scripts/smoke/smoke.mjs || status=$?

echo
echo "--- app/driver log ($LOG): errors and warnings ---"
# (tauri-driver logs "Error serving connection" + a backtrace while
# WebKitWebDriver is still starting, and Mesa warns about DRI3 under Xvfb.)
grep -iE "panick|error|warn|\[watcher\]|ugg:|http_cache:|ddragon:" "$LOG" \
  | grep -vE "Error serving connection|DRI3|^\s+[0-9]+: |^\s+at " | head -40 || true
exit $status

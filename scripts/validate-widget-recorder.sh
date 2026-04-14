#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "[validate-widget-recorder] verify widget macOS config"
cd "$ROOT_DIR"
node <<'EOF'
const fs = require("fs");
const config = JSON.parse(fs.readFileSync("apps/desktop/src-tauri/tauri.conf.json", "utf8"));
const ballWindow = config.app?.windows?.find((window) => window.label === "ball");

if (!ballWindow) {
  throw new Error("ball window config is missing");
}
if (ballWindow.acceptFirstMouse !== true) {
  throw new Error("ball window must enable acceptFirstMouse so the first click triggers Record");
}
if (ballWindow.focus !== true) {
  throw new Error("ball window must stay focusable so the widget button can activate");
}
if (config.bundle?.macOS?.infoPlist !== "Info.plist") {
  throw new Error("bundle.macOS.infoPlist must point to src-tauri/Info.plist");
}
EOF
grep -q "<key>NSMicrophoneUsageDescription</key>" "$ROOT_DIR/apps/desktop/src-tauri/Info.plist"

echo "[validate-widget-recorder] cargo test -p talkiwi-track"
cargo test -p talkiwi-track

echo "[validate-widget-recorder] cargo test -p talkiwi-desktop session_manager"
cargo test -p talkiwi-desktop session_manager

echo "[validate-widget-recorder] npm test"
cd "$ROOT_DIR/apps/desktop"
npm test

echo "[validate-widget-recorder] npm run build"
npm run build

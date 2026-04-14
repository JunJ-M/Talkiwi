#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "[validate-widget-recorder] cargo test -p talkiwi-track"
cargo test -p talkiwi-track

echo "[validate-widget-recorder] cargo test -p talkiwi-desktop session_manager"
cargo test -p talkiwi-desktop session_manager

echo "[validate-widget-recorder] npm test"
cd "$ROOT_DIR/apps/desktop"
npm test

echo "[validate-widget-recorder] npm run build"
npm run build

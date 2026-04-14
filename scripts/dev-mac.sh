#!/usr/bin/env bash
#
# Talkiwi desktop dev launcher for macOS.
#
# Why this script exists:
#   `npm run tauri dev` invokes cargo, which relinks the debug binary with
#   a content-hash-derived ad-hoc codesign identifier (e.g.
#   `talkiwi_desktop-<hash>`). Every rebuild mutates the identifier, so
#   macOS TCC treats the app as a new unknown binary and the user has to
#   re-grant mic access after each change. This script pins the identifier
#   to `com.talkiwi.app` so TCC keeps your grant across rebuilds.
#
# What it does:
#   1. Kills any stale talkiwi-desktop / vite processes from previous runs
#   2. Starts the vite dev server in the background
#   3. cargo build -p talkiwi-desktop --features whisper
#   4. codesign --force --sign - --identifier com.talkiwi.app
#      (stable identifier, Info.plist gets bound into the signature)
#   5. Launches the signed binary with tracing enabled by default
#   6. On exit (Ctrl+C), tears down vite cleanly
#
# Usage:
#   ./scripts/dev-mac.sh                         # defaults, tracing on
#   RUST_LOG="info" ./scripts/dev-mac.sh         # override log filter
#   ./scripts/dev-mac.sh --no-vite               # skip starting vite
#                                                # (e.g. when running
#                                                # vite in another term)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BIN_PATH="$REPO_ROOT/target/debug/talkiwi-desktop"
VITE_DIR="$REPO_ROOT/apps/desktop"
STABLE_ID="com.talkiwi.app"

START_VITE=1
for arg in "$@"; do
    case "$arg" in
        --no-vite) START_VITE=0 ;;
        *) ;;
    esac
done

VITE_PID=""

cleanup() {
    if [[ -n "${VITE_PID}" ]] && kill -0 "$VITE_PID" 2>/dev/null; then
        echo "[dev-mac] stopping vite (pid $VITE_PID)..."
        kill "$VITE_PID" 2>/dev/null || true
        wait "$VITE_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# 1. Kill stale processes so we don't end up with multiple instances
#    fighting for the mic.
for name in "talkiwi-desktop" "vite"; do
    pids=$(pgrep -f "$name" || true)
    if [[ -n "$pids" ]]; then
        echo "[dev-mac] killing stale $name processes: $pids"
        kill $pids 2>/dev/null || true
        sleep 0.2
    fi
done

# 2. Vite dev server in background
if [[ "$START_VITE" == "1" ]]; then
    echo "[dev-mac] starting vite dev server..."
    (cd "$VITE_DIR" && npm run dev) &
    VITE_PID=$!
    echo "[dev-mac] vite pid=$VITE_PID — waiting for it to boot..."
    # Wait until vite is actually serving localhost:1420 or 3 seconds elapse.
    for _ in $(seq 1 30); do
        if curl -sf http://localhost:1420/ > /dev/null 2>&1; then
            echo "[dev-mac] vite ready."
            break
        fi
        sleep 0.1
    done
else
    echo "[dev-mac] --no-vite: assuming vite is already running on :1420"
fi

# 3. Build (incremental — skips if nothing changed)
echo "[dev-mac] cargo build -p talkiwi-desktop --features whisper"
cargo build -p talkiwi-desktop --features whisper

# 4. Re-sign with stable identifier so TCC keeps mic grant.
#    We drop stderr's "replacing existing signature" noise since it fires
#    every run and is expected.
echo "[dev-mac] codesign --identifier $STABLE_ID"
codesign --force --sign - --identifier "$STABLE_ID" "$BIN_PATH" 2>&1 \
    | grep -v "replacing existing signature" || true

# Sanity check: identifier must be stable + Info.plist must be bound.
sig_info=$(codesign -dvvv "$BIN_PATH" 2>&1)
if ! echo "$sig_info" | grep -q "Identifier=$STABLE_ID"; then
    echo "[dev-mac] ERROR: codesign identifier is NOT $STABLE_ID" >&2
    echo "$sig_info" >&2
    exit 1
fi
if ! echo "$sig_info" | grep -q "Info.plist entries"; then
    echo "[dev-mac] WARNING: Info.plist not bound to signature (TCC may not see permissions)" >&2
fi

# 5. Launch
export RUST_LOG="${RUST_LOG:-info,talkiwi_asr=debug,talkiwi_track=debug,talkiwi_desktop=debug}"
echo "[dev-mac] RUST_LOG=$RUST_LOG"
echo "[dev-mac] launching $BIN_PATH"
exec "$BIN_PATH"

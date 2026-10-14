#!/usr/bin/env bash
set -euo pipefail
MODE="${1:-run}"
case "$MODE" in run|--debug|--logs|--telemetry|--verify) ;; *) echo "Usage: $0 [--debug|--logs|--telemetry|--verify]" >&2; exit 2 ;; esac
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
pkill -x framecraft >/dev/null 2>&1 || true
npm run tauri -- build --debug --bundles app
APP_BUNDLE="$ROOT_DIR/src-tauri/target/debug/bundle/macos/Framecraft.app"
./script/sign_macos.sh "$APP_BUNDLE"
case "$MODE" in
 --debug) lldb -- "$APP_BUNDLE/Contents/MacOS/framecraft" ;;
 --logs|--telemetry) /usr/bin/open -n "$APP_BUNDLE"; /usr/bin/log stream --info --style compact --predicate 'process == "framecraft"' ;;
 --verify) /usr/bin/open -n "$APP_BUNDLE"; sleep 2; pgrep -x framecraft >/dev/null ;;
 run) /usr/bin/open -n "$APP_BUNDLE" ;;
esac

#!/usr/bin/env bash
# Fork-only (bendfeldt/meetily): build "Meetily Fork" as a .dmg on your Mac,
# the same way .github/workflows/fork-macos-dmg.yml does. The fork build has
# its own app id (com.meetily.fork), name and recordings folder, so it can be
# installed next to upstream Meetily.
#
#   scripts/fork-build-dmg.sh             release build
#   scripts/fork-build-dmg.sh --debug     debug build (faster, bigger)
#   scripts/fork-build-dmg.sh --install   release build, then copy the app to /Applications
#   scripts/fork-build-dmg.sh --dev       run the fork in development mode (hot reload)
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
TARGET=aarch64-apple-darwin
CONFIG="$ROOT/frontend/src-tauri/tauri.fork.conf.json"

PROFILE=release
BUILD_ARGS=()
INSTALL=false
DEV=false
for arg in "$@"; do
  case "$arg" in
    --debug) PROFILE=debug; BUILD_ARGS+=(--debug) ;;
    --install) INSTALL=true ;;
    --dev) DEV=true ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done
if $DEV && { $INSTALL || [ "$PROFILE" = debug ]; }; then
  echo "--dev cannot be combined with --install or --debug." >&2; exit 2
fi

[ "$(uname -s)" = Darwin ] || { echo "This script builds the macOS app; run it on a Mac." >&2; exit 1; }
[ "$(uname -m)" = arm64 ] || { echo "Only Apple Silicon (arm64) is supported, like upstream." >&2; exit 1; }
for tool in cargo pnpm node cmake; do
  command -v "$tool" > /dev/null || { echo "Missing '$tool'. See docs/BUILD_DMG.md, 'Option B'." >&2; exit 1; }
done
command -v rustup > /dev/null && rustup target add "$TARGET" > /dev/null

# The built-in summary model runs in this sidecar (bundled as externalBin).
echo "==> Building llama-helper sidecar (Metal)"
cargo build --release -p llama-helper --features metal
mkdir -p frontend/src-tauri/binaries
cp target/release/llama-helper "frontend/src-tauri/binaries/llama-helper-$TARGET"

echo "==> Installing UI dependencies"
(cd frontend && pnpm install --frozen-lockfile)

if $DEV; then
  # The dev UI server uses port 3118 (upstream's devUrl); only one dev
  # session (fork or upstream) can run at a time.
  if lsof -nP -iTCP:3118 -sTCP:LISTEN > /dev/null 2>&1; then
    echo "Port 3118 is in use (another dev session?). Stop it first." >&2; exit 1
  fi
  echo "==> Starting Meetily Fork in development mode (Ctrl+C to stop)"
  cd frontend
  exec pnpm tauri dev --config "$CONFIG"
fi

# beforeBuildCommand runs the Next.js export first; build.rs fetches FFmpeg.
echo "==> Building Meetily Fork ($PROFILE) — first build takes 15-30 min"
MARKER=$(mktemp)
(cd frontend && pnpm tauri build --target "$TARGET" --bundles app,dmg --config "$CONFIG" ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"})

# Only accept files written by this build (not a stale earlier one).
BUNDLE="$ROOT/target/$TARGET/$PROFILE/bundle"
DMG=$(find "$BUNDLE/dmg" -maxdepth 1 -name 'Meetily Fork_*.dmg' -newer "$MARKER" 2> /dev/null | head -1 || true)
APP="$BUNDLE/macos/Meetily Fork.app"
# The bundler rewrites Info.plist on every build, so its time proves freshness.
if [ -z "$DMG" ] || [ -z "$(find "$APP/Contents/Info.plist" -newer "$MARKER" 2> /dev/null)" ]; then
  echo "The build finished but produced no new .dmg/.app in $BUNDLE." >&2; rm -f "$MARKER"; exit 1
fi
rm -f "$MARKER"
echo
echo "DMG: $DMG"
echo "App: $APP"

if $INSTALL; then
  DEST="/Applications/$(basename "$APP")"
  # Never replace the app under a running (possibly recording) instance.
  if pgrep -f "$DEST/Contents/MacOS/" > /dev/null; then
    echo "Meetily Fork is running. Quit it (after any recording finishes), then run --install again." >&2; exit 1
  fi
  echo "==> Installing to $DEST"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
  xattr -dr com.apple.quarantine "$DEST" 2> /dev/null || true
  echo "Installed. Open it from Applications (or: open \"$DEST\")."
else
  open -R "$DMG"
fi

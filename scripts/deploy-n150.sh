#!/usr/bin/env bash
# Copies release builds, the perf script, test settings/saves and your game files to the N150
# test box (plan tasks 2 and 14). The box needs no build tools: only the runtime libraries
# (SDL3, mpg123, Mesa).
#
# usage: scripts/deploy-n150.sh [user@host] [remote dir]
# then:  ssh user@host '~/revc-bench/scripts/perf.sh --backend vulkan'   (also gl, gl3)
# The game window opens on the box's desktop session (WAYLAND_DISPLAY/DISPLAY from it).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
HOST=${1:-steam@192.168.20.239}
DIR=${2:-revc-bench}

(cd "$ROOT/rust" && cargo build --locked -q --release && CARGO_TARGET_DIR=target-wgpu cargo build --locked -q --release --features wgpu)
STAGE=$(mktemp -d); trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin" "$STAGE/scripts" "$STAGE/tests"
cp "$ROOT/rust/target/release/revc" "$STAGE/bin/revc-gl3"
cp "$ROOT/rust/target-wgpu/release/revc" "$STAGE/bin/revc-wgpu"
cp "$ROOT/scripts/perf.sh" "$STAGE/scripts/"
cp -r "$ROOT/tests/saves" "$ROOT/tests/config" "$STAGE/tests/"
git -C "$ROOT" rev-parse --short HEAD > "$STAGE/COMMIT"
rsync -a --delete "$STAGE/" "$HOST:$DIR/" --exclude game
# Game files: your own copy, never committed; skips user mods and settings.
rsync -a --delete --exclude mods --exclude userfiles --exclude reVC.ini "$ROOT/game/" "$HOST:$DIR/game/"
echo "deployed $(cat "$STAGE/COMMIT") to $HOST:$DIR"

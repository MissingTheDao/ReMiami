#!/usr/bin/env bash
# Frame-time baseline at the fixed spots (plan tasks 2 and 14).
#
# usage: scripts/perf.sh [--bin PATH] [--game DIR] [--size WxH] [--backend gl3|vulkan|gl] [--runs N]
#
# Runs REVC_TEST=spots with the test settings but VSync and the frame limiter off, and prints the
# `perf` lines (frame times over the 3 s before each spot's capture). Screenshots are not compared.
# Works from a repository checkout or from a deployed folder holding bin/, tests/ and game/
# (scripts/deploy-n150.sh). --backend vulkan|gl needs a wgpu build (--bin); gl3 a normal one.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="" GAME=$ROOT/game SIZE=1280x720 BACKEND=gl3 RUNS=1
while (( $# )); do
  case "$1" in
    --bin) BIN=$2; shift ;;
    --game) GAME=$2; shift ;;
    --size) SIZE=$2; shift ;;
    --backend) BACKEND=$2; shift ;;
    --runs) RUNS=$2; shift ;;
    *) echo "unknown option $1"; exit 2 ;;
  esac
  shift
done
if [[ -z "$BIN" ]]; then
  if [[ "$BACKEND" == gl3 ]]; then BIN=$ROOT/rust/target/release/revc; else BIN=$ROOT/rust/target-wgpu/release/revc; fi
  [[ -x "$BIN" ]] || BIN=$ROOT/bin/revc-$([[ "$BACKEND" == gl3 ]] && echo gl3 || echo wgpu)
fi
[[ -x "$BIN" ]] || { echo "no binary $BIN"; exit 1; }
W=${SIZE%x*} H=${SIZE#*x}

TMP=$(mktemp -d); T=$TMP/game; mkdir -p "$T/userfiles"
trap 'rm -rf "$TMP"' EXIT
for f in "$GAME"/*; do
  case "$(basename "$f")" in reVC.ini|userfiles|mods) ;; *) ln -s "$f" "$T/" ;; esac
done
cp "$ROOT"/tests/saves/*.b "$T/userfiles/"
printf 'width=%s\nheight=%s\nmaximized=0\n' "$W" "$H" > "$T/userfiles/revc_window.ini"
sed -e 's/^VSync=.*/VSync=0/' -e 's/^FrameLimiter=.*/FrameLimiter=0/' "$ROOT/tests/config/reVC.ini" > "$T/reVC.ini"

envs=(REVC_GAME_DIR="$T" REVC_TEST=spots)
[[ "$BACKEND" != gl3 ]] && envs+=(REVC_RENDERER_BACKEND="$BACKEND")
echo "== $(basename "$BIN") backend $BACKEND ${W}x${H}"
for ((i = 1; i <= RUNS; i++)); do
  log=$TMP/run$i.log
  env "${envs[@]}" "$BIN" > "$log" 2>&1
  rc=$?
  grep -E '^\[revc-test\] perf' "$log" | sed 's/^\[revc-test\] /   /'
  (( rc == 0 )) || { echo "   run $i failed (exit $rc)"; grep -E 'FAIL|panic|error' "$log" | head -5; }
done

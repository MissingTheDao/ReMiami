#!/usr/bin/env bash
# Regression tests: builds revc, runs each scenario in an isolated game folder, reports pass/fail.
# usage: scripts/test.sh [--bless] [--keep] [scenario ...]     (default scenarios: boot saveload)
#   --bless  accept the current screenshots as the new references (tests/reference/)
#   --keep   keep the temporary game folder
#   --wgpu   test the wgpu renderer build (cargo feature wgpu, rust/target-wgpu) against its own
#            references in tests/reference-wgpu/; REVC_RENDERER_BACKEND=vulkan|gl picks the backend
# Needs a display (the game opens a window). Game data: REVC_GAME_DIR or ./game.
# Scenarios: boot, saveload, formats, texmods, gamepad, spots (default), audio (scripted scene with sound; REVC_TEST_WAV=x.wav records it).
# formats: compares what the C++ loaders read with the Rust parsers in rust/revc-formats.
# texmods: saveload with the texture mods in tests/mods installed (own reference screenshot).
# gamepad: virtual SDL pads: detection, player 1/2 assignment, prompt glyphs per pad (screenshots vs
#   tests/reference/gamepad/), rumble.
# spots: screenshots at fixed places on the map (no peds/traffic, noon, sunny) vs tests/reference/spots/.
# A scenario uses tests/config/<scenario>.ini if present, else tests/config/reVC.ini.
# REVC_TEST_BIN=path runs that binary instead of building rust/.
# Output and logs: rust/target/test-output/
set -uo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BLESS=0; KEEP=0; WGPU=0; SCENARIOS=()
for a in "$@"; do
  case "$a" in
    --bless) BLESS=1 ;;
    --keep) KEEP=1 ;;
    --wgpu) WGPU=1 ;;
    -*) echo "unknown option $a" >&2; exit 2 ;;
    *) SCENARIOS+=("$a") ;;
  esac
done
(( ${#SCENARIOS[@]} )) || SCENARIOS=(boot saveload formats texmods gamepad spots)

GAME=${REVC_GAME_DIR:-$ROOT/game}
[[ -d "$GAME/models" ]] || { echo "game data not found at $GAME (set REVC_GAME_DIR)" >&2; exit 2; }
OUT=$ROOT/rust/target/test-output
mkdir -p "$OUT"
REF=$ROOT/tests/reference
(( WGPU )) && REF=$ROOT/tests/reference-wgpu
# The wgpu references come from Vulkan. The GL backend matches them within 0.01 % except where
# anisotropic filtering samples high-frequency textures at grazing angles (water sparkles, fences),
# which the two drivers do slightly differently (up to ~1 %): allow 1.5 % there.
if (( WGPU )) && [[ "${REVC_RENDERER_BACKEND:-vulkan}" == gl* ]] && [[ -z "${REVC_TEST_MAXDIFF:-}" ]]; then
  export REVC_TEST_MAXDIFF=0.015
  echo "(wgpu on GL: screenshot limit 1.5 %, see scripts/test.sh)"
fi

if [[ -n "${REVC_TEST_BIN:-}" ]]; then
  BIN=$REVC_TEST_BIN
else
  echo "== build"
  if (( WGPU )); then
    (cd "$ROOT/rust" && CARGO_TARGET_DIR=target-wgpu cargo build --locked -q --features wgpu) || { echo "build failed"; exit 1; }
    BIN=$ROOT/rust/target-wgpu/debug/revc
  else
    (cd "$ROOT/rust" && cargo build --locked -q) || { echo "build failed"; exit 1; }
    BIN=$ROOT/rust/target/debug/revc
  fi
fi

# Isolated game folder: symlink the game files, but use the test settings and saves.
TMP=$(mktemp -d); T=$TMP/game; mkdir -p "$T/userfiles"
for f in "$GAME"/* ; do
  # The user's own mods (game/mods) never take part: texmods installs tests/mods itself.
  case "$(basename "$f")" in reVC.ini|userfiles|mods) ;; *) ln -s "$f" "$T/" ;; esac
done
cp "$ROOT"/tests/saves/*.b "$T/userfiles/"
printf 'width=1280\nheight=720\nmaximized=0\n' > "$T/userfiles/revc_window.ini"
cleanup() { (( KEEP )) && echo "kept $T" || rm -rf "$TMP"; }
trap cleanup EXIT

failed=0
for s in "${SCENARIOS[@]}"; do
  log=$OUT/$s.log
  ini=$ROOT/tests/config/$s.ini; [[ -f "$ini" ]] || ini=$ROOT/tests/config/reVC.ini
  cp "$ini" "$T/reVC.ini"
  envs=(REVC_GAME_DIR="$T" REVC_TEST="$s")
  [[ "$s" == texmods ]] && cp -r "$ROOT/tests/mods" "$T/mods"
  if [[ "$s" == spots || "$s" == gamepad ]]; then
    # Screenshots are overwritten on every run.
    mkdir -p "$OUT/$s"
    envs+=(REVC_TEST_SHOT_DIR="$OUT/$s")
    (( BLESS == 0 )) && envs+=(REVC_TEST_REF_DIR="$REF/$s")
  fi
  [[ "$s" == boot ]] && envs+=(REVC_TEST_SHOT="$OUT/$s.png")
  if [[ "$s" == saveload || "$s" == texmods ]]; then
    envs+=(REVC_TEST_SHOT="$OUT/$s.png")
    ref=$REF/$s.png
    (( BLESS == 0 )) && [[ -f "$ref" ]] && envs+=(REVC_TEST_REF="$ref")
    (( BLESS == 0 )) && [[ ! -f "$ref" ]] && echo "     (no reference ${ref#$ROOT/}: screenshot not compared; --bless makes one)"
  fi
  printf '== %-10s ' "$s"
  env "${envs[@]}" "$BIN" > "$log" 2>&1
  rc=$?
  rm -rf "$T/mods"
  # A clean shutdown frees every pool; leftovers point at leaks or a broken exit path.
  leaks=$(grep -E '^\[DBG\]: (Peds|Vehicles|Objects|Buildings) left [1-9]' "$log" || true)
  if (( rc == 0 )) && [[ -z "$leaks" ]]; then
    echo "PASS"
  else
    echo "FAIL (exit $rc)"; failed=1
    grep -E '^\[revc-test\] FAIL|^revc:' "$log" | sed 's/^/     /'
    [[ -n "$leaks" ]] && echo "$leaks" | sed 's/^/     leak: /'
    echo "     log: $log"
  fi
  grep -oE '\[revc-test\] (player at|image diff|audio|formats:|texmods:|gamepad:|spot |[a-z0-9_]+: image diff).*' "$log" | sed 's/^\[revc-test\] /     /'
  if (( BLESS )) && [[ "$s" == spots || "$s" == gamepad ]]; then
    mkdir -p "$REF/$s"
    for f in "$OUT/$s"/*.png; do [[ "$f" == *.diff.png ]] || cp "$f" "$REF/$s/"; done
    echo "     blessed ${REF#$ROOT/}/$s/"
  fi
  if (( BLESS )) && [[ "$s" == saveload || "$s" == texmods ]] && [[ -f "$OUT/$s.png" ]]; then
    mkdir -p "$REF" && cp "$OUT/$s.png" "$REF/$s.png" && echo "     blessed ${REF#$ROOT/}/$s.png"
  fi
done
exit $failed

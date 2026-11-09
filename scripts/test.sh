#!/usr/bin/env bash
# Regression tests: builds revc, runs each scenario in an isolated game folder, reports pass/fail.
# usage: scripts/test.sh [--bless] [--keep] [scenario ...]     (default scenarios: boot saveload)
#   --bless  accept the current screenshots as the new references (tests/reference/)
#   --keep   keep the temporary game folder
# Needs a display (the game opens a window). Game data: REVC_GAME_DIR or ./game.
# Scenarios: boot, saveload, formats, texmods (default), audio (scripted scene with sound; REVC_TEST_WAV=x.wav records it).
# formats: compares what the C++ loaders read with the Rust parsers in rust/revc-formats.
# texmods: saveload with the texture mods in tests/mods installed (own reference screenshot).
# A scenario uses tests/config/<scenario>.ini if present, else tests/config/reVC.ini.
# REVC_TEST_BIN=path runs that binary instead of building rust/.
# Output and logs: rust/target/test-output/
set -uo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BLESS=0; KEEP=0; SCENARIOS=()
for a in "$@"; do
  case "$a" in
    --bless) BLESS=1 ;;
    --keep) KEEP=1 ;;
    -*) echo "unknown option $a" >&2; exit 2 ;;
    *) SCENARIOS+=("$a") ;;
  esac
done
(( ${#SCENARIOS[@]} )) || SCENARIOS=(boot saveload formats texmods)

GAME=${REVC_GAME_DIR:-$ROOT/game}
[[ -d "$GAME/models" ]] || { echo "game data not found at $GAME (set REVC_GAME_DIR)" >&2; exit 2; }
OUT=$ROOT/rust/target/test-output
mkdir -p "$OUT"

if [[ -n "${REVC_TEST_BIN:-}" ]]; then
  BIN=$REVC_TEST_BIN
else
  echo "== build"
  (cd "$ROOT/rust" && cargo build --locked -q) || { echo "build failed"; exit 1; }
  BIN=$ROOT/rust/target/debug/revc
fi

# Isolated game folder: symlink the game files, but use the test settings and saves.
TMP=$(mktemp -d); T=$TMP/game; mkdir -p "$T/userfiles"
for f in "$GAME"/* ; do
  case "$(basename "$f")" in reVC.ini|userfiles) ;; *) ln -s "$f" "$T/" ;; esac
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
  if [[ "$s" == saveload || "$s" == texmods ]]; then
    envs+=(REVC_TEST_SHOT="$OUT/$s.png")
    ref=$ROOT/tests/reference/$s.png
    (( BLESS == 0 )) && [[ -f "$ref" ]] && envs+=(REVC_TEST_REF="$ref")
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
  grep -oE '\[revc-test\] (player at|image diff|audio|formats:|texmods:).*' "$log" | sed 's/^\[revc-test\] /     /'
  if (( BLESS )) && [[ -f "$OUT/$s.png" ]]; then
    cp "$OUT/$s.png" "$ROOT/tests/reference/$s.png" && echo "     blessed tests/reference/$s.png"
  fi
done
exit $failed

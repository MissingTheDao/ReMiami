#!/usr/bin/env bash
# Regression tests: builds revc, runs each scenario in an isolated game folder, reports pass/fail.
# usage: scripts/test.sh [--bless] [--keep] [scenario ...]     (default scenarios: boot saveload)
#   --bless  accept the current screenshots as the new references (tests/reference/)
#   --keep   keep the temporary game folder
#   --wgpu   (accepted, no effect: the wgpu renderer is the only one; REVC_RENDERER_BACKEND=vulkan|gl
#            picks its backend, Vulkan by default)
# Needs a display (the game opens a window). Game data: REVC_GAME_DIR or ./game.
# Scenarios: boot, saveload, savegame (save slot 1's game to slot 2, then load slot 2: own reference), formats, texmods, gamepad, spots (default), motion, pedhit (car hits peds at 320 fps: drawn where the physics has them), audio (scripted scene with sound; REVC_TEST_WAV=x.wav records it).
# formats: compares what the C++ loaders read with the Rust parsers in rust/revc-formats.
# oldsave: saveload of tests/saves/other/GTAVCsf1-64bit-gcc.b (slot 1's game as a 64-bit GCC build
#   saves it) in slot 3: the game converts it (FixSave, keeps a .bak) and loads it; saveload's reference.
# texmods: saveload with the texture mods in tests/mods installed (own reference screenshot).
# gamepad: virtual SDL pads: detection, player 1/2 assignment, prompt glyphs per pad (screenshots vs
#   tests/reference/gamepad/), rumble.
# spots: screenshots at fixed places on the map (no peds/traffic, noon, sunny) vs tests/reference/spots/.
# motion: walks and turns at a fixed 144 fps with render interpolation, checks the player moves
#   smoothly (jitter of body and bones), and makes rust/target/test-output/motion.mp4 to watch.
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
(( ${#SCENARIOS[@]} )) || SCENARIOS=(boot saveload savegame oldsave formats texmods gamepad spots motion pedhit)

GAME=${REVC_GAME_DIR:-$ROOT/game}
[[ -d "$GAME/models" ]] || { echo "game data not found at $GAME (set REVC_GAME_DIR)" >&2; exit 2; }
OUT=$ROOT/rust/target/test-output
mkdir -p "$OUT"
REF=$ROOT/tests/reference
# The wgpu references come from Vulkan. The GL backend matches them within 0.01 % except where
# anisotropic filtering samples high-frequency textures at grazing angles (water sparkles, fences),
# which the two drivers do slightly differently (up to ~1 %): allow 1.5 % there.
if [[ "${REVC_RENDERER_BACKEND:-vulkan}" == gl* ]] && [[ -z "${REVC_TEST_MAXDIFF:-}" ]]; then
  export REVC_TEST_MAXDIFF=0.015
  echo "(wgpu on GL: screenshot limit 1.5 %, see scripts/test.sh)"
fi

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
  # formats also loads every game texture and checks it against librw's recorded result.
  [[ "$s" == formats ]] && envs+=(REVC_TXD_GOLDEN="$ROOT/tests/golden/textures.txt")
  # pedhit: a car hits peds; at a high display rate (many drawn frames per 30 Hz step) the peds
  # must be drawn where the physics has them.
  [[ "$s" == pedhit ]] && envs+=(REVC_FIXED_FPS=320 REVC_TEST_PEDHIT_KMH=50)
  if [[ "$s" == oldsave ]]; then
    rm -f "$T"/userfiles/GTAVCsf3.b*
    cp "$ROOT/tests/saves/other/GTAVCsf1-64bit-gcc.b" "$T/userfiles/GTAVCsf3.b"
    envs=(REVC_GAME_DIR="$T" REVC_TEST=saveload REVC_TEST_SLOT=3 REVC_TEST_SHOT="$OUT/$s.png")
    (( BLESS == 0 )) && [[ -f "$REF/saveload.png" ]] && envs+=(REVC_TEST_REF="$REF/saveload.png")
  fi
  if [[ "$s" == spots || "$s" == gamepad ]]; then
    # Screenshots are overwritten on every run.
    mkdir -p "$OUT/$s"
    envs+=(REVC_TEST_SHOT_DIR="$OUT/$s")
    (( BLESS == 0 )) && envs+=(REVC_TEST_REF_DIR="$REF/$s")
  fi
  # Real controllers plugged in would take player slots from the virtual pads: SDL ignores every
  # pad except the virtual ones' models (DualSense, Steam Deck).
  [[ "$s" == gamepad ]] && envs+=(SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT=0x054c/0x0ce6,0x28de/0x1205)
  [[ "$s" == boot ]] && envs+=(REVC_TEST_SHOT="$OUT/$s.png")
  if [[ "$s" == motion ]]; then
    rm -rf "$OUT/motion-frames"; mkdir -p "$OUT/motion-frames"
    envs+=(REVC_FIXED_FPS=144 REVC_TEST_VIDEO_DIR="$OUT/motion-frames")
  fi
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
  if [[ "$s" == oldsave ]] && (( rc == 0 )); then
    if ls "$T"/userfiles/GTAVCsf3.b.*.bak >/dev/null 2>&1 && ! cmp -s "$T/userfiles/GTAVCsf3.b" "$ROOT/tests/saves/other/GTAVCsf1-64bit-gcc.b"; then
      echo "     converted: slot 3 rewritten, original kept as .bak"
    else
      echo "     FAIL: slot 3 was not converted"; failed=1
    fi
  fi
  grep -oE '\[revc-test\] (player at|loaded:|image diff|audio|formats:|textures:|pedhit: as|texmods:|gamepad:|savegame:|motion:|spot |[a-z0-9_]+: image diff).*' "$log" | sed 's/^\[revc-test\] /     /'
  # The captured frames as a video (144 fps, as rendered) to watch the motion.
  if [[ "$s" == motion ]] && ls "$OUT/motion-frames"/frame_*.png >/dev/null 2>&1; then
    if command -v ffmpeg >/dev/null; then
      ffmpeg -loglevel error -y -framerate 144 -i "$OUT/motion-frames/frame_%04d.png" -c:v libx264 -pix_fmt yuv420p -crf 20 "$OUT/motion.mp4" \
        && echo "     video: ${OUT#$ROOT/}/motion.mp4"
    else
      echo "     (ffmpeg not found: frames are in ${OUT#$ROOT/}/motion-frames/)"
    fi
  fi
  # savegame, part 2: load the slot just saved and compare its screenshot.
  if [[ "$s" == savegame ]] && (( rc == 0 )); then
    log2=$OUT/savegame-load.log
    envs=(REVC_GAME_DIR="$T" REVC_TEST=saveload REVC_TEST_SLOT=2 REVC_TEST_SHOT="$OUT/savegame.png")
    (( BLESS == 0 )) && [[ -f "$REF/savegame.png" ]] && envs+=(REVC_TEST_REF="$REF/savegame.png")
    printf '== %-10s ' "(load it)"
    if env "${envs[@]}" "$BIN" > "$log2" 2>&1; then echo PASS; else echo "FAIL"; failed=1; grep -E '^\[revc-test\] FAIL' "$log2" | sed 's/^/     /'; fi
    grep -oE '\[revc-test\] (player at|loaded:|image diff).*' "$log2" | sed 's/^\[revc-test\] /     /'
    (( BLESS )) && [[ -f "$OUT/savegame.png" ]] && cp "$OUT/savegame.png" "$REF/savegame.png" && echo "     blessed ${REF#$ROOT/}/savegame.png"
  fi
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

#!/usr/bin/env bash
# Menu comparison shots (plan task 13): every menu page in the PS2 and Xbox UI styles, at the
# consoles' sizes (PS2 640x448: shot at 640x480 and scaled, Xbox 640x480) and at 1440x1080 (<set>_<id>_hi.png), from a loaded
# game with the pause menu open (REVC_TEST=menus, rust/revc/src/testdrive.rs MENU_SHOTS_*).
# usage: scripts/menu-shots.sh [--clips] [out dir] [ps2|xbox|ps2_endgame|xbox_endgame ...]
#   *_endgame: Stats with a finished game, REVC_TEST_SAVE=<save file> as slot 1 (local file).
#   --clips: instead, one 60 fps walk-through per style (<set>_walkthrough.mp4, needs ffmpeg): each
#   state held 1.5 s, the highlight moving on row pages, the radio picker stepping.
#   REVC_IMPORT_DIR=<import cache> for the Xbox style's sprites (an import made from your disc).
#   The game folder's mods (game/mods: texture packs such as ps2-ui-remake.zip with the PS2
#   controller picture) are installed for the shots; REVC_TEST_MODS=<dir> picks another folder.
set -euo pipefail
CLIPS=0; [[ "${1:-}" == --clips ]] && { CLIPS=1; shift; }
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT=${1:-$ROOT/rust/target/test-output/menus}
shift || true
SETS=("$@"); (( ${#SETS[@]} )) || SETS=(ps2 xbox)
mkdir -p "$OUT"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
[[ -z "${REVC_TEST_MODS:-}" && -d "$ROOT/game/mods" ]] && export REVC_TEST_MODS="$ROOT/game/mods"
shoot() {	# set, ui style, width, height, suffix[, final height]
  local cfg=$TMP/menus.ini
  sed -e "s/^\[Graphics\]/[Graphics]\nUiStyle=$2/" -e "s/^Width=.*/Width=$3/" -e "s/^Height=.*/Height=$4/" \
    "$ROOT/tests/config/reVC.ini" > "$cfg"
  mkdir -p "$TMP/shots"; rm -f "$TMP"/shots/*
  REVC_TEST_CONFIG=$cfg REVC_TEST_WINDOW=${3}x$4 REVC_TEST_MENU_SET=$1 REVC_TEST_SHOT_DIR=$TMP/shots \
    "$ROOT/scripts/test.sh" menus || echo "menu-shots: the menus test reported a failure; keeping the shots it took" >&2
  compgen -G "$TMP/shots/*.png" > /dev/null || { echo "menu-shots: no shots taken" >&2; exit 1; }
  for f in "$TMP"/shots/*.png; do cp "$f" "$OUT/$(basename "${f%.png}")$5.png"; done
  # The PS2 frame is 640x448 (shown 4:3); the game has no 448-line mode, so 640x480 is scaled.
  if [[ -n "${6:-}" ]]; then
    python3 -c 'import sys
from PIL import Image
for f in sys.argv[2:]:
    Image.open(f).resize((640, int(sys.argv[1])), Image.LANCZOS).save(f)' "$6" "$TMP"/shots/*.png
    for f in "$TMP"/shots/*.png; do cp "$f" "$OUT/$(basename "${f%.png}")$5.png"; done
  fi
}
clip() {	# set, ui style, width, height
  local cfg=$TMP/menus.ini frames=$TMP/frames
  sed -e "s/^\[Graphics\]/[Graphics]\nUiStyle=$2/" -e "s/^Width=.*/Width=$3/" -e "s/^Height=.*/Height=$4/" \
    "$ROOT/tests/config/reVC.ini" > "$cfg"
  rm -rf "$frames"; mkdir -p "$frames"
  REVC_TEST_CONFIG=$cfg REVC_TEST_WINDOW=${3}x$4 REVC_TEST_MENU_SET=$1 REVC_FIXED_FPS=60 REVC_TEST_VIDEO_DIR=$frames \
    REVC_TEST_TIMEOUT=900 "$ROOT/scripts/test.sh" menus
  ffmpeg -loglevel error -y -framerate 60 -i "$frames/frame_%05d.png" -c:v libx264 -pix_fmt yuv420p -crf 18 "$OUT/${1}_walkthrough.mp4"
}
if (( CLIPS )); then
  for set in "${SETS[@]}"; do
    case $set in
      ps2) clip ps2 1 640 480 ;;
      xbox) clip xbox 2 640 480 ;;
      *) echo "unknown set $set" >&2; exit 2 ;;
    esac
  done
  echo "clips in $OUT"; exit 0
fi
for set in "${SETS[@]}"; do
  case $set in
    ps2) shoot ps2 1 640 480 "" 448; shoot ps2 1 1440 1080 _hi ;;
    xbox) shoot xbox 2 640 480 ""; shoot xbox 2 1440 1080 _hi ;;
    ps2_endgame|xbox_endgame)	# needs REVC_TEST_SAVE=<an endgame save>
      [[ -n "${REVC_TEST_SAVE:-}" ]] || { echo "$set needs REVC_TEST_SAVE" >&2; exit 2; }
      if [[ $set == ps2_endgame ]]; then shoot ps2_endgame 1 640 480 "" 448; else shoot xbox_endgame 2 640 480 ""; fi ;;
    *) echo "unknown set $set" >&2; exit 2 ;;
  esac
done
echo "shots in $OUT"

#!/usr/bin/env bash
# Menu comparison shots (plan task 13): every menu page in the PS2 and Xbox UI styles, at the
# consoles' sizes (PS2 640x448: shot at 640x480 and scaled, Xbox 640x480) and at 1440x1080 (<set>_<id>_hi.png), from a loaded
# game with the pause menu open (REVC_TEST=menus, rust/revc/src/testdrive.rs MENU_SHOTS_*).
# usage: scripts/menu-shots.sh [out dir] [ps2|xbox ...]
#   REVC_IMPORT_DIR=<import cache> for the Xbox style's sprites (an import made from your disc).
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT=${1:-$ROOT/rust/target/test-output/menus}
shift || true
SETS=("$@"); (( ${#SETS[@]} )) || SETS=(ps2 xbox)
mkdir -p "$OUT"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
shoot() {	# set, ui style, width, height, suffix[, final height]
  local cfg=$TMP/menus.ini
  sed -e "s/^\[Graphics\]/[Graphics]\nUiStyle=$2/" -e "s/^Width=.*/Width=$3/" -e "s/^Height=.*/Height=$4/" \
    "$ROOT/tests/config/reVC.ini" > "$cfg"
  mkdir -p "$TMP/shots"; rm -f "$TMP"/shots/*
  REVC_TEST_CONFIG=$cfg REVC_TEST_WINDOW=${3}x$4 REVC_TEST_MENU_SET=$1 REVC_TEST_SHOT_DIR=$TMP/shots \
    "$ROOT/scripts/test.sh" menus
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
for set in "${SETS[@]}"; do
  case $set in
    ps2) shoot ps2 1 640 480 "" 448; shoot ps2 1 1440 1080 _hi ;;
    xbox) shoot xbox 2 640 480 ""; shoot xbox 2 1440 1080 _hi ;;
    *) echo "unknown set $set" >&2; exit 2 ;;
  esac
done
echo "shots in $OUT"

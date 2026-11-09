#!/usr/bin/env bash
# Re-records tests/golden/timecyc.txt and the synthetic files in tests/golden/timecyc/ from the
# original CTimeCycle::Initialise (commit 064aa97). With the game data present its timecyc.dat is
# recorded too.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 064aa97 cpp/re3-miami/src/renderer/Timecycle.cpp cpp/re3-miami/src/renderer/Timecycle.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/renderer
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/Timecycle.h" "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define debug(...)
enum { NUMWEATHERS = 7, NUMHOURS = 24 };
H
# The static tables and Initialise (up to the interpolation helpers).
e=$(grep -n '^static float interp_c0' "$V/Timecycle.cpp" | cut -d: -f1)
sed -n "1,$((e - 1))p" "$V/Timecycle.cpp" | grep -v '^#include' > "$W/inc/timecyc.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/timecyc"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/timecyc.dat" ]] && real=("$GAME/data/timecyc.dat")
"$W/rec" "$ROOT/tests/golden/timecyc" "${real[@]}" > "$ROOT/tests/golden/timecyc.txt"
echo "wrote tests/golden/timecyc.txt ($(wc -l < "$ROOT/tests/golden/timecyc.txt") files)"

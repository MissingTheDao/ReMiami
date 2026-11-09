#!/usr/bin/env bash
# Re-records tests/golden/handling.txt and the synthetic files in tests/golden/handling/ from the
# original handling loader (HandlingMgr.cpp, cTransmission::InitGearRatios; commit 113352a). With
# the game data present (REVC_GAME_DIR or ./game) the game's handling.cfg is recorded too (only
# hashes; the Rust test checks it when the game data is there).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 113352a cpp/re3-miami/src/vehicles/HandlingMgr.cpp cpp/re3-miami/src/vehicles/HandlingMgr.h cpp/re3-miami/src/vehicles/Transmission.h cpp/re3-miami/src/vehicles/Transmission.cpp | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/vehicles
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$W/inc/" && cp "$V"/{HandlingMgr,Transmission}.h "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
inline float sq(float x) { return x*x; }
#define SQR(x) ((x) * (x))
#define DEGTORAD(x) ((x) * PI / 180.0f)
H
# Only InitGearRatios from Transmission.cpp; HandlingMgr.cpp without its includes.
s=$(grep -n '^cTransmission::InitGearRatios' "$V/Transmission.cpp" | cut -d: -f1); e=$(grep -n '^cTransmission::CalculateGearForSimpleCar' "$V/Transmission.cpp" | cut -d: -f1)
sed -n "$((s - 1)),$((e - 2))p" "$V/Transmission.cpp" > "$W/inc/transmission.inc"
grep -v '^#include' "$V/HandlingMgr.cpp" > "$W/inc/handlingmgr.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/handling"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/handling.cfg" ]] && real=("$GAME/data/handling.cfg")
"$W/rec" "$ROOT/tests/golden/handling" "${real[@]}" > "$ROOT/tests/golden/handling.txt"
echo "wrote tests/golden/handling.txt ($(wc -l < "$ROOT/tests/golden/handling.txt") entries)"

#!/usr/bin/env bash
# Re-records tests/golden/pedtype.txt and the synthetic files in tests/golden/pedtype/ from the
# original CPedType::LoadPedData / CPedStats::LoadPedStats (commit de54c0a). With the game data
# present its ped.dat and pedstats.dat are recorded too (hashes only).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive de54c0a cpp/re3-miami/src/peds/PedType.cpp cpp/re3-miami/src/peds/PedType.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/peds
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/PedType.h" "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define VALIDATE_SIZE(s, n) static_assert(sizeof(s) == n, "size")
#define debug(...)
#define Error(...)
H
grep -v '^#include' "$V/PedType.cpp" > "$W/inc/pedtype.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/pedtype"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/ped.dat" && -f "$GAME/data/pedstats.dat" ]] && real=("$GAME/data/ped.dat" "$GAME/data/pedstats.dat")
"$W/rec" "$ROOT/tests/golden/pedtype" "${real[@]}" > "$ROOT/tests/golden/pedtype.txt"
echo "wrote tests/golden/pedtype.txt ($(wc -l < "$ROOT/tests/golden/pedtype.txt") cases)"

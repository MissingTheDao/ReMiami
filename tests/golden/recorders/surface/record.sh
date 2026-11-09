#!/usr/bin/env bash
# Re-records tests/golden/surface.txt and the synthetic files in tests/golden/surface/ from the
# original CSurfaceTable (commit 4aa9a8c). With the game data present its surface.dat is recorded too.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 4aa9a8c cpp/re3-miami/src/core/SurfaceTable.cpp cpp/re3-miami/src/core/SurfaceTable.h cpp/re3-miami/src/collision/ColPoint.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/core/SurfaceTable.h" "$V/collision/ColPoint.h" "$W/inc/"
printf '#include <stdio.h>\n#include <stdlib.h>\n' >> "$W/inc/common.h"
grep -v '^#include' "$V/core/SurfaceTable.cpp" > "$W/inc/surfacetable.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/surface"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/surface.dat" ]] && real=("$GAME/data/surface.dat")
"$W/rec" "$ROOT/tests/golden/surface" "${real[@]}" > "$ROOT/tests/golden/surface.txt"
echo "wrote tests/golden/surface.txt ($(wc -l < "$ROOT/tests/golden/surface.txt") files)"

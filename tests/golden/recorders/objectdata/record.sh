#!/usr/bin/env bash
# Re-records tests/golden/objectdata.txt and the synthetic files in tests/golden/objectdata/ from the
# original CObjectData::Initialise (commit 3d2739d). With the game data present its object.dat is
# recorded too.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 3d2739d cpp/re3-miami/src/objects/ObjectData.cpp cpp/re3-miami/src/objects/ObjectData.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/objects
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/ObjectData.h" "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define VALIDATE_SIZE(s, n) static_assert(sizeof(s) == n, "size")
#define debug(...)
#define GRAVITY (0.008f)
enum { NUMOBJECTINFO = 210 };
H
# Initialise only (SetObjectData needs CObject).
e=$(grep -n '^CObjectData::SetObjectData' "$V/ObjectData.cpp" | cut -d: -f1)
sed -n "1,$((e - 2))p" "$V/ObjectData.cpp" | grep -v '^#include' > "$W/inc/objectdata.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/objectdata"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/object.dat" ]] && real=("$GAME/data/object.dat")
"$W/rec" "$ROOT/tests/golden/objectdata" "${real[@]}" > "$ROOT/tests/golden/objectdata.txt"
echo "wrote tests/golden/objectdata.txt ($(wc -l < "$ROOT/tests/golden/objectdata.txt") files)"

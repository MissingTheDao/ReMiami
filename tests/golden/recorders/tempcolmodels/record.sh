#!/usr/bin/env bash
# Re-records tests/golden/tempcolmodels.txt: the original CTempColModels::Initialise (commit
# 1017bca) run once, every model printed (floats as hex bits).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/c" && git -C "$ROOT" archive 1017bca cpp/re3-miami/src/collision cpp/re3-miami/src/core/SurfaceTable.h | tar -x -C "$W/c"
M=$W/cpp/re3-miami/src/math C=$W/c/cpp/re3-miami/src/collision
cp "$W/c/cpp/re3-miami/src/core/SurfaceTable.h" "$C/"
rm "$C/ColModel.h"  # the stub (stub/ColModel.h) instead
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$W/inc/"
echo '#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))' >> "$W/inc/common.h"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" -I"$C" "$HERE/recorder.cpp" "$C"/TempColModels.cpp "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/tempcolmodels.txt"
echo "wrote tests/golden/tempcolmodels.txt ($(wc -l < "$ROOT/tests/golden/tempcolmodels.txt") models)"

#!/usr/bin/env bash
# Re-records tests/golden/collision.txt from the original C++ collision tests (Collision.cpp's PC
# paths, ColTriangle.cpp, ColLine.cpp; last in commit 1017bca) and the original math (e6c80bb),
# compiled like the game (clang++ -O2).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
mkdir "$W/inc"
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/c" && git -C "$ROOT" archive 1017bca cpp/re3-miami/src/collision cpp/re3-miami/src/core/SurfaceTable.h | tar -x -C "$W/c"
M=$W/cpp/re3-miami/src/math C=$W/c/cpp/re3-miami/src/collision
cp "$W/c/cpp/re3-miami/src/core/SurfaceTable.h" "$C/"
# The functions from TestSphereSphere up to CalculateTrianglePlanes.
s=$(grep -n '^CCollision::TestSphereSphere' "$C/Collision.cpp" | cut -d: -f1)
e=$(grep -n '^CCollision::CalculateTrianglePlanes' "$C/Collision.cpp" | cut -d: -f1)
sed -n "$((s - 1)),$((e - 2))p" "$C/Collision.cpp" > "$W/inc/collision.inc"
cp "$HERE/common.h" "$HERE/colstub.h" "$W/inc/"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" -I"$C" "$HERE/recorder.cpp" "$C"/{ColTriangle,ColLine,ColBox}.cpp "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/collision.txt"
echo "wrote tests/golden/collision.txt ($(wc -l < "$ROOT/tests/golden/collision.txt") cases)"

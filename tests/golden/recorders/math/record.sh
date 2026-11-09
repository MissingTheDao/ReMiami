#!/usr/bin/env bash
# Re-records tests/golden/math.txt from the original C++ math module (last in commit e6c80bb),
# compiled like the game (clang++ -O2). The matrix products' w column is masked (the original
# leaves it uninitialised).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
M=$W/cpp/re3-miami/src/math
mkdir "$W/inc" && cp "$HERE/common.h" "$W/inc/"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" | awk '$1 == "mmul" || $1 == "mmuleq" { split($0, p, " => "); n = split(p[2], v, " ");
  for (i = 4; i <= 16; i += 4) v[i] = "--------"; s = v[1]; for (i = 2; i <= n; i++) s = s " " v[i]; print p[1] " => " s; next } { print }' > "$ROOT/tests/golden/math.txt"
echo "wrote tests/golden/math.txt ($(wc -l < "$ROOT/tests/golden/math.txt") cases)"

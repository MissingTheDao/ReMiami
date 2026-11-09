#!/usr/bin/env bash
# Re-records tests/golden/clock.txt from the original CClock (commit 201fa14).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 201fa14 cpp/re3-miami/src/core/Clock.cpp cpp/re3-miami/src/core/Clock.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/core
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/Clock.h" "$W/inc/"
printf '#include <stdio.h>\n#define debug(...)\n#define _TODO(x)\n' >> "$W/inc/common.h"
grep -v '^#include' "$V/Clock.cpp" > "$W/inc/clock.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/clock.txt"
echo "wrote tests/golden/clock.txt ($(wc -l < "$ROOT/tests/golden/clock.txt") ops)"

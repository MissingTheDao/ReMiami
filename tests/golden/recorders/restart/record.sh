#!/usr/bin/env bash
# Re-records tests/golden/restart.txt from the original CRestart (commit 8c457af).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 8c457af cpp/re3-miami/src/control/Restart.cpp cpp/re3-miami/src/control/Restart.h cpp/re3-miami/src/save/SaveBuf.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$V/control/Restart.h" "$W/inc/"
printf '#include <stdio.h>\n#define VALIDATE_SIZE(s, n)\n' >> "$W/inc/common.h"
grep -v '^#include\|^#pragma' "$V/save/SaveBuf.h" > "$W/inc/savebuf.inc"
printf '#include "Restart.h"\n' > "$W/inc/restart.inc"
grep -v '^#include' "$V/control/Restart.cpp" >> "$W/inc/restart.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/restart.txt"
echo "wrote tests/golden/restart.txt ($(wc -l < "$ROOT/tests/golden/restart.txt") ops)"

#!/usr/bin/env bash
# Re-records tests/golden/directory.txt and the synthetic files in tests/golden/directory/ from the
# original CDirectory (commit e60cd69).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive e60cd69 cpp/re3-miami/src/core/Directory.cpp cpp/re3-miami/src/core/Directory.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/core
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/Directory.h" "$W/inc/"
printf '#include <stdio.h>\n#include <stdlib.h>\n' >> "$W/inc/common.h"
grep -v '^#include' "$V/Directory.cpp" > "$W/inc/directory.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/directory"
"$W/rec" "$ROOT/tests/golden/directory" > "$ROOT/tests/golden/directory.txt"
echo "wrote tests/golden/directory.txt ($(wc -l < "$ROOT/tests/golden/directory.txt") files)"

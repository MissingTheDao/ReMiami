#!/usr/bin/env bash
# Re-records tests/golden/rwcharset.txt from librw's debug font as it was before the Rust port
# (vendor/librw/src/charset.cpp in commit 05e88f2), compiled like the game (clang++ -O2,
# RW_WGPU), with stubs for images, rasters, render states and Im2D.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
clang++ -O2 -Wno-everything -DRW_WGPU -I"$S" "$HERE/recorder.cpp" "$S/charset.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwcharset.txt"
echo "wrote tests/golden/rwcharset.txt ($(wc -l < "$ROOT/tests/golden/rwcharset.txt") lines, $(du -h "$ROOT/tests/golden/rwcharset.txt" | cut -f1))"

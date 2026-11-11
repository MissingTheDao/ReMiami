#!/usr/bin/env bash
# Re-records tests/golden/rwraster.txt from librw's rasters and revc's wgpu raster driver as they
# were before the Rust port (vendor/librw/src/raster.cpp and wgpu/wgpuraster.cpp in commit
# 05e88f2, with that commit's image.cpp), compiled like the game (clang++ -O2), with stubs for the
# engine, the plugin lists and the host's texture upload.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
clang++ -O2 -Wno-everything -DRW_WGPU -I"$S" -I"$S/wgpu" "$HERE/recorder.cpp" "$S/raster.cpp" "$S/image.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwraster.txt"
echo "wrote tests/golden/rwraster.txt ($(wc -l < "$ROOT/tests/golden/rwraster.txt") lines, $(du -h "$ROOT/tests/golden/rwraster.txt" | cut -f1))"

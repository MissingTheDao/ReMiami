#!/usr/bin/env bash
# Re-records tests/golden/rwscene.txt from librw's scene objects as they were before the Rust port
# (vendor/librw/src/clump.cpp, light.cpp, world.cpp and camera.cpp in commit 05e88f2, with that
# commit's frame, geometry, texture, matrix and box code), compiled like the game (clang++ -O2),
# with stubs for the engine, plugin lists, streams and rasters.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$S/clump.cpp" "$S/light.cpp" "$S/world.cpp" "$S/camera.cpp" "$S/frame.cpp" \
  "$S/geometry.cpp" "$S/geoplg.cpp" "$S/tristrip.cpp" "$S/texture.cpp" "$S/prim.cpp" "$W/math.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwscene.txt"
echo "wrote tests/golden/rwscene.txt ($(wc -l < "$ROOT/tests/golden/rwscene.txt") lines, $(du -h "$ROOT/tests/golden/rwscene.txt" | cut -f1))"

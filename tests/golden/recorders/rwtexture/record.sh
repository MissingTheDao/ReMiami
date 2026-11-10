#!/usr/bin/env bash
# Re-records tests/golden/rwtexture.txt from librw's texture object model as it was before the Rust port
# (vendor/librw/src/texture.cpp in commit 05e88f2),
# compiled like the game (clang++ -O2), with stubs for the engine, plugin lists and streams.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
# librw's vector/matrix section of base.cpp, as the frame recorder takes it
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwtexture.txt"
echo "wrote tests/golden/rwtexture.txt ($(wc -l < "$ROOT/tests/golden/rwtexture.txt") lines, $(du -h "$ROOT/tests/golden/rwtexture.txt" | cut -f1))"

#!/usr/bin/env bash
# Re-records tests/golden/rwgeomobj.txt from librw's geometry and material objects as they were
# before the Rust port (vendor/librw/src/geometry.cpp, geoplg.cpp, tristrip.cpp, texture.cpp and
# base.cpp's math in commit 05e88f2), compiled like the game (clang++ -O2), with stubs for the
# engine, plugin lists, streams and rasters.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
# texture.cpp with a hook to run its (static) module open
{ cat "$S/texture.cpp"; echo 'void *textureOpenForRecorder(void) { return rw::textureOpen(rw::engine, 4096, sizeof(rw::TextureGlobals)); }'; } > "$W/texture.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$S/geometry.cpp" "$S/geoplg.cpp" "$S/tristrip.cpp" "$W/texture.cpp" "$W/math.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwgeomobj.txt"
echo "wrote tests/golden/rwgeomobj.txt ($(wc -l < "$ROOT/tests/golden/rwgeomobj.txt") lines, $(du -h "$ROOT/tests/golden/rwgeomobj.txt" | cut -f1))"

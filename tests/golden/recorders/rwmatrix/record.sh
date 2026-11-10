#!/usr/bin/env bash
# Re-records tests/golden/rwmatrix.txt from librw's rw::Matrix functions as they were before the
# Rust port (vendor/librw/src/base.cpp in commit bfddf5c), compiled like the game (clang++ -O2).
# Only base.cpp's math section is compiled (quaternion/vector helpers and Matrix); the padding words
# of the matrices are masked (librw copies uninitialised temporaries into them).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive bfddf5c cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
# base.cpp from "//\n// Quat" (the first math function) to the end of Matrix::identityError
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
grep -n "^static Matrix identMat" "$S/base.cpp" >/dev/null
{ echo '#include <stdio.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#include <stdlib.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
clang++ -O2 -Wno-everything -I"$S" -include stdint.h -include string.h "$HERE/recorder.cpp" "$W/math.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwmatrix.txt"
echo "wrote tests/golden/rwmatrix.txt ($(wc -l < "$ROOT/tests/golden/rwmatrix.txt") cases)"

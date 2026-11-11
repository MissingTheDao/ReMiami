#!/usr/bin/env bash
# Re-records tests/golden/rwquat.txt from librw's Quat::rotate as it was before the Rust port
# (the vector/matrix section of vendor/librw/src/base.cpp in commit 05e88f2), compiled like the
# game (clang++ -O2).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$W/math.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwquat.txt"
echo "wrote tests/golden/rwquat.txt ($(wc -l < "$ROOT/tests/golden/rwquat.txt") lines)"

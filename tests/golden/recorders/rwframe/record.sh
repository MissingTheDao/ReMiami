#!/usr/bin/env bash
# Re-records tests/golden/rwframe.txt from librw's frame hierarchy code as it was before the Rust
# port (vendor/librw/src/frame.cpp in commit 05e88f2) on librw's matrix code of the same commit,
# compiled like the game (clang++ -O2), with stubs for the engine and plugin lists.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
awk '/^Quat$/ && !on { on = 1 } on { print } on && /^Matrix::identityError/ { last = 1 } last && /^}$/ { exit }' "$S/base.cpp" > "$W/section.cpp"
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include <math.h>'; echo '#define rwMalloc(s, h) malloc(s)'; echo '#define rwFree(p) free(p)'; echo 'namespace rw {';
  sed -n '/^static Matrix identMat = {/,/^};/p' "$S/base.cpp"; cat "$W/section.cpp"; echo '}'; } > "$W/math.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$W/math.cpp" "$S/frame.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwframe.txt"
echo "wrote tests/golden/rwframe.txt ($(wc -l < "$ROOT/tests/golden/rwframe.txt") lines)"
# the lifecycle (create, destroy, clone) with plugin hook calls and the allocation counter
clang++ -O2 -Wno-everything -I"$S" "$HERE/lifecycle.cpp" "$W/math.cpp" "$S/frame.cpp" -o "$W/life"
"$W/life" > "$ROOT/tests/golden/rwframe-life.txt"
echo "wrote tests/golden/rwframe-life.txt ($(wc -l < "$ROOT/tests/golden/rwframe-life.txt") lines)"

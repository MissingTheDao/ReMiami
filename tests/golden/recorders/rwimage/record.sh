#!/usr/bin/env bash
# Re-records tests/golden/rwimage.txt from librw's image pixel functions as they were before the
# Rust port (vendor/librw/src/image.cpp in commit 05e88f2, with raster.cpp's pixel converters it
# uses), compiled like the game (clang++ -O2), with stubs for the engine and the file search.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
{ echo '#include <stdio.h>'; echo '#include "rwbase.h"'; echo '#include "rwerror.h"'; echo '#include "rwplg.h"'; echo '#include "rwpipeline.h"'; echo '#include "rwobjects.h"'; echo 'namespace rw {';
  for f in conv_RGBA8888_from_RGBA8888 conv_RGBA8888_from_RGB888 conv_RGBA8888_from_ARGB1555; do
    awk -v f="$f" '$0 == "void" { hold = $0; next } hold && $0 ~ "^"f"\\(" { on = 1; print hold } { hold = "" } on { print } on && /^}$/ { on = 0 }' "$S/raster.cpp"
  done; echo '}'; } > "$W/conv.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$S/image.cpp" "$W/conv.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwimage.txt"
echo "wrote tests/golden/rwimage.txt ($(wc -l < "$ROOT/tests/golden/rwimage.txt") lines, $(du -h "$ROOT/tests/golden/rwimage.txt" | cut -f1))"

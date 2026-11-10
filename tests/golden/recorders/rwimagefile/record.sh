#!/usr/bin/env bash
# Re-records tests/golden/rwimagefile.txt and the image corpus tests/golden/imagefiles/ from
# librw's image file code as it was before the Rust port (vendor/librw/src/png.cpp, bmp.cpp,
# tga.cpp, image.cpp, lodepng and base.cpp's path functions in commit 05e88f2), compiled like the
# game (clang++ -O2), with stubs for the engine.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
mkdir -p "$ROOT/tests/golden/imagefiles"
rm -f "$ROOT/tests/golden/imagefiles/"*
{ echo '#include <stdio.h>'; echo '#include <stdlib.h>'; echo '#include "rwbase.h"'; echo '#include "rwerror.h"'; echo '#include "rwplg.h"'; echo '#include "rwpipeline.h"'; echo '#include "rwobjects.h"'; echo 'namespace rw {';
  for f in conv_RGBA8888_from_RGBA8888 conv_RGBA8888_from_RGB888 conv_RGBA8888_from_ARGB1555 expandPal4_BE compressPal4_BE; do
    awk -v f="$f" '$0 == "void" { hold = $0; next } hold && $0 ~ "^"f"\\(" { on = 1; print hold } { hold = "" } on { print } on && /^}$/ { on = 0 }' "$S/raster.cpp"
  done; echo '}'; } > "$W/conv.cpp"
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$S/image.cpp" "$S/png.cpp" "$S/bmp.cpp" "$S/tga.cpp" "$S/lodepng/lodepng.cpp" \
  "$S/base.cpp" "$W/conv.cpp" -o "$W/rec"
"$W/rec" "$ROOT/tests/golden/rwimagefile.txt" "$ROOT/tests/golden/imagefiles" > /dev/null
echo "wrote tests/golden/rwimagefile.txt ($(wc -l < "$ROOT/tests/golden/rwimagefile.txt") lines) and $(ls "$ROOT/tests/golden/imagefiles" | wc -l) files ($(du -sh "$ROOT/tests/golden/imagefiles" | cut -f1))"

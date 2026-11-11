#!/usr/bin/env bash
# Re-records tests/golden/rwplugin.txt from librw's plugin lists and memory streams as they were
# before the Rust port (vendor/librw/src/plg.cpp and base.cpp in commit 05e88f2), compiled like
# the game (clang++ -O2), with stubs for the engine.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive 05e88f2 cpp/re3-miami/vendor/librw/src | tar -x -C "$W"
S=$W/cpp/re3-miami/vendor/librw/src
clang++ -O2 -Wno-everything -I"$S" "$HERE/recorder.cpp" "$S/plg.cpp" "$S/base.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/rwplugin.txt"
echo "wrote tests/golden/rwplugin.txt ($(wc -l < "$ROOT/tests/golden/rwplugin.txt") lines, $(du -h "$ROOT/tests/golden/rwplugin.txt" | cut -f1))"

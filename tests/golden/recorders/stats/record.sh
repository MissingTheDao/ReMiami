#!/usr/bin/env bash
# Re-records tests/golden/stats.txt from the original CStats (commit a4e3d68).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive a4e3d68 cpp/re3-miami/src/core/Stats.cpp cpp/re3-miami/src/core/Stats.h | tar -x -C "$W"
V=$W/cpp/re3-miami/src/core
mkdir "$W/inc"
grep -v '^#include\|^#pragma' "$V/Stats.h" > "$W/inc/stats.h.inc"
# Everything up to the stats screen (BuildStatLine/ConstructStatLine need the font and menu).
python3 - "$V/Stats.cpp" "$W/inc/stats.inc" <<'PY'
import sys
s=open(sys.argv[1]).read()
s=s[:s.index("void\nCStats::BuildStatLine")]
open(sys.argv[2],"w").write("\n".join(l for l in s.split("\n") if not l.startswith("#include")))
PY
clang++ -O2 -Wno-everything -I"$W/inc" -I"$HERE" "$HERE/recorder.cpp" -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/stats.txt"
echo "wrote tests/golden/stats.txt ($(wc -l < "$ROOT/tests/golden/stats.txt") ops)"

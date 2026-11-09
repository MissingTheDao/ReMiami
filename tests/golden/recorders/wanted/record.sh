#!/usr/bin/env bash
# Re-records tests/golden/wanted.txt from the original CWanted (commit 632e2bc).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 632e2bc cpp/re3-miami/src/core/Wanted.cpp cpp/re3-miami/src/core/Wanted.h cpp/re3-miami/src/core/Crime.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/core
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/Wanted.h" "$V/Crime.h" "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdint.h>
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define VALIDATE_SIZE(s, n)
#define Error(...)
template <class T> static T Min(T a, T b) { return a < b ? a : b; }
template <class T> static T Max(T a, T b) { return a > b ? a : b; }
H
# All but WorkOutPolicePresence and ResetPolicePursuit (stubs in the recorder).
python3 - "$V/Wanted.cpp" "$W/inc/wanted.inc" <<'PY'
import sys
s=open(sys.argv[1]).read()
for name in ["CWanted::WorkOutPolicePresence(", "CWanted::ResetPolicePursuit("]:
    i=s.index(name); a=s.rfind("\n\n",0,i)+2; b=s.index("\n}\n",i)+3; s=s[:a]+s[b:]
open(sys.argv[2],"w").write("\n".join(l for l in s.split("\n") if not l.startswith("#include")))
PY
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/wanted.txt"
echo "wrote tests/golden/wanted.txt ($(wc -l < "$ROOT/tests/golden/wanted.txt") ops)"

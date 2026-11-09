#!/usr/bin/env bash
# Re-records tests/golden/zones.txt from the original CTheZones logic (Zones.cpp from Init on, save
# and load included; commit 42e26f2).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 42e26f2 cpp/re3-miami/src/core/Zones.cpp cpp/re3-miami/src/core/Zones.h cpp/re3-miami/src/save/SaveBuf.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/core
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V/Zones.h" "$W/v/cpp/re3-miami/src/save/SaveBuf.h" "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
typedef uint16_t wchar;
enum { NUMAUDIOZONES = 14, NUMINFOZONES = 169, NUMMAPZONES = 39, NUMNAVIGZONES = 20 };
CVector FindPlayerCoors(void);
H
s=$(grep -n '^CTheZones::Init(void)' "$V/Zones.cpp" | cut -d: -f1)
{ sed -n '1,/^#define SWAPF/p' "$V/Zones.cpp" | grep -v '^#include'; sed -n "$((s - 1)),\$p" "$V/Zones.cpp"; } > "$W/inc/zones.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
"$W/rec" > "$ROOT/tests/golden/zones.txt"
echo "wrote tests/golden/zones.txt ($(wc -l < "$ROOT/tests/golden/zones.txt") ops)"

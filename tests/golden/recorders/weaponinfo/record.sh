#!/usr/bin/env bash
# Re-records tests/golden/weaponinfo.txt and the synthetic files in tests/golden/weaponinfo/ from the
# original CWeaponInfo::Initialise/LoadWeaponData (commit e2ccd6c). With the game data present the
# game's weapon.dat is recorded too (hashes only).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive e2ccd6c cpp/re3-miami/src/weapons/WeaponInfo.cpp cpp/re3-miami/src/weapons/WeaponInfo.h cpp/re3-miami/src/weapons/WeaponType.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/weapons
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/* "$V"/{WeaponInfo,WeaponType}.h "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define VALIDATE_SIZE(s, n) static_assert(sizeof(s) == n, "size")
#define debug(...)
#define Error(...)
H
grep -v '^#include' "$V/WeaponInfo.cpp" > "$W/inc/weaponinfo.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/weaponinfo"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/weapon.dat" ]] && real=("$GAME/data/weapon.dat")
"$W/rec" "$ROOT/tests/golden/weaponinfo" "${real[@]}" > "$ROOT/tests/golden/weaponinfo.txt"
echo "wrote tests/golden/weaponinfo.txt ($(wc -l < "$ROOT/tests/golden/weaponinfo.txt") files)"

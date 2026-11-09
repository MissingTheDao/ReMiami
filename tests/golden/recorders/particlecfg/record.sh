#!/usr/bin/env bash
# Re-records tests/golden/particlecfg.txt and the synthetic files in tests/golden/particlecfg/ from
# the original cParticleSystemMgr::LoadParticleData (commit 8953e8f). With the game data present
# its particle.cfg is recorded too.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" archive e6c80bb cpp/re3-miami/src/math | tar -x -C "$W"
mkdir "$W/v" && git -C "$ROOT" archive 8953e8f cpp/re3-miami/src/renderer/ParticleMgr.cpp cpp/re3-miami/src/renderer/ParticleMgr.h cpp/re3-miami/src/renderer/ParticleType.h | tar -x -C "$W/v"
M=$W/cpp/re3-miami/src/math V=$W/v/cpp/re3-miami/src/renderer
mkdir "$W/inc" && cp "$HERE/../collision/common.h" "$HERE"/stub/main.h "$HERE"/stub/FileMgr.h "$HERE"/stub/extra.h "$V"/{ParticleMgr,ParticleType}.h "$W/inc/"
cat >> "$W/inc/common.h" <<'H'
#include <stdio.h>
#include <stdlib.h>
#define VALIDATE_SIZE(s, n)
#define ASSERT(x)
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define SQR(x) ((x) * (x))
#define BIT(num) (1<<(num))
template <class T> static T Min(T a, T b) { return a < b ? a : b; }
H
# LoadParticleData only (Initialise allocates particles).
s=$(grep -n '^void cParticleSystemMgr::LoadParticleData' "$V/ParticleMgr.cpp" | cut -d: -f1)
sed -n "$s,\$p" "$V/ParticleMgr.cpp" > "$W/inc/particlemgr.inc"
clang++ -O2 -Wno-everything -I"$W/inc" -I"$M" "$HERE/recorder.cpp" "$M"/{Matrix,Vector,Quaternion,math,Rect}.cpp -o "$W/rec"
mkdir -p "$ROOT/tests/golden/particlecfg"
GAME=${REVC_GAME_DIR:-$ROOT/game}
real=(); [[ -f "$GAME/data/particle.cfg" ]] && real=("$GAME/data/particle.cfg")
"$W/rec" "$ROOT/tests/golden/particlecfg" "${real[@]}" > "$ROOT/tests/golden/particlecfg.txt"
echo "wrote tests/golden/particlecfg.txt ($(wc -l < "$ROOT/tests/golden/particlecfg.txt") files)"

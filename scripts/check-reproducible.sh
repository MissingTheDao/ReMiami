#!/usr/bin/env bash
# Builds HEAD twice from fresh clones in two different directories and compares the
# binaries. usage: [REVC_SOURCE=<reVC copy>] scripts/check-reproducible.sh [workdir]
#   default: a temporary dir under /var/tmp (on disk: /tmp is often RAM, and the two builds take
#   ~0.5 GB), removed afterwards; a given workdir is kept.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ -n "${1:-}" ]]; then WORK=$1
else WORK=$(mktemp -d -p /var/tmp revc-repro.XXXXXX); trap 'rm -rf "$WORK"' EXIT; fi
mkdir -p "$WORK"
for n in a b; do
  rm -rf "$WORK/$n"
  git clone -q "$ROOT" "$WORK/$n/revc-$n-$RANDOM"
done
# The reVC source isn't in the repository: each clone builds its C++ tree from the same copy.
SOURCE=${REVC_SOURCE:-$ROOT/cpp/re3-miami/.reVC-original}
for d in "$WORK"/a/* "$WORK"/b/*; do "$d/scripts/revc-source.sh" prepare "$SOURCE" > /dev/null; done
sums=()
for d in "$WORK"/a/* "$WORK"/b/*; do
  echo "== building $d"
  CARGO_TARGET_DIR="$d/target" "$d/scripts/build.sh" > "$d.log" 2>&1 || { tail -30 "$d.log"; exit 1; }
  sums+=("$(sha256sum "$d/target/release/revc" | awk '{print $1}')")
  echo "   ${sums[-1]}"
done
if [[ "${sums[0]}" == "${sums[1]}" ]]; then
  echo "REPRODUCIBLE: identical binaries"
else
  echo "NOT REPRODUCIBLE: binaries differ (logs in $WORK)"; exit 1
fi

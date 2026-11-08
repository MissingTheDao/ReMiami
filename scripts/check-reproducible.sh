#!/usr/bin/env bash
# Builds HEAD twice from fresh clones in two different directories and compares the
# binaries. usage: scripts/check-reproducible.sh [workdir]   (default: a mktemp dir)
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=${1:-$(mktemp -d)}
mkdir -p "$WORK"
for n in a b; do
  rm -rf "$WORK/$n"
  git clone -q "$ROOT" "$WORK/$n/revc-$n-$RANDOM"
done
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

#!/usr/bin/env bash
# Fresh-setup check (roadmap R7a): does a clean clone run from an untouched Vice City install?
#
# usage: scripts/fresh-check.sh <reVC source (folder/zip/git)> <untouched VC install> [work dir] [--boot]
#
# Clones this repository's HEAD into the work dir (default: a new temp dir), prepares the C++ tree
# from the reVC source, copies the VC install in as game/ (nothing else added), builds offline
# (Cargo's cache must hold the dependencies, as on a machine that built reMiami before), and with
# --boot runs the boot test against that game folder. Prints what the clean setup lacked.
set -euo pipefail
src=${1:?reVC source}; vc=${2:?untouched Vice City install}; work=${3:-$(mktemp -d)}; boot=0
for a in "$@"; do [[ "$a" == --boot ]] && boot=1; done
[[ "$work" == --boot ]] && work=$(mktemp -d)
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
echo "== work dir $work"
mkdir -p "$work"
rm -rf "$work/revc"
git clone -q "$here" "$work/revc"
cd "$work/revc"
echo "== prepare the C++ tree"
scripts/revc-source.sh prepare "$src" | tail -2
echo "== game folder: the untouched install"
cp -r --reflink=auto "$vc" game
echo "== build (offline)"
(cd rust && cargo build --release --offline -q 2>&1 | grep -E "^(error|warning: unused)" || true)
[[ -x rust/target/release/remiami ]] || { echo "FAIL: no binary"; exit 1; }
echo "   built rust/target/release/remiami"
if (( boot )); then
  echo "== boot test"
  scripts/test.sh boot 2>&1 | grep -E "^== |FAIL|missing" || true
fi
echo "== done ($work)"

#!/usr/bin/env bash
# Re-records tests/golden/savefix.txt: the original C++ GetSaveType + FixSave (last in commit
# 48e9e38) over other-build saves the Rust test makes from tests/saves/GTAVCsf1.b.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../../.." && pwd)
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
git -C "$ROOT" show 48e9e38:cpp/re3-miami/src/save/GenericGameStorage.cpp |
  awk '/^#ifdef FIX_INCOMPATIBLE_SAVES/ { on = 1; next } on && /^#undef SkipPtr/ { exit } on' > "$W/fix.inc"
clang++ -O2 -Wno-everything -I"$HERE" -I"$W" "$HERE/recorder.cpp" -o "$W/rec"
mkdir "$W/in" "$W/slot"
(cd "$ROOT/rust/revc" && REVC_SAVEFIX_DUMP="$W/in" cargo test --release -q savefix::tests::dump >/dev/null)
"$W/rec" "$W/slot" "$W"/in/*.b > "$ROOT/tests/golden/savefix.txt"
cat "$ROOT/tests/golden/savefix.txt"

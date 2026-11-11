#!/usr/bin/env bash
# Frame-by-frame motion tests (plan task 13): records each menu animation listed in a motions.json in
# revc (REVC_TEST_MOTION=<id>, one run per motion, every frame at REVC_FIXED_FPS=60, table MOTIONS in
# rust/revc/src/testdrive.rs) and compares it with the console reference using motiontest.py (next to
# the motions.json; both are local research files, not in the repo).
#
# usage: scripts/motion-tests.sh <motions.json> [out dir] [id ...]
#   Runs every id of the json (or the given ones) that revc has a motion for; others keep the revc
#   source the json names. For a run, an optional "revc_run" entry gives "start"/"end" in frames
#   relative to the input (default -3 and the end of the run) and "roi" in revc's 640x480 frame
#   (default: the revc entry's roi, else the ref roi). Ids need the out dir to be given first.
#   Writes <out>/<id>/frame_*.png, <out>/motions.json and motiontest.py's out/motion plots, and for
#   motions revc reaches by the console's button route (MOTION_ROUTES) <out>/<id>.pad.json.
#   Exit 1 if any motion fails (or a run fails).
# Wrap it in scripts/headless.sh like the other game runs.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
JSON=$(realpath "${1:?usage: motion-tests.sh <motions.json> [out dir] [id ...]}")
OUT=$(realpath -m "${2:-$ROOT/rust/target/test-output/motion}")
shift $(( $# >= 2 ? 2 : 1 ))
mkdir -p "$OUT"
mapfile -t IDS < <(python3 - "$JSON" "$@" <<'EOF'
import json, sys
ids = [m['id'] for m in json.load(open(sys.argv[1]))]
want = sys.argv[2:]
print('\n'.join(i for i in ids if not want or i in want))
EOF
)
KNOWN=$(/usr/bin/grep -oE '^ *\("(ps2|xbox)_[a-z0-9_]+"' "$ROOT/rust/revc/src/testdrive.rs" | tr -d ' ("')
FAILED=0
RAN=()
for id in "${IDS[@]}"; do
  /usr/bin/grep -qx -- "$id" <<< "$KNOWN" || { echo "motion $id: no revc motion, using the json's revc source"; continue; }
  st=1; [[ $id == xbox_* ]] && st=2
  cfg="$OUT/$id.ini"
  sed -e "s/^\[Graphics\]/[Graphics]\nUiStyle=$st/" -e "s/^Width=.*/Width=640/" -e "s/^Height=.*/Height=480/" \
    "$ROOT/tests/config/reVC.ini" > "$cfg"
  rm -rf "${OUT:?}/$id"; mkdir -p "$OUT/$id"
  if REVC_TEST_CONFIG=$cfg REVC_TEST_WINDOW=640x480 REVC_TEST_MOTION=$id REVC_TEST_PAD_OUT="$OUT" REVC_FIXED_FPS=60 \
      REVC_TEST_VIDEO_DIR="$OUT/$id" REVC_TEST_TIMEOUT=300 "$ROOT/scripts/test.sh" menus > "$OUT/$id.log" 2>&1; then
    RAN+=("$id"); echo "motion $id: $(ls "$OUT/$id" | wc -l) frames"
  else
    echo "motion $id: the revc run FAILED (log $OUT/$id.log)" >&2; FAILED=1
  fi
done
# the json with the revc side pointing at the runs (frames relative to the input frame, 30)
python3 - "$JSON" "$OUT" "${RAN[@]}" <<'EOF'
import json, os, sys
src, out, ran = sys.argv[1], sys.argv[2], set(sys.argv[3:])
base = os.path.dirname(src)
ms = json.load(open(src))
for m in ms:
    for side in ('ref', 'revc'):	# keep the json's own paths valid from the new location
        if side in m and m[side].get('video') and not os.path.isabs(os.path.expanduser(m[side]['video'])):
            m[side]['video'] = os.path.join(base, m[side]['video'])
    if m['id'] in ran:
        d = os.path.join(out, m['id']); n = len([f for f in os.listdir(d) if f.startswith('frame_')])
        r = m.get('revc', {}); run = m.get('revc_run', {})
        m['revc'] = dict(video=d, fps=60, start=30 + run.get('start', -3), end=min(n, 30 + run.get('end', n - 30)),
                         roi=run.get('roi', r.get('roi', m['ref']['roi'])))
json.dump(ms, open(os.path.join(out, 'motions.json'), 'w'), indent=1)
EOF
python3 "$(dirname "$JSON")/motiontest.py" "$OUT/motions.json" "${IDS[@]}" || FAILED=1
exit $FAILED

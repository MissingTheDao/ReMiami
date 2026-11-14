#!/usr/bin/env bash
# fps parity (R6): the game simulates at 30 Hz and draws as many frames as the display allows; what
# it shows per game step must not depend on that. Runs the fpsparity scenario (testdrive.rs: a fixed
# scene, then a scripted drive) at REVC_FIXED_FPS 30 and each RATES value, and compares every
# per-step value with the 30 fps run. Ends with a table; exit 1 if any value differs.
# usage: scripts/fps-parity.sh [rate ...]      (default: 60 144 320)
#   REVC_TEST_BIN, REVC_GAME_DIR as for scripts/test.sh. Logs: rust/target/test-output/fps-parity/
set -uo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT=$ROOT/rust/target/test-output/fps-parity
mkdir -p "$OUT"
RATES=("$@"); (( ${#RATES[@]} )) || RATES=(60 144 320)
FAILED=()
for rate in 30 "${RATES[@]}"; do
  rm -f "$OUT/fps$rate.log"
  if REVC_FIXED_FPS=$rate REVC_TEST_FPS_LOG="$OUT/fps$rate.log" "$ROOT/scripts/test.sh" fpsparity > "$OUT/run$rate.out" 2>&1 \
     && (( $(wc -l < "$OUT/fps$rate.log" 2>/dev/null || echo 0) >= 400 )); then
    echo "== $rate fps: PASS"
  else
    echo "== $rate fps: run FAILED (see $OUT/run$rate.out)"; FAILED+=("$rate")
  fi
done
FAILED_LIST=$(IFS=,; echo "${FAILED[*]:-}")
python3 -I - "$OUT" "$FAILED_LIST" "${RATES[@]}" <<'PY'
import os, sys
out, failed, rates = sys.argv[1], [r for r in sys.argv[2].split(',') if r], sys.argv[3:]

def load(rate):
    p = os.path.join(out, 'fps%s.log' % rate)
    if not os.path.exists(p):
        return {}
    steps = {}
    for line in open(p):
        e = dict(kv.split('=', 1) for kv in line.split() if '=' in kv)
        if 'step' in e:
            steps[int(e['step'])] = e
    return steps

# key: (last step compared or None for all, tolerance or None for exact, informational)
RULES = {}
for k in 'glassmin glassmax msgmin msgmax mkmin mkmax'.split(): RULES[k] = (150, None, False)  # fixed camera
# the sum of fading-in alphas: one model may finish streaming in at another step (loading is
# asynchronous), so one model's worth (255) is allowed; a fade at the display rate is far more
RULES['lod'] = (150, 255, False)
for k in 'wu wv oddjob'.split(): RULES[k] = (None, 1e-5, False)
# the mask and wake scrolls add a term of the game clock, whose step times can differ by 1 ms
# between runs (allowed 1e-3; a scroll at the display rate is off by 0.3 and more)
for k in 'mu mv ku kv'.split(): RULES[k] = (None, 1e-3, False)
for k in 'sprite tflash pfx'.split(): RULES[k] = (None, None, False)
RULES['cross'] = (None, 0.021, False)   # a real-time spin: up to one 30 Hz frame apart
for k in 'px py pz vx vy vz'.split(): RULES[k] = (None, 1e-3, False)  # the scripted drive
RULES['rngd'] = (None, None, True)      # the random-number state after the step (before drawing)
RULES['rng'] = (None, None, True)       # and after drawing it: reported only

ref = load('30')
if not ref:
    print('no 30 fps log'); sys.exit(1)
logs = {r: load(r) for r in rates}
fail = bool(failed)
print('\n| value | rate | first differing step | max abs diff | result |\n|---|---|---|---|---|')
for key, (upto, tol, info) in RULES.items():
    for r in rates:
        other = logs[r]
        first, worst = None, 0.0
        for s in sorted(set(ref) & set(other)):
            # step 0 also holds the frames drawn between the scene's setup and the first step
            if s == 0 or (upto is not None and s >= upto):
                continue
            a, b = ref[s].get(key), other[s].get(key)
            if a is None or b is None:
                continue
            if tol is None:
                bad = a != b
                if bad and not info:
                    worst = max(worst, abs(float(a) - float(b)))
            else:
                d = abs(float(a) - float(b)); worst = max(worst, d); bad = d > tol
            if bad and first is None:
                first = s
        # known follow-up: above 144 fps the scripted drive drifts by centimetres late in the drive
        # (deterministic, also before R6); reported, does not fail
        known = key in ('px', 'py', 'pz', 'vx', 'vy', 'vz') and int(r) > 144
        res = 'same' if first is None else ('differs (info)' if info else 'differs (known)' if known else 'DIFFERS')
        fail |= res == 'DIFFERS'
        print('| %s | %s | %s | %.6g | %s |' % (key, r, '-' if first is None else first, worst, res))
print('\nmean frames per step: ' + ', '.join('%s fps %.2f' % (r, sum(float(e['frames']) for e in l.values()) / max(1, len(l)))
                                          for r, l in [('30', ref)] + list(logs.items())))
n = lambda k: sum(1 for s, e in ref.items() if s < 150 and int(e[k]) > 0)
print('exercised in the fixed scene (30 fps steps with a non-empty list): glass %d, price texts %d, markers %d'
      % (n('glassmax'), n('msgmax'), n('mkmax')))
if failed:
    print('runs failed: ' + ', '.join(failed))
sys.exit(1 if fail else 0)
PY

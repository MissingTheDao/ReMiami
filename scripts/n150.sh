#!/usr/bin/env bash
# The N150 test box (plan tasks 2, 14, 22, 24): a weak Intel iGPU (Mesa ANV / iris) on a 4K TV,
# the performance floor of the port. Everything runs over SSH in the box's desktop session; the box
# needs no build tools (builds are made here and copied).
#
# usage: scripts/n150.sh <command> [args]
#   deploy                   build release, copy it with assets/, tests/ and game/ to the box
#   test [--backend vulkan|gl] [scenario ...]    (Vulkan by default; --wgpu accepted, no effect)
#                            run the regression suite there (Intel driver: catches driver-specific
#                            rendering bugs; runs alongside a local suite as a second runner)
#   perf [--runs N] [--sizes "WxH ..."] [--backends "vulkan gl"]
#                            frame times at the fixed spots; saves logs/n150/perf-<time>.txt and
#                            compares it with tests/perf/n150-baseline.txt (slower by more than
#                            REVC_PERF_TOLERANCE, default 10 %, at any spot: exit 1)
#                            The box is fanless: every run waits until the CPU package is at or below
#                            N150_COOL_C (55) C, and the backend order alternates between runs.
#   wait <job>               collect a perf/soak job (they run detached on the box and survive
#                            this machine going away; perf and soak wait for theirs themselves)
#   baseline [FILE]          make a perf result (default: the latest) the new baseline
#   soak [--minutes M] [--backend B] [--size WxH] [--capped]
#                            repeat the spots run for M minutes (default 30): every run's frame
#                            times, failures with their logs, and every 5 s the CPU package
#                            temperature, CPU/GPU clocks and the game's memory; reports drift
#                            (thermal throttling, leaks) and crashes. Saved under logs/n150/.
#                            --capped: player defaults (frame limiter: display refresh, 60 Hz headless).
#   info                     hardware, driver versions, temperatures
# Host: N150_HOST (default steam@192.168.20.239), folder there: N150_DIR (default revc-bench).
# Game runs (test, perf, soak) are headless by default: inside a private, invisible compositor
# (scripts/headless.sh), so the box's screen may be locked, off or in use. N150_SCREEN=1 runs them
# on its desktop instead (then it must be unlocked and awake; that is what the TV playtests need).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
HOST=${N150_HOST:-steam@192.168.20.239}
DIR=${N150_DIR:-revc-bench}
LOCAL=$ROOT/logs/n150
BASELINE=$ROOT/tests/perf/n150-baseline.txt
TOL=${REVC_PERF_TOLERANCE:-0.10}
SCREEN=${N150_SCREEN:-0}
# Prefix for game-running commands on the box.
if [[ "$SCREEN" == 1 ]]; then RUN=""; else RUN="scripts/headless.sh"; fi
mkdir -p "$LOCAL"

# Runs a bash script on the box inside its graphical session (Wayland socket of user 1000).
remote() {
  ssh -o BatchMode=yes "$HOST" bash -s -- "$@" <<EOF
set -uo pipefail
export XDG_RUNTIME_DIR=/run/user/\$(id -u)
if [[ $SCREEN == 1 ]]; then
  export WAYLAND_DISPLAY=\$(basename "\$(ls -d \$XDG_RUNTIME_DIR/wayland-[0-9] 2>/dev/null | head -1)")
  [[ -n "\$WAYLAND_DISPLAY" ]] || { echo "no desktop session on the box (log in on its screen)"; exit 2; }
fi
cd ~/$DIR || exit 2
$REMOTE_SCRIPT
EOF
}

# The game's window must be on a visible, unlocked screen: otherwise the compositor sends no frame
# callbacks, Vulkan is throttled (~20 fps) and GL waits forever. Only with N150_SCREEN=1 (headless
# runs don't use the screen). Override: N150_ALLOW_LOCKED=1.
check_screen() {
  [[ "$SCREEN" != 1 || "${N150_ALLOW_LOCKED:-0}" == 1 ]] && return 0
  local state
  state=$(ssh -o BatchMode=yes "$HOST" bash -s <<'EOF'
s=$(loginctl list-sessions --no-legend | awk '$4 == "seat0" { print $1; exit }')
locked=$(loginctl show-session "$s" -p LockedHint --value 2>/dev/null)
on=0; for c in /sys/class/drm/card*-*; do [[ $(cat $c/status) == connected && $(cat $c/dpms) == On ]] && on=1; done
echo "$locked $on"
EOF
)
  case "$state" in
    "no 1") return 0 ;;
    yes*) echo "the box's screen is locked: unlock it (and keep it from locking while testing)"; exit 3 ;;
    *) echo "the box's display is off ($state): wake it (and keep it from blanking while testing)"; exit 3 ;;
  esac
}

# Mean frame time per backend/size/spot of a result file (several runs averaged), sorted.
means() {
  awk '!/^#/ { k = $1 " " $2 " " $3; s[k] += $4; n[k]++ } END { for (k in s) printf "%s %.3f\n", k, s[k] / n[k] }' "$1" | sort
}

compare() {
  local result=$1
  if [[ ! -f "$BASELINE" ]]; then
    echo "no baseline yet (scripts/n150.sh baseline makes one from this run)"
    return 0
  fi
  echo "== against $(basename "$BASELINE") (tolerance ${TOL})"
  # Results are comparable only with the same display state (header: "display on|off").
  local db dr
  db=$(head -1 "$BASELINE" | sed -n 's/.*display \([a-z]*\).*/\1/p'); dr=$(head -1 "$result" | sed -n 's/.*display \([a-z]*\).*/\1/p')
  [[ "${db:-?}" != "${dr:-?}" ]] && echo "   WARNING: display ${dr:-unknown} now, ${db:-unknown} for the baseline: not comparable (the box's visible desktop costs ~1.9 ms a frame)"
  join <(means "$BASELINE" | awk '{ print $1 "/" $2 "/" $3, $4 }') <(means "$result" | awk '{ print $1 "/" $2 "/" $3, $4 }') |
    awk -v tol="$TOL" '
      { r = $3 / $2; flag = r > 1 + tol ? "  SLOWER" : (r < 1 - tol ? "  faster" : "");
        printf "   %-40s %7.2f -> %7.2f ms (%+.0f%%)%s\n", $1, $2, $3, (r - 1) * 100, flag; if (r > 1 + tol) bad++ }
      END { if (bad) { printf "%d spot(s) slower than the baseline\n", bad; exit 1 } else print "no regressions" }'
}

# Long jobs run detached on the box (nohup), so they survive this machine or the connection going
# away: results/<job>.log (progress), results/<job>.txt (data), results/<job>.done when finished.
start_job() {
  local name=$1 body=$2
  REMOTE_SCRIPT="mkdir -p results
cat > results/$name.sh <<'JOB'
trap 'touch results/$name.done' EXIT
$body
JOB
nohup $RUN bash results/$name.sh > results/$name.log 2>&1 < /dev/null &
echo \"started $name on $HOST (scripts/n150.sh wait $name collects it)\"" remote
}

# Waits for a job, prints its log, copies its data to logs/n150/ and evaluates it.
wait_job() {
  local name=$1
  until ssh -o BatchMode=yes "$HOST" "test -f $DIR/results/$name.done"; do sleep 15; done
  ssh -o BatchMode=yes "$HOST" "cat $DIR/results/$name.log"
  scp -q "$HOST:$DIR/results/$name.txt" "$LOCAL/" || return 1
  scp -q "$HOST:$DIR/results/$name-sensors.txt" "$LOCAL/" 2>/dev/null
  echo "saved logs/n150/$name.txt"
  case "$name" in
    perf-*) compare "$LOCAL/$name.txt" ;;
    soak-*)
      # Drift: mean frame time of the first and the last quarter of the measurements.
      awk '!/^#/ { v[++n] = $4 } END { q = int(n / 4); if (q < 1) exit; for (i = 1; i <= q; i++) a += v[i]; for (i = n - q + 1; i <= n; i++) b += v[i];
        printf "drift: first quarter %.2f ms, last quarter %.2f ms (%+.0f%%)\n", a / q, b / q, (b / a - 1) * 100 }' "$LOCAL/$name.txt" ;;
  esac
}

cmd=${1:-}; shift || true
case "$cmd" in
  deploy)
    (cd "$ROOT/rust" && cargo build --locked -q --release) || exit 1
    STAGE=$(mktemp -d); trap 'rm -rf "$STAGE"' EXIT
    mkdir -p "$STAGE/bin" "$STAGE/scripts"
    cp "$ROOT/rust/target/release/revc" "$STAGE/bin/revc"
    cp "$ROOT/scripts/perf.sh" "$ROOT/scripts/test.sh" "$ROOT/scripts/headless.sh" "$STAGE/scripts/"
    cp -r "$ROOT/tests" "$ROOT/assets" "$STAGE/"   # assets: controller glyphs, found next to bin/
    git -C "$ROOT" describe --always --dirty > "$STAGE/COMMIT"
    rsync -a --delete --exclude game --exclude rust --exclude results "$STAGE/" "$HOST:$DIR/" || exit 1
    # Your own game files (never committed); user mods and settings stay here.
    rsync -a --delete --exclude mods --exclude userfiles --exclude reVC.ini "$ROOT/game/" "$HOST:$DIR/game/" || exit 1
    echo "deployed $(cat "$STAGE/COMMIT") to $HOST:$DIR"
    ;;
  test)
    check_screen
    BACKEND=vulkan ARGS=()
    while (( $# )); do
      case "$1" in
        --wgpu) ;;
        --backend) BACKEND=$2; shift ;;
        *) ARGS+=("$1") ;;
      esac
      shift
    done
    REMOTE_SCRIPT="REVC_RENDERER_BACKEND=$BACKEND REVC_TEST_BIN=\$PWD/bin/revc $RUN scripts/test.sh ${ARGS[*]:-}" remote
    ;;
  perf)
    check_screen
    RUNS=1 SIZES="1280x720 1920x1080" BACKENDS="vulkan gl" COOL=${N150_COOL_C:-55}
    while (( $# )); do
      case "$1" in
        --runs) RUNS=$2; shift ;;
        --sizes) SIZES=$2; shift ;;
        --backends) BACKENDS=$2; shift ;;
        *) echo "unknown option $1"; exit 2 ;;
      esac
      shift
    done
    NAME=perf-$(date +%Y%m%d-%H%M%S)
    start_job "$NAME" "
# The TV's state matters: compositing a visible 4K desktop costs the game ~1.9 ms a frame here.
tv=off; for c in /sys/class/drm/card*-*; do [[ \$(cat \$c/status) == connected && \$(cat \$c/dpms) == On ]] && tv=on; done
echo \"# \$(cat COMMIT) runs $RUNS display \$tv\" > results/$NAME.txt
# The box has no fan: under load it reaches ~95 C and throttles. Each run starts cooled down to
# $COOL C (at most 10 min of waiting), runs are one at a time and the backend order alternates, so
# no backend always measures on a hotter chip. The start temperature is logged as a comment.
z=\$(grep -l x86_pkg_temp /sys/class/thermal/thermal_zone*/type | head -1); z=\${z%/type}/temp
cool() { local i=0; while (( \$(cat \$z) / 1000 > $COOL && i < 120 )); do sleep 5; i=\$((i + 1)); done; echo \$(( \$(cat \$z) / 1000 )); }
order=\"$BACKENDS\"
for r in \$(seq $RUNS); do
  for s in $SIZES; do for b in \$order; do
    c=\$(cool); echo \"# run \$r \$b \$s start \$c C\" >> results/$NAME.txt
    echo \"== run \$r \$b \$s (start \$c C)\"
    timeout -k 10 300 scripts/perf.sh --backend \$b --size \$s --runs 1 --out results/$NAME.txt --logs results/failed |
      grep -E 'all spots|failed' || echo \"   \$b \$s: no result (hung or crashed; killed after the timeout)\"
  done; done
  order=\$(echo \$order | tr ' ' '\\n' | tac | tr '\\n' ' ')
done"
    wait_job "$NAME"
    ;;
  wait)
    [[ -n "${1:-}" ]] || { echo "usage: n150.sh wait <job> (perf-... or soak-..., see logs/n150 or the box's results/)"; exit 2; }
    wait_job "$1"
    ;;
  baseline)
    SRC=${1:-$(ls -t "$LOCAL"/perf-*.txt 2>/dev/null | head -1)}
    [[ -f "$SRC" ]] || { echo "no perf result (scripts/n150.sh perf)"; exit 1; }
    mkdir -p "$(dirname "$BASELINE")"
    cp "$SRC" "$BASELINE"
    echo "baseline: $(basename "$SRC") -> ${BASELINE#$ROOT/}"
    ;;
  soak)
    check_screen
    MIN=30 BACKEND=vulkan SIZE=1280x720 CAP=""
    while (( $# )); do
      case "$1" in
        --minutes) MIN=$2; shift ;;
        --backend) BACKEND=$2; shift ;;
        --size) SIZE=$2; shift ;;
        --capped) CAP=--capped ;;
        *) echo "unknown option $1"; exit 2 ;;
      esac
      shift
    done
    NAME=soak-$(date +%Y%m%d-%H%M%S)
    start_job "$NAME" "
out=results/$NAME.txt; mon=results/$NAME-sensors.txt
echo \"# \$(cat COMMIT) soak $MIN min $BACKEND $SIZE $CAP\" > \$out
echo '# time pkg_temp_C cpu_avg_MHz gpu_MHz revc_rss_MB' > \$mon
( while :; do
    z=\$(grep -l x86_pkg_temp /sys/class/thermal/thermal_zone*/type | head -1)
    temp=\$(( \$(cat \${z%/type}/temp) / 1000 ))
    mhz=\$(awk '/cpu MHz/ { s += \$4; n++ } END { printf \"%.0f\", s / n }' /proc/cpuinfo)
    gpu=\$(cat /sys/class/drm/card*/gt_act_freq_mhz 2>/dev/null | head -1)
    rss=\$(ps -C revc -o rss= 2>/dev/null | awk '{ s += \$1 } END { printf \"%.0f\", s / 1024 }')
    echo \"\$(date +%s) \$temp \$mhz \${gpu:-0} \$rss\" >> \$mon
    sleep 5
  done ) &
sampler=\$!
end=\$(( \$(date +%s) + $MIN * 60 )); runs=0; fails=0
while (( \$(date +%s) < end )); do
  runs=\$((runs + 1))
  before=\$(grep -vc '^#' \$out)
  timeout -k 10 300 scripts/perf.sh --backend $BACKEND --size $SIZE $CAP --out \$out --logs results/failed > /dev/null
  got=\$(( \$(grep -vc '^#' \$out) - before ))
  (( got < 8 )) && fails=\$((fails + 1))
  echo \"run \$runs: \$( (( got >= 8 )) && tail -8 \$out | awk '{ s += \$4 } END { printf \"mean %.2f ms\", s / NR }' || echo FAILED)\"
done
kill \$sampler
echo \"soak $MIN min $BACKEND $SIZE: \$runs runs, \$fails failed\"
echo '-- sensors: first, last, highest package temperature'
sed -n 2p \$mon; tail -1 \$mon; awk 'NR > 1 && \$2 > m { m = \$2 } END { print \"max package temp \" m \" C\" }' \$mon"
    wait_job "$NAME"
    ;;
  info)
    REMOTE_SCRIPT='
echo "commit deployed: $(cat COMMIT 2>/dev/null)"
lscpu | grep "Model name"; free -h | sed -n 2p
vulkaninfo --summary 2>/dev/null | grep -E "deviceName|driverInfo" | head -2
for z in /sys/class/thermal/thermal_zone*; do echo "$(cat $z/type) $(( $(cat $z/temp) / 1000 )) C"; done
for c in /sys/class/drm/card*-*; do [[ $(cat $c/status) == connected ]] && echo "$(basename $c) $(head -1 $c/modes)"; done; true' remote
    ;;
  *)
    sed -n '2,/^set -uo/p' "$0" | sed '$d; s/^# \{0,1\}//'
    exit 2
    ;;
esac

#!/usr/bin/env bash
# Runs a command inside a private, invisible Wayland compositor (KDE's kwin_wayland --virtual), so
# game runs need no visible, unlocked screen: a locked or blanked desktop sends no frame callbacks
# (Vulkan throttled, GL stalled), the virtual output always does. The GPU still renders; only the
# compositor's output goes nowhere.
#
# usage: scripts/headless.sh [--size WxH] command [args ...]   (default size 4096x2304, room for a
#        3840x2160 window with its decorations)
set -u
# Run in a memory-capped systemd scope of its own (REVC_RUN_MEM, default 5G; REVC_RUN_MEM=0 for
# none): a game run that grows can then only kill itself, never the session or app that started
# it (systemd-oomd acts on the whole cgroup that runs out).
if [[ -z ${REVC_IN_SCOPE:-} && ${REVC_RUN_MEM:-5G} != 0 ]] && command -v systemd-run > /dev/null; then
  export REVC_IN_SCOPE=1
  exec systemd-run --user --scope --quiet --collect -p MemoryMax="${REVC_RUN_MEM:-5G}" -p MemorySwapMax=1G \
    -p ManagedOOMMemoryPressure=kill -- "$0" "$@"
fi
# Temporary files on disk: /tmp is often RAM (tmpfs), and run frames and shots pushed into swap
# there were behind systemd-oomd killing the desktop session's apps (2026-11-11). REVC_TMPDIR picks
# another place.
export TMPDIR=${REVC_TMPDIR:-/var/tmp}
SIZE=4096x2304
[[ "${1:-}" == --size ]] && { SIZE=$2; shift 2; }
(( $# )) || { echo "usage: headless.sh [--size WxH] command [args ...]"; exit 2; }
command -v kwin_wayland > /dev/null || { echo "headless.sh: kwin_wayland not installed"; exit 2; }
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
sock=revc-headless-$$
log=$(mktemp)
unset DISPLAY WAYLAND_DISPLAY
kwin_wayland --virtual --socket "$sock" --width "${SIZE%x*}" --height "${SIZE#*x}" \
  --no-lockscreen --no-global-shortcuts > "$log" 2>&1 &
kwin=$!
# The local model (bin/ai-serve.sh) would take most of the VRAM: stop it for the run and restart it
# afterwards if it was running (bin/ai-pause.sh counts overlapping runs).
AI_PAUSE_PID=$$ "$(dirname "$0")/../bin/ai-pause.sh" hold
trap 'kill $kwin 2>/dev/null; wait $kwin 2>/dev/null; rm -f "$log"; AI_PAUSE_PID=$$ "$(dirname "$0")/../bin/ai-pause.sh" release' EXIT
for _ in {1..100}; do [[ -S $XDG_RUNTIME_DIR/$sock ]] && break; sleep 0.1; done
[[ -S $XDG_RUNTIME_DIR/$sock ]] || { echo "headless.sh: compositor did not start:"; cat "$log"; exit 2; }
WAYLAND_DISPLAY=$sock "$@"

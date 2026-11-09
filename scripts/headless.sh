#!/usr/bin/env bash
# Runs a command inside a private, invisible Wayland compositor (KDE's kwin_wayland --virtual), so
# game runs need no visible, unlocked screen: a locked or blanked desktop sends no frame callbacks
# (Vulkan throttled, GL stalled), the virtual output always does. The GPU still renders; only the
# compositor's output goes nowhere.
#
# usage: scripts/headless.sh [--size WxH] command [args ...]   (default size 2560x1440, room for a
#        1080p window with its decorations)
set -u
SIZE=2560x1440
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
trap 'kill $kwin 2>/dev/null; wait $kwin 2>/dev/null; rm -f "$log"' EXIT
for _ in {1..100}; do [[ -S $XDG_RUNTIME_DIR/$sock ]] && break; sleep 0.1; done
[[ -S $XDG_RUNTIME_DIR/$sock ]] || { echo "headless.sh: compositor did not start:"; cat "$log"; exit 2; }
WAYLAND_DISPLAY=$sock "$@"

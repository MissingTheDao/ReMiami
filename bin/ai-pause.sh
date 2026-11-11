#!/usr/bin/env bash
# Frees the GPU from the local model while a game or emulator run needs it, and gives it back after.
#   bin/ai-pause.sh hold      (at the start of a run: stops revc-ai if it runs, remembers that)
#   bin/ai-pause.sh release   (at the end: restarts it AI_RESUME_DELAY s (120) after the last run ends)
# Holds are counted per process (several runs at once are fine); dead holders are ignored. While a
# hold exists, ask-local.py / the MCP bridge don't auto-start the server.
set -u
S=$HOME/.local/state/local-ai; H=$S/hold; mkdir -p "$H"
me=${AI_PAUSE_PID:-$PPID}
prune() { for f in "$H"/*; do [[ -e $f ]] || continue; kill -0 "$(basename "$f")" 2>/dev/null || rm -f "$f"; done; }
case "${1:-}" in
  hold)
    prune; touch "$H/$me"
    if systemctl --user is-active --quiet revc-ai; then
      touch "$S/was-running"; systemctl --user stop revc-ai; echo "ai-pause: local model stopped for this run" >&2
    fi ;;
  release)
    rm -f "$H/$me"; prune
    if [[ -z $(ls -A "$H") && -e $S/was-running ]]; then
      # Debounced: test.sh runs one headless run per scenario, and reloading the model between them
      # (~45 s of disk, CPU and VRAM traffic) would disturb the next run. Restart only after
      # AI_RESUME_DELAY seconds (default 120) with no new hold; the first waiter to claim the marker wins.
      setsid bash -c '
        sleep "${AI_RESUME_DELAY:-120}"
        [[ -z $(ls -A "$1/hold" 2>/dev/null) ]] || exit 0
        mv "$1/was-running" "$1/was-running.$$" 2>/dev/null || exit 0
        rm -f "$1/was-running.$$"
        exec "$2/ai-start.sh"' _ "$S" "$(dirname "$0")" > /dev/null 2>&1 < /dev/null &
      echo "ai-pause: local model restarts in ${AI_RESUME_DELAY:-120} s if no other run starts" >&2
    fi ;;
  *) echo "usage: ai-pause.sh hold|release" >&2; exit 2 ;;
esac

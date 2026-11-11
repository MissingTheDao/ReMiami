#!/usr/bin/env bash
# Frees the GPU from the local model while a game or emulator run needs it, and gives it back after.
#   bin/ai-pause.sh hold      (at the start of a run: stops revc-ai if it runs, remembers that)
#   bin/ai-pause.sh release   (at the end: restarts it in the background once no run holds it)
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
      rm -f "$S/was-running"
      setsid "$(dirname "$0")/ai-start.sh" > /dev/null 2>&1 < /dev/null &
      echo "ai-pause: restarting the local model" >&2
    fi ;;
  *) echo "usage: ai-pause.sh hold|release" >&2; exit 2 ;;
esac

#!/usr/bin/env bash
# Frees the GPU from the local model while a game or emulator run needs it, and gives it back after.
#   bin/ai-pause.sh hold      (at the start of a run: stops revc-ai if it runs, remembers that;
#                              waits for a request in progress, exit 1 if it does not finish)
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
      # Never cut a request off: wait until the server works on nothing (AI_HOLD_WAIT s, default
      # 600), then stop it. Still busy after that: no hold, exit 1, and the caller must not run.
      url=${LOCAL_AI_URL:-http://127.0.0.1:8091}/slots; waited=0
      while curl -s -m 2 "$url" | grep -q '"is_processing": *true'; do
        if (( waited >= ${AI_HOLD_WAIT:-600} )); then
          rm -f "$H/$me"
          echo "ai-pause: the local model is still working on a request after ${waited} s; not stopped, run not started (try again later)" >&2
          exit 1
        fi
        (( waited == 0 )) && echo "ai-pause: waiting for the local model to finish its request" >&2
        sleep 3; waited=$((waited + 3))
      done
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
        for f in "$1"/hold/*; do [[ -e $f ]] && ! kill -0 "${f##*/}" 2>/dev/null && rm -f "$f"; done  # crashed runs
        [[ -z $(ls -A "$1/hold" 2>/dev/null) ]] || exit 0
        mv "$1/was-running" "$1/was-running.$$" 2>/dev/null || exit 0
        rm -f "$1/was-running.$$"
        exec "$2/ai-start.sh"' _ "$S" "$(dirname "$0")" > /dev/null 2>&1 < /dev/null &
      echo "ai-pause: local model restarts in ${AI_RESUME_DELAY:-120} s if no other run starts" >&2
    fi ;;
  *) echo "usage: ai-pause.sh hold|release" >&2; exit 2 ;;
esac

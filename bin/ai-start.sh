#!/usr/bin/env bash
# Starts the local AI server (bin/ai-serve.sh) as its own systemd user service with a memory limit,
# outside whatever app launched it: if memory runs out, only the server is killed (on 2026-11-11 a
# server started inside the Claude app's cgroup got the whole app killed by systemd-oomd).
#   bin/ai-start.sh          start (or restart) it      bin/ai-start.sh stop    stop it
# MEMHIGH / MEMMAX override the soft and hard limits (default 4G / 6G of system RAM; the model
# itself lives in VRAM, only the CPU expert layers and buffers use RAM).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
UNIT=revc-ai
systemctl --user stop "$UNIT" 2>/dev/null || true
systemctl --user reset-failed "$UNIT" 2>/dev/null || true
[[ "${1:-}" == stop ]] && { echo "stopped"; exit 0; }
systemd-run --user --unit="$UNIT" --collect \
  -p MemoryHigh="${MEMHIGH:-4G}" -p MemoryMax="${MEMMAX:-6G}" -p MemorySwapMax=1G \
  "$HERE/ai-serve.sh" "$@"
for i in $(seq 1 120); do
  curl -s -m 1 http://127.0.0.1:8091/health | grep -q ok && { echo "revc-ai up (journalctl --user -u $UNIT for its log)"; exit 0; }
  sleep 1
done
echo "revc-ai did not come up; see journalctl --user -u $UNIT" >&2
exit 1

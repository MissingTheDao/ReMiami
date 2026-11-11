#!/usr/bin/env bash
# Starts the local AI server (bin/ai-serve.sh) as its own systemd user service with a memory limit,
# outside whatever app launched it: if memory runs out, only the server is killed (on 2026-11-11 a
# server started inside the Claude app's cgroup got the whole app killed by systemd-oomd).
#   bin/ai-start.sh          start (or restart) it      bin/ai-start.sh stop    stop it
# MEMHIGH / MEMMAX override the soft and hard limits. Defaults follow the RAM (ai-serve.sh picks the
# matching profile): 4G / 6G below 28 GiB, 13G / 16G from 28 GiB (2x16 GB: the Q4 model keeps ~10 GB
# of expert layers in RAM). The rest of the model lives in VRAM.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
RAM_GIB=$(awk '/^MemTotal:/ {print int($2 / 1048576)}' /proc/meminfo)
if (( RAM_GIB >= 28 )); then DEF_HIGH=13G DEF_MAX=16G; else DEF_HIGH=4G DEF_MAX=6G; fi
UNIT=revc-ai
systemctl --user stop "$UNIT" 2>/dev/null || true
systemctl --user reset-failed "$UNIT" 2>/dev/null || true
[[ "${1:-}" == stop ]] && { echo "stopped"; exit 0; }
systemd-run --user --unit="$UNIT" --collect \
  -p MemoryHigh="${MEMHIGH:-$DEF_HIGH}" -p MemoryMax="${MEMMAX:-$DEF_MAX}" -p MemorySwapMax=1G \
  --setenv=PROFILE="${PROFILE:-}" --setenv=MODEL="${MODEL:-}" --setenv=NCPUMOE="${NCPUMOE:-}" --setenv=CTX="${CTX:-}" \
  "$HERE/ai-serve.sh" "$@"
for i in $(seq 1 300); do	# a 22 GB model can take minutes to load from disk
  curl -s -m 1 http://127.0.0.1:8091/health | grep -q ok && { echo "revc-ai up (journalctl --user -u $UNIT for its log)"; exit 0; }
  sleep 1
done
echo "revc-ai did not come up; see journalctl --user -u $UNIT" >&2
exit 1

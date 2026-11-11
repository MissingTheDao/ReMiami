#!/usr/bin/env bash
# Starts a local OpenAI-compatible server on http://127.0.0.1:8091 (model alias qwen3.6-35b-a3b).
# The profile follows the installed RAM (PROFILE=small|large overrides):
#   small (< 28 GiB RAM): Qwen3.6-35B-A3B UD-Q3_K_M, 6 expert layers in RAM, 32K context
#   large (>= 28 GiB, e.g. 2x16 GB): UD-Q4_K_M if downloaded (~/Projects/local-ai-ram-upgrade.sh), else Q3_K_M;
#                                    more expert layers in RAM and a 64K context
# NCPUMOE = expert layers kept in system RAM; raise it if you hit out-of-VRAM, lower it for speed.
# MODEL / CTX / PORT override the profile's choices.
AI=/home/steam/Projects/revc/ai
RAM_GIB=$(awk '/^MemTotal:/ {print int($2 / 1048576)}' /proc/meminfo)
PROFILE=${PROFILE:-$( (( RAM_GIB >= 28 )) && echo large || echo small)}
Q3=$AI/models/Qwen3.6-35B-A3B-UD-Q3_K_M.gguf
Q4=$AI/models/Qwen3.6-35B-A3B-UD-Q4_K_M.gguf
if [[ $PROFILE == large && -f $Q4.done ]]; then
  MODEL=${MODEL:-$Q4}; NCPUMOE=${NCPUMOE:-18}; CTX=${CTX:-65536}
elif [[ $PROFILE == large ]]; then
  MODEL=${MODEL:-$Q3}; NCPUMOE=${NCPUMOE:-6}; CTX=${CTX:-65536}
else
  MODEL=${MODEL:-$Q3}; NCPUMOE=${NCPUMOE:-6}; CTX=${CTX:-32768}
fi
PORT=${PORT:-8091}
echo "ai-serve: profile $PROFILE (${RAM_GIB} GiB RAM), $(basename "$MODEL"), n-cpu-moe $NCPUMOE, ctx $CTX" >&2
exec "$AI/llama.cpp/build/bin/llama-server" -m "$MODEL" --alias qwen3.6-35b-a3b \
  -ngl 99 --n-cpu-moe "$NCPUMOE" -c "$CTX" \
  -fa on --cache-type-k q8_0 --cache-type-v q8_0 \
  --temp 0.6 --top-p 0.95 --top-k 20 --min-p 0.0 \
  --host 127.0.0.1 --port "$PORT" "$@"

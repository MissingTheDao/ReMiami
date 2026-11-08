#!/usr/bin/env bash
# Starts a local OpenAI-compatible server on http://127.0.0.1:8091
# NCPUMOE = expert layers kept in system RAM; raise it if you hit out-of-VRAM, lower it for speed.
NCPUMOE=${NCPUMOE:-6}
CTX=${CTX:-32768}
PORT=${PORT:-8091}
exec "/home/steam/Projects/revc/ai/llama.cpp/build/bin/llama-server" -m "/home/steam/Projects/revc/ai/models/Qwen3.6-35B-A3B-UD-Q3_K_M.gguf" \
  -ngl 99 --n-cpu-moe "$NCPUMOE" -c "$CTX" \
  -fa on --cache-type-k q8_0 --cache-type-v q8_0 \
  --temp 0.6 --top-p 0.95 --top-k 20 --min-p 0.0 \
  --host 127.0.0.1 --port "$PORT" "$@"

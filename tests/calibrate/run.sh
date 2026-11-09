#!/usr/bin/env bash
# Measures OpenAL Soft's stereo gains (pan law, distance attenuation, 2D stereo) with the game's
# listener setup, using OpenAL Soft's wave driver. Output: gains per test position.
# The Rust mixer (rust/revc-audio) is tested against openal-soft-gains.txt.
set -euo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
cc -O1 -o "$TMP/openal-pan" "$HERE/openal-pan.c" -lopenal -lm
printf '[general]\ndrivers = wave\nfrequency = 48000\nchannels = stereo\nsample-type = float32\n[wave]\nfile = %s/pan.wav\n' "$TMP" > "$TMP/conf"
ALSOFT_CONF="$TMP/conf" "$TMP/openal-pan" > "$TMP/steps.txt"
python3 -I "$HERE/measure.py" "$TMP/pan.wav" "$TMP/steps.txt"

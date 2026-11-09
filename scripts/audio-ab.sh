#!/usr/bin/env bash
# A/B audio comparison: plays the scripted "audio" scene with the original OpenAL audio and with
# the Rust mixer, records both, and compares them (loudness, L/R balance, frequency bands,
# loudness over time). usage: scripts/audio-ab.sh [all|sfx|streams]
#   all      candidate = everything on the Rust mixer (default)
#   sfx      candidate = only sound effects (+ reverb) on Rust, streams on OpenAL
#   streams  candidate = only streams on Rust, sound effects on OpenAL
#   baseline candidate = OpenAL again: shows how much two runs of the same build differ
# Both runs are audible (sound effects at normal volume; music is muted in tests/config/audio.ini
# because where the hotel track starts varies per run and swamps everything else).
# Noise: two runs of the same build differ by up to ~1.2 dB level, ~0.8 dB balance, ~1.3 dB per
# band, so one run catches gross breakage, not 1-2 dB differences. The exact check is the
# calibration test (rust/revc-audio, tests/calibrate/openal-soft-gains.txt).
# Leave the window alone while it runs.
# Output: rust/target/test-output/audio-{openal,rust}.wav and the comparison report.
set -uo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WHAT=${1:-all}
case "$WHAT" in
  all) CAND_ARGS="" ;;
  sfx) CAND_ARGS="-DREVC_RUST_SFX=ON -DREVC_RUST_STREAMS=OFF" ;;
  streams) CAND_ARGS="-DREVC_RUST_SFX=OFF -DREVC_RUST_STREAMS=ON" ;;
  baseline) CAND_ARGS="" ;;
  *) echo "usage: $0 [all|sfx|streams|baseline]" >&2; exit 2 ;;
esac
OUT=$ROOT/rust/target/test-output; mkdir -p "$OUT"
cd "$ROOT/rust"

echo "== build: candidate ($WHAT on the Rust mixer)"
if [[ "$WHAT" == all ]]; then
  CAND_BIN=$ROOT/rust/target/debug/revc
  cargo build --locked -q || exit 1
elif [[ "$WHAT" == baseline ]]; then
  CAND_BIN=$ROOT/rust/target/ab-openal/debug/revc # built below
else
  CAND_BIN=$ROOT/rust/target/ab-$WHAT/debug/revc
  REVC_CMAKE_ARGS="$CAND_ARGS" CARGO_TARGET_DIR="$ROOT/rust/target/ab-$WHAT" cargo build --locked -q || exit 1
fi
echo "== build: OpenAL reference (separate target dir)"
REVC_CMAKE_ARGS="-DREVC_RUST_SFX=OFF -DREVC_RUST_STREAMS=OFF" CARGO_TARGET_DIR="$ROOT/rust/target/ab-openal" \
  cargo build --locked -q || exit 1
cargo build --locked -q --release -p revc-audio --bin audiocmp || exit 1

# OpenAL Soft's wave driver writes everything OpenAL plays to a file instead of the speakers.
CONF=$(mktemp); trap 'rm -f "$CONF"' EXIT
cat > "$CONF" <<CFG
[general]
drivers = wave
frequency = 48000
channels = stereo
sample-type = float32
[wave]
file = $OUT/audio-openal.wav
CFG
CAND_WAV=$OUT/audio-rust-$WHAT.wav
rm -f "$OUT/audio-openal.wav" "$CAND_WAV"

echo "== record: OpenAL"
ALSOFT_CONF=$CONF REVC_TEST_BIN="$ROOT/rust/target/ab-openal/debug/revc" "$ROOT/scripts/test.sh" audio || exit 1
cp "$OUT/audio.log" "$OUT/audio-openal.log"
echo "== record: candidate ($WHAT on Rust)"
if [[ "$WHAT" == all ]]; then
  REVC_TEST_WAV="$CAND_WAV" REVC_TEST_BIN="$CAND_BIN" "$ROOT/scripts/test.sh" audio || exit 1
  CAND_SPEC=$CAND_WAV
elif [[ "$WHAT" == baseline ]]; then
  sed "s|^file = .*|file = $CAND_WAV|" "$CONF" > "$CONF.part"
  ALSOFT_CONF="$CONF.part" REVC_TEST_BIN="$CAND_BIN" "$ROOT/scripts/test.sh" audio || exit 1
  rm -f "$CONF.part"
  CAND_SPEC=$CAND_WAV
else
  # A mixed build plays through both outputs: record both and compare their sum.
  sed "s|^file = .*|file = $OUT/audio-rust-$WHAT-openalpart.wav|" "$CONF" > "$CONF.part"
  ALSOFT_CONF="$CONF.part" REVC_TEST_WAV="$CAND_WAV" REVC_TEST_BIN="$CAND_BIN" "$ROOT/scripts/test.sh" audio || exit 1
  rm -f "$CONF.part"
  CAND_SPEC="$CAND_WAV+$OUT/audio-rust-$WHAT-openalpart.wav"
fi
cp "$OUT/audio.log" "$OUT/audio-rust-$WHAT.log"

echo "== compare (reference = OpenAL)"
"$ROOT/rust/target/release/audiocmp" "$OUT/audio-openal.wav" "$CAND_SPEC"

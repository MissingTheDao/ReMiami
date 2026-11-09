# OpenAL Soft calibration

`run.sh` builds `openal-pan.c`, plays a 1 kHz tone through OpenAL Soft (wave driver, so nothing
is audible) from 30 positions with the game's listener setup, and prints the left/right gains.
The result is saved as `openal-soft-gains.txt`; `rust/revc-audio` has a unit test
(`matches_measured_openal_soft_gains`) that requires the Rust mixer to produce the same gains.

Finding (OpenAL Soft 1.25.2): stereo panning is linear in the sideways component,
L = (1 - p)/2, R = (1 + p)/2, i.e. 0.5 per channel for a centred source (not constant power).

The game no longer uses OpenAL; re-measuring needs the `openal` package installed:

```bash
tests/calibrate/run.sh > tests/calibrate/openal-soft-gains.txt
```

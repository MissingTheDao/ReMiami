# Regression tests

Run with `scripts/test.sh` (see that script for options). Each run builds a throwaway game
folder: your game files are symlinked in from `game/` (or `REVC_GAME_DIR`), but settings and
saves come from here, so tests never touch your own config or saves.

| Path | What |
|---|---|
| `config/reVC.ini` | Fixed settings: windowed 1280x720, silent, no movies |
| `saves/GTAVCsf1.b` | Save used by the save-load test (slot 1) |
| `reference/*.png` | Approved screenshots; regenerate with `scripts/test.sh --bless` |

Scenarios (implemented in `rust/revc/src/testdrive.rs`):

- **boot**: reach the front-end menu and exit cleanly.
- **saveload**: load slot 1, wait 8 s of game time and for the fade-in to end, check the player position,
  capture the frame and compare it with `reference/saveload.png`.

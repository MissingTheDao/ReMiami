# Regression tests

Run with `scripts/test.sh` (see that script for options). Each run builds a throwaway game
folder: your game files are symlinked in from `game/` (or `REVC_GAME_DIR`), but settings and
saves come from here, so tests never touch your own config or saves.

| Path | What |
|---|---|
| `config/reVC.ini` | Fixed settings: windowed 1280x720, silent, no movies |
| `saves/GTAVCsf1.b` | Save used by the save-load test (slot 1) |
| `reference/*.png` | Approved screenshots; regenerate with `scripts/test.sh --bless` |
| `mods/` | Texture mods installed for the texmods scenario only (see `mods/README.md`) |

Scenarios (implemented in `rust/revc/src/testdrive.rs`):

- **boot**: reach the front-end menu and exit cleanly.
- **saveload**: load slot 1, wait 8 s of game time and for the fade-in to end, check the player position,
  capture the frame and compare it with `reference/saveload.png`.
- **formats**: load slot 1, then compare what the C++ loaders hold (model info from the IDEs,
  collision, GXT text, geometry of loaded models, texture sizes) with what `rust/revc-formats`
  reads from the same files (`rust/revc/src/formatcheck.rs`). Differences are listed in the log.
- **texmods**: like saveload with `mods/` installed: the HUD weapon icon, the radar save icon and the
  hotel floor are replaced, a broken file must be rejected (original kept); compared with
  `reference/texmods.png`.
- **audio** (not in the default set): scripted scene with sound; see `docs/BUILDING.md`.

`cargo test -p revc-formats` additionally parses every file of your install (`tests/install.rs`,
skipped without one).

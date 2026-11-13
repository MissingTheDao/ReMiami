# Building reVC

The repository holds reMiami's own work. The reVC C++ source (re3, "miami" branch) that the game
still builds from is **not** in it: you supply your own copy, and `scripts/revc-source.sh` turns it
into the C++ tree the build uses.

| Path | What |
|---|---|
| `rust/` | Cargo workspace: the `revc` launcher and SDL3 platform host |
| `cpp/revc/reVC.sha256` | The reVC files reMiami needs, with hashes (snapshot `dc21472`, 2021-09-02) |
| `cpp/revc/patches/` | reMiami's changes to those files |
| `cpp/revc/overlay/` | reMiami's own C++ files, and librw (aap/librw `5501c4f` plus our changes) |
| `cpp/re3-miami/` | Generated, never committed: your reVC files + patches + overlay |
| `scripts/` | `build.sh`, `check-reproducible.sh`, `deps.lock` |
| `bin/` | Local helpers (`ai-serve.sh`, `ask-local.py`) |

Never committed: `game/` (your private copy of Vice City), `ai/` (llama.cpp and models), build output.

## Requirements

Arch / CachyOS packages (exact versions the build was verified with are in `scripts/deps.lock`):

```bash
sudo pacman -S --needed rust clang cmake ninja sdl3 mpg123 libglvnd
```

Optional: `sccache` (C++ compile cache, used automatically when installed).

Rust is pinned in `rust/rust-toolchain.toml`. rustup honours it; with the system `rust` package,
`scripts/build.sh` checks the version instead.

## The reVC source

You need the reVC source at commit `dc2147268b17b86656b629d5370bf2e90229e618` (re3's "miami"
branch, 2021-09-02) as a folder, a `.zip`/`.tar.*` archive of it, or a git repository URL that
has that commit. reMiami never downloads it for you.

```bash
scripts/revc-source.sh prepare ~/Downloads/re3-dc21472.zip
```

It checks all 518 needed files against `cpp/revc/reVC.sha256` (a different version is refused),
then writes `cpp/re3-miami/`: those files, reMiami's patches applied, reMiami's own files copied in.
Files of the copy that reMiami doesn't need (game data, Windows libraries, old build files) are ignored.

Working on the C++: edit `cpp/re3-miami/` as usual, then

```bash
scripts/revc-source.sh export   # writes your edits into cpp/revc/patches and cpp/revc/overlay
scripts/revc-source.sh check    # fails if cpp/re3-miami has edits not exported yet
```

Only `cpp/revc/` is committed. `prepare` refuses to overwrite a tree with unexported edits
(`--force` overrides).

## Build

```bash
scripts/build.sh            # warns if toolchain/packages differ from the pins
scripts/build.sh --strict   # refuses to build if they differ
```

The result is `rust/target/release/remiami` and its SHA-256 is printed.

For day-to-day work, `cd rust && cargo run` builds a dev binary and starts the game.

## Game data

The game needs your own Vice City files. `revc` looks for them in this order:

1. `REVC_GAME_DIR`
2. the nearest `game/` folder (one containing `models/`) above the current directory or the executable

So a checkout with your game copy in `./game` just works. Optionally copy `gamefiles/` from your
reVC source copy (the free-roam script and SDL's gamepad database) into the game folder once. reVC's other extra
files (its GXT text with the new menu strings, `particle.txd`, `generic.txd`, the controller
pictures and the Xbox Neo data in `neo/`) are Rockstar material and are not in this
repository; they come from your own game copies (plan: imported from your PC/Xbox files).

## Reproducibility

Builds don't depend on where the checkout lives: C++ uses `-ffile-prefix-map` (set in
`rust/revc/build.rs`), Rust uses `--remap-path-prefix` (set by `scripts/build.sh`), and nothing
uses `__DATE__`/`__TIME__`. To verify:

```bash
scripts/check-reproducible.sh   # clones HEAD twice into different paths, builds both, compares
```

The script prepares each clone from `$REVC_SOURCE` (default: the verified original files of your
current tree, `cpp/re3-miami/.reVC-original`).

Inputs that are pinned: Rust toolchain, `Cargo.lock`, the reVC snapshot (hash-checked), librw (vendored), system packages
(`scripts/deps.lock`, checked, not enforced unless `--strict`).

## Tests

```bash
scripts/test.sh            # boot saveload formats texmods gamepad spots; exit code 0 when all pass
scripts/test.sh --bless    # accept the current screenshots as the new references
```

Each run uses a throwaway game folder (game files symlinked, settings and saves from `tests/`),
so your own `reVC.ini` and saves are never touched. A window opens while it runs. Logs,
screenshots and diff images go to `rust/target/test-output/`. Details: `tests/README.md`.

## Audio

Sound effects and streams (music, radio, cutscene audio) are mixed in Rust (`rust/revc-audio`,
unit-tested with `cargo test -p revc-audio`) and played through SDL3, including the environmental
reverb (`revc-audio/src/reverb.rs`). OpenAL is not used any more; the game's OpenAL-style audio code
(`sampman_oal.cpp`, `oal/stream.cpp`) talks to the Rust mixer through `oal/rust_al.h`.

The mixer reproduces OpenAL Soft's measured gains (`tests/calibrate/`, checked by a unit test).
For listening comparisons: `scripts/test.sh audio` plays a scripted scene (walk, fire) and
`REVC_TEST_WAV=out.wav` records the mixer output; `cargo run -p revc-audio --bin audiocmp a.wav b.wav`
compares two recordings. The last commit that could still build with OpenAL (for an A/B reference)
is `15d2669`.

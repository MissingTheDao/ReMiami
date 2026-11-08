# Building reVC

One repository holds everything needed to build:

| Path | What |
|---|---|
| `rust/` | Cargo workspace: the `revc` launcher and SDL3 platform host |
| `cpp/re3-miami/` | The C++ game (re3-miami snapshot 2021-09-02 plus our changes) |
| `cpp/re3-miami/vendor/librw/` | librw, vendored at aap/librw `5501c4f` plus the HOST gfxlib commit |
| `scripts/` | `build.sh`, `check-reproducible.sh`, `deps.lock` |
| `bin/` | Local helpers (`ai-serve.sh`, `ask-local.py`) |

Never committed: `game/` (your private copy of Vice City), `ai/` (llama.cpp and models), build output.

## Requirements

Arch / CachyOS packages (exact versions the build was verified with are in `scripts/deps.lock`):

```bash
sudo pacman -S --needed rust clang cmake ninja sdl3 openal mpg123 libglvnd
```

Optional: `sccache` (C++ compile cache, used automatically when installed).

Rust is pinned in `rust/rust-toolchain.toml`. rustup honours it; with the system `rust` package,
`scripts/build.sh` checks the version instead.

## Build

```bash
scripts/build.sh            # warns if toolchain/packages differ from the pins
scripts/build.sh --strict   # refuses to build if they differ
```

The result is `rust/target/release/revc` and its SHA-256 is printed.

For day-to-day work, `cd rust && cargo run` builds a dev binary and starts the game.

## Game data

The game needs your own Vice City files. `revc` looks for them in this order:

1. `REVC_GAME_DIR`
2. the nearest `game/` folder (one containing `models/`) above the current directory or the executable

So a checkout with your game copy in `./game` just works. reVC's extra data files
(`cpp/re3-miami/gamefiles/`) must be copied into the game folder once.

## Reproducibility

Builds don't depend on where the checkout lives: C++ uses `-ffile-prefix-map` (set in
`rust/revc/build.rs`), Rust uses `--remap-path-prefix` (set by `scripts/build.sh`), and nothing
uses `__DATE__`/`__TIME__`. To verify:

```bash
scripts/check-reproducible.sh   # clones HEAD twice into different paths, builds both, compares
```

Inputs that are pinned: Rust toolchain, `Cargo.lock`, librw (vendored), system packages
(`scripts/deps.lock`, checked, not enforced unless `--strict`).

## Tests

```bash
scripts/test.sh            # boot + saveload; prints PASS/FAIL, exit code 0 when all pass
scripts/test.sh --bless    # accept the current screenshots as the new references
```

Each run uses a throwaway game folder (game files symlinked, settings and saves from `tests/`),
so your own `reVC.ini` and saves are never touched. A window opens while it runs. Logs,
screenshots and diff images go to `rust/target/test-output/`. Details: `tests/README.md`.

## Audio

Sound effects and streams (music, radio, cutscene audio) are mixed in Rust (`rust/revc-audio`,
unit-tested with `cargo test -p revc-audio`) and played through SDL3, including the environmental
reverb (`revc-audio/src/reverb.rs`). OpenAL is still linked for device/provider detection and the
audio-provider menu until those are removed. To compare with the original OpenAL
output, build once with either or both switches off:

```bash
cd rust && REVC_CMAKE_ARGS="-DREVC_RUST_SFX=OFF -DREVC_RUST_STREAMS=OFF" cargo run
```

The next plain `cargo run` switches back.

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


## Build kit (AppImage)

`packaging/buildkit/make-buildkit.sh <commit> <version> <runtime>` packs the repository at a commit into one
AppImage: the source (`git archive`), the Rust crates vendored for every target in `Cargo.lock`, and a launcher. It
holds no game data, no reVC source and no prebuilt game. The player runs it with their own copies:

```bash
./remiami-buildkit-<version>-x86_64.AppImage --check
./remiami-buildkit-<version>-x86_64.AppImage --revc-source <reVC copy> --game <Vice City folder> \
    [--ps2-disc <PS2 US v1.40 image>] [--xbox-disc <Xbox US Rev 1 image>]
```

It accepts exactly the versions the project is built and tested with, each checked by SHA-256, so every build plays
the same: the reVC source at snapshot `dc21472`, Vice City for PC as the Steam release, and optionally the PS2 US v1.40
and Xbox US (Rev 1) discs (`--ps2-disc`, `--xbox-disc`: imported with the game's own importer, which refuses other
versions). `--help` lists the hashes.

`--check` lists missing build tools (the same as under Requirements, plus Rust 1.99.1). The first run prepares the
C++ tree from the reVC copy (`scripts/revc-source.sh prepare`, hash-checked), builds with `scripts/build.sh` without
network access into `~/.local/share/remiami/<version>`, then starts the game; later runs need only `--game`. The pack
step needs `mksquashfs` and the AppImage type2 runtime of the release pinned in the script (checked by sha256).

### Build kit for Windows (.exe)

`packaging/buildkit/make-buildkit-win.sh <commit> <version> <remiami-kit.exe>` packs the same contents (source,
vendored crates, `VERSION`, `packaging/buildkit/build.ps1`) as a `.tar.gz` behind a small launcher
(`packaging/buildkit/win-launcher`, built with `cargo build --release` on Windows). The launcher unpacks it with the
`tar.exe` that comes with Windows into `%LOCALAPPDATA%\remiami\<version>\kit` and runs `build.ps1` with Windows
PowerShell. It takes the same options and checks the same hashes as the AppImage, plus two folders that Linux gets from
the system:

```bat
remiami-buildkit-<version>-windows-x86_64.exe --check --sdl3 <SDL3-devel VC folder> --mpg123 <mpg123 folder>
remiami-buildkit-<version>-windows-x86_64.exe --revc-source <reVC copy> --game <Vice City folder> --sdl3 <...> --mpg123 <...>
```

It needs Git for Windows (its `bash`, `diff`, `patch`), Visual Studio Build Tools with the C++ workload (MSVC, CMake,
Ninja), rustup with Rust 1.99.1, the official `SDL3-devel-<version>-VC` package unpacked, and mpg123 built as a static
library with CMake and MSVC (`ports/cmake`, `-DBUILD_SHARED_LIBS=OFF`). It builds into
`%LOCALAPPDATA%\remiami\<version>` and copies `SDL3.dll` next to the game.

## Renderers

The game draws through **retro-wgpu** by default: an engine-neutral renderer core, published at
github.com/MissingTheDao/retro-wgpu and fetched by Cargo at the rev pinned in
`rust/revc-rw/Cargo.toml` (the first build needs network access, or a vendored/cached copy).
reMiami's RenderWare shading runs on it.

- `REVC_RENDERER=wgpu` selects the older wgpu back end, kept as a fallback (same images in the
  test suites). It also takes over automatically, with a `WARNING` in the log, if retro-wgpu fails
  to start.
- `REVC_RENDERER_BACKEND=vulkan` (default on Linux), `dx12` (default on Windows) or `gl` picks the
  graphics API for either.
- `REVC_RENDER_THREAD=1` draws on a render thread.
- `REVC_RENDER_SHADOW=1` uses the older back end to present and draws retro-wgpu off-screen next
  to it, comparing the two (see `docs/RENDERER_PORT.md`).

### Windows (MSVC)

Tested on Windows 11 with Visual Studio 2022 Build Tools (C++ workload), its CMake and Ninja,
and the Rust toolchain pinned in `rust/rust-toolchain.toml` (x86_64-pc-windows-msvc):

- Check out with LF line endings (`git config core.autocrlf false` before checkout): the source
  manifest and patches are compared byte for byte.
- `scripts/revc-source.sh prepare <copy>` runs in Git Bash (it needs `diff`, `patch`, `sha256sum`).
- SDL3: the official `SDL3-devel-<version>-VC.zip`; put its `lib\x64` on `LIB` for the link and
  `SDL3.dll` next to `remiami.exe`.
- mpg123: built from its source with CMake and MSVC as a static library
  (`ports/cmake`, `-DBUILD_SHARED_LIBS=OFF -DBUILD_LIBOUT123=OFF -DBUILD_PROGRAMS=OFF`), installed
  to a folder named by `MPG123_DIR`.
- Build from the "x64 Native Tools" environment (vcvars64):
  `cd rust && cargo build --release -p revc` with `MPG123_DIR` set.
- The renderer uses Direct3D 12 by default; `REVC_RENDERER_BACKEND=vulkan` switches to Vulkan.
  The spots views match the Linux references on both.

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

# reMiami

A long-term modernisation of [reVC](https://github.com/GTAmodding/re3) (the reverse-engineered
source of GTA: Vice City, re3-miami branch): the game is ported to Rust piece by piece, in
strangler style, while staying playable at every step.

The game binary is `remiami`; inside the code the project keeps its short working name `revc`
(crate names, the `REVC_*` settings and test switches, the `cpp/revc` folder).

## Goals

- **Faithful gameplay.** The game should play exactly like the original; behaviour changes are
  bugs unless they are opt-in options.
- **Rust, incrementally.** C++ subsystems are replaced one at a time behind the existing
  interfaces (platform host, timing, audio, file formats, renderer), each checked against the
  original with automated screenshot and save/load tests.
- **A modern renderer.** The game draws with `revc-rw`, a wgpu renderer (Vulkan by default,
  OpenGL selectable with `REVC_RENDERER_BACKEND=gl`, DirectX 12 for Windows builds); the old GL3
  renderer is gone, and librw's object model is next to be replaced. Optional extras: the Xbox "Neo" vehicle and world effects, screen
  droplets, colour filter choices, and a full antialiasing suite (FXAA, SMAA, MSAA, SSAA).
- **Reproducible builds** from one repository, with pinned dependencies.
- **Better presentation, optionally.** Upscaled HUD and menu art made locally from your own game
  files, texture mods, and later temporal upscalers (FSR, XeSS, DLSS).

## No game data

This repository contains no game assets: no game text, textures, models, data files or
screenshots. You need your own copy of Vice City; put its files in `game/` (never committed).
Tools that derive files from it (such as the HUD upscaler) write them there too. The automated
tests' reference screenshots are made locally (`scripts/test.sh --bless`) and are not committed
either.

## Layout

| Path | What |
|---|---|
| `rust/` | Cargo workspace: the `revc` launcher and host, `revc-rw` (renderer), `revc-formats`, `revc-audio`, `revc-timing` |
| `cpp/revc/` | reMiami's changes to the reVC C++ code (patches), its own C++ files and librw. The reVC source itself is not here: you build from your own copy (see [docs/BUILDING.md](docs/BUILDING.md)) |
| `scripts/` | Build, test and tool scripts |
| `tests/` | Test scenarios, golden data (synthetic inputs, hashes) and test saves |
| `docs/` | Building, renderer port, modding, playtest checklists |

Start with [docs/BUILDING.md](docs/BUILDING.md); the renderer work is described in
[docs/RENDERER_PORT.md](docs/RENDERER_PORT.md).

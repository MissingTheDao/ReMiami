# retro-wgpu: notices and provenance

retro-wgpu is licensed under **MIT OR Apache-2.0** (see LICENSE-MIT, LICENSE-APACHE and the
License section in README.md). The third-party parts below keep their own licences.

## Origin of the code

retro-wgpu was written fresh for an engine-neutral model. **Nothing in it is derived from
librw, RenderWare or re3/reVC**: no librw vertex layouts, Im2D transform, alpha-test window,
depth-test quirks, gl3 shader maths, MatFX or Neo shaders.

A few engine-neutral pieces were copied or adapted from **revc-rw** (the owner's own code in the
revc workspace, not librw-derived):

| retro-wgpu | from revc-rw | what |
|---|---|---|
| `src/gpu.rs` | `lib.rs` `Backend`/`Gpu` | backend enum, WebGL2-level limits, BC/adapter feature requests, logging error handler, MSAA sample-count probe |
| `src/present.rs` | `present.rs` | raw-handle surface set-up, non-sRGB 8-bit format choice, GL vsync driver switches, acquire-retry |
| `src/readback.rs` | `frame.rs` `readback_start/finish` | texture-to-buffer copy and row unpadding |
| `src/post.rs` `Smaa`, `src/smaa.wgsl`, `assets/smaa/*` | `frame.rs` SMAA set-up, `smaa.wgsl`, `assets/smaa` | SMAA port and its three-pass plumbing (third-party, see below) |
| `src/renderer.rs` (ideas only) | `frame.rs` | record-then-encode structure, redundant-bind tracking, frame copies |

Everything else (state model, layouts, shader generator and WGSL, combiners, texture formats
and BC/CLUT decoding, FXAA, scaler, texture replacement, tests) is new.

## Third-party material

### SMAA — MIT

`src/smaa.wgsl` is a WGSL port of `SMAA.hlsl` and `assets/smaa/*.r8|*.rg8` are the lookup
textures from https://github.com/iryoku/smaa (commit 71c806a8; see `assets/smaa/SOURCE.md`).

    Copyright (C) 2013 Jorge Jimenez (jorge@iryoku.com)
    Copyright (C) 2013 Jose I. Echevarria (joseignacioechevarria@gmail.com)
    Copyright (C) 2013 Belen Masia (bmasia@unizar.es)
    Copyright (C) 2013 Fernando Navarro (fernandn@microsoft.com)
    Copyright (C) 2013 Diego Gutierrez (diegog@unizar.es)

Full MIT licence text: `assets/smaa/LICENSE.txt` (ship it with binaries).

### FXAA — algorithm attribution

`src/fxaa.wgsl` implements the FXAA algorithm by **Timothy Lottes (NVIDIA)**, as published in
the FXAA whitepaper (2009) and the FXAA 3.11 "quality" description. The WGSL was written for
retro-wgpu from that description; no NVIDIA source file was copied, so no NVIDIA licence text
applies. Attribution is kept as a courtesy.

### Dependencies

| crate | licence | use |
|---|---|---|
| wgpu 30.0.1 (+ wgpu-core/hal/types, naga) | MIT OR Apache-2.0 | GPU API |
| raw-window-handle 0.6 (via wgpu) | MIT OR Apache-2.0 OR Zlib | window handles |
| pollster 1.0 | MIT OR Apache-2.0 | blocking on adapter/device requests |
| png 0.18 | MIT OR Apache-2.0 | texture packs and dumps, golden images in tests |

### Feature `slang`: librashader and its native dependencies

Only built with `--features slang`.

| crate | licence | notes |
|---|---|---|
| librashader-runtime-wgpu, -presets, -common, -runtime (+ -reflect, -preprocess, -pack, -cache) 0.12.1 | MPL-2.0 OR GPL-3.0-only | used under **MPL-2.0**: file-level copyleft; ship the MPL-2.0 text and point to the source (https://github.com/SnowflakePowered/librashader). retro-wgpu's own files are not affected. |
| glslang / glslang-sys 0.8 | MIT OR Apache-2.0 (crate); bundles Khronos glslang (BSD-3-Clause, MIT, Apache-2.0 and Khronos licences, mixed per file) | C++ compiled from source |
| spirv-cross2 / spirv-cross-sys 0.7 | MIT OR Apache-2.0 (crate); bundles SPIRV-Cross (Apache-2.0) | C++ compiled from source |
| persy 1.8 | MPL-2.0 | shader cache (disabled at run time) |
| smartstring 1.0 | MPL-2.0+ | |
| rspirv 0.13 | Apache-2.0 | |
| encoding_rs 0.8 | (Apache-2.0 OR MIT) AND BSD-3-Clause | |
| others (rayon, image, regex, blake3, bincode, nom, ...) | permissive (MIT / Apache-2.0 / Zlib / CC0 / Unlicense / BSD) | `cargo metadata --features slang` lists them |

The test presets in `tests/slang/` were written for retro-wgpu's tests. **No preset packs are
included**; Mega Bezel and other (often GPL) packs are user-supplied and only loaded at run time.

### Not included

- CRT/VHS shaders (crt-lottes, cathode-retro, vhs_and_crt) are **not** part of retro-wgpu yet;
  they would bring their own notices.
- No game code or data, no Mega Bezel or other GPL shader packs.

# retro-wgpu: notices and provenance

retro-wgpu has **no licence yet**: the owner chooses one (e.g. MIT OR Apache-2.0) before the
crate is published or shared. Until then `Cargo.toml` has no `license` field on purpose.

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
| png 0.18 (dev-dependency) | MIT OR Apache-2.0 | golden images in tests |

### Not included

- The CRT/VHS shaders in `~/claude-n150/work/43mode` (crt-lottes, cathode-retro, vhs_and_crt)
  are **not** part of retro-wgpu yet; they would bring their own notices.
- No Hit & Run code or data, no Mega Bezel or other GPL shader packs.

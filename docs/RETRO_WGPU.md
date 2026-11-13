# retro-wgpu

An engine-neutral renderer core on wgpu 30 for ports and remasters of fixed-function era games
(PS2, GameCube, Xbox, PC Direct3D 8/9). It runs on **Vulkan, OpenGL and Direct3D 12**.

- **Where:** github.com/MissingTheDao/retro-wgpu (public, MIT OR Apache-2.0). reMiami depends on
  it as a git dependency pinned to a rev (`rust/revc-rw/Cargo.toml`); its own design document is
  `DESIGN.md` there. This file keeps reMiami's notes on it.

reMiami presents with retro-wgpu by default since 2026-11-14; the older wgpu back end stays as a
fallback (`REVC_RENDERER=wgpu`, automatic if retro-wgpu fails to start). Removal of the older back
end is to be reviewed around 2026-12-05, not before the user decides. Untested: Windows, DX12.

### Working on retro-wgpu

Changes land in the retro-wgpu repository first (neutral commit messages, a tag), then the rev is
bumped here. To try local changes before that, point Cargo at a checkout without committing it:
an untracked `rust/.cargo/config.toml` with

```toml
[patch."https://github.com/MissingTheDao/retro-wgpu"]
retro-wgpu = { path = "../../retro-wgpu" }
```

(the path relative to `rust/`). Cargo then rewrites `Cargo.lock` for the patch: don't commit that
lock state; remove the file and rebuild before committing.

## Status per stage

| Stage | Content | Status | Commit |
|---|---|---|---|
| 1 | Core: device/backends, presentation, recorder, state model, layouts, shaders, textures, targets, MSAA/SSAA/FXAA/SMAA, output stage, readback | **done**, 31 integration and 6 unit tests green on Vulkan and GL | `a273e0a` |
| 2 | High-resolution texture replacement: hash keys, PNG/DDS packs, BC7, mips, anisotropy, dump mode | **done**, 8 integration and 2 unit tests green on Vulkan and GL | `40b4040` |
| 3 | Slang presets through librashader 0.12.1 (feature `slang`; wgpu 30 compatible, no fork needed): Original at internal resolution, history/feedback, full-output or scaler-rect placement, parameters | **done**, 4 integration tests green on Vulkan and GL (stage 1+2 also green with the feature on) | `39d2f87` |

Stage 4, front-end programs (`create_program` / `draw_program`, `tests/programs.rs`): **done** in
reMiami, green on Vulkan and GL (RADV) — see "Programs" below.

Machine (stages 1-3): Intel N150 (ADL-N iGPU), Mesa ANV for Vulkan and Mesa iris for GL (EGL, headless).

The `--features slang` build compiles glslang and SPIRV-Cross (C++). Here it took about
50 minutes under a thermal governor that paused the build above 90 °C and resumed it below
70 °C. Polling every 3 s let the peak reach 97 °C.
**DX12 cannot run on this Linux machine**; see "Direct3D 12 / Windows".

## Crate layout

| File | Lines | Content |
|---|---|---|
| `src/gpu.rs` | 156 | `Backend {Vulkan, Gl, Dx12}` (all variants on all platforms, `available()`, `parse()`), `GpuOptions`, `Gpu::headless/create`, MSAA and anisotropy probes |
| `src/present.rs` | 177 | `WindowHandles` (Wayland, Xlib, Win32, or any raw-window-handle source), `Presenter` (surface, vsync, resize, acquire, `present(renderer, capture)`) |
| `src/state.rs` | 525 | `RenderState` and its parts, `DrawParams` (world, `Material`, `Lighting`), `View`, `Mat4` helpers |
| `src/layout.rs` | 165 | `VertexLayout` / `Attribute` / `Semantic` / `AttrFormat` |
| `src/shader.rs` | 413 | WGSL core plus a per-layout generated vertex entry point; uniform block packing |
| `src/renderer.rs` | 1362 | `Renderer`: resources, recording, encoding, post chain, output stage |
| `src/texture.rs` | 365 | `TexFormat`, CPU conversion (16-bit, CLUT, BC1-3 decode), mip chain |
| `src/post.rs` | 414 | `PostPass` trait, `Fxaa`, `Smaa`, blitter, `OutputSettings`, `output_rect` |
| `src/fxaa.wgsl`, `src/smaa.wgsl` | 121, 374 | AA shaders |
| `src/replace.rs` | 384 | Texture packs (stage 2) |
| `src/slang.rs` | 84 | `SlangOutput` (stage 3, feature `slang`) |
| `src/readback.rs` | 70 | `Image`, `readback_start/finish`, `read_texture` |
| `tests/` | ~850 | `stage1.rs`, `stage2.rs`, `stage3.rs`, helpers, 42 golden PNGs, 2 test presets |

## API overview

```rust
let gpu = Gpu::headless(Backend::Vulkan)?;                     // or Presenter::new(backend, WindowHandles::wayland(..), w, h, vsync, &GpuOptions::default())
let mut r = Renderer::new(&gpu, output_format, SceneSettings { internal_size: Some((640, 448)), supersample: 1, msaa: 4 });
r.output = OutputSettings { mode: ScaleMode::Integer, filter: ScaleFilter::SharpBilinear, display_aspect: Some(4.0 / 3.0), ..Default::default() };
r.set_post_chain(vec![Box::new(Smaa::new(&gpu))]);

let layout = r.create_layout(&gpu, VertexLayout::packed(&[(Semantic::Position, AttrFormat::F32x3), (Semantic::Normal, AttrFormat::F32x3),
                                                            (Semantic::Color, AttrFormat::Unorm8x4Bgra), (Semantic::TexCoord(0), AttrFormat::F32x2)]))?;
let tex = r.upload_texture(&gpu, 256, 256, TexFormat::Indexed8, &[&indices], Some(&palette), true)?;
let vb = r.create_vertex_buffer(&gpu, &bytes);
let ib = r.create_index_buffer16(&gpu, &indices16);

// per frame
r.set_target(Target::Scene);
r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
r.set_view(&View::new(view, proj));
let state = RenderState { blend: Some(Blend::ALPHA), alpha_test: AlphaTest { compare: CompareFunction::Greater, reference: 0.5 }, ..RenderState::default() }
    .with_texture(tex, Sampler::TRILINEAR_WRAP);
r.draw(&gpu, &Draw { layout, topology: Topology::TriangleStrip, vertices: Vertices::Buffer(vb, 0), indices: Indices::Buffer(ib),
                     range: None, base_vertex: 0, state: &state, params: &DrawParams { world, lighting, material }, bones: None });
r.copy_to_texture(Target::Scene, None, feedback_tex, [0, 0]);   // frame-feedback effects
presenter.present(&mut r, /*capture*/ false)?;                    // or r.encode(..) + r.end_frame()
```

### Main types

- **`RenderState`**: the explicit state object.
  - `blend: Option<Blend>`: separate colour and alpha `BlendEq {src, dst, op}`, all wgpu factors and operations, `blend_constant`.
  - `color_mask`.
  - `depth: DepthState {compare, write}`.
  - `stencil: StencilState {front, back: StencilFace {compare, fail, depth_fail, pass}, read_mask, write_mask}` and `stencil_reference` (dynamic, no new pipeline).
  - `cull: Option<Face>`, `front_face`.
  - `depth_bias {constant, slope, clamp}` (triangles only).
  - `alpha_test: AlphaTest {compare, reference}`: a shader feature, all 8 compare functions, compared on 8-bit values like the original hardware.
  - `fog: Fog {mode: Off | Linear{start, end} | Exp{density} | Exp2{density}, color}`: per vertex, by view-space depth.
  - `stages: [Stage; 4]`: texture, `Sampler`, colour and alpha `Op` with two `Arg`s (source Current/Diffuse/Texture/Constant, invert, alpha-replicate), constant, `TexCoordSource` (UV set 0-3, sphere map, reflection, normal, position) and a texture matrix.
- **`Sampler`**: min/mag `Filter` (Nearest/Linear), `mip: Option<Filter>`, `Address` Wrap/Mirror/Clamp/Border (Border clamps to the edge), anisotropy.
- **`DrawParams`**:
  - `world`.
  - `Material`: diffuse, ambient, emissive, each optionally taken from the vertex colour (like D3D material sources).
  - `Lighting`: ambient plus up to 4 directional lights, Gouraud, `normalize`.
- **`VertexLayout`**:
  - Semantics: `Position`, `PositionT` (screen-space XYZRHW), `Normal`, `Color`, `TexCoord(0..3)`, `BlendWeights`, `BlendIndices`.
  - Formats: F32x2/3/4, Unorm8x4, Unorm8x4Bgra (D3DCOLOR), U8x4, U16x4, Snorm16x2/4.
  - Explicit offsets and stride, so a game's own vertex structs are drawn in place.
- **`Draw`**:
  - Immediate data (`Vertices::Slice`) or retained buffers, u16/u32/buffer/no indices.
  - `range`, `base_vertex`, bones (up to 64, 4 weights).
  - Topologies: points, lines, line strip, triangle list and strip; fans are expanded on the CPU, also from retained index buffers (they keep a CPU copy).
- **`Target`**:
  - `Scene`: internal resolution x supersampling, with MSAA.
  - `Texture(id)`: render targets, optionally MSAA, each with its own Depth24PlusStencil8 buffer.
  - Clears take colour, depth and stencil. Viewport and scissor per draw.
  - `copy_to_texture` is an exact copy when formats match, filtered otherwise.
  - Sampling the texture being drawn into binds white instead (no feedback loop).
- **Textures** (`TexFormat`):
  - Rgba8, Bgra8, Rgb565, Argb1555, Abgr1555 (PS2 PSMCT16), Rgba5551, Argb4444, Rgba4444.
  - Indexed4 (nibble order selectable), Indexed8 with `Palette {colors, layout: Linear | Ps2Csm1}`.
  - Bc1/2/3, and Bc7 (replacements).
  - Upload, region updates, automatic mips (`mip_levels: 0`).
- **After the scene:**
  - SSAA downsample to the internal size.
  - `PostPass` chain (built-in `Fxaa`, `Smaa`, or your own) at internal resolution.
  - The output stage: `OutputSettings`:
    - `ScaleMode` Stretch/Fit/Integer.
    - `ScaleFilter` Nearest/Bilinear/SharpBilinear.
    - `display_aspect` (e.g. 4:3 for a 640x448 frame; integer mode scales the height by an integer and the width by the aspect).
    - `viewport` rect, `background`, `gamma` (output curve).
  - Or a custom `OutputStage` (the slang chain).
  - `final_image()` is the post-processed image before the output stage (for screenshots and dumps).

## Design notes

- **One shader core, uniforms for behaviour.**
  - Lighting, fog mode, alpha test and the four combiner stages are driven by a 720-byte per-draw uniform block. Pipelines therefore differ only in fixed-function state (blend/depth/stencil/cull/bias/mask), topology, sample count and vertex layout.
  - The vertex entry point is generated per layout. Missing attributes get defaults: white colour, zero normal, zero UVs. With no vertex colour, the material is used.
- **Record, then encode.**
  - Recording makes no wgpu calls.
  - `encode` does the rest:
    - writes the frame's vertex and index streams and the view/draw/bone blocks in one buffer write each (dynamic offsets);
    - creates missing pipelines, samplers and bind groups;
    - replays the commands into passes with redundant-state skipping.
  - Consecutive identical blocks are deduplicated.
- **GLES3-level limits on every backend**, so GL, Vulkan and DX12 run the same paths. Only uniform buffers are used (no storage buffers).
- **16-bit and paletted formats are expanded on upload.** This choice is documented in `texture.rs`:
  - wgpu has no portable 16-bit colour formats and no palette sampler;
  - a shader-side CLUT cannot filter bilinearly without four lookups.
  - Palette animation therefore means re-uploading. The CLUT and indices still feed the replacement hash.
- **Low internal resolution is first-class.** The scene, the post chain and `final_image()` stay at the game's resolution, and only the output stage scales. This is also what slang presets need.

## Programs

A front end can bring its own shader for draws the fixed-function model doesn't cover: an
engine's lighting model, MatFX-style env maps, custom effects. `Renderer::create_program(gpu,
&ProgramDesc { label, wgsl, vertex_entry, fragment_entry, layout, block_size })` compiles it (a
bad module or block size is an `Err`); `draw_program(gpu, &draw, program, block_bytes)` records
a draw with it. The renderer keeps everything else: recording, retained resources, targets,
MSAA, the post chain, the output stage, and the fixed-function pipeline state of the draw's
`RenderState` (blend, depth, stencil, cull, bias, colour mask). Lighting, fog, alpha test and the
combiners are the program's job; `draw.params` is unused.

The WGSL uses the renderer's bind groups: `@group(0)` the view block, `@group(1) @binding(0)` the
program's per-draw block (`block_size` bytes, a multiple of 16), `@group(2)` the four stages
(texture i at binding 2i, sampler at 2i + 1), `@group(3)` the bones for skinned layouts. Vertex
inputs are at `VertexLayout::location(semantic)`. `stages` (1..=4) is how many texture stages the
program samples; fewer stages make texture changes cheaper. Blocks are pooled per block size,
stored flat at the GPU stride, and consecutive identical blocks are deduplicated;
`draw_program_parts` takes the block in pieces (e.g. uniforms and a model) without joining them.
The renderer's caches use an Fx hash (`fxhash.rs`).

**Instance blocks.** A program may have a second per-draw block (`instance_block_size`) for what
changes with every draw (a model's matrix and lights) while the first block stays the same.
Declare it with `Renderer::instance_block_decl(name, type)` and read `name[instance_index]`. Where
the device has storage buffers in vertex shaders (`GpuOptions::instance_storage`, or any device
created with that limit), all of a frame's instance blocks sit in one buffer bound once and the
draw's instance index picks one; otherwise it is a uniform block at a per-draw offset (index 0).
Both modes draw the same picture (`tests/programs.rs`, `instance_blocks`).

**The lean call.** `draw_with_program(&ProgramDraw { program, pipeline: &PipelineState, textures,
block, instance, .. })` records a program draw without building a `RenderState` (four stages
with matrices) that a program ignores. `draw_program*` are the same call for `Draw` users.

## Draw path

Recording a draw resolves everything to indices; replaying compares and binds by index.

- **Resources** (`slab.rs`): textures and buffers live in tables indexed by id, not hash maps.
- **Pipelines and stage bind groups** are found (or created) when the draw is recorded: first the
  previous draw's, then, for one-stage draws, a per-texture slot, then a hash map. A changed or
  destroyed texture drops its bind groups; draws already recorded keep theirs until the frame ends.
- **Draw commands** are a few indices plus the vertex/index references. Viewport, scissor, stencil
  reference and blend constant are separate commands, recorded when they change.
- **Uniform data** (view, fixed-function draw, bone, program and instance blocks) goes into flat
  per-frame pools already laid out at the GPU stride: one buffer write each per frame, consecutive
  identical blocks shared, no per-draw allocation.
- Uniform words are uploaded as they are in memory (little-endian targets only; checked at
  compile time).

## Overlay

`Renderer::begin_overlay()` ends a frame's scene: later `Target::Scene` clears, draws and copies go
to an output-size image, after the scene's downsampling, post chain and output scaling. It has
its own depth/stencil, which starts cleared, and draws are single-sampled. A HUD drawn there stays sharp
whatever the internal resolution, supersampling or post AA. The output curve (gamma) is applied
afterwards, to both. `final_image()` is then that output-size image with the overlay, before the
curve. A custom output stage (slang) runs on the game image before the overlay. Frame copies
after the marker copy the overlay image. `copy_to_texture_scaled(src, dst, at, size)` copies a
whole target filtered to a given size, e.g. a supersampled scene into a window-size texture.
Tested in `tests/overlay.rs`.

## How engine front ends map onto it

| Front end concept | retro-wgpu |
|---|---|
| **RenderWare (revc)** Im2D vertices (screen pixels + w) | `PositionT` layout (x, y, z, rhw) with the camera's screen size in `View::screen`; the front end converts its own Im2D maths |
| RW Im3D / world geometry | `Position` layouts, `View` (view, proj), `DrawParams.world` |
| RW raster formats, DXT | `TexFormat` (BC1-3, RGBA8, PAL4/8 via `Palette`) |
| RW render states (blend, z test/write, cull, alpha ref, fog) | `RenderState`. RW-specific quirks stay in the front end, not here: gl3's alpha-test window, its fog formula, "z test off but z write on" → `Always` |
| RW lights | `Lighting` (directional + ambient). Point/spot and RW's lighting model need either the front end or a later extension (see follow-ups) |
| RW skinning, MatFX env map | `BlendWeights`/`BlendIndices` + bones; env map = stage 1 with `TexCoordSource::SphereMap` or `Reflection` and a texture matrix |
| re3 Neo / CPostFX shaders | Not portable as uniforms: implement as custom `PostPass`es (CPostFX) or front-end shaders (follow-up: custom pipelines) |
| GC CMPR / DXT1 | `TexFormat::Bc1` |

## Provenance

| Item | Origin |
|---|---|
| `gpu.rs`: backend enum, limits, feature requests, error logger, sample probe | **from revc-rw (user's own code, engine-neutral)** `lib.rs` |
| `present.rs`: surface/instance set-up, format choice, GL vsync switches, acquire retry | **from revc-rw (user's own code, engine-neutral)** `present.rs`; `WindowHandles` with the Win32 path is new |
| `readback.rs` | **from revc-rw (user's own code, engine-neutral)** `frame.rs` readback |
| `smaa.wgsl`, `assets/smaa/*`, `post.rs` `Smaa` plumbing | **from revc-rw (user's own code, engine-neutral)**: the user's WGSL port of iryoku/SMAA (third-party MIT, notice kept) |
| `renderer.rs` record/encode structure, bind tracking, frame copies | ideas from revc-rw, code new |
| Everything else: state model, layouts, shader generator, WGSL core, combiners, lighting/fog/texgen, texture formats, BC1-3/CLUT decoding, mips, FXAA, scaler, sharp bilinear, replacement, slang glue, tests | **new** for retro-wgpu |
| librw / RenderWare / re3 | **nothing**: no layouts, Im2D xform, alpha-test window, depth-test quirk, gl3 maths, MatFX/Neo |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Copyright (c) 2026 The retro-wgpu contributors.

Third-party parts keep their own licences:
- SMAA: MIT; its licence file `assets/smaa/LICENSE.txt` stays as is (ship it with binaries).
- librashader: MPL-2.0 OR GPL-3.0-only, used unmodified as an optional dependency (feature `slang`), under MPL-2.0.
- Any bundled shaders keep their own licences.

## Third-party notices and dependency licences

The full text is in retro-wgpu's `NOTICE.md`.

**Bundled third-party material:**
- **SMAA** (`smaa.wgsl` port, lookup textures): MIT, (C) 2013 Jimenez, Echevarria, Masia, Navarro, Gutierrez. `assets/smaa/LICENSE.txt` must ship with binaries.
- **FXAA**: an independent implementation of Timothy Lottes' (NVIDIA) published algorithm. It is attributed, and no NVIDIA code was used.

**Dependencies:**

| Dependency | Licence | Notes |
|---|---|---|
| wgpu 30.0.1 family, naga | MIT OR Apache-2.0 | |
| pollster | MIT OR Apache-2.0 | |
| png 0.18 | MIT OR Apache-2.0 | |
| raw-window-handle | MIT OR Apache-2.0 OR Zlib | |
| **librashader 0.12.1** crates (runtime-wgpu, presets, common, runtime, reflect, preprocess, pack, cache) | **MPL-2.0 OR GPL-3.0-only** | Feature `slang` only; use them under **MPL-2.0**. File-level copyleft: modified librashader files must stay MPL, while retro-wgpu's own files are unaffected. Ship the MPL text and a source link |
| glslang-sys (bundles Khronos glslang, C++) | crate MIT OR Apache-2.0 | The bundled glslang is BSD-3-Clause / MIT / Apache-2.0 / Khronos mixed |
| spirv-cross-sys (bundles SPIRV-Cross, C++) | crate MIT OR Apache-2.0 | The bundled SPIRV-Cross is Apache-2.0 |
| persy (shader cache, via librashader-cache) | MPL-2.0 | |
| smartstring | MPL-2.0+ | |
| rspirv | Apache-2.0 | |
| encoding_rs | (Apache-2.0 OR MIT) AND BSD-3-Clause | |
| blake3, image, rayon, regex, bincode, nom, … | MIT / Apache-2.0 / Zlib / CC0 / Unlicense permissive | Listed by `cargo metadata`, see `NOTICE.md` |

**Not included:**
- **Mega Bezel and any GPL shader pack.** Presets are user-supplied. GPL presets are only *loaded at run time* from the user's files and are not distributed with the crate.
- **The 43mode CRT shaders** (crt-lottes public domain, cathode-retro MIT/Boost, vhs CC0) are not moved yet. They would be ported as `PostPass`es with their notices, or used as slang presets.

## Direct3D 12 / Windows

- **Selection API:** `Backend::Dx12` exists on every platform and `Backend::available()` lists it on Windows. `Gpu::headless(Backend::Dx12)` / `Presenter::new(Backend::Dx12, WindowHandles::win32(hwnd, hinstance), …)` use wgpu's DX12 backend. wgpu's default features, the same as revc-rw's, include `dx12` on Windows.
- **Compile check here: not done.** The `x86_64-pc-windows-msvc` std is not installed in this toolchain. Installing it would modify `~/.rustup`, which this task was not allowed to touch. To check on a machine where that is fine:
  ```sh
  rustup target add x86_64-pc-windows-msvc
  cd rust && cargo check -p retro-wgpu --target x86_64-pc-windows-msvc            # core
  cargo check -p retro-wgpu --target x86_64-pc-windows-msvc --features slang      # needs a C++ cross toolchain for glslang/SPIRV-Cross; easier natively on Windows
  ```
  The code has no Unix-only API: `no_gl_vsync` is `cfg!(unix)`, and the Win32 handles come from raw-window-handle.
- **To test on Windows** (native: `cargo test -p retro-wgpu [--features slang]`; the `dx12` test variants are `#[cfg(windows)]`):
  1. All stage 1, 2 and 3 tests on Dx12, Vulkan and Gl. Bless goldens once per GPU with `RETRO_WGPU_BLESS=1` if needed.
  2. Formats checked by the tests:
     - `Depth24PlusStencil8` MSAA 4x on DX12;
     - BC1-3 and BC7 sampling;
     - `Bgra8Unorm` textures.
  3. `Presenter` with a real HWND: `WindowHandles::win32` from SDL3's `SDL_PROP_WINDOW_WIN32_HWND_POINTER`. Also check resize, vsync on/off and the surface format pick (Bgra8Unorm).
  4. GL on Windows (WGL) presenting, and that `no_gl_vsync` is correctly a no-op there.
  5. librashader on DX12: preset compile (glslang/SPIRV-Cross build under MSVC) and history/feedback textures.
  6. A 32-bit `i686-pc-windows-msvc` build, if a hook DLL for the original game is planned.

## Tests

`cargo test -p retro-wgpu -j 2` (add `--features slang` for stage 3). Every GPU test runs per backend, as `<test>::vulkan`, `::gl` and `::dx12` (Windows only). It checks pixels analytically and compares with `tests/golden/<name>-<backend>.png`. Those goldens were written by the tests on this machine. A comparison allows 0.5 % of pixels to differ by more than 2. `RETRO_WGPU_BLESS=1` rewrites them.

**Stage 1** (`tests/stage1.rs`):

| Test | What it checks |
|---|---|
| `clear` | Clearing |
| `vertex_colour` | Gradient interpolation |
| `formats` | BC1 on GPU vs CPU decode, RGB565, PS2 CSM1 CLUT, ARGB4444, texel-exact |
| `blending` | alpha, add, reverse-subtract, min, constant, dst-colour |
| `depth_stencil` | depth order, stencil write/equal/not-equal, polygon offset |
| `alpha_test` | 8-bit Greater 100 |
| `fog` | linear, exp, exp2 |
| `lit` | ambient + 2 directional, material vs vertex colour, emissive |
| `skinning` | 2 bones, blended weights |
| `render_to_texture` | 1x and 4x MSAA targets, frame copy |
| `msaa_resolve` | MSAA 4x, SSAA 2x, FXAA, SMAA each soften an edge |
| `combiners` | modulate + add constant; blend-texture-alpha with inverted alpha argument |
| `pretransformed` | XYZRHW, BGRA colour, pixel-exact rectangle edges |
| `output_stage` | 16x16 internal → integer 3x nearest centred in 64x48; sharp-bilinear + gamma 2 |
| `retained_and_mips` | retained buffers, indexed strip, auto mips, trilinear + anisotropy, mirror |
| `backends_listed` | backend list and parsing |

Unit tests cover the 16-bit unpacking, BC1 modes, BC3 alpha, CLUT/CSM1 and nibble order, mips and output rectangles.

**Stage 2** (`tests/stage2.rs`):

| Test | What it checks |
|---|---|
| `dump_and_replace` | Dump file round trip; PNG replacement from a sub-folder (32x32 for a 2x2); restore on re-upload |
| `palette_any` | `_any` key across two palettes |
| `dds_replacements` | BC1 DDS and BC7 (DX10) DDS replacement |
| `filtering_upgrade` | Game sampling kept by default; `force_mips` + anisotropy filters a dense replacement |

Unit tests cover the stable key (pinned value, cross-checked with an independent Python FNV) and DDS header parsing.

**Stage 3** (`tests/stage3.rs`, feature `slang`):

| Test | What it checks |
|---|---|
| `invert` | A one-pass preset on a 16x16 game image placed by integer 3x into 64x48 |
| `history` | `OriginalHistory1` shows the previous frame; `#pragma parameter` read and set |

**Backends:**
- Vulkan (Mesa ANV) and GL (Mesa iris, EGL headless) run everything on this machine.
- DX12 is not available here.

## Follow-ups

1. **Win32 window path, tested.** The code exists (`WindowHandles::win32`) but has not been run.
2. **reMiami migration** (2026-11-13): revc-rw has a retro back end (`revc-rw/src/retro.rs`)
   that draws its recorded frames with retro-wgpu, its RW shading ported as programs, and a
   shadow mode that proves it against the wgpu back end (`docs/RENDERER_PORT.md`, "Layering").
   Left: switching the default once the suites pass in shadow mode.
3. **Lighting:** point and spot lights with D3D attenuation, specular, per-pixel option.
4. **Custom pipelines:** done as programs (above).
5. **Stencil:** clear-by-rectangle and stencil-only targets. Depth/stencil sharing between targets, which D3D games sometimes expect.
6. **Partial clears** (D3D `Clear` with rects or viewport): currently whole-target only.
7. **Overlay at output resolution:** done (`begin_overlay`, below).
8. **Border colour addressing** needs the wgpu `ADDRESS_MODE_CLAMP_TO_BORDER` feature. It currently clamps to the edge.
9. **Port the 43mode CRT shaders** as `PostPass`es with their notices.
10. **librashader:**
    - The background around a `GameRect` is black, because librashader clears.
    - The shader cache is off.
    - glslang/SPIRV-Cross make `--features slang` a long C++ build.
    - Consider `wgsl_preset_pack` for precompiled presets.
11. **Texture replacement:**
    - An async/background loader with a cache budget.
    - A per-game pack manifest.
    - Hashing of only the used region for textures uploaded as sub-rectangles.

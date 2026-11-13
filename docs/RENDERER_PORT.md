# Renderer port (plan tasks 12-14): replacing librw with revc-rw

Goal: librw removed, rendering in Rust on wgpu with a user-selectable Vulkan or OpenGL backend,
screenshot tests passing on both (plan task 12), then the effects (task 13) and the N150 check
(task 14).

## Where it stands

**2026-11-10: wgpu is the only renderer.** The `wgpu` Cargo feature, the `target-wgpu` build and
the GL3 references are gone: `cargo build` builds the game on librw's WGPU platform, drawing with
revc-rw (Vulkan by default, `REVC_RENDERER_BACKEND=gl` for OpenGL, DX12 in Windows builds), and
`tests/reference/` holds the wgpu screenshots (the GL3 set they replace was within 1-2 % of them
after the texture-state and anisotropy work, step 11 below). The host no longer creates GL
contexts, and librw's GL3 platform is deleted (`vendor/librw/src/gl/`, its CMake options, the
GLFW demo apps and the premake/conan builds), as are the game's `RW_GL3` branches and
`custompipes_gl.cpp`; the WarDrum "WDGL" model reader moved to `src/wdgl.cpp` (format only).
Since stage 2 step 1, librw's D3D8/D3D9/Xbox/PS2/WDGL platform code is gone too: textures are read
in Rust (below). Next in stage 2: the rest of librw's object
model in Rust). N150 frame times (2026-11-10, headless, cooled runs): wgpu Vulkan 3.56 ms, wgpu GL
4.29 ms (+20 %, wgpu's GLES backend overhead; see docs/N150.md).

**Step 11 (2026-11-10): closer to GL3.** Texture stage 0 follows GL3's state machine exactly
(filter, addressing and anisotropy live on the texture, as on a GL texture object; draws sample
with the texture's own state), anisotropic filtering is off by default (GL3 asks for it, but it
never takes effect there; `REVC_RW_ANISOTROPY=1` turns it on), and the shaders multiply
projection, view and world like GL3. Spots and saveload went from 1-10 % to 1-2 % of pixels
different from GL3; what's left is mostly single-pixel edge coverage. Draws that sample their own
render target sample white (wgpu rejects the feedback loop GL3 tolerated), and wgpu validation
errors are logged instead of aborting.

`rust/revc-rw` renders whole RenderWare models offscreen on both backends: clump parts placed by
their frame hierarchy, every material with its TXD texture (all mips) and colour, RW alpha test,
a fixed light. `tests/backends.rs` renders the textured Infernus on Vulkan and OpenGL and
requires the two images to agree (0.002 % of pixels differ). Texture decoding covers every D3D8
format in the game files (12391 textures decode).

Presenting to a window works too (`revc_rw::present::Presenter`): an SDL3 window *without* a GL
context, its native Wayland/X11 handles from SDL's window properties, a wgpu surface on Vulkan or
OpenGL (the instance gets the display handle, which EGL needs on Wayland), an 8-bit non-sRGB
swapchain (the game's colours are gamma-space, like GL3's output). Check by hand with
`cargo run -p revc-rw --example window_clear -- vulkan|gl`: both present on the dev machine's
RX 9070 XT. Headless GL picked the integrated GPU; with a window the surface decides and both
backends use the discrete GPU.

**Stage 1, step 1 done: the game runs on librw's new WGPU platform** (`vendor/librw/src/wgpu/`,
`PLATFORM_WGPU`). Build it with `cd rust && CARGO_TARGET_DIR=target-wgpu cargo build --features wgpu`
(the default build stays GL3); pick the backend with `REVC_RENDERER_BACKEND=vulkan|gl`. The host
creates the window without a GL context and `rust/revc/src/host/wgpu.rs` presents through
`Presenter`. The device clears the camera and presents (`revc_rw_begin_frame/clear/present`);
rasters are RGBA8 in memory for every mip level (`wgpuraster.cpp`), not on the GPU yet; Im2D,
Im3D and atomics are no-ops, so frames are blank. Checked: `REVC_TEST=boot` reaches the front end
on Vulkan and GL, `saveload` loads into the world with a clean shutdown (screenshots aren't
possible yet: the camera raster can't be read). The game's PC-host input code, gated on
`RW_GL3`, now also builds for `RW_WGPU`; the GL3/D3D9-only extras (colour filter, custom
pipelines, droplets, new renderer) are compiled out for WGPU until the renderer can do them.

**Stage 1, step 2 done: textures and Im2D, so the whole front end and the HUD draw.**
`revc_rw::frame::Renderer` records each frame (camera clears, Im2D primitives) and encodes it into
render passes on present; texture rasters are uploaded from the device's RGBA8 copies after every
write lock (`uploadRaster`, host mip generation for AUTOMIPMAP). The device keeps a GL3-equivalent
render-state cache and passes one `DrawState` per draw; the shader mirrors gl3's im2d.vert and
simple.frag (fog, [low, high) alpha test), blending follows glBlendFunc (same factors for alpha),
and draws with equal state are merged. Screenshots work: the swapchain is copied (Vulkan) or the
frame is drawn once more offscreen (GL, whose swapchain can't be copied).
Checked: `tests/im2d.rs` (offscreen, both backends: orientation, nearest filtering, blending,
alpha test, mid-frame depth clear); boot on Vulkan and GL shows the menu like GL3; saveload shows
HUD, radar, zone name, sky gradient and corona sprites (no world yet). Known difference: on the
menu text about 0.7 % of text-area pixels (single columns at glyph edges, up to 55 levels) differ
from GL3. Not a rounding tie (moving x by 0.01 px changes nothing) and not mip or filter choice;
GL3 keeps DXT textures compressed on the GPU (`d3d_to_gl3`) while WGPU decodes them on the CPU,
so the next texture step (BC formats on the GPU) is the first thing to check. The boot screen
can't be compared automatically: the menu background animates (two GL3 runs differ by 4.7 %).

**Stage 1, step 3 done: DXT textures stay compressed, and Im3D draws.**
- DXT1/3/5 TXD textures keep their blocks in WGPU rasters (`d3d_to_wgpu` in raster.cpp, no flip:
  GL3's `d3d_to_gl3` flips them because its shaders sample `1 - v`) and are sampled as BC1/2/3
  where the device has `TEXTURE_COMPRESSION_BC` (Vulkan and GL on the dev machine); elsewhere, or
  for sizes that aren't whole blocks, the host decodes them with revc-formats. The game log says
  which (`DXT on the GPU` / `decoded on the CPU`). `tests/im2d.rs` checks both paths against the
  CPU decoder on both backends.
- This did *not* change the menu-text difference to GL3 (still 0.73 % of text-area pixels), so
  texture decoding is ruled out. Also ruled out by experiment: filtering and mip choice, a
  rounding tie at pixel centres, the depth test and the alpha test. Forcing wrap addressing
  changes a few of those pixels: GL3's `setAddressU/V` only apply an addressing change while a
  raster is being bound (they compare with `==`), so GL3 sometimes samples with stale addressing.
  The WGPU device applies states immediately, like D3D8 and the original game; matching GL3
  pixel for pixel here would mean copying its bugs.
- Im3D: the device builds view and projection like gl3 `beginUpdate` (left-handed view) with
  depth mapped to 0..1, and sends one projection*view*world matrix per draw; the shader mirrors
  gl3 im3d.vert (fog from clip w). Transforms without UVs unbind the texture, as in GL3. In game:
  sea at the horizon, the player's shadow, car lights. Tested offscreen: projection and depth
  order against draw order, on both backends.

**Stage 1, step 4 done: the default atomic pipeline, so the world draws.** `wgpu/wgpupipe.cpp`
instances each geometry once into a host vertex/index buffer (positions, normals, prelit
colours, first UV set; GL's defaults for missing attributes; strips turned into lists), rebuilt
when the mesh changes or data was locked, freed with the geometry (geoplg.cpp). Every mesh is
drawn with gl3's default-pipeline maths: prelit + ambient * surfAmbient + directional / point /
spot lights * surfDiffuse, clamped, times the material colour (white unless MODULATE), with the
lights gathered like gl3 `lightingCB`, textures set like gl3 `setTexture` (texture's filter and
addressing), vertex alpha per mesh, and the PS2 GS alpha-test emulation. The host keeps
geometries by id; destroyed textures and geometries live until the frame that may still use them
is drawn. MatFX atomics (cars) use the default pipeline for now (no environment-map pass).
Checked: `tests/im2d.rs` lit-mesh case on both backends; in game the saveload lobby and all spot
screenshots show the same geometry and lighting as GL3. Still different: no colour filter / post
effects (GL3's warmer grading; needs the frame as a texture), skinned models (peds, the player)
aren't drawn yet (librw gives platforms without a skin pipeline a do-nothing placeholder), no
car reflections. One GL3 bug not copied: its spot light code returns early and drops the other
lights.

**Stage 1, step 5 done: skinned models (peds, the player).** WGPU registers its own skin and
MatFX pipelines (librw otherwise gives a platform a do-nothing placeholder, so skinned and MatFX
atomics were invisible). Skinned geometry carries four weights and bone indices per vertex; the
bone matrices are computed like gl3 `uploadSkinMatrices` (inverse bind matrix times the bone's
matrix relative to the atomic, or local-space hierarchies as they are; identity without a
hierarchy; at most 64) and the shader blends position and normal like gl3 skin.vert, then lights
like the default pipeline (fog from clip w; gl3's skin shader uses clip z). Checked: skinning
test on both backends; in game the player stands in the same pose as in the GL3 shot. The
remaining difference to GL3 in saveload (19 %) is the colour filter, which the WGPU build doesn't
run yet; the user likes the unfiltered look, so the filter will get an off switch (plan 13, 23).

**Stage 1, step 6 done: MatFX environment maps (car reflections) and the colour filter.**
- MatFX env: one pass like gl3 matfxEnvRender / matfx_env shaders (env matrix from the env
  frame or camera as gl3 uploadEnvMatrix, coefficient, PC/PS2 lighting clamp, env colour,
  premultiplied, source blend ONE). In game the env pass runs (about 28k env draws over the spots
  run); the look of cars still needs a person (docs/PLAYTEST.md), the test scenes show no car
  up close. Offscreen test: env texel choice and coefficient on both backends.
- Frames are drawn into an offscreen RGBA8 scene texture and blitted to the window. librw
  `rasterRenderFast` (camera into a texture, CMBlur's frame copy) becomes a copy of the scene so
  far; screenshots read the scene texture (no swapchain copy, no extra redraw on GL). The scene
  has no alpha channel, like D3D's X8R8G8B8 back buffer (writes are colour only, alpha reads 1).
- With that CMBlur's colour filter and glow run: the difference to the GL3 references drops from
  ~19 % to 0.7-3.7 % (saveload 1.1 %), average colour equal. What remains are fringes at bright
  edges in the glow layer. Ruled out: frame pacing (identical without vsync), a global shift, and
  the glow direction (mirroring the game's +2 px glow offset doesn't help). The WGPU path follows
  the game code and D3D semantics, so the GL3 references are close guides here, not pixel truth;
  the wgpu build will need its own reference set when it becomes the default.

**Colour filter options (stage 1, step 7).** The wgpu build now has re3's colour filter options
too (`EXTENDED_COLOURFILTER`): the WGPU device has an Im2D override shader like gl3's
`im2dOverrideShader`, with re3's colourfilterVC (PS2/Xbox filter) and contrast (mobile) shaders
ported to WGSL (`tests/im2d.rs` checks both against the formulas). Display Setup > Colour Filter
offers None, Simple, PS2/Xbox (re3's "Normal", the default), Mobile and the new PC mode (the
original PC filter, CMBlur, with trails per the Trails setting; value 4 so saved settings keep
their meaning). This explained the earlier gap: the GL3 references use PS2/Xbox, the wgpu build
had been running the PC filter. With the same filter the wgpu build passes saveload against the
GL3 reference (0.31 %); five spots are 0.31-0.37 %, three 0.6-1.0 %.
References: no screenshots of the real PS2 or Xbox versions were found to compare with. The
re3/SkyGfx documentation describes the filter as shared by PS2 and Xbox ("the original PS2/XBOX
filter"), so there is one mode for both.

**Own regression references for the wgpu build (stage 1, step 8).** `scripts/test.sh --wgpu`
builds the wgpu variant (rust/target-wgpu) and compares it with `tests/reference-wgpu/` (made on
Vulkan); `REVC_RENDERER_BACKEND=gl` runs the same against the GL backend, with a 1.5 % screenshot
limit because the drivers' anisotropic filtering differs on high-frequency textures (Starfish
Island water and fences: 1.0 %; 0.008 % with anisotropy off). All scenarios pass on GL3, wgpu
Vulkan and wgpu GL.
- Anisotropic filtering: re3's ANISOTROPIC_FILTERING gives every texture the device's maximum;
  WGPU now reports 16 and samples trilinear textures anisotropically. GL3 applies a texture's
  anisotropy only when the stage's filter mode changes (`setFilterMode`), so there it is on for a
  scattering of textures only; WGPU follows the option's intent (crisper distant roofs/roads).
- The gamepad scenario takes its screenshots at fixed game times after its start (4, 9, 14 s)
  instead of 1.5 s after each step, so animated things in the shot (save pickup, zone name) no
  longer depend on frame pacing; references re-blessed for both renderers.
- test.sh: blessing saveload/texmods creates the reference folder (it silently failed before),
  and a missing saveload/texmods reference is reported instead of silently skipping the check.

**Neo pipelines, the Xbox look (stage 1, step 9).** `EXTENDED_PIPELINES` now works on WGPU:
`src/extras/custompipes_wgpu.cpp` ports custompipes_gl.cpp's render callbacks onto a small model
API of the WGPU device (`instanceAtomic`, `beginModel`, `setModelMaterial`, `setTexture`,
`drawMesh`, `renderDefault` in rwwgpu.h), and revc-rw has the Neo shaders in WGSL
(MESH_NEO_VEHICLE / WORLD / GLOSS / RIM; rim also skinned). Needed for it:
- Render targets: a camera whose frame buffer is a camera texture draws into that texture
  (`Op::Target`; depth buffer per size). The env map camera renders the scene into it every frame.
- Model vertices carry the second UV set (world lightmaps); the model uniform has a 12-vector
  block for the Neo parameters.
- Faithful to gl3: vehicle and rim shaders light with directionals only, the world shader with
  ambient only (default_UV2.vert is built without light defines), and the gloss pass reads
  light 0's direction as gl3 leaves it from the last lit atomic (roads are unlit).
Checked against GL3 with all four effects on (scratch config, not a committed test): per shot the
Neo-on minus Neo-off delta matches between the renderers (saveload, where Neo changes 5.7 % of
pixels: 98 % of those agree; elsewhere the mismatches are at most 0.12 % of pixels). Offscreen
tests: render to texture and the world lightmap formula, both backends. Cars with the Neo vehicle
shader still need a look in person (no car in the test shots).

**Screen droplets (stage 1, step 10).** `SCREEN_DROPLETS` works on WGPU: the device draws
re3's two-coordinate Im2D (`im2DRenderIndexedPrimitiveUV2`) and revc-rw has the droplet shader
(drop mask times the frame copy at the second coordinates, no perspective) in its own small
WGSL module. Checking aid: `REVC_TEST_DROPLETS=n` puts n drops on each captured frame (after
the capture reseed, so both renderers place them the same); with 60 drops on the spots, the
pixels the drops change agree 71-89 % between GL3 and wgpu (rest: filtering). Offscreen test of
the droplet shader on both backends. Not part of the regression set (no rain there).

**Leeds-style world renderer (NEW_RENDERER).** re3's optional world renderer (ini `[Rendering]
NewRenderer=1`, debug menu Render > New Renderer; off by default) now builds on WGPU too, so no
render extra is compiled out any more. `WorldRender` in custompipes_wgpu.cpp is custompipes_gl.cpp's
on the device's model API: buildings lit by the ambient light only, opaque meshes in the first
pass, atomics with blended meshes deferred to the no-z/additive/blend passes, fading by the
building's alpha. Checked with NewRenderer=1 in both builds (scratch ini): where the new renderer
changes the picture noticeably (Washington Beach, 1 % of pixels), the change agrees between GL3
and wgpu on 82 % of those pixels; elsewhere it changes under 0.5 % and the shots stay within the
usual GL3/wgpu difference. One visible effect explained: water sets only its raster and draws
with the filter the last draw left (D3D semantics, which WGPU follows). In the default renderer
that is the last building's trilinear filter, so wgpu water is smooth; with the new renderer the
last mesh leaves plain linear, which is also the water texture's own mode, giving the same sparkly water as
GL3, which always draws water with the texture's own filter.

**MSAA (plan task 22, first antialiasing option).** The game's own MultiSampling setting
(Display Setup, 2x/4x/8x; applied by recreating the window, as in GL3) now reaches the wgpu
renderer: the scene is drawn into a multisampled colour and depth target and resolved into the
scene texture at the end of every pass, so frame copies (colour filter, droplets), screenshots
and the window all see the resolved image; texture targets (env map) stay single-sampled. The
count is clamped to what the device supports for the scene formats
(`Renderer::supported_samples`); the log says `revc: MSAA 4x (asked 4x)`. Checked: offscreen
test (edge pixels only with MSAA) on both backends; in game 4x on Vulkan and GL, visibly smooth
edges.

**Antialiasing suite (plan task 22).** One setting, Display Setup > Antialiasing (wgpu build;
replaces MultiSampling there; ini `[Graphics] Antialiasing=0..8`), switches live, levels from
cheapest to most expensive:

| Level | Method | Cost |
|---|---|---|
| 0 Off | | none |
| 1 FXAA | post pass on the final image | one cheap full-screen pass |
| 2 SMAA | SMAA 1x (preset High): edges, blend weights, blend | three full-screen passes |
| 3 MSAA 2x | multisampled scene, resolved each pass | 2 samples on geometry edges |
| 4 MSAA 4x | | 4 samples |
| 5 MSAA 4x + SMAA | both | |
| 6 MSAA 8x | | 8 samples |
| 7 SSAA 2x | scene drawn at 2x width and height, filtered down | 4x the pixels |
| 8 SSAA 2x + MSAA 4x | both | highest |

`revc_rw::frame::Antialiasing::LEVELS`; the device sends the level to the renderer when it
changes (`rw::wgpu::antialiasing`, checked in beginUpdate) and again for a new window. After the
scene, a short chain: supersampled scene filtered down to window size, then FXAA or SMAA, then shown;
screenshots read the last image; frame copies (colour filter, droplets) are filtered down too when
supersampling. MSAA counts other than 1 and 4 need wgpu's adapter-specific format features, now
requested when available (2x/8x then work on Vulkan; without them the count falls back).
Checked: offscreen test of all nine levels on both backends (pure inside/outside, Off leaves the
edge hard, every other level softens it); in game every level applies (log
`revc: antialiasing ...`). FXAA also softens HUD text a little, as FXAA does. SMAA is a WGSL port of
the reference SMAA.hlsl (iryoku/smaa, MIT) with its precomputed area and search textures in
`rust/revc-rw/assets/smaa/` (source and hashes in SOURCE.md); the offscreen test requires it to
come closer to MSAA 8x than Off and FXAA (Vulkan and GL: off 0.83, FXAA 0.77, SMAA 0.62 mean
difference). The levels were renumbered when SMAA came in: an ini saved before now picks the
next cheaper level. Not in the suite: TAA (no motion
vectors in this renderer). Temporal upscalers (FSR 3/4, XeSS, DLSS) are a later goal.

**HUD overlay and render scale (plan task 22).** The game marks the end of the 3D scene
(`rw::wgpu::beginOverlay()` in `main.cpp` right before `Render2dStuff`): the renderer then
finishes the scene (scaled to the window, then FXAA or SMAA) and draws the HUD on top at window
resolution, single-sampled. So FXAA/SMAA/SSAA no longer touch the HUD. Frames without the marker
(front-end menus, loading screens) are drawn whole as before. Everything up to the marker (colour
filter, motion blur, droplets) works on the scene. The renderer can draw the scene at a lower
resolution (`Renderer::set_render_scale`, 25-100 %, plain bilinear upscale) with the HUD staying
sharp; it is not a player setting (user decision 2026-11-09) but the hook for the upscalers
(FSR, XeSS, DLSS), which will set it from their quality modes. Checked: offscreen test
(`overlay_*`: the HUD's edges stay hard at every scale and antialiasing level, the scene's soften
only when scaled or antialiased) on both backends; in game at 50 % (temporary setting, since
removed): world at half resolution, HUD identical to 100 %.

**Visual style presets (plan task 13).** Graphics > Visual Style (both builds) picks a platform
look in one step: PC (PC colour filter, no PS2 alpha test, MatFX cars), PS2 (PS2/Xbox filter,
PS2 alpha test), XBOX (as PS2 plus the Neo pipelines: vehicle reflections, rim light, world
lightmaps, road gloss; the reference look), MOBILE (mobile filter). The Neo switches are set only
when game/neo/neo.txd exists. It stores nothing itself: it reads and sets the individual options
(which stay in the menu and the ini), and shows CUSTOM for any other combination. Motion blur and
antialiasing are not part of a style. Xbox/PS2 HUD and menus are a later step.

## Layering (2026-11-13)

The renderer is layered so its lower parts stay engine-neutral and reusable:

| Layer | Crate / file | Knows |
|---|---|---|
| GPU core | `rust/retro-wgpu` (engine-neutral, `docs/RETRO_WGPU.md`) | wgpu only: state model, textures, targets, MSAA/SSAA/FXAA/SMAA, output stage, programs |
| RW translator, front end | `revc-rw/src/front.rs` (`Recorder`, `Frame`) | RenderWare device semantics; no GPU types, runs on the game thread |
| RW translator, back ends | `revc-rw/src/frame.rs` (`WgpuBackend`, presenting today), `revc-rw/src/retro.rs` (`RetroBackend` on retro-wgpu) | drawing a recorded `Frame` |
| Threading | `revc-rw/src/thread.rs` (`RenderThread`) | one frame queued, frames recycled, captures waited for |
| Game glue | `revc/src/host/wgpu.rs` (+ the C++ WGPU device in `cpp/revc`) | SDL window, the game's C ABI, settings |

**Front end / back end.** The device's calls (clears, uploads, Im2D/Im3D, model draws, targets,
frame copies) are recorded into a `Frame`: commands plus their vertex, uniform, model and bone
arenas, the resource uploads and destructions that came with them, and the frame's settings
(MSAA, supersampling, FXAA/SMAA, render scale). Resource ids are allocated by the front end.
A back end applies the uploads before the frame's draws (as before: queue writes land before
the frame's command buffer) and frees destroyed resources after it. A settings change applies
to the next frame, or to the current one while nothing is recorded yet, so recorded pipelines
always match the passes they're drawn in.

**Render thread** (`REVC_RENDER_THREAD=1`, off by default): presenter and back end move to
`revc-render`; the game records frame n + 1 while frame n is drawn (at most one frame ahead).
saveload and spots give the same images inline and threaded (within run-to-run noise).

**Choosing the presenting back end** (`REVC_RENDERER=retro`, opt-in): retro-wgpu presents
instead of `WgpuBackend` (through the `DrawFrames` trait both implement; screenshots read its
final image). saveload, spots, texmods and gamepad pass against the references with it. It draws
on the game thread for now: the render thread needs a `Send` back end, and retro-wgpu's post-pass
and output-stage trait objects aren't `Send` yet. Shadow mode needs `WgpuBackend` presenting.
Speed (2026-11-13, RX 9070 XT, Vulkan, 1280x720, `scripts/perf.sh`, alternating runs):
`WgpuBackend` 0.75 ms mean over the spots (worst p99 1.23 ms); retro-wgpu first 1.66 ms, after
the profiling pass (perf, `cycles:u`) 0.94 ms (worst p99 1.51 ms), about 1.25x. Fixed: per-draw
block allocation and conversion (blocks now stored flat at the GPU stride, given in parts
straight from the frame), SipHash in retro-wgpu's caches and the front end's id sets (Fx hash),
four texture stages bound where RW needs one or two (programs declare `stages`), and duplicate
pipeline/bind-group lookups. Tried and dropped: a packed bind-key hash (slower). What's left is
mostly wgpu-core's per-draw binding: retro-wgpu rebinds the program block for every model,
where `WgpuBackend` indexes models in one storage buffer (retro-wgpu keeps to GLES limits, no
storage buffers). Not the default yet.

**retro-wgpu back end and shadow mode** (`REVC_RENDER_SHADOW=1`, inline renderer only): the
wgpu back end presents as usual while `RetroBackend` draws the same frames off-screen. RW's
shading (lighting with point/spot lights, MatFX env, Neo vehicle/world/gloss/rim, CPostFX colour
filter and contrast, screen droplets, alpha window, fog) runs as retro-wgpu programs generated
from `frame.rs`'s WGSL (one source; `retro::mesh_wgsl` remaps bindings and vertex locations).
Every `REVC_RENDER_SHADOW_EVERY` frames (default 30) and on every capture both images are read
back before the output stage and compared: the gate is 99.5 % of pixels within 2 levels. Frames
below it are logged and, with `REVC_RENDER_SHADOW_DIR`, written as PNG pairs; a summary line is
printed at exit. `revc-rw/tests/shadow.rs` runs the gate headless on a frame with every draw kind
(both back ends match exactly on Vulkan and GL), also at every antialiasing level and with a
render scale (`shadow_levels_*`). The overlay split (render scale, supersampling or post AA with
a HUD marker) uses retro-wgpu's output-resolution overlay; FXAA runs as this renderer's own pass
(`retro::RwFxaa`) so both back ends antialias alike. With a render scale below 100 % plus post
AA the order differs (WgpuBackend filters to the window first, retro-wgpu after its post chain);
render scale is only the upscaler hook, not a player option.

## What the game calls

The game reaches librw only through the RenderWare-style API in `cpp/re3-miami/src/fakerw`
(527 functions declared). **270 of them are called** from game code (all listed in the
appendix), grouped here with the number of call sites. (The plan's "326" counted every
Rw/Rp/Rt-named call; 56 of those are game functions with RW-style names, e.g. RpAnimBlend*.)

| Area | Functions | Call sites | Most used |
|---|---|---|---|
| Immediate mode 2D/3D (Im2D/Im3D) | 21 | 1392 | `RwIm3DVertexSetPos` (184), `RwIm2DVertexSetIntRGBA` (148), `RwIm3DVertexSetRGBA` (115), `RwIm3DVertexSetV` (110), `RwIm3DVertexSetU` (101), `RwIm2DVertexSetScreenZ` (87) ... |
| Render state | 5 | 685 | `RwRenderStateSet` (678), `RwRenderStateGet` (4), `RwD3D8EngineSetRefreshRate` (1), `RwD3D8EngineSetMultiSamplingLevels` (1), `RwD3D8EngineGetMaxMultiSamplingLevels` (1) |
| Camera / viewport | 23 | 351 | `RwCameraGetNearClipPlane` (118), `RwCameraGetRaster` (53), `RwCameraSetNearClipPlane` (52), `RwCameraGetFrame` (27), `RwCameraEndUpdate` (14), `RwCameraBeginUpdate` (14) ... |
| Raster / texture / image | 44 | 439 | `RwTextureRead` (81), `RwTextureDestroy` (81), `RwTextureGetRaster` (75), `RwRasterGetWidth` (24), `RwRasterGetHeight` (20), `RwRasterCreate` (19) ... |
| Frame / matrix / vector math | 43 | 540 | `RwFrameGetMatrix` (142), `RwFrameGetLTM` (43), `RwV3dTransformPoints` (28), `RwMatrixGetPos` (25), `RwFrameCreate` (23), `RwMatrixScale` (22) ... |
| Clump / atomic / geometry / material | 56 | 514 | `RpAtomicGetGeometry` (53), `RpAtomicGetFrame` (52), `RpClumpGetFrame` (45), `RpClumpForAllAtomics` (33), `RpAtomicSetFlags` (30), `RpGeometryForAllMaterials` (18) ... |
| Lights / world | 14 | 67 | `RpLightSetColor` (24), `RpLightSetFlags` (9), `RpLightGetFrame` (9), `RpLightSetFrame` (4), `RpLightDestroy` (4), `RpLightCreate` (4) ... |
| Skinning / animation hierarchy | 16 | 88 | `RpHAnimHierarchyGetMatrixArray` (33), `RpHAnimIDGetIndex` (31), `RpSkinGeometryGetSkin` (6), `RpSkinGetSkinToBoneMatrices` (2), `RpSkinGetNumBones` (2), `RpSkinAtomicGetHAnimHierarchy` (2) ... |
| Material effects / anisotropy | 12 | 21 | `RpMatFXMaterialSetEnvMapFrame` (3), `RpMatFXMaterialGetEffects` (3), `RpMatFXAtomicEnableEffects` (3), `RpMatFXMaterialSetEnvMapCoefficient` (2), `RpMatFXMaterialSetEffects` (2), `RpAnisotGetMaxSupportedMaxAnisotropy` (2) ... |
| Engine / streams / plugins / memory | 24 | 203 | `RwObjectGetType` (37), `RwMalloc` (24), `RwFree` (24), `RwStreamFindChunk` (23), `RwStreamRead` (22), `RwStreamClose` (19) ... |
| Character sets / debug text | 7 | 17 | `RtCharsetDestroy` (5), `RtCharsetCreate` (5), `RtCharsetPrintBuffered` (2), `RtCharsetBufferFlush` (2), `RtCharsetOpen` (1), `RtCharsetGetDesc` (1) ... |
| Other | 5 | 10 | `RwRGBAAssign` (6), `RwOsGetFileInterface` (1), `RtPNGImageWrite` (1), `RtBMPImageWrite` (1), `RtBMPImageRead` (1) |

## Approach

These functions all work on librw's object model (`RwTexture`, `RwFrame`, `RpClump` ... are
librw objects), so they can't be swapped one by one. The port goes in two stages:

1. **A wgpu platform for librw.** (librw's NULL platform can't be the base: its
   `rasterCreate` is `assert(0 && "not implemented")`, so nothing could even load a texture.
   The new platform needs real rasters and native-texture conversion, about the size of the GL3
   driver's raster and device parts.) librw talks to graphics through a device/driver interface
   (`vendor/librw/src/rwengine.h`): camera begin/end update and clear, show raster, render
   states, rasters (create, lock/unlock, from/to image), Im2D/Im3D primitives, plus per-platform
   instancing and render pipelines for atomics, skinned and MatFX geometry. Today that is the GL3
   driver; the first milestone implements it in Rust on revc-rw, behind a C ABI, as a new librw
   platform. The game and librw's objects stay as they are, but every frame is drawn by wgpu on
   Vulkan or OpenGL, and the existing screenshot tests (saveload, spots, texmods, gamepad)
   compare it with the GL3 driver. Interface entry points:

```
struct Camera;
struct Image;
struct Texture;
struct Raster;
struct Device
void  (*beginUpdate)(Camera*);
void  (*endUpdate)(Camera*);
void  (*clearCamera)(Camera*, RGBA *col, uint32 mode);
void  (*showRaster)(Raster *raster, uint32 flags);
bool32 (*rasterRenderFast)(Raster *raster, int32 x, int32 y);
void  (*setRenderState)(int32 state, void *value);
void *(*getRenderState)(int32 state);
void   (*im2DRenderLine)(void *vertices, int32 numVertices, int32 vert1, int32 vert2);
void   (*im2DRenderTriangle)(void *vertices, int32 numVertices, int32 vert1, int32 vert2, int32 vert3);
void   (*im2DRenderPrimitive)(PrimitiveType primType, void *vertices, int32 numVertices);
void   (*im2DRenderIndexedPrimitive)(PrimitiveType primType, void *vertices, int32 numVertices, void *indices, int32 numIndices);
void (*im3DTransform)(void *vertices, int32 numVertices, Matrix *world, uint32 flags);
void (*im3DRenderPrimitive)(PrimitiveType primType);
void (*im3DRenderIndexedPrimitive)(PrimitiveType primType, void *indices, int32 numIndices);
void (*im3DEnd)(void);
struct Driver
Raster* (*rasterCreate)(Raster*);
uint8 *(*rasterLock)(Raster*, int32 level, int32 lockMode);
void   (*rasterUnlock)(Raster*, int32 level);
uint8 *(*rasterLockPalette)(Raster*, int32 lockMode);
void   (*rasterUnlockPalette)(Raster*);
int32  (*rasterNumLevels)(Raster*);
bool32 (*imageFindRasterFormat)(Image *img, int32 type,
bool32 (*rasterFromImage)(Raster*, Image*);
Image *(*rasterToImage)(Raster*);
Constructor ctor, Destructor dtor){
struct EngineOpenParams;
```

2. **The object model into Rust**, one subsystem at a time, each with recorded-C++
   characterisation tests like the text and save ports (see `tests/golden/`): frame and matrix
   math, textures and dictionaries (revc-formats already reads TXDs), geometry and clumps
   (revc-formats reads DFFs), skinning and animation hierarchies, lights, camera. When nothing
   calls librw any more it is deleted, together with the C++ shim.

Order inside stage 1, smallest testable steps first: device + camera clear + show raster (a
blank game frame, done) -> rasters and textures + Im2D + render states (whole front end, done) ->
DXT kept compressed (BC, done) -> Im3D (effects, done) -> default atomic pipeline (world, cars,
done) -> skin (done) -> MatFX (done) -> post effects (colour filter done; others to check).
The front-end and HUD come first because the boot scenario and every in-game screenshot show
them.

### Stage 2, step 1: textures (plan, 2026-11-10)

Goal: game textures come from Rust readers, converted once into a form the renderer loads
directly, so librw's native texture readers (D3D8/D3D9/Xbox/PS2, `rwNativeTextureHackRead`) are
no longer used for game data and can leave the game binary (they stay in the importer only).

- **1a. TXDs read by revc-formats.** `RwTextureGtaStreamRead` (rw/TexRead.cpp, used by
  CTxdStore, FileLoader and the Neo TXD) hands the native chunk to Rust (`revc_formats::txd`, which
  already reads PC D3D8 natives and is checked against librw by the `formats` test), which
  returns name, mask, filter/addressing, raster format, alpha and the levels: DXT blocks as they
  are, everything else as RGBA8. C++ builds the WGPU raster from that (DXT: as `dxt_to_wgpu` /
  texmods' `DxtRaster` do; RGBA8: per level, keeping the original's mip and AUTOMIPMAP flags) and
  the `RwTexture` around it. Xbox natives (platform 5) are added to revc-formats for the importer.
  Check: all screenshot scenarios identical to now (same pixels, not just within tolerance), plus
  a characterisation test comparing every TXD in the game's IMG and loose files texture by
  texture (format, flags, levels, bytes) between the two paths.
  **Done 2026-11-10:** `rust/revc/src/txdload.rs` + `RwTextureGtaStreamRead` (rw/TexRead.cpp),
  default; `REVC_TXD=librw` keeps the old reader, `REVC_TXD=check` reads with both and compares
  the WGPU rasters field by field and byte by byte. The `formats` test runs in check mode and
  loads every TXD in gta3.img (`revc_test_load_all_txds`): 12,811 textures, 0 differences.
  Xbox natives: revc-formats reads them too (unswizzled, BGRA palettes swapped; LZO-compressed
  `gta3.img` entries unpacked by `revc_formats::xboximg`). With `REVC_TXD_CHECK_DIR` pointing at
  the user's Xbox disc files the formats test also reads all of those: 1,392 TXDs, 17,401
  textures, 0 differences to librw.
- **1b skipped:** loading all 1,295 game TXDs takes ~30 ms with either reader (main machine;
  PC textures are mostly DXT, copied as they are), so a converted cache would gain nothing.
- **1b. Converted cache.** On first run (and after the game files or the converter version
  change), every TXD in gta3.img and the loose ones is converted into one reMiami texture pack under
  `userfiles/cache/` (header with a source hash and converter version; per texture the fields
  above and the GPU-ready levels: BC blocks or RGBA8 with mips already generated). Loading then
  copies levels straight into rasters. A progress screen like the original's "converting
  textures" one; missing or stale cache falls back to 1a.
- **1c. Remove the old path.** Once 1a/1b pass on both backends and the N150, the game no longer
  calls librw's native texture readers; the D3D/Xbox/PS2 texture code leaves the game build
  (geometry natives are not used by PC data).
  **Done 2026-11-10:** `vendor/librw/src/d3d`, `ps2` and the WDGL reader are deleted (~13,000
  lines); librw keeps only its core and the WGPU platform. Console geometry (native data in the
  Xbox/PS2 DFFs) is skipped with a message; reading it is the importer's job, in Rust. The check
  against librw survives as recorded hashes: `tests/golden/textures.txt` (every PC texture) and
  `tests/golden/textures-xbox.txt` (plus the Xbox release's, with `REVC_TXD_CHECK_DIR`).



### Stage 2, step 2: models (plan, 2026-11-10)

Goal: DFF models (clumps, and the loose atomics/geometries the game streams) are read in Rust, so
librw's stream readers for frames, geometry, materials, atomics and their plugins leave the game
binary. The librw objects themselves (RpClump, RwFrame, RpGeometry ...) stay for now: the C++
side builds them from what Rust read, as TexRead.cpp does for textures. Replacing the objects
is the step after (frames and matrices first).

What a DFF read through librw reads today (everything else, e.g. Rockstar's 2dfx and night vertex
colours, is skipped already): frame list (+ HAnim hierarchy, the game's NodeName, user data),
geometry list (vertices, normals, UVs, prelight, morph targets, triangles, materials; + BinMesh,
Skin, user data; native data is skipped since 1c), materials (colour, surface properties,
texture name/mask/filter; + MatFX, UV animation, rights, user data), textures (+ anisotropy),
atomics (frame, geometry; + rights, MatFX, skin pipeline). Entry points: RpClumpStreamRead and
RpClumpGtaStreamRead1/2 (core/FileLoader.cpp, rw/ClumpRead.cpp, renderer/PlayerSkin.cpp).

- **2a. Rust reader + builder + check mode.** revc-formats::dff gets the plugins it keeps as raw
  bytes today (HAnim, NodeName, Skin, MatFX, BinMesh, UV animation, user data, anisotropy). The
  game's clump readers hand the chunk to Rust; C++ builds the librw objects from the result
  (`REVC_DFF=rust|librw|check`, like REVC_TXD). Check mode compares the two object graphs field by
  field and byte by byte (frames: matrices, names, hierarchy, HAnim; geometry: flags, every
  vertex array, triangles, meshes, skin weights/indices/inverse matrices; materials; atomics), for
  every model in gta3.img (a test hook loads them all, as for TXDs).
  **2a done 2026-11-10:** `rust/revc/src/dffload.rs` (reader) + `rw/DffRead.cpp` (builder,
  comparison), default for RpClumpStreamRead and the vehicles' split read (RpClumpGtaStreamRead1/2;
  `REVC_DFF=librw` keeps librw's). The formats test reads every model both ways and compares
  (REVC_DFF_CHECK): 4,621 DFFs (gta3.img, models/, models/generic), 0 differences. Found on the
  way: RW 3.1 files (air_vlo.dff) keep the clump extension after the CLUMP chunk. Plugin data is
  still parsed by librw's plugin readers (same bytes); moving those is next.
- **2b. Golden + removal.** Per-model hashes recorded from librw (tests/golden/models.txt), then
  librw's clump/geometry/frame stream readers and plugin stream callbacks are deleted.
  **Done 2026-11-10:** every model plugin the game uses (BinMesh, NodeName, HAnim, Skin, MatFX,
  rights) is read in Rust; tests/golden/models.txt holds a hash per model as librw built it
  (4,621); librw's Clump/FrameList/Geometry/MaterialList/Material/Texture/Atomic/Light stream
  readers and those plugin readers are deleted, with re3's split-read helpers on top of them.


### Stage 2, step 3: frames and matrices (plan, 2026-11-10)

Goal: librw's object model starts moving into Rust, beginning with the parts everything else
hangs on: matrices and the frame hierarchy.

- **3a. rw::Matrix in Rust.** librw's matrix functions (base.cpp: optimize, mult, invert,
  transpose, rotate/translate/scale/transform, getRotation, lookAt, makeRotation, the error
  measures) become forwarders to `rust/revc/src/rwmatrix.rs`, bit-identical including aliased
  calls (dst == src). Checked by `tests/golden/rwmatrix.txt`: 6,000 cases recorded from the
  original C++ (`tests/golden/recorders/rwmatrix`, clang++ -O2), replayed by `cargo test`.
- **3b. Frame logic in Rust.** The hierarchy operations (frame.cpp: add/remove child, transform,
  rotate, translate, scale, getLTM, syncHierarchyLTM, the dirty list, updateObjects) in Rust on
  the existing C++ objects, recorded the same way (a recorder that builds random hierarchies and
  logs every LTM and flag after each operation).
- **3c. Frames owned by Rust.** Frame storage moves to Rust (an arena with stable handles); the
  C++ rw::Frame becomes a thin handle the game keeps using through the fakerw API. Needs the
  plugin data (NodeName, HAnim, the game's frame plugins) to live with it.

**3b done 2026-11-10:** `rust/revc/src/rwframe.rs` holds the frame hierarchy logic on librw's
own memory layout (repr(C) Frame, Object, LLLink): addChild, removeChild, count, the transform
ops, getLTM, syncHierarchyLTM, syncDirty, updateObjects and the dirty list. frame.cpp keeps
thin forwarders. Checked by `tests/golden/rwframe.txt` (60 random scenarios, 3,060 lines with
every LTM, flag and object sync callback after each operation).

**3c done 2026-11-10, in a smaller form than planned:** frame create, destroy,
destroyHierarchy, cloneAndLink and purgeClone run in Rust, and Rust allocates the frames. The
plugin data stays in C++: frame.cpp registers its plugin construct/destruct/copy calls as
hooks (`revc_frame_set_hooks`), so NodeName, HAnim and the game's frame plugins work
unchanged. The frames keep raw pointers, not arena handles: the game reaches into rw::Frame
directly in too many places for handles to pay off yet. Checked by
`tests/golden/rwframe-life.txt` (3,000 random operations with every plugin call in order;
the frame state per line is a hash, as the full dump would be ~500 MB).

Note on that recording: librw's cloneRecurse copies a frame's private flags, so an inner clone
of a frame that was a dirty root carries HIERARCHYSYNC without being in the dirty list.
Reparenting such a frame then unlinks a stale link (the original crashes). The game scenarios don't hit
this; the recorder skips those operations, and the port keeps librw's behaviour.

### Stage 2, step 4: what hangs on frames (plan, 2026-11-10)

The math that reads frame LTMs every frame moves next, each part checked against a recording of
the original C++ as in step 3.

- **4a. Camera.** camera.cpp's view matrix, frustum corners, planes and bounding box
  (cameraSync, buildClipPersp/Parallel, buildPlanes), frustumTestSphere, calczShiftScale and
  setFOV's view window, in `rust/revc/src/rwcamera.rs`. rw::Camera stays a C++ object and
  passes its fields. **Done 2026-11-10:** checked by `tests/golden/rwcamera.txt` (3,000 camera
  syncs with 12,000 sphere tests, 3,000 depth ranges and 3,000 FOVs;
  `tests/golden/recorders/rwcamera`).
- **4b. HAnim and the animation interpolator.** hanim.cpp's updateMatrices (the bone matrix
  stack), the keyframe interpolate/apply callbacks, and anim.cpp's AnimInterpolator
  (setCurrentAnim, addTime). **Done 2026-11-10:** `rust/revc/src/rwhanim.rs`, with base.cpp's
  Quat mult/lerp/slerp; hanimOpen registers the Rust callbacks. Checked by
  `tests/golden/rwhanim.txt` (3,000 quaternion cases; 400 random hierarchies of up to 64 nodes
  with random PUSH/POP trees and animations, 16,400 time steps, the full interpolator state and
  every bone matrix hashed per step; `tests/golden/recorders/rwhanim`).
- **4c. Geometry and atomic math.** Bounding spheres (MorphTarget, Atomic's world sphere),
  generateTriangles, buildMeshes, correctTristripWinding, removeUnusedMaterials.
  **Done 2026-11-10, for what the game runs:** `rust/revc/src/rwgeometry.rs` has
  V3d::transformPoints/transformVectors (the game's RwV3dTransformPoints, and the atomics'
  world bounding spheres), MorphTarget::calculateBoundingSphere, and buildMeshes' indices:
  per-material lists and tristrip.cpp's strip builder (the game strips the water geometry at
  start-up; librw's tunnel pass was already disabled). verifyMesh still checks the strips in
  C++. Checked by `tests/golden/rwgeometry.txt` (3,000 transforms, 2,000 spheres, 600 meshes:
  grids like the water's, with dropped and shuffled triangles, and random triangle soups;
  `tests/golden/recorders/rwgeometry`). generateTriangles, correctTristripWinding and
  removeUnusedMaterials have no callers in the game; they stay in C++ and go with librw.

### Stage 2, step 5: textures, images and rasters (plan, 2026-11-10)

What the game does with librw's texture side at run time: 80-odd RwTextureRead and
RwTextureDestroy sites, texture dictionaries (create, add, find, set current, for all), images
for the HUD/font masks and texture mods, and rasters. Each part moves into Rust on librw's
layouts and is checked against a recording of the original, as in steps 3 and 4.

- **5a. Textures and dictionaries.** texture.cpp's object model: Texture create/destroy with
  reference counts, the global texture list, TexDictionary create/destroy/add/addFront/remove/
  find (case-insensitive, 32 characters) and the current dictionary, and Texture::read's
  find-then-load-then-dummy logic (the find and read callbacks stay overridable; custompipes
  sets its own). Rust allocates both; plugin data stays in C++ through hooks, as for frames.
  **Done 2026-11-10:** `rust/revc/src/rwtexture.rs`; texture.cpp forwards (the default read
  callback, which loads image files, stays C++ until 5b). Checked by
  `tests/golden/rwtexture.txt` (3,675 random operations: dictionaries, textures, references,
  adds, removes, case-insensitive finds, reads with loading and dummies switched on and off,
  every plugin and raster call in order; `tests/golden/recorders/rwtexture`).
- **5b. Images.** The parts of image.cpp the game reaches (create/allocate/free, unpalettize,
  convertTo32, hasAlpha, removeMask, readMasked's search path and file formats) in Rust. The
  rest has no callers in the game or the wgpu driver (DXT decompress and flip, palettize and
  ColorQuant, compressPalette, makeMask/applyMask/extractMask) and is deleted instead.
  **Pixel functions done 2026-11-11:** `rust/revc/src/rwimage.rs` has setPixelsDXT (DXT1/3/5,
  which the wgpu driver uses to read compressed rasters back), hasAlpha, convertTo32,
  unpalettize, palettize with the octree quantizer, makeMask, applyMask and removeMask; image
  objects and their memory stay in C++. A closer look showed readMasked does reach makeMask,
  applyMask and palettize (an image file with a mask), so those were ported, not deleted; what
  had no callers (DXT flipping, compressPalette, extractMask, setPalette, printSearchPath) is
  gone. Checked by `tests/golden/rwimage.txt` (4,000 cases on seeded images of every depth,
  with padded strides; `tests/golden/recorders/rwimage`).
  **Files done 2026-11-11:** `rust/revc/src/rwimagefile.rs` has the search path, getFilename,
  makePath with the case-insensitive path correction, and readPNG/writePNG, readBMP/writeBMP,
  readTGA/writeTGA. PNG goes through the png crate; the bundled lodepng is gone. BMP and TGA
  keep librw's code byte for byte, quirks included. Checked by `tests/golden/rwimagefile.txt`
  over a corpus of synthetic files the recorder writes (`tests/golden/imagefiles/`: 90 PNGs of
  every colour type and bit depth, some interlaced, 60 BMPs, 40 TGAs), byte hashes of the BMP
  and TGA writers, decoded pixels of the PNG writer, and path lookups in a mixed-case tree.
  Found on the way: librw's writePNG of a 16-bit image encodes a buffer its own conversion had
  just freed, and frees it again (the game only writes 32-bit images; the port writes the
  converted pixels), and its BMP reader skips width*bpp % 4 bytes per row, so it reads past the
  end of some BMPs its writer makes (the corpus files carry slack so the recording stays
  defined). Image::read/readMasked and the format table stay C++ glue for now.
- **5c. Rasters.** raster.cpp's bookkeeping (create/destroy, subrasters, createFromImage,
  format helpers) and the wgpu driver's imageFindRasterFormat/rasterFromImage/rasterToImage.
  **Done 2026-11-11:** `rust/revc/src/rwraster.rs` makes and keeps the rasters (Rust allocates;
  plugin data through hooks): create/destroy, subRaster, calculateNumLevels, formatHasAlpha, the
  context stack, and the wgpu driver's raster code (rasterCreate, lock/unlock, imageFindRaster-
  Format, rasterFromImage, rasterToImage, allocateDXT, uploadRaster, the native raster plugin).
  The driver dispatch stays in raster.cpp. The pixel converters, the 4-bit palette packers and
  convertTexToCurrentPlatform had no callers left and are deleted. Checked by
  `tests/golden/rwraster.txt` (2,220 random operations; `tests/golden/recorders/rwraster`).
  Three faults of the stage-1 driver turned up, none reached by the game; the port makes them
  safe: setting an image into a level-less, unlocked texture raster wrote through a null
  pointer; copies to or from a locked mip level of an odd width ran past the level (rasterLock
  halves width and stride separately); and rasterFromImage leaked the unpalettized copy of
  every 4/8-bit image.

### Stage 2, step 6: the scene objects (plan, 2026-11-11)

What's left of librw after step 5 (about 9,800 lines) splits into the scene objects the game
builds and draws (this step) and the engine core under them (step 7). Same method as before:
each part moves into Rust on librw's layouts, checked against a recording of the original, and
code nothing calls is deleted rather than ported.

- **6a. Geometry and materials.** geometry.cpp's objects: Geometry create/destroy/lock/unlock,
  addMorphTargets, allocateData, hasColoredMaterial, the mesh header (geoplg.cpp:
  allocateMeshes, setupIndices, guessNumTriangles); MaterialList (append, find) and Material
  create/clone/destroy/setTexture with their reference counts. The stream writers and the
  tool-only functions (generateTriangles, correctTristripWinding, removeUnusedMaterials) have no
  callers and go.
  **Done 2026-11-11:** `rust/revc/src/rwgeomobj.rs` (Rust objects on librw's layouts, librw's
  allocator and plugin lists through hooks; tristrip.cpp is gone, its verifyMesh moved too).
  Before it, a clean-up commit deleted every model writer (Clump, Atomic, FrameList, Geometry,
  MaterialList, Material, Light, Camera, Texture, MatFX) and the tool-only functions: the game
  writes no models (its only stream write is the TXD conversion, which keeps its writer).
  Checked by `tests/golden/rwgeomobj.txt` (2,321 random operations over geometries, materials,
  textures and mesh headers, every plugin call in order; `tests/golden/recorders/rwgeomobj`).
  The local model (bin/ask-local.py) drafted the functions and the test; both drafts needed
  real fixes (a missed material release, a wrong-direction byte move, flags parsed as hex), so
  the module was finished by hand.
- **6b. Atomics, clumps, lights, world.** clump.cpp, light.cpp and world.cpp: Atomic create/
  clone/destroy/setGeometry, the world bounding sphere's dirty logic, the frame sync callbacks;
  Clump create/clone/destroy and its atomic, light and camera lists; Light create/destroy and
  its parameters; World add/remove and the object lists the renderer walks.
  **Done 2026-11-11:** `rust/revc/src/rwscene.rs` (clumps, atomics, lights, worlds and the
  camera object; the sync callbacks; enumerateLights). Pipelines, instancing and the default
  render callback stay C++ until step 7; the camera's device callbacks are hooks. Checked by
  `tests/golden/rwscene.txt` (5,160 random operations over frames, geometries, atomics, clumps,
  lights, cameras and worlds: hierarchies, clones, world membership, frame syncs, world
  bounding spheres, light enumeration and render order; `tests/golden/recorders/rwscene`). The
  local model drafted the functions; its list walks, setFrame, clone frames and flag values
  were wrong and were rewritten.
- **6c. Plugin data.** skin.cpp (skin weights and bone data: create, copy, destroy, the setters
  the game uses), matfx.cpp (effect setup per material: env map, bump, dual), hanim.cpp's
  frame plugin and HAnimHierarchy create/destroy/attach. uvanim.cpp and userdata.cpp are never
  registered or used by the game and are deleted.
  **Done 2026-11-11:** `rust/revc/src/rwplugins.rs` (the HAnim hierarchy object and frame
  plugin, AnimInterpolator create/destroy, Skin init/findNumWeights/findUsedBones and the
  geometry plugin, all of MatFX's effect data and the material and atomic plugins); the skin
  writers went with the model writers; uvanim.cpp, userdata.cpp and rwuserdata.h are deleted.
  Picking the skin and MatFX pipelines and the old-format skin reader stay C++ for step 7.
  Checked by `tests/golden/rwplugins.txt` (6,607 random operations over frames with HAnim data
  and hierarchies, skinned geometries, materials with every effect combination and their
  textures; the recorder runs librw's own register*Plugin functions on a small working plugin
  list; `tests/golden/recorders/rwplugins`).

Step 7 (outlook): the engine core (Engine open/start/stop, plugin lists, streams, pipelines,
render state, errors), the debug charset, and the wgpu driver's remaining C++ glue; after that
librw and the fakerw shim are gone.

### Stage 2, step 7: the engine core, then librw goes (plan, 2026-11-11)

After step 6 every librw object lives in Rust; the C++ left is the engine core, the wgpu
driver's glue, thin forwarding shells, and fakerw (the RW API the game calls, 358 C++
functions in src/fakerw/fake.cpp). Same method as before: recordings of the original where
there is logic, deletion where nothing calls it.

- **7a. Plugin lists, engine and streams.** plg.cpp (PluginList: register, construct,
  destruct, copy, the stream callbacks) in Rust; the hooks of steps 3 to 6 then collapse into
  calls inside Rust. engine.cpp (Engine init/open/start/stop/term, the memory functions, the
  driver and device tables, engine plugins), error.cpp, prim.cpp's box helpers, and base.cpp's
  streams (StreamFile, StreamMemory, chunk headers, findChunk) that the TXD writer and the
  game's RwStream calls use.
  **Done 2026-11-11 (plugin lists and streams; the engine, errors and prim helpers moved to
  7b):** `rust/revc/src/rwplugin.rs` owns PluginList (register, stream/rights/always callbacks,
  construct/destruct/copy, extension read/write/size/skip, rights) and the global plugin list,
  plus StreamMemory, the 32/16-bit transfers, chunk header read/write and findChunk. plg.cpp and
  the stream part of base.cpp are forwarding shells; Rust calls the virtual stream methods
  (so StreamFile and the game's streams still work) through trampolines installed by a static
  initialiser. The typed reads (readU32 etc.) now start from 0, so a read past the end gives 0
  instead of stack garbage (librw left them uninitialised; the recorder avoids those reads).
  Checked by `tests/golden/rwplugin.txt` (5,065 random operations over four plugin lists with
  eight callback sets, extension chunks read, skipped and written, and the memory stream;
  `tests/golden/recorders/rwplugin`). librw quirk kept out of the recording: streamWrite calls
  a null writer when a plugin has a size callback but no writer.
- **7b. Pipelines and render state.** pipeline.cpp (ObjPipeline and the vertex instancing
  helpers), render.cpp (render state get/set), and the wgpu driver's remaining C++
  (wgpudevice.cpp: device begin/end update, Im2D/Im3D, render states, raster binding;
  wgpupipe.cpp: the default, skin and MatFX pipelines' instancing and render callbacks). The
  Rust renderer under them (revc-rw) already exists, so this is mostly moving the glue, plus
  the pipeline choice left out of 6c.
  **Done 2026-11-11:** in three parts. (1) `rust/revc/src/rwdevice.rs`: the device
  (host window and video modes, GL3-style render states and texture stage 0, camera matrices,
  clears, Im2D/Im3D, the system requests); the render-state block was drafted by the local
  model and corrected (it truncated the raster pointer). (2) `rust/revc/src/rwpipe.rs`: the
  model pipeline (instancing, gl3 lighting, the GS alpha test emulation, MatFX env maps, skin
  matrices); C++ keeps the ObjPipeline objects, the skin/MatFX driver plugins that pick them,
  and the names custompipes_wgpu.cpp calls. (3) `rust/revc/src/rwengine.rs`: the Engine state
  machine, the default memory functions, the driver tables, the subsystem/video queries and
  the error record; RWERROR now prints through the Rust message table (parsed from base.err,
  so the C++ enum and the strings keep one source). error.cpp and prim.cpp are deleted, as are
  pipeline.cpp's instancing helpers, librw's managed memory functions and the null device,
  none of which had callers. This is reMiami's own driver code or librw orchestration without
  logic of its own, so the scenario suites check it on both backends. render.cpp (thin
  wrappers over the device table) goes with fakerw in 7d.
- **7c. The debug font.** charset.cpp (RtCharset, used by the debug menu and debug text):
  ported, or redrawn with the Rust text code if that gives the same pixels.
  **Done 2026-11-11:** ported as is: `rust/revc/src/rwcharset.rs` (open/close, create,
  setColors, destroy, print, printBuffered, flushBuffer; the font bitmap is read from librw's
  vgafont.inc). Checked by `tests/golden/rwcharset.txt` (6,000 random operations: charsets in
  random colours, strings with spaces hidden or not, batches past the 100-character limit,
  raster changes, render states saved and restored, near plane and near Z changes; every Im2D
  batch hashed bit for bit; `tests/golden/recorders/rwcharset`). The game creates the debug
  font at startup, so the suites run the raster path; the debug menu's text itself is not in
  a scenario.
- **7d. librw and fakerw go.** The RW API becomes a Rust C ABI: fakerw's headers declare the
  Rw*/Rp*/Rt* functions extern "C" and Rust exports them (generated from the headers, a good
  job for the local model, then reviewed); fake.cpp and every librw .cpp are deleted. librw's
  headers stay only as the C++ declarations of the Rust-owned layouts (structs, inline field
  accessors) for the game code that is still C++, until the game's own port (stages 4 and 5)
  removes them too. Done when the librw CMake target is gone and both suites still pass.

  **7d in parts (survey 2026-11-11, after 7c).** What C++ is left in librw: plugin stream code
  (HAnim, skin with its legacy reader, MatFX, the BinMesh writer, the atomic and material
  rights plugins, texture anisotropy), Animation's stream read/write and the interpolator
  registry, the TXD dictionary stream, Image::read/readMasked and the file format registry, the
  class statics (each class's plugin list and numAllocated) with the module open/close and
  driver plugin registration, and one-line forwarders. The game's own C++ (rw/DffRead.cpp,
  rw/TexRead.cpp, RwHelper, custompipes_wgpu.cpp) calls rw:: methods directly, so the methods
  must stay declared and defined after the .cpp files go.
  - **7d1. Stream and plugin code to Rust**, with recordings where there is logic: the plugin
    streams above, Animation streams, the TXD dictionary stream, image file lookup.
    **Part 1 done 2026-11-11 (dead code):** nothing writes models, TXDs or animations (the
    model writers went in step 6; the TXD writer already failed in streamWriteNative and is
    only reached when the video card can't do DXT, which the wgpu device always can; the game
    reads its own .ifp files), and nothing reads TXDs or animations through librw (the game's
    rw/TexRead.cpp reads natives in Rust under LIBRW_HOST). So the BinMesh, native data, HAnim,
    rights and anisotropy writers, the Animation streams, the TXD dictionary and native
    streams, and fakerw's native texture hack reader are deleted; RwTexDictionaryStreamWrite
    returns nil (its caller handles that). The plugin registrations stay, since they set the
    object layouts and the IDs the extension reader matches.
    **Part 2 done 2026-11-11:** the image file format registry, Image::read and
    Image::readMasked are in Rust (`rwimagefile.rs`), checked by `tests/golden/rwimageread.txt`
    (4,048 lines: random registrations up to librw's limit of ten, readers or none, search
    paths, files under extensions of either case in the directory and a subdirectory, masks
    found or not; `tests/golden/recorders/rwimageread`; librw's TGA reader asserts on other
    formats, so the recording gives each reader its own format only). The legacy skin reader
    (pre-3.3 skins in the atomic's extension) is deleted: a scan of all 4,621 DFFs in the
    game's models finds no such chunk (all 275 skins are in the geometry's extension).
    **Part 3 done 2026-11-11:** Quat::rotate (the ped IK uses it) is in Rust, checked by
    `tests/golden/rwquat.txt` (4,000 rotations, bit for bit; `tests/golden/recorders/rwquat`);
    the interpolator registry and Animation create/destroy are in Rust (`rwhanim.rs`; layout
    and allocation only, the suites run them through the game's ped animations). Deleted for
    lack of callers: Animation::getNumNodes, strcmp_ci, strncmp_ci, findPointer and the
    RawMatrix functions (only the D3D9 pipelines, compiled out, used them). What is left of
    7d1 is glue (texture anisotropy reading, the stream classes), which goes with 7d2 to 7d4.
  - **7d2. Statics and registration to Rust**: the class plugin lists and counters become Rust
    statics the headers reach through extern "C"; module open/close and the wgpu driver
    plugins register from Rust; the hooks of steps 3 to 7 collapse into direct Rust calls.
    **Part (a) done 2026-11-11:** the class plugin lists, the drivers' lists and the
    numAllocated counters are Rust statics (`revc_plugin_lists`, `revc_driver_plugin_lists`,
    `revc_alloc_counts` in `rwplugin.rs`); `PLUGINBASE(CLASS)` makes `Class::s_plglist` and
    `Class::numAllocated` C++17 inline static references to them (constant-initialised, so no
    static initialisation order), and their C++ definitions are gone. The lists start at the
    Rust layouts' sizes (asserted against `sizeof` on the C++ side) when Engine::init sets them
    up; the driver lists are rebuilt on every init as librw did.
    **Part (b) plan and first phase (2026-11-11).** The hooks stay as Rust structs (the
    golden tests inject their logging through them), but Rust installs default
    implementations itself instead of C++ static initialisers, and librw's globals become Rust
    statics behind C++ inline references. What can only be C++ until the game's own file and
    stream code is ported (the trampolines that make virtual calls on the game's Stream
    objects) goes into one glue file in the game tree for stages 4 and 5. Phase 1 done:
    rw::engine, Engine::memfuncs, defaultMemfuncs, allocLocation, version, build, platform,
    streamAppendFrames and debugFile are Rust statics (`rwengine.rs`); Engine::state is gone
    (Rust keeps the state).
  - **7d3. librw's .cpp files go**: every remaining method becomes an inline forwarder in its
    header (calling `revc_*`); the librw CMake target is removed.
    **7d2 (b) and 7d3 done 2026-11-11:** every librw .cpp file is gone and librw is a header-only
    CMake INTERFACE library (build.rs no longer links an archive for it). How:
    - The modules' hooks fall back to Rust defaults (the class plugin lists and counters, the
      engine's memory functions, Rust callbacks), so no C++ installs them; the golden tests
      still set their own. Module and plugin registrations that only touched Rust moved to Rust
      (frame, image, raster, texture modules, the wgpu raster plugin, anisotropy); the texture
      module's open/close, the default find/read callbacks and the image objects are Rust.
    - The remaining methods are inline in `src/rwimpl.h` (included at the end of rw.h): Rust
      forwarders, plus the C++ glue that must stay C++ while the game is C++ (the trampolines
      that make virtual calls on the game's Stream objects, the plugin callbacks the game
      registers, the driver/pipeline objects the custom pipelines use). Their one-time set-up
      runs from C++17 inline variables (initialised once per program).
    - librw's globals are Rust statics or C++17 inline variables; all librw headers have
      `#pragma once`. (The game compiles as C++11; clang accepts the C++17 inline variables as
      an extension. Moving the game to C++17 is worth doing in stage 4.)
  - **7d4. fakerw goes**: fake.cpp's RW API (358 functions) is exported by Rust under the C
    names fakerw's headers declare (extern "C"); fake.cpp is deleted.
    **Done 2026-11-11, differently from the plan above:** the RW API adapters are inline in
    `src/fakerw/fakeimpl.h` (included at the end of rwcore.h; each body opens with
    `using namespace rw;` so no header imports librw's names into the game) and fake.cpp is
    deleted. Exporting them from Rust under their C names would add a second FFI hop for C++
    callers only, and rewriting ~250 adapters has no behaviour to gain; they move to Rust with
    their callers when the game is ported (stages 4 and 5). With this, stage 2 is complete: no
    C++ is left in librw or fakerw, both are headers over the Rust objects.

## Appendix: every RW function the game calls

| Function | Call sites | Area |
|---|---|---|
| `RwRenderStateSet` | 678 | Render state |
| `RwIm3DVertexSetPos` | 184 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetIntRGBA` | 148 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwFrameGetMatrix` | 142 | Frame / matrix / vector math |
| `RwCameraGetNearClipPlane` | 118 | Camera / viewport |
| `RwIm3DVertexSetRGBA` | 115 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm3DVertexSetV` | 110 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm3DVertexSetU` | 101 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetRecipCameraZ` | 87 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetScreenX` | 87 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetScreenY` | 87 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetScreenZ` | 87 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwTextureDestroy` | 81 | Raster / texture / image |
| `RwTextureRead` | 81 | Raster / texture / image |
| `RwTextureGetRaster` | 75 | Raster / texture / image |
| `RwIm2DVertexSetCameraZ` | 73 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetU` | 68 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm2DVertexSetV` | 68 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RpAtomicGetGeometry` | 53 | Clump / atomic / geometry / material |
| `RwCameraGetRaster` | 53 | Camera / viewport |
| `RpAtomicGetFrame` | 52 | Clump / atomic / geometry / material |
| `RwCameraSetNearClipPlane` | 52 | Camera / viewport |
| `RwIm2DGetNearScreenZ` | 51 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RpClumpGetFrame` | 45 | Clump / atomic / geometry / material |
| `RwFrameGetLTM` | 43 | Frame / matrix / vector math |
| `RwObjectGetType` | 37 | Engine / streams / plugins / memory |
| `RpClumpForAllAtomics` | 33 | Clump / atomic / geometry / material |
| `RpHAnimHierarchyGetMatrixArray` | 33 | Skinning / animation hierarchy |
| `RpHAnimIDGetIndex` | 31 | Skinning / animation hierarchy |
| `RpAtomicSetFlags` | 30 | Clump / atomic / geometry / material |
| `RwV3dTransformPoints` | 28 | Frame / matrix / vector math |
| `RwCameraGetFrame` | 27 | Camera / viewport |
| `RwIm3DTransform` | 27 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm3DEnd` | 26 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwMatrixGetPos` | 25 | Frame / matrix / vector math |
| `RpLightSetColor` | 24 | Lights / world |
| `RwFree` | 24 | Engine / streams / plugins / memory |
| `RwIm2DRenderIndexedPrimitive` | 24 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm3DRenderIndexedPrimitive` | 24 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwMalloc` | 24 | Engine / streams / plugins / memory |
| `RwRasterGetWidth` | 24 | Raster / texture / image |
| `RwFrameCreate` | 23 | Frame / matrix / vector math |
| `RwStreamFindChunk` | 23 | Engine / streams / plugins / memory |
| `RwFrameForAllObjects` | 22 | Frame / matrix / vector math |
| `RwMatrixScale` | 22 | Frame / matrix / vector math |
| `RwStreamRead` | 22 | Engine / streams / plugins / memory |
| `RwFrameDestroy` | 21 | Frame / matrix / vector math |
| `RtQuatRotate` | 20 | Frame / matrix / vector math |
| `RwFrameForAllChildren` | 20 | Frame / matrix / vector math |
| `RwRasterGetHeight` | 20 | Raster / texture / image |
| `RwRasterCreate` | 19 | Raster / texture / image |
| `RwStreamClose` | 19 | Engine / streams / plugins / memory |
| `RpGeometryForAllMaterials` | 18 | Clump / atomic / geometry / material |
| `RwIm2DRenderPrimitive` | 18 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RpAtomicDestroy` | 17 | Clump / atomic / geometry / material |
| `RpAtomicSetFrame` | 17 | Clump / atomic / geometry / material |
| `RwFrameUpdateObjects` | 17 | Frame / matrix / vector math |
| `RwMatrixDestroy` | 16 | Frame / matrix / vector math |
| `RwRasterDestroy` | 16 | Raster / texture / image |
| `RpAtomicRender` | 15 | Clump / atomic / geometry / material |
| `RwMatrixUpdate` | 15 | Frame / matrix / vector math |
| `RwV3dDotProduct` | 15 | Frame / matrix / vector math |
| `RpAtomicGetClump` | 14 | Clump / atomic / geometry / material |
| `RpClumpDestroy` | 14 | Clump / atomic / geometry / material |
| `RpGeometrySetFlags` | 14 | Clump / atomic / geometry / material |
| `RwCameraBeginUpdate` | 14 | Camera / viewport |
| `RwCameraEndUpdate` | 14 | Camera / viewport |
| `RwMatrixCreate` | 12 | Frame / matrix / vector math |
| `RwStreamOpen` | 12 | Engine / streams / plugins / memory |
| `RpAtomicSetGeometry` | 11 | Clump / atomic / geometry / material |
| `RpGeometryGetFlags` | 11 | Clump / atomic / geometry / material |
| `RpAtomicClone` | 10 | Clump / atomic / geometry / material |
| `RwCameraSetRaster` | 10 | Camera / viewport |
| `RpAtomicGetBoundingSphere` | 9 | Clump / atomic / geometry / material |
| `RpLightGetFrame` | 9 | Lights / world |
| `RpLightSetFlags` | 9 | Lights / world |
| `RwEngineGetVideoModeInfo` | 9 | Engine / streams / plugins / memory |
| `RwMatrixGetUp` | 9 | Frame / matrix / vector math |
| `RwTexDictionaryForAllTextures` | 9 | Raster / texture / image |
| `RwV3dSub` | 9 | Frame / matrix / vector math |
| `RpGeometryGetMorphTarget` | 8 | Clump / atomic / geometry / material |
| `RpMaterialGetColor` | 8 | Clump / atomic / geometry / material |
| `RpMorphTargetGetVertexNormals` | 8 | Clump / atomic / geometry / material |
| `RpMorphTargetGetVertices` | 8 | Clump / atomic / geometry / material |
| `RwCameraGetCurrentCamera` | 8 | Camera / viewport |
| `RwCameraSetFarClipPlane` | 8 | Camera / viewport |
| `RwMatrixInvert` | 8 | Frame / matrix / vector math |
| `RwRasterGetDepth` | 8 | Raster / texture / image |
| `RwTexDictionaryDestroy` | 8 | Raster / texture / image |
| `RpClumpRemoveAtomic` | 7 | Clump / atomic / geometry / material |
| `RwCameraClear` | 7 | Camera / viewport |
| `RwCameraGetFarClipPlane` | 7 | Camera / viewport |
| `RwMatrixGetAt` | 7 | Frame / matrix / vector math |
| `RwTexDictionaryCreate` | 7 | Raster / texture / image |
| `RwTextureCreate` | 7 | Raster / texture / image |
| `RpClumpAddAtomic` | 6 | Clump / atomic / geometry / material |
| `RpClumpStreamRead` | 6 | Clump / atomic / geometry / material |
| `RpGeometryUnlock` | 6 | Clump / atomic / geometry / material |
| `RpSkinGeometryGetSkin` | 6 | Skinning / animation hierarchy |
| `RwCameraGetZRaster` | 6 | Camera / viewport |
| `RwFrameGetParent` | 6 | Frame / matrix / vector math |
| `RwImageDestroy` | 6 | Raster / texture / image |
| `RwMatrixGetRight` | 6 | Frame / matrix / vector math |
| `RwMatrixSetIdentity` | 6 | Frame / matrix / vector math |
| `RwRGBAAssign` | 6 | Other |
| `RpGeometryDestroy` | 5 | Clump / atomic / geometry / material |
| `RpMaterialSetColor` | 5 | Clump / atomic / geometry / material |
| `RpMaterialSetTexture` | 5 | Clump / atomic / geometry / material |
| `RtCharsetCreate` | 5 | Character sets / debug text |
| `RtCharsetDestroy` | 5 | Character sets / debug text |
| `RwCameraSetZRaster` | 5 | Camera / viewport |
| `RwEngineTerm` | 5 | Engine / streams / plugins / memory |
| `RwFrameTransform` | 5 | Frame / matrix / vector math |
| `RwImageCreate` | 5 | Raster / texture / image |
| `RwImageFindRasterFormat` | 5 | Raster / texture / image |
| `RwRasterSetFromImage` | 5 | Raster / texture / image |
| `RwTexDictionaryAddTexture` | 5 | Raster / texture / image |
| `RwTexDictionarySetCurrent` | 5 | Raster / texture / image |
| `RwTextureSetFilterMode` | 5 | Raster / texture / image |
| `RwV3dAdd` | 5 | Frame / matrix / vector math |
| `RpAtomicCreate` | 4 | Clump / atomic / geometry / material |
| `RpAtomicSetRenderCallBack` | 4 | Clump / atomic / geometry / material |
| `RpGeometryGetMaterial` | 4 | Clump / atomic / geometry / material |
| `RpGeometryGetPreLightColors` | 4 | Clump / atomic / geometry / material |
| `RpGeometryGetVertexTexCoords` | 4 | Clump / atomic / geometry / material |
| `RpGeometryLock` | 4 | Clump / atomic / geometry / material |
| `RpGeometryTriangleSetMaterial` | 4 | Clump / atomic / geometry / material |
| `RpGeometryTriangleSetVertexIndices` | 4 | Clump / atomic / geometry / material |
| `RpLightCreate` | 4 | Lights / world |
| `RpLightDestroy` | 4 | Lights / world |
| `RpLightSetFrame` | 4 | Lights / world |
| `RwCameraSetFogDistance` | 4 | Camera / viewport |
| `RwEngineClose` | 4 | Engine / streams / plugins / memory |
| `RwEngineGetNumVideoModes` | 4 | Engine / streams / plugins / memory |
| `RwFrameAddChild` | 4 | Frame / matrix / vector math |
| `RwFrameRotate` | 4 | Frame / matrix / vector math |
| `RwFrameTranslate` | 4 | Frame / matrix / vector math |
| `RwRasterGetParent` | 4 | Raster / texture / image |
| `RwRenderStateGet` | 4 | Render state |
| `RwTextureGetName` | 4 | Raster / texture / image |
| `RpAtomicGetFlags` | 3 | Clump / atomic / geometry / material |
| `RpClumpCreate` | 3 | Clump / atomic / geometry / material |
| `RpGeometryCreate` | 3 | Clump / atomic / geometry / material |
| `RpGeometryGetNumTriangles` | 3 | Clump / atomic / geometry / material |
| `RpGeometryStreamRead` | 3 | Clump / atomic / geometry / material |
| `RpMatFXAtomicEnableEffects` | 3 | Material effects / anisotropy |
| `RpMatFXMaterialGetEffects` | 3 | Material effects / anisotropy |
| `RpMatFXMaterialSetEnvMapFrame` | 3 | Material effects / anisotropy |
| `RpMaterialCreate` | 3 | Clump / atomic / geometry / material |
| `RpMaterialDestroy` | 3 | Clump / atomic / geometry / material |
| `RpWorldAddLight` | 3 | Lights / world |
| `RpWorldRemoveLight` | 3 | Lights / world |
| `RwCameraGetViewWindow` | 3 | Camera / viewport |
| `RwCameraSetFrame` | 3 | Camera / viewport |
| `RwCameraShowRaster` | 3 | Camera / viewport |
| `RwIm2DRenderLine` | 3 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwIm3DRenderLine` | 3 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwImageSetPixels` | 3 | Raster / texture / image |
| `RwImageSetStride` | 3 | Raster / texture / image |
| `RwMatrixRotate` | 3 | Frame / matrix / vector math |
| `RwMatrixTransform` | 3 | Frame / matrix / vector math |
| `RwRasterPopContext` | 3 | Raster / texture / image |
| `RwRasterPushContext` | 3 | Raster / texture / image |
| `RwRasterRenderFast` | 3 | Raster / texture / image |
| `RwStreamSkip` | 3 | Engine / streams / plugins / memory |
| `RwTextureSetRaster` | 3 | Raster / texture / image |
| `RwV3dLength` | 3 | Frame / matrix / vector math |
| `RpAnisotGetMaxSupportedMaxAnisotropy` | 2 | Material effects / anisotropy |
| `RpAtomicGetRenderCallBack` | 2 | Clump / atomic / geometry / material |
| `RpAtomicGetWorldBoundingSphere` | 2 | Clump / atomic / geometry / material |
| `RpClumpGetNumAtomics` | 2 | Clump / atomic / geometry / material |
| `RpClumpRegisterPlugin` | 2 | Clump / atomic / geometry / material |
| `RpClumpRender` | 2 | Clump / atomic / geometry / material |
| `RpClumpSetFrame` | 2 | Clump / atomic / geometry / material |
| `RpGeometryAddMorphTarget` | 2 | Clump / atomic / geometry / material |
| `RpGeometryGetNumVertices` | 2 | Clump / atomic / geometry / material |
| `RpGeometryGetTriangles` | 2 | Clump / atomic / geometry / material |
| `RpHAnimAnimationDestroy` | 2 | Skinning / animation hierarchy |
| `RpHAnimHierarchySetFlags` | 2 | Skinning / animation hierarchy |
| `RpLightSetRadius` | 2 | Lights / world |
| `RpMatFXMaterialSetEffects` | 2 | Material effects / anisotropy |
| `RpMatFXMaterialSetEnvMapCoefficient` | 2 | Material effects / anisotropy |
| `RpMaterialGetTexture` | 2 | Clump / atomic / geometry / material |
| `RpMorphTargetCalcBoundingSphere` | 2 | Clump / atomic / geometry / material |
| `RpMorphTargetSetBoundingSphere` | 2 | Clump / atomic / geometry / material |
| `RpSkinAtomicGetHAnimHierarchy` | 2 | Skinning / animation hierarchy |
| `RpSkinGetNumBones` | 2 | Skinning / animation hierarchy |
| `RpSkinGetSkinToBoneMatrices` | 2 | Skinning / animation hierarchy |
| `RtCharsetBufferFlush` | 2 | Character sets / debug text |
| `RtCharsetPrintBuffered` | 2 | Character sets / debug text |
| `RwCalloc` | 2 | Engine / streams / plugins / memory |
| `RwCameraCreate` | 2 | Camera / viewport |
| `RwCameraDestroy` | 2 | Camera / viewport |
| `RwCameraSetViewWindow` | 2 | Camera / viewport |
| `RwEngineGetCurrentSubSystem` | 2 | Engine / streams / plugins / memory |
| `RwEngineSetVideoMode` | 2 | Engine / streams / plugins / memory |
| `RwEngineStop` | 2 | Engine / streams / plugins / memory |
| `RwFrameOrthoNormalize` | 2 | Frame / matrix / vector math |
| `RwFrameRegisterPlugin` | 2 | Frame / matrix / vector math |
| `RwImageAllocatePixels` | 2 | Raster / texture / image |
| `RwImageGetWidth` | 2 | Raster / texture / image |
| `RwMatrixMultiply` | 2 | Frame / matrix / vector math |
| `RwMatrixTranslate` | 2 | Frame / matrix / vector math |
| `RwStreamWrite` | 2 | Engine / streams / plugins / memory |
| `RwTexDictionaryFindNamedTexture` | 2 | Raster / texture / image |
| `RwTexDictionaryGetCurrent` | 2 | Raster / texture / image |
| `RwTextureSetAddressing` | 2 | Raster / texture / image |
| `RpAnisotPluginAttach` | 1 | Material effects / anisotropy |
| `RpAnisotTextureSetMaxAnisotropy` | 1 | Material effects / anisotropy |
| `RpAtomicRegisterPlugin` | 1 | Clump / atomic / geometry / material |
| `RpClumpClone` | 1 | Clump / atomic / geometry / material |
| `RpHAnimAnimationCreate` | 1 | Skinning / animation hierarchy |
| `RpHAnimFrameGetHierarchy` | 1 | Skinning / animation hierarchy |
| `RpHAnimHierarchySetCurrentAnim` | 1 | Skinning / animation hierarchy |
| `RpHAnimHierarchyUpdateMatrices` | 1 | Skinning / animation hierarchy |
| `RpHAnimPluginAttach` | 1 | Skinning / animation hierarchy |
| `RpMatFXMaterialGetEnvMapCoefficient` | 1 | Material effects / anisotropy |
| `RpMatFXMaterialSetEnvMapFrameBufferAlpha` | 1 | Material effects / anisotropy |
| `RpMatFXMaterialSetupEnvMap` | 1 | Material effects / anisotropy |
| `RpMatFXPluginAttach` | 1 | Material effects / anisotropy |
| `RpSkinAtomicSetHAnimHierarchy` | 1 | Skinning / animation hierarchy |
| `RpSkinGetVertexBoneWeights` | 1 | Skinning / animation hierarchy |
| `RpSkinPluginAttach` | 1 | Skinning / animation hierarchy |
| `RpWorldAddCamera` | 1 | Lights / world |
| `RpWorldCreate` | 1 | Lights / world |
| `RpWorldDestroy` | 1 | Lights / world |
| `RpWorldPluginAttach` | 1 | Lights / world |
| `RpWorldRemoveCamera` | 1 | Lights / world |
| `RtBMPImageRead` | 1 | Other |
| `RtBMPImageWrite` | 1 | Other |
| `RtCharsetClose` | 1 | Character sets / debug text |
| `RtCharsetGetDesc` | 1 | Character sets / debug text |
| `RtCharsetOpen` | 1 | Character sets / debug text |
| `RtPNGImageWrite` | 1 | Other |
| `RtQuatConvertToMatrix` | 1 | Frame / matrix / vector math |
| `RwCameraFrustumTestSphere` | 1 | Camera / viewport |
| `RwCameraGetViewMatrix` | 1 | Camera / viewport |
| `RwCameraSetProjection` | 1 | Camera / viewport |
| `RwD3D8EngineGetMaxMultiSamplingLevels` | 1 | Render state |
| `RwD3D8EngineSetMultiSamplingLevels` | 1 | Render state |
| `RwD3D8EngineSetRefreshRate` | 1 | Render state |
| `RwEngineGetCurrentVideoMode` | 1 | Engine / streams / plugins / memory |
| `RwEngineGetNumSubSystems` | 1 | Engine / streams / plugins / memory |
| `RwEngineGetSubSystemInfo` | 1 | Engine / streams / plugins / memory |
| `RwEngineInit` | 1 | Engine / streams / plugins / memory |
| `RwEngineOpen` | 1 | Engine / streams / plugins / memory |
| `RwEngineSetSubSystem` | 1 | Engine / streams / plugins / memory |
| `RwEngineStart` | 1 | Engine / streams / plugins / memory |
| `RwFrameRegisterPluginStream` | 1 | Frame / matrix / vector math |
| `RwFrameRemoveChild` | 1 | Frame / matrix / vector math |
| `RwFrameScale` | 1 | Frame / matrix / vector math |
| `RwFrameSetIdentity` | 1 | Frame / matrix / vector math |
| `RwIm2DGetFarScreenZ` | 1 | Immediate mode 2D/3D (Im2D/Im3D) |
| `RwImageGetPixels` | 1 | Raster / texture / image |
| `RwImageGetStride` | 1 | Raster / texture / image |
| `RwImageSetFromRaster` | 1 | Raster / texture / image |
| `RwImageSetPalette` | 1 | Raster / texture / image |
| `RwImageSetPath` | 1 | Raster / texture / image |
| `RwMatrixCopy` | 1 | Frame / matrix / vector math |
| `RwOsGetFileInterface` | 1 | Other |
| `RwRasterLock` | 1 | Raster / texture / image |
| `RwRasterSubRaster` | 1 | Raster / texture / image |
| `RwRasterUnlock` | 1 | Raster / texture / image |
| `RwTexDictionaryStreamWrite` | 1 | Raster / texture / image |
| `RwTextureSetAutoMipmapping` | 1 | Raster / texture / image |
| `RwTextureSetMipmapping` | 1 | Raster / texture / image |
| `RwTextureSetName` | 1 | Raster / texture / image |
| `RwV3dIncrementScaled` | 1 | Frame / matrix / vector math |
| `RwV3dNegate` | 1 | Frame / matrix / vector math |
| `RwV3dScale` | 1 | Frame / matrix / vector math |

# Renderer port (plan tasks 12-14): replacing librw with revc-rw

Goal: librw removed, rendering in Rust on wgpu with a user-selectable Vulkan or OpenGL backend,
screenshot tests passing on both (plan task 12), then the effects (task 13) and the N150 check
(task 14).

## Where it stands

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
DXT kept compressed (BC, done) -> Im3D (effects, done) -> default atomic pipeline (world, cars) -> skin -> MatFX.
The front-end and HUD come first because the boot scenario and every in-game screenshot show
them.

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

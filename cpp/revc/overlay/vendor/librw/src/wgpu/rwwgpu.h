// WGPU platform (revc, plan task 12): librw objects as usual, drawing done by the Rust host on
// wgpu (rust/revc-rw) with a selectable Vulkan or OpenGL backend. The window comes from the
// host like the GL3 HOST gfxlib, but without a GL context; the device forwards frame work
// (clear, present, ...) to C functions the host implements (revc_rw_*).
#pragma once

namespace rw {

#ifdef RW_WGPU
namespace wgpu {
// Window services of the Rust host (rust/revc/src/host/mod.rs, HostWindowApi): the device asks it
// for displays, modes and the window; revc-rw draws into and presents that window.
struct HostVideoMode
{
	int32 width, height;
	int32 depth;
	int32 refreshRate;
	uint32 flags;
};

struct HostWindowApi
{
	void *user;
	int32 (*numDisplays)(void *user);
	const char *(*displayName)(void *user, int32 display);
	int32 (*numVideoModes)(void *user, int32 display);
	int32 (*getVideoMode)(void *user, int32 display, int32 index, HostVideoMode *mode);
	void *(*createWindow)(void *user, const char *title, int32 display, const HostVideoMode *mode,
	                      int32 width, int32 height, int32 samples);
	void (*destroyWindow)(void *user, void *window);
	void (*getDrawableSize)(void *user, void *window, int32 *width, int32 *height);
};
}

struct EngineOpenParams
{
	const wgpu::HostWindowApi *host;
	void **window;
	int width, height;
	const char *windowtitle;
};
#endif

namespace wgpu {

// Immediate-mode vertices: same layout and accessors as gl3's, so game code that fills them
// (RwIm2DVertex / RwIm3DVertex are RWDEVICE::Im2DVertex / Im3DVertex) works unchanged.
struct Im3DVertex
{
	V3d     position;
	uint8   r, g, b, a;
	float32 u, v;

	void setX(float32 x) { this->position.x = x; }
	void setY(float32 y) { this->position.y = y; }
	void setZ(float32 z) { this->position.z = z; }
	void setColor(uint8 r, uint8 g, uint8 b, uint8 a) {
		this->r = r; this->g = g; this->b = b; this->a = a; }
	void setU(float32 u) { this->u = u; }
	void setV(float32 v) { this->v = v; }

	float getX(void) { return this->position.x; }
	float getY(void) { return this->position.y; }
	float getZ(void) { return this->position.z; }
	RGBA getColor(void) { return makeRGBA(this->r, this->g, this->b, this->a); }
	float getU(void) { return this->u; }
	float getV(void) { return this->v; }
};

struct Im2DVertex
{
	float32 x, y, z, w;
	uint8   r, g, b, a;
	float32 u, v;

	void setScreenX(float32 x) { this->x = x; }
	void setScreenY(float32 y) { this->y = y; }
	void setScreenZ(float32 z) { this->z = z; }
	void setCameraZ(float32 z) { this->w = z; }
	void setRecipCameraZ(float32 recipz) { this->w = 1.0f/recipz; }
	void setColor(uint8 r, uint8 g, uint8 b, uint8 a) {
		this->r = r; this->g = g; this->b = b; this->a = a; }
	void setU(float32 u, float recipz) { this->u = u; }
	void setV(float32 v, float recipz) { this->v = v; }

	float getScreenX(void) { return this->x; }
	float getScreenY(void) { return this->y; }
	float getScreenZ(void) { return this->z; }
	float getCameraZ(void) { return this->w; }
	float getRecipCameraZ(void) { return 1.0f/this->w; }
	RGBA getColor(void) { return makeRGBA(this->r, this->g, this->b, this->a); }
	float getU(void) { return this->u; }
	float getV(void) { return this->v; }
};

void registerPlatformPlugins(void);


// Raster data kept in memory as RGBA8 (top row first) for every mip level and uploaded to a GPU
// texture in the Rust host before it is drawn with (after every write lock).
struct WgpuRaster
{
	int32 numLevels;
	uint8 *levels[16];
	bool32 hasAlpha;
	int32 baseWidth, baseHeight;
	int32 dxt;		// 0: RGBA8 levels; 1, 3, 5: DXT blocks as in the TXD (top row first)
	uint32 texId;		// host texture, 0 = not uploaded yet
	bool32 dirty;		// written since the last upload
	bool32 autogenMipmap;	// MIPMAP|AUTOMIPMAP: one level here, the host builds the chain
	int32 maxAnisotropy;	// as last set by a texture using the raster (gl3 natras->maxAnisotropy)
	// Sampler state of the texture as GL3 keeps it on its GL texture object (gl3 natras->filterMode,
	// addressU, addressV; 0 = never set, GL's defaults): a draw samples with these, not the stage's.
	int32 filterMode, addressU, addressV;
};

// One draw's device state, as the Rust host reads it (revc_rw::frame::DrawState).
struct DrawState
{
	uint32 texture;
	uint32 filter, addressU, addressV;
	uint32 blend, srcBlend, dstBlend;
	uint32 zTest, zWrite;
	uint32 cull;
	uint32 alphaFunc;
	float32 alphaRef;
	uint32 fog;
	float32 fogStart, fogEnd;
	uint32 fogColor;
	float32 xform[4];
	// Im2D override shader (re3 CPostFX colour filters): 0 none, 1 PS2/Xbox colour filter
	// (params[0..3] blur colour), 2 mobile contrast (params[0..2] mult, [4..6] add).
	uint32 shader;
	float32 shaderParams[8];
	uint32 maxAnisotropy;
};

// Im2D vertices with a second coordinate set (re3 screen droplets: Im2DVertex + u2, v2), drawn
// with the bound raster times `tex1` (nil = white) at the second coordinates.
void im2DRenderIndexedPrimitiveUV2(PrimitiveType primType, void *vertices, int32 numVertices,
	void *indices, int32 numIndices, Texture *tex1);

// Antialiasing level (revc_rw::frame::Antialiasing::LEVELS, cheapest first: off, FXAA, SMAA,
// MSAA 2x/4x, MSAA 4x + SMAA, MSAA 8x, SSAA 2x, SSAA 2x + MSAA 4x). The game's menu and ini set
// it; the device applies changes with the next frame.
extern int8 antialiasing;

// The 3D scene of this frame is finished: what follows (HUD, menus) is drawn at window
// resolution on top of the scaled and antialiased scene.
void beginOverlay(void);

// Like gl3::im2dOverrideShader: Im2D draws use this shader until it is reset to 0.
void setIm2DOverride(uint32 shader, const float32 *params);

// Uploads the raster if it changed; returns its host texture (0 if it has no pixels).
uint32 uploadRaster(Raster *raster);

// Makes a (DONTALLOCATE) texture raster hold DXT blocks (D3D8 TXD textures keep their
// compression; the host samples them as BC or decodes them).
void allocateDXT(Raster *raster, int32 dxt, int32 numLevels, bool32 hasAlpha);
int32 dxtLevelSize(int32 dxt, int32 width, int32 height);

// Device state for the next draw (uploads the bound raster if needed) and the camera's
// projection * view (column-major), as set up by the last beginUpdate.
void currentDrawState(DrawState *s);
const float32 *cameraXform(void);

// Model drawing (wgpupipe.cpp), shared by the default, MatFX and skin pipelines and re3's custom
// (Neo) pipelines in custompipes_wgpu.cpp.
// revc_rw::frame::MeshVertex
struct MeshVertex
{
	float32 pos[3];
	float32 normal[3];
	uint8 color[4];
	float32 uv[2];
	float32 uv2[2];	// second texture coordinate set (Neo world lightmaps)
};

// revc_rw::frame::SkinVertex
struct SkinVertex
{
	MeshVertex base;
	float32 weights[4];
	uint8 indices[4];
};

// revc_rw::frame::ModelState
struct ModelState
{
	float32 world[16];
	float32 matColor[4];
	float32 surfProps[4];
	float32 ambient[4];
	float32 lights[8][4][4];	// params, position, direction, colour
	float32 texMatrix[16];	// MatFX env: normal to env texcoords
	float32 colorClamp[4];
	float32 envColor[4];
	float32 fxParams[4];	// coefficient, 1 - frame-buffer alpha
	float32 extra[12][4];	// Neo shader parameters (revc_rw::frame::MESH_NEO_*)
};

struct InstanceData
{
	uint32 first, count;	// triangle-list index range in the host geometry
	Material *material;
	bool32 vertexAlpha;
};

struct InstanceDataHeader : rw::InstanceDataHeader
{
	uint32 serialNumber;
	int32 numMeshes;
	InstanceData *inst;
	uint32 geometry;	// host geometry id
	bool32 skinned;
};

// DrawState::shader values for meshes (revc_rw::frame::MESH_NEO_*).
enum { MESH_NEO_VEHICLE = 10, MESH_NEO_WORLD = 11, MESH_NEO_GLOSS = 12, MESH_NEO_RIM = 13 };

// Instances the atomic's geometry with `pipe` (skinned if it is a skin pipeline); nil if there
// is nothing to draw.
InstanceDataHeader *instanceAtomic(ObjPipeline *pipe, Atomic *atomic);
// World matrix, lights (gl3 lightingCB) and, for skinned geometry, bone matrices.
void beginModel(Atomic *atomic, InstanceDataHeader *header, ModelState *model);
// Direction of light slot 0 as gl3's u_lightDirection[0] holds it: set by the last atomic whose
// first light has a direction, kept across unlit atomics (the Neo gloss shader reads it).
const float32 *lastLightDirection(void);
// gl3 setMaterial: colour and surface properties.
void setModelMaterial(ModelState *model, const RGBA &color, const SurfaceProperties &surfProps);
// gl3 setTexture on stage 0: raster, filter, addressing (nil: untextured).
void setTexture(Texture *tex);
// gl3device setTexture(0, tex): the device side of setTexture (wgpudevice.cpp).
void setTextureStage0(Texture *tex);
// gl3 evictRaster: unbinds a raster that is being destroyed.
void evictRaster(Raster *raster);
// Draws one mesh (with the PS2 alpha test emulation) using shader `shader` (0 = default
// lighting) and, if `useTex1`, `tex1` as second texture (nil = white).
void drawMesh(InstanceDataHeader *header, InstanceData *inst, const ModelState *model, uint32 shader,
	bool32 useTex1, Texture *tex1);
// The default/MatFX/skin pipeline's drawing of an instanced atomic (gl3 defaultRenderCB etc.).
void renderDefault(ObjPipeline *pipe, Atomic *atomic, InstanceDataHeader *header);

// Default atomic pipeline (wgpupipe.cpp) and freeing of its instance data with the geometry.
ObjPipeline *makeDefaultPipeline(void);
// MatFX atomics (cars, ...): the default pipeline for now (no environment map pass yet).
void initMatFX(void);
// Skinned atomics (peds): bone matrices like gl3 uploadSkinMatrices, skinned on the GPU.
void initSkin(void);
void *destroyNativeData(void *object, int32, int32);

extern int32 nativeRasterOffset;
#define GETWGPURASTEREXT(raster) PLUGINOFFSET(rw::wgpu::WgpuRaster, raster, rw::wgpu::nativeRasterOffset)

}
}

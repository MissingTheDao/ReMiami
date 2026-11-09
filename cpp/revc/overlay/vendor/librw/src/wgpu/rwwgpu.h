// WGPU platform (revc, plan task 12): librw objects as usual, drawing done by the Rust host on
// wgpu (rust/revc-rw) with a selectable Vulkan or OpenGL backend. The window comes from the
// host like the GL3 HOST gfxlib, but without a GL context; the device forwards frame work
// (clear, present, ...) to C functions the host implements (revc_rw_*).
#pragma once

namespace rw {

#ifdef RW_WGPU
namespace wgpu {
// Same layout and meaning as gl3::HostWindowApi (the host fills one table for either platform);
// createWindow makes a window without a GL context here and getProcAddress/swapBuffers are unused.
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
	                      int32 width, int32 height, int32 samples, int32 *gles, int32 *glversion);
	void (*destroyWindow)(void *user, void *window);
	void (*swapBuffers)(void *user, void *window, int32 vsync);
	void *(*getProcAddress)(void *user, const char *name);
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

extern Device renderdevice;

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
};

// Uploads the raster if it changed; returns its host texture (0 if it has no pixels).
uint32 uploadRaster(Raster *raster);

// Makes a (DONTALLOCATE) texture raster hold DXT blocks (D3D8 TXD textures keep their
// compression; the host samples them as BC or decodes them).
void allocateDXT(Raster *raster, int32 dxt, int32 numLevels, bool32 hasAlpha);
int32 dxtLevelSize(int32 dxt, int32 width, int32 height);

extern int32 nativeRasterOffset;
#define GETWGPURASTEREXT(raster) PLUGINOFFSET(rw::wgpu::WgpuRaster, raster, rw::wgpu::nativeRasterOffset)

}
}

// revc (docs/RENDERER_PORT.md, stage 2 step 7d2): librw's C++ methods are inline forwarders to
// the Rust port (rust/revc/src/rw*.rs); their .cpp files are gone. Included at the end of rw.h.
#pragma once
#include <assert.h>
#include <stdio.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwrender.h"
#include "rwengine.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwanim.h"
#include "rwplugins.h"
#include "rwcharset.h"
#include "wgpu/rwwgpu.h"
#include "rwscene_ffi.h"
#include "rwplugins_ffi.h"

// RWERROR without the including file's PLUGIN_ID
#define RWERROR_HDR(ecode) do{ rw::Error _e; _e.plugin = 0; _e.code = _ERRORCODE ecode; \
	fprintf(stderr, _ERRORFORMAT ecode); fputc('\n', stderr); rw::setError(&_e); }while(0)

// ---------------------------------------------------------------- Frame (rwframe.rs)
namespace rw {


// revc (docs/RENDERER_PORT.md, stage 2 step 3b): the hierarchy logic is in Rust
// (rust/revc/src/rwframe.rs): child links, roots, the dirty list and LTM syncing, checked
// against tests/golden/rwframe.txt; creation, cloning and the module too (steps 3c, 7d2).
extern "C" {
void revc_frame_add_child(Frame *f, Frame *child, int32 append, LinkList *dirty);
void revc_frame_remove_child(Frame *f, LinkList *dirty);
int32 revc_frame_count(Frame *f);
void revc_frame_sync_hierarchy_ltm(Frame *f);
Matrix *revc_frame_get_ltm(Frame *f);
void revc_frame_sync_dirty(LinkList *dirty);
void revc_frame_update_objects(Frame *f, LinkList *dirty);
void revc_frame_set_hierarchy_root(Frame *f, Frame *root);
void revc_frame_modify(Frame *f, int32 op, const V3d *v, float32 angle, const Matrix *m, int32 combine, LinkList *dirty);
void revc_frame_register_module(void);
Frame *revc_frame_create(void);
void revc_frame_destroy(Frame *f, LinkList *dirty);
void revc_frame_destroy_hierarchy(Frame *f);
Frame *revc_frame_clone_and_link(Frame *f, LinkList *dirty);
void revc_frame_purge_clone(Frame *f);
}

inline void Frame::registerModule(void) { revc_frame_register_module(); }

inline Frame*
Frame::create(void)
{
	return revc_frame_create();
}

inline Frame*
Frame::cloneHierarchy(void)
{
	Frame *frame = this->cloneAndLink();
	this->purgeClone();
	return frame;
}
inline void
Frame::destroy(void)
{
	revc_frame_destroy(this, &engine->frameDirtyList);
}

inline void
Frame::destroyHierarchy(void)
{
	revc_frame_destroy_hierarchy(this);
}

inline Frame*
Frame::addChild(Frame *child, bool32 append)
{
	revc_frame_add_child(this, child, append, &engine->frameDirtyList);
	return this;
}

inline Frame*
Frame::removeChild(void)
{
	revc_frame_remove_child(this, &engine->frameDirtyList);
	return this;
}

inline Frame*
Frame::forAllChildren(Callback cb, void *data)
{
	Frame *next;
	for(Frame *f = this->child; f; f = next){
		next = f->next;
		if(cb(f, data) == nil)
			return this;
	}
	return this;
}

inline int32
Frame::count(void)
{
	return revc_frame_count(this);
}

/*
 * Synching is a bit complicated. If anything in the hierarchy is not synched,
 * the root of the hierarchy is marked with the HIERARCHYSYNC flags.
 * Every unsynched frame is marked with the SUBTREESYNC flags.
 * If the LTM is not synched, the LTM flags are set.
 * If attached objects need synching, the OBJ flags are set.
 */

/* Synch just LTM matrices in a hierarchy */
/* Synch just objects in a hierarchy */
/* Synch LTM and objects */
/* Sync the LTMs of the hierarchy of which 'this' is the root */
inline void
Frame::syncHierarchyLTM(void)
{
	revc_frame_sync_hierarchy_ltm(this);
}

inline Matrix*
Frame::getLTM(void)
{
	return revc_frame_get_ltm(this);
}

/* Synch all dirty frames; LTMs and objects */
inline void
Frame::syncDirty(void)
{
	revc_frame_sync_dirty(&engine->frameDirtyList);
}

inline void
Frame::rotate(const V3d *axis, float32 angle, CombineOp op)
{
	revc_frame_modify(this, 0, axis, angle, nil, op, &engine->frameDirtyList);
}

inline void
Frame::translate(const V3d *trans, CombineOp op)
{
	revc_frame_modify(this, 1, trans, 0.0f, nil, op, &engine->frameDirtyList);
}

inline void
Frame::scale(const V3d *scl, CombineOp op)
{
	revc_frame_modify(this, 2, scl, 0.0f, nil, op, &engine->frameDirtyList);
}

inline void
Frame::transform(const Matrix *mat, CombineOp op)
{
	revc_frame_modify(this, 3, nil, 0.0f, mat, op, &engine->frameDirtyList);
}

inline void
Frame::updateObjects(void)
{
	revc_frame_update_objects(this, &engine->frameDirtyList);
}

inline void
Frame::setHierarchyRoot(Frame *root)
{
	revc_frame_set_hierarchy_root(this, root);
}

// Clone a frame hierarchy. Link cloned frames into Frame::root of the originals.
inline Frame*
Frame::cloneAndLink(void)
{
	return revc_frame_clone_and_link(this, &engine->frameDirtyList);
}

// Remove links to cloned frames from hierarchy.
inline void
Frame::purgeClone(void)
{
	revc_frame_purge_clone(this);
}


}

// ---------------------------------------------------------------- Light, World, Clump, Atomic, Camera (rwscene.rs, rwcamera.rs)
namespace rw {



static_assert(sizeof(Light) == 120 && offsetof(Light, inWorld) == 64 && offsetof(Light, world) == 104, "Light layout (rwscene.rs)");

inline Light*
Light::create(int32 type)
{
	Light *l = revc_light_create(type);
	if(l == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return l;
}

inline void
Light::destroy(void)
{
	revc_light_destroy(this);
}

inline void
Light::setAngle(float32 angle)
{
	revc_light_set_angle(this, angle);
}

inline float32
Light::getAngle(void)
{
	return revc_light_get_angle(this);
}

inline void
Light::setColor(float32 r, float32 g, float32 b)
{
	revc_light_set_color(this, r, g, b);
}


}

namespace rw {



static_assert(sizeof(World) == 64, "World layout (rwscene.rs)");

inline World*
World::create(BBox *bbox)
{
	World *w = revc_world_create();
	if(w == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return w;
}

inline void
World::destroy(void)
{
	revc_world_destroy(this);
}

inline void
World::addLight(Light *light)
{
	revc_world_add_light(this, light);
}

inline void
World::removeLight(Light *light)
{
	revc_world_remove_light(this, light);
}

inline void
World::addCamera(Camera *cam)
{
	revc_world_add_camera(this, cam);
}

inline void
World::removeCamera(Camera *cam)
{
	revc_world_remove_camera(this, cam);
}

inline void
World::addAtomic(Atomic *atomic)
{
	revc_world_add_atomic(this, atomic);
}

inline void
World::removeAtomic(Atomic *atomic)
{
	revc_world_remove_atomic(this, atomic);
}

inline void
World::addClump(Clump *clump)
{
	revc_world_add_clump(this, clump);
}

inline void
World::removeClump(Clump *clump)
{
	revc_world_remove_clump(this, clump);
}

inline void
World::render(void)
{
	revc_world_render(this);
}

// Find lights that illuminate an atomic
inline void
World::enumerateLights(Atomic *atomic, WorldLights *lightData)
{
	revc_world_enumerate_lights(this, atomic, lightData);
}

}

namespace rw {



static_assert(sizeof(Atomic) == 136 && offsetof(Atomic, world) == 120 && sizeof(Clump) == 88 && sizeof(WorldLights) == 48,
              "scene layouts (rwscene.rs)");

//
// Clump
//

inline Clump*
Clump::create(void)
{
	Clump *c = revc_clump_create();
	if(c == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return c;
}

inline Clump*
Clump::clone(void)
{
	return revc_clump_clone(this);
}

inline void
Clump::destroy(void)
{
	revc_clump_destroy(this);
}

inline void
Clump::addAtomic(Atomic *a)
{
	revc_clump_add_atomic(this, a);
}

inline void
Clump::removeAtomic(Atomic *a)
{
	revc_clump_remove_atomic(this, a);
}

inline void
Clump::addLight(Light *l)
{
	revc_clump_add_light(this, l);
}

inline void
Clump::removeLight(Light *l)
{
	revc_clump_remove_light(this, l);
}

inline void
Clump::addCamera(Camera *c)
{
	revc_clump_add_camera(this, c);
}

inline void
Clump::removeCamera(Camera *c)
{
	revc_clump_remove_camera(this, c);
}


inline void
Clump::render(void)
{
	revc_clump_render(this);
}

//
// Atomic
//


inline Atomic*
Atomic::create(void)
{
	Atomic *a = revc_atomic_create();
	if(a == nil){
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
		return nil;
	}
	a->renderCB = Atomic::defaultRenderCB;
	return a;
}

inline Atomic*
Atomic::clone()
{
	return revc_atomic_clone(this);
}

inline void
Atomic::destroy(void)
{
	revc_atomic_destroy(this);
}

inline void
Atomic::setGeometry(Geometry *geo, uint32 flags)
{
	revc_atomic_set_geometry(this, geo, flags);
}

inline Sphere*
Atomic::getWorldBoundingSphere(void)
{
	return revc_atomic_get_world_bounding_sphere(this);
}

inline uint32 atomicRights[2];	// revc: also set by rw/DffRead.cpp (not static)

inline ObjPipeline*
Atomic::getPipeline(void)
{
	return this->pipeline ?
		this->pipeline :
		engine->driver[platform]->defaultPipeline;
}

inline void
Atomic::instance(void)
{
	if(this->geometry->flags & Geometry::NATIVE)
		return;
	this->getPipeline()->instance(this);
	this->geometry->flags |= Geometry::NATIVE;
}

inline void
Atomic::uninstance(void)
{
	if(!(this->geometry->flags & Geometry::NATIVE))
		return;
	this->getPipeline()->uninstance(this);
	// this should be done by the CB already, just make sure
	this->geometry->flags &= ~Geometry::NATIVE;
}

inline void
Atomic::defaultRenderCB(Atomic *atomic)
{
	atomic->getPipeline()->render(atomic);
}

// Atomic Rights plugin


inline void
registerAtomicRightsPlugin(void)
{
	Atomic::registerPlugin(0, ID_RIGHTTORENDER, nil, nil, nil);
	// revc: read in Rust (rw/DffRead.cpp); models aren't written
}

}

namespace rw {



// revc (docs/RENDERER_PORT.md, stage 2 step 4a): the camera math is in Rust
// (rust/revc/src/rwcamera.rs): view matrix, frustum corners/planes/bounding box, the sphere test,
// the depth-range constants and setFOV's view window, checked against tests/golden/rwcamera.txt.
extern "C" {
// viewWindow points at viewWindow, viewOffset, nearPlane, farPlane, fogPlane, projection
void revc_camera_sync(const Matrix *ltm, const V2d *viewWindow, Matrix *view,
                      FrustumPlane *planes, V3d *corners, BBox *bound);
int32 revc_camera_frustum_test_sphere(const FrustumPlane *planes, const Sphere *s);
void revc_camera_z_shift_scale(float32 n, float32 f, int32 projection, float32 zNear, float32 zFar, float32 *out);
void revc_camera_fov_window(float32 hfov, float32 ratio, int32 width, int32 height, float32 *out);
}
static_assert(sizeof(FrustumPlane) == 20, "FrustumPlane layout (rwcamera.rs)");
static_assert(offsetof(Camera, projection) - offsetof(Camera, viewWindow) == 28, "Camera params layout (rwcamera.rs)");

// the camera callbacks and the scene hooks are Rust defaults (rwscene.rs, stage 2 step 7d2)
static_assert(sizeof(Camera) == 600 && offsetof(Camera, viewWindow) == 56 && offsetof(Camera, projection) == 84 &&
              offsetof(Camera, viewMatrix) == 88 && offsetof(Camera, frameBuffer) == 400 && offsetof(Camera, clump) == 544 &&
              offsetof(Camera, inClump) == 552 && offsetof(Camera, world) == 568 && offsetof(Camera, originalEndUpdate) == 592,
              "Camera layout (rwscene.rs)");

inline Camera*
Camera::create(void)
{
	Camera *cam = revc_camera_create();
	if(cam == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return cam;
}

inline Camera*
Camera::clone(void)
{
	return revc_camera_clone(this);
}

inline void
Camera::destroy(void)
{
	revc_camera_destroy(this);
}

inline void
Camera::clear(RGBA *col, uint32 mode)
{
	engine->device.clearCamera(this, col, mode);
}

inline void
Camera::showRaster(uint32 flags)
{
	this->frameBuffer->show(flags);
}

inline void
calczShiftScale(Camera *cam)
{
	float32 out[2];
	revc_camera_z_shift_scale(cam->nearPlane, cam->farPlane, cam->projection,
	                          engine->device.zNear, engine->device.zFar, out);
	cam->zScale = out[0];
	cam->zShift = out[1];
}

inline void
Camera::setNearPlane(float32 near)
{
	this->nearPlane = near;
	calczShiftScale(this);
	if(this->getFrame())
		this->getFrame()->updateObjects();
}

inline void
Camera::setFarPlane(float32 far)
{
	this->farPlane = far;
	calczShiftScale(this);
	if(this->getFrame())
		this->getFrame()->updateObjects();
}

inline void
Camera::setViewWindow(const V2d *window)
{
	this->viewWindow = *window;
	if(this->getFrame())
		this->getFrame()->updateObjects();
}

inline void
Camera::setViewOffset(const V2d *offset)
{
	this->viewOffset = *offset;
	if(this->getFrame())
		this->getFrame()->updateObjects();
}

inline void
Camera::setProjection(int32 proj)
{
	this->projection = proj;
	if(this->getFrame())
		this->getFrame()->updateObjects();
}

inline int32
Camera::frustumTestSphere(const Sphere *s) const
{
	return revc_camera_frustum_test_sphere(this->frustumPlanes, s);
}


// Assumes horizontal FOV for 4:3, but we convert to vertical FOV
inline void
Camera::setFOV(float32 hfov, float32 ratio)
{
	V2d v;
	revc_camera_fov_window(hfov, ratio, this->frameBuffer->width, this->frameBuffer->height, &v.x);
	this->setViewWindow(&v);
	v.set(0.0f, 0.0f);
	this->setViewOffset(&v);
}

}

// ---------------------------------------------------------------- image files (rwimagefile.rs)
extern "C" {
rw::Image *revc_read_png(const char *filename);
rw::int32 revc_write_png(rw::Image *image, const char *filename);
rw::Image *revc_read_bmp(const char *filename);
rw::int32 revc_write_bmp(rw::Image *image, const char *filename);
rw::Image *revc_read_tga(const char *filename);
rw::int32 revc_write_tga(rw::Image *image, const char *filename);
}
namespace rw {
inline Image *readPNG(const char *filename) { return revc_read_png(filename); }
inline void writePNG(Image *image, const char *filename) { if(!revc_write_png(image, filename)) RWERROR_HDR((ERR_FILE, filename)); }
inline Image *readBMP(const char *filename) { return revc_read_bmp(filename); }
inline void writeBMP(Image *image, const char *filename) { if(!revc_write_bmp(image, filename)) RWERROR_HDR((ERR_FILE, filename)); }
inline Image *readTGA(const char *filename) { return revc_read_tga(filename); }
inline void writeTGA(Image *image, const char *filename) { if(!revc_write_tga(image, filename)) RWERROR_HDR((ERR_FILE, filename)); }
}

// ---------------------------------------------------------------- ObjPipeline
namespace rw {

inline void revcNothingCB(ObjPipeline *, Atomic*) {}

inline void
ObjPipeline::init(uint32 platform)
{
	Pipeline::init(platform);
	this->impl.instance = revcNothingCB;
	this->impl.uninstance = revcNothingCB;
	this->impl.render = revcNothingCB;
}

inline ObjPipeline*
ObjPipeline::create(void)
{
	ObjPipeline *pipe = rwNewT(ObjPipeline, 1, MEMDUR_GLOBAL);
	pipe->init(PLATFORM_NULL);
	return pipe;
}

inline void
ObjPipeline::destroy(void)
{
	rwFree(this);
}

}

// ---------------------------------------------------------------- render states, Im2D, Im3D (the device, rwdevice.rs)
namespace rw {

inline void SetRenderState(int32 state, uint32 value){
	engine->device.setRenderState(state, (void*)(uintptr)value); }

inline void SetRenderStatePtr(int32 state, void *value){
	engine->device.setRenderState(state, value); }

inline uint32 GetRenderState(int32 state){
	return (uint32)(uintptr)engine->device.getRenderState(state); }

inline void *GetRenderStatePtr(int32 state){
	return engine->device.getRenderState(state); }

// Im2D

namespace im2d {

inline float32 GetNearZ(void) { return engine->device.zNear; }
inline float32 GetFarZ(void) { return engine->device.zFar; }
inline void
RenderLine(void *verts, int32 numVerts, int32 vert1, int32 vert2)
{
	engine->device.im2DRenderLine(verts, numVerts, vert1, vert2);
}
inline void
RenderTriangle(void *verts, int32 numVerts, int32 vert1, int32 vert2, int32 vert3)
{
	engine->device.im2DRenderTriangle(verts, numVerts, vert1, vert2, vert3);
}
inline void
RenderIndexedPrimitive(PrimitiveType type, void *verts, int32 numVerts, void *indices, int32 numIndices)
{
	engine->device.im2DRenderIndexedPrimitive(type, verts, numVerts, indices, numIndices);
}
inline void
RenderPrimitive(PrimitiveType type, void *verts, int32 numVerts)
{
	engine->device.im2DRenderPrimitive(type, verts, numVerts);
}

}

// Im3D

namespace im3d {

inline void
Transform(void *vertices, int32 numVertices, Matrix *world, uint32 flags)
{
	engine->device.im3DTransform(vertices, numVertices, world, flags);
}
inline void
RenderLine(int32 vert1, int32 vert2)
{
	int16 indices[2];
	indices[0] = vert1;
	indices[1] = vert2;
	RenderIndexedPrimitive(rw::PRIMTYPELINELIST, indices, 2);
}
inline void
RenderTriangle(int32 vert1, int32 vert2, int32 vert3)
{
	int16 indices[3];
	indices[0] = vert1;
	indices[1] = vert2;
	indices[2] = vert3;
	RenderIndexedPrimitive(rw::PRIMTYPETRILIST, indices, 3);
}
inline void
RenderPrimitive(PrimitiveType primType)
{
	engine->device.im3DRenderPrimitive(primType);
}
inline void
RenderIndexedPrimitive(PrimitiveType primType, void *indices, int32 numIndices)
{
	engine->device.im3DRenderIndexedPrimitive(primType, indices, numIndices);
}
inline void
End(void)
{
	engine->device.im3DEnd();
}

}

}

// ---------------------------------------------------------------- plugin lists (rwplugin.rs)
// Plugin lists live in Rust (rust/revc/src/rwplugin.rs), checked against
// tests/golden/rwplugin.txt recorded from the original plg.cpp.


namespace rw {

static_assert(sizeof(PluginList) == 24, "PluginList layout");
static_assert(offsetof(PluginList, plugins) == 8, "PluginList layout");
static_assert(sizeof(Plugin) == 120, "Plugin layout");
static_assert(offsetof(Plugin, constructor) == 16, "Plugin layout");
static_assert(offsetof(Plugin, parentList) == 80, "Plugin layout");
static_assert(offsetof(Plugin, inParentList) == 88, "Plugin layout");
static_assert(offsetof(Plugin, inGlobalList) == 104, "Plugin layout");

extern "C" {
void revc_pl_open(void);
void revc_pl_close(void);
void revc_pl_construct(PluginList *l, void *o);
void revc_pl_destruct(PluginList *l, void *o);
void revc_pl_copy(PluginList *l, void *d, void *s);
int32 revc_pl_stream_read(PluginList *l, Stream *s, void *o);
void revc_pl_stream_write(PluginList *l, Stream *s, void *o);
int32 revc_pl_stream_get_size(PluginList *l, void *o);
void revc_pl_stream_skip(Stream *s);
void revc_pl_assert_rights(PluginList *l, void *o, uint32 id, uint32 data);
int32 revc_pl_register(PluginList *l, int32 size, uint32 id, Constructor c, Destructor d, CopyConstructor y);
int32 revc_pl_register_stream(PluginList *l, uint32 id, StreamRead r, StreamWrite w, StreamGetSize g);
int32 revc_pl_set_rights(PluginList *l, uint32 id, RightsCallback cb);
int32 revc_pl_set_always(PluginList *l, uint32 id, AlwaysCallback cb);
int32 revc_pl_get_offset(PluginList *l, uint32 id);
}

inline void PluginList::open(void) { revc_pl_open(); }
inline void PluginList::close(void) { revc_pl_close(); }
inline void PluginList::construct(void *o) { revc_pl_construct(this, o); }
inline void PluginList::destruct(void *o) { revc_pl_destruct(this, o); }
inline void PluginList::copy(void *dst, void *src) { revc_pl_copy(this, dst, src); }
inline bool PluginList::streamRead(Stream *s, void *o) { return revc_pl_stream_read(this, s, o) != 0; }
inline void PluginList::streamWrite(Stream *s, void *o) { revc_pl_stream_write(this, s, o); }
inline int PluginList::streamGetSize(void *o) { return revc_pl_stream_get_size(this, o); }
inline void PluginList::streamSkip(Stream *s) { revc_pl_stream_skip(s); }
inline void PluginList::assertRights(void *o, uint32 id, uint32 data) { revc_pl_assert_rights(this, o, id, data); }

inline int32
PluginList::registerPlugin(int32 size, uint32 id, Constructor ctor, Destructor dtor, CopyConstructor copy)
{
	return revc_pl_register(this, size, id, ctor, dtor, copy);
}

inline int32
PluginList::registerStream(uint32 id, StreamRead read, StreamWrite write, StreamGetSize getSize)
{
	return revc_pl_register_stream(this, id, read, write, getSize);
}

inline int32 PluginList::setStreamRightsCallback(uint32 id, RightsCallback cb) { return revc_pl_set_rights(this, id, cb); }
inline int32 PluginList::setStreamAlwaysCallback(uint32 id, AlwaysCallback cb) { return revc_pl_set_always(this, id, cb); }
inline int32 PluginList::getPluginOffset(uint32 id) { return revc_pl_get_offset(this, id); }

}

// ---------------------------------------------------------------- animations (rwhanim.rs)
namespace rw {

//
// AnimInterpolatorInfo
//

// revc (stage 2 step 7d1): the interpolator registry and Animation's memory are in Rust
// (rust/revc/src/rwhanim.rs).
extern "C" {
void revc_interp_register(AnimInterpolatorInfo *info);
void revc_interp_unregister(AnimInterpolatorInfo *info);
AnimInterpolatorInfo *revc_interp_find(int32 id);
Animation *revc_animation_create(AnimInterpolatorInfo *info, int32 numFrames, int32 flags, float32 duration);
void revc_animation_destroy(Animation *anim);
}
static_assert(sizeof(Animation) == 40 && sizeof(AnimInterpolatorInfo) == 80, "animation layouts (rwhanim.rs)");

inline void AnimInterpolatorInfo::registerInterp(AnimInterpolatorInfo *info) { revc_interp_register(info); }
inline void AnimInterpolatorInfo::unregisterInterp(AnimInterpolatorInfo *info) { revc_interp_unregister(info); }
inline AnimInterpolatorInfo *AnimInterpolatorInfo::find(int32 id) { return revc_interp_find(id); }

inline Animation*
Animation::create(AnimInterpolatorInfo *interpInfo, int32 numFrames, int32 flags, float duration)
{
	return revc_animation_create(interpInfo, numFrames, flags, duration);
}

inline void Animation::destroy(void) { revc_animation_destroy(this); }

//
// AnimInterpolator
//

// revc (docs/RENDERER_PORT.md, stage 2 step 4b): setCurrentAnim and addTime are in Rust
// (rust/revc/src/rwhanim.rs), checked against tests/golden/rwhanim.txt.
extern "C" {
int32 revc_anim_set_current_anim(AnimInterpolator *it, Animation *anim);
void revc_anim_add_time(AnimInterpolator *it, float32 t);
}
static_assert(sizeof(AnimInterpolator) == 72 && sizeof(Animation) == 40 && sizeof(KeyFrameHeader) == 16,
              "anim layouts (rwhanim.rs)");

inline AnimInterpolator*
AnimInterpolator::create(int32 numNodes, int32 maxFrameSize)
{
	AnimInterpolator *interp = revc_interp_create(numNodes, maxFrameSize);
	if(interp == nil)
		RWERROR_HDR((ERR_ALLOC, (int32)sizeof(AnimInterpolator)));
	return interp;
}

inline void
AnimInterpolator::destroy(void)
{
	revc_interp_destroy(this);
}

inline bool32
AnimInterpolator::setCurrentAnim(Animation *anim)
{
	if(!revc_anim_set_current_anim(this, anim)){
		RWERROR_HDR((ERR_GENERAL, "interpolation frame too big"));
		return 0;
	}
	return 1;
}

inline void
AnimInterpolator::addTime(float32 t)
{
	revc_anim_add_time(this, t);
}

}

// ---------------------------------------------------------------- Geometry, Material, meshes (rwgeomobj.rs)
namespace rw {



// revc (docs/RENDERER_PORT.md, stage 2 step 6a): geometries, mesh headers, material lists and
// materials are made and kept in Rust (rust/revc/src/rwgeomobj.rs), checked against
// tests/golden/rwgeomobj.txt; librw's allocator and the plugin lists through Rust's default hooks.
extern "C" {
Geometry *revc_geo_create(int32 nv, int32 nt, uint32 flags);
void revc_geo_destroy(Geometry *g);
void revc_geo_lock(Geometry *g, int32 flags);
void revc_geo_unlock(Geometry *g);
void revc_geo_add_morph_targets(Geometry *g, int32 n);
void revc_geo_calculate_bounding_sphere(Geometry *g);
int32 revc_geo_has_colored_material(const Geometry *g);
void revc_geo_allocate_data(Geometry *g);
void revc_geo_build_meshes_obj(Geometry *g);
void revc_geo_build_tristrips(Geometry *g);
void revc_matlist_init(MaterialList *ml);
void revc_matlist_deinit(MaterialList *ml);
int32 revc_matlist_append(MaterialList *ml, Material *m);
int32 revc_matlist_find(const MaterialList *ml, Material *m);
Material *revc_mat_create(void);
Material *revc_mat_clone(Material *m);
void revc_mat_destroy(Material *m);
void revc_mat_set_texture(Material *m, Texture *t);
void revc_geo_bounding_sphere(const V3d *verts, int32 n, Sphere *out);
}
static_assert(sizeof(Geometry) == 168 && sizeof(Material) == 40 && sizeof(MorphTarget) == 40 &&
              sizeof(MeshHeader) == 16 && sizeof(Mesh) == 24 && sizeof(Triangle) == 8, "geometry layouts (rwgeomobj.rs)");

// the hooks are Rust defaults (rwgeomobj.rs, stage 2 step 7d2)

// We allocate twice because we have to allocate the data separately for uninstancing
inline Geometry*
Geometry::create(int32 numVerts, int32 numTris, uint32 flags)
{
	Geometry *geo = revc_geo_create(numVerts, numTris, flags);
	if(geo == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return geo;
}

inline void
Geometry::destroy(void)
{
	revc_geo_destroy(this);
}


inline void
Geometry::lock(int32 lockFlags)
{
	revc_geo_lock(this, lockFlags);
}

inline void
Geometry::unlock(void)
{
	revc_geo_unlock(this);
}


inline void
Geometry::addMorphTargets(int32 n)
{
	revc_geo_add_morph_targets(this, n);
}

inline void
Geometry::calculateBoundingSphere(void)
{
	revc_geo_calculate_bounding_sphere(this);
}

inline bool32
Geometry::hasColoredMaterial(void)
{
	return revc_geo_has_colored_material(this);
}

// Force allocate data, even when native flag is set
inline void
Geometry::allocateData(void)
{
	revc_geo_allocate_data(this);
}

// This functions assumes there is enough space allocated
// for triangles. Use MeshHeader::guessNumTriangles() and
// Geometry::allocateData()
inline void
Geometry::buildMeshes(void)
{
	revc_geo_build_meshes_obj(this);
}

inline void
Geometry::buildTristrips(void)
{
	revc_geo_build_tristrips(this);
}

/* The idea is that even in meshes where winding is not preserved
 * every tristrip starts at an even vertex. So find the start of
 * strips and insert duplicate vertices if necessary. */
inline Sphere
MorphTarget::calculateBoundingSphere(void) const
{
	Sphere sphere;
	revc_geo_bounding_sphere(this->vertices, this->parent->numVertices, &sphere);
	return sphere;
}


//
// MaterialList
//
#undef PLUGIN_ID

inline void
MaterialList::init(void)
{
	revc_matlist_init(this);
}

inline void
MaterialList::deinit(void)
{
	revc_matlist_deinit(this);
}

inline int32
MaterialList::appendMaterial(Material *mat)
{
	return revc_matlist_append(this, mat);
}

inline int32
MaterialList::findIndex(Material *mat)
{
	return revc_matlist_find(this, mat);
}

//
// Material
//

inline Material*
Material::create(void)
{
	Material *mat = revc_mat_create();
	if(mat == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return mat;
}

inline Material*
Material::clone(void)
{
	Material *mat = revc_mat_clone(this);
	if(mat == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return mat;
}

inline void
Material::destroy(void)
{
	revc_mat_destroy(this);
}

inline void
Material::setTexture(Texture *tex)
{
	revc_mat_set_texture(this, tex);
}


inline uint32 materialRights[2];	// revc: also set by rw/DffRead.cpp (not static)

// Material Rights plugin


inline void
registerMaterialRightsPlugin(void)
{
	Material::registerPlugin(0, ID_RIGHTTORENDER, nil, nil, nil);
	// revc: read in Rust (rw/DffRead.cpp); models aren't written
}


}
namespace rw {

// revc (stage 2 step 6a): the mesh header functions are in Rust (rust/revc/src/rwgeomobj.rs)
extern "C" {
MeshHeader *revc_geo_allocate_meshes(Geometry *g, int32 n, uint32 numIndices, int32 noIndices);
void revc_mesh_setup_indices(MeshHeader *mh);
uint32 revc_mesh_guess_num_triangles(const MeshHeader *mh);
}

// Mesh

// Allocate a mesh header, meshes and optionally indices.
// If existing meshes already exist, retain their information.
inline MeshHeader*
Geometry::allocateMeshes(int32 numMeshes, uint32 numIndices, bool32 noIndices)
{
	return revc_geo_allocate_meshes(this, numMeshes, numIndices, noIndices);
}

inline void
MeshHeader::setupIndices(void)
{
	revc_mesh_setup_indices(this);
}


inline void
registerMeshPlugin(void)
{
	Geometry::registerPlugin(0, ID_MESH, nil, nil, nil);
	// revc: read in Rust (rw/DffRead.cpp); models aren't written
}

// Returns the maximum number of triangles. Just so
// we can allocate enough before instancing. This does not
// take into account degerate triangles or ADC bits as
// we don't look at the data.
inline uint32
MeshHeader::guessNumTriangles(void)
{
	return revc_mesh_guess_num_triangles(this);
}

// Native Data

// revc: only WGPU instance data exists at run time. Native geometry of other platforms (the
// console releases' DFFs) isn't read here: PC DFFs have none, and the console data is to be
// converted by revc's importer (Rust), not loaded through librw.
inline void*
revcDestroyNativeData(void *object, int32 offset, int32 size)
{
	Geometry *geometry = (Geometry*)object;
	if(geometry->instData == nil)
		return object;
	if(geometry->instData->platform == PLATFORM_WGPU)
		return wgpu::destroyNativeData(object, offset, size);
	return object;
}


inline void
registerNativeDataPlugin(void)
{
	Geometry::registerPlugin(0, ID_NATIVEDATA,
	                         nil, revcDestroyNativeData, nil);
}

}

// ---------------------------------------------------------------- Image (rwimage.rs, rwimagefile.rs)
namespace rw {


// revc (docs/RENDERER_PORT.md, stage 2 step 5b): the pixel functions are in Rust
// (rust/revc/src/rwimage.rs): DXT decompression, hasAlpha, convertTo32, (un)palettize with the
// octree quantizer, the masks; checked against tests/golden/rwimage.txt. Image objects and their
// memory stay here (hooks). librw's DXT flipping, compressPalette, extractMask, setPalette and
// printSearchPath had no callers and are gone.
extern "C" {
Image *revc_image_create(int32 w, int32 h, int32 depth);
void revc_image_destroy(Image *img);
void revc_image_allocate(Image *img);
void revc_image_free(Image *img);
void revc_image_register_module(void);
void revc_image_set_pixels_dxt(Image *img, int32 type, const uint8 *src);
int32 revc_image_has_alpha(const Image *img);
void revc_image_unpalettize(Image *img, int32 forceAlpha);
void revc_image_convert_to32(Image *img);
void revc_image_palettize(Image *img, int32 depth);
void revc_image_make_mask(Image *img);
void revc_image_apply_mask(Image *img, const Image *mask);
void revc_image_remove_mask(Image *img);
}
static_assert(sizeof(Image) == 40, "Image layout (rwimage.rs)");

// Image formats are as follows:
//  32 bit has 4 bytes: 8888 RGBA
//  24 bit has 3 bytes: 888 RGB
//  16 bit has 2 bytes: 1555 ARGB stored in platform native order (TODO?)
// palettes always have 4 bytes: r, g, b, a
//   8 bit has 1 byte: x
//   4 bit has 1 byte per two pixels: 0xLR, where L and R are the left and right pixel resp.

inline Image *Image::create(int32 width, int32 height, int32 depth) { return revc_image_create(width, height, depth); }
inline void Image::destroy(void) { revc_image_destroy(this); }
inline void Image::allocate(void) { revc_image_allocate(this); }
inline void Image::free(void) { revc_image_free(this); }

inline void
Image::setPixels(uint8 *pixels)
{
	this->pixels = pixels;
	this->flags |= 1;
}

inline void
Image::setPixelsDXT(int32 type, uint8 *pixels)
{
	revc_image_set_pixels_dxt(this, type, pixels);
}

inline bool32
Image::hasAlpha(void)
{
	return revc_image_has_alpha(this);
}

inline void
Image::convertTo32(void)
{
	assert(this->pixels);
	revc_image_convert_to32(this);
}

inline void
Image::palettize(int32 depth)
{
	assert(depth <= 8);
	revc_image_palettize(this, depth);
}

inline void
Image::unpalettize(bool forceAlpha)
{
	revc_image_unpalettize(this, forceAlpha);
}

// Copy the biggest channel value to alpha
inline void
Image::makeMask(void)
{
	revc_image_make_mask(this);
}

inline void
Image::applyMask(Image *mask)
{
	revc_image_apply_mask(this, mask);
}

inline void
Image::removeMask(void)
{
	revc_image_remove_mask(this);
}

// revc (stage 2 step 5b): the search path and the file lookup (with makePath's case
// correction) are in Rust (rust/revc/src/rwimagefile.rs)
extern "C" {
void revc_image_set_search_path(const char *path);
int32 revc_image_get_filename(const char *name, char *out, size_t size);
}

inline void
Image::setSearchPath(const char *path)
{
	revc_image_set_search_path(path);
}

inline char*
Image::getFilename(const char *name)
{
	char buf[4096];
	if(!revc_image_get_filename(name, buf, sizeof(buf)))
		return nil;
	return rwStrdup(buf, MEMDUR_EVENT);
}

// revc (stage 2 step 7d1): the file format registry, read and readMasked are in Rust
// (rust/revc/src/rwimagefile.rs), checked against tests/golden/rwimageread.txt.
extern "C" {
int32 revc_image_register_file_format(const char *ext, Image::fileRead read, Image::fileWrite write);
void revc_image_clear_file_formats(void);
Image *revc_image_read(const char *name);
Image *revc_image_read_masked(const char *name, const char *mask);
}

inline Image *Image::readMasked(const char *imageName, const char *maskName) { return revc_image_read_masked(imageName, maskName); }
inline Image *Image::read(const char *imageName) { return revc_image_read(imageName); }
inline bool32 Image::registerFileFormat(const char *ext, fileRead read, fileWrite write) { return revc_image_register_file_format(ext, read, write); }

inline void Image::registerModule(void) { revc_image_register_module(); }



}

// ---------------------------------------------------------------- Raster and the wgpu driver's rasters (rwraster.rs)
namespace rw {


struct RasterGlobals
{
	int32 sp;
	Raster *stack[32];
};


// revc (docs/RENDERER_PORT.md, stage 2 step 5c): rasters are made and kept in Rust
// (rust/revc/src/rwraster.rs), with the wgpu driver's raster code; checked against
// tests/golden/rwraster.txt. The driver dispatch is inline here; the module is Rust too (step 7d2). The pixel converters, the 4-bit
// palette packers and convertTexToCurrentPlatform had no callers left and are gone.
extern "C" {
Raster *revc_raster_create(int32 w, int32 h, int32 depth, int32 format, int32 platform);
void revc_raster_destroy(Raster *r);
void revc_raster_sub(Raster *r, Raster *parent, const Rect *rect);
Raster *revc_raster_push_context(RasterGlobals *g, Raster *r);
Raster *revc_raster_pop_context(RasterGlobals *g);
Raster *revc_raster_current_context(const RasterGlobals *g);
int32 revc_raster_calculate_num_levels(int32 w, int32 h);
int32 revc_raster_format_has_alpha(int32 format);
RasterGlobals *revc_raster_globals(void);
void revc_raster_register_module(void);
}
static_assert(sizeof(Raster) == 96 && sizeof(RasterGlobals) == 264, "raster layouts (rwraster.rs)");

inline void Raster::registerModule(void) { revc_raster_register_module(); }

inline Raster*
Raster::create(int32 width, int32 height, int32 depth, int32 format, int32 platform)
{
	Raster *raster = revc_raster_create(width, height, depth, format, platform ? platform : rw::platform);
	return engine->driver[raster->platform]->rasterCreate(raster);
}

inline void
Raster::subRaster(Raster *parent, Rect *r)
{
	revc_raster_sub(this, parent, r);
}

inline void
Raster::destroy(void)
{
	revc_raster_destroy(this);
}

inline uint8*
Raster::lock(int32 level, int32 lockMode)
{
	return engine->driver[this->platform]->rasterLock(this, level, lockMode);
}

inline void
Raster::unlock(int32 level)
{
	engine->driver[this->platform]->rasterUnlock(this, level);
}

inline uint8*
Raster::lockPalette(int32 lockMode)
{
	return engine->driver[this->platform]->rasterLockPalette(this, lockMode);
}

inline void
Raster::unlockPalette(void)
{
	engine->driver[this->platform]->rasterUnlockPalette(this);
}

inline int32
Raster::getNumLevels(void)
{
	return engine->driver[this->platform]->rasterNumLevels(this);
}

inline int32
Raster::calculateNumLevels(int32 width, int32 height)
{
	return revc_raster_calculate_num_levels(width, height);
}

inline bool
Raster::formatHasAlpha(int32 format)
{
	return revc_raster_format_has_alpha(format);
}

inline bool32
Raster::imageFindRasterFormat(Image *image, int32 type,
	int32 *pWidth, int32 *pHeight, int32 *pDepth, int32 *pFormat,
	int32 platform)
{
	return engine->driver[platform ? platform : rw::platform]->imageFindRasterFormat(
		image, type, pWidth, pHeight, pDepth, pFormat);
}

inline Raster*
Raster::setFromImage(Image *image, int32 platform)
{
	if(engine->driver[platform ? platform : rw::platform]->rasterFromImage(this, image))
		return this;
	return nil;
}

inline Raster*
Raster::createFromImage(Image *image, int32 platform)
{
	Raster *raster;
	int32 width, height, depth, format;
	if(!imageFindRasterFormat(image, TEXTURE, &width, &height, &depth, &format, platform))
		return nil;
	raster = Raster::create(width, height, depth, format, platform);
	if(raster == nil)
		return nil;
	return raster->setFromImage(image, platform);
}

inline Image*
Raster::toImage(void)
{
	return engine->driver[this->platform]->rasterToImage(this);
}

inline void
Raster::show(uint32 flags)
{
	engine->device.showRaster(this, flags);
}

inline Raster*
Raster::pushContext(Raster *raster)
{
	return revc_raster_push_context(revc_raster_globals(), raster);
}

inline Raster*
Raster::popContext(void)
{
	return revc_raster_pop_context(revc_raster_globals());
}

inline Raster*
Raster::getCurrentContext(void)
{
	return revc_raster_current_context(revc_raster_globals());
}

inline bool32
Raster::renderFast(int32 x, int32 y)
{
	return engine->device.rasterRenderFast(this,x, y);
}


}
// WGPU platform rasters: RGBA8 in memory for every mip level (see rwwgpu.h).




// revc (docs/RENDERER_PORT.md, stage 2 step 5c): the raster code is in Rust
// (rust/revc/src/rwraster.rs), checked against tests/golden/rwraster.txt.
namespace rw { namespace wgpu { struct WgpuRaster; } }
extern "C" {
void revc_wgpu_register_native_raster(void);
rw::Raster *revc_wgpu_raster_create(rw::Raster *r);
uint8_t *revc_wgpu_raster_lock(rw::Raster *r, int32_t level, int32_t mode);
void revc_wgpu_raster_unlock(rw::Raster *r);
int32_t revc_wgpu_raster_from_image(rw::Raster *r, rw::Image *img);
rw::Image *revc_wgpu_raster_to_image(rw::Raster *r);
void revc_wgpu_allocate_dxt(rw::Raster *r, int32_t dxt, int32_t n, int32_t hasAlpha);
uint32_t revc_wgpu_upload_raster(rw::Raster *r);
void revc_wgpu_native_clear(rw::Raster *r);
void revc_wgpu_native_destroy(rw::Raster *r);
int32_t revc_wgpu_dxt_level_size(int32_t dxt, int32_t w, int32_t h);
int32_t revc_wgpu_image_find_raster_format(rw::Image *img, int32_t type, int32_t *w, int32_t *h, int32_t *depth, int32_t *format);
}

namespace rw {
namespace wgpu {


static_assert(sizeof(WgpuRaster) == 184, "WgpuRaster layout (rwraster.rs)");

inline Raster*
rasterCreate(Raster *raster)
{
	Raster *r = revc_wgpu_raster_create(raster);
	if(r == nil)
		RWERROR_HDR((ERR_INVRASTER));
	return r;
}

inline uint8*
rasterLock(Raster *raster, int32 level, int32 lockMode)
{
	return revc_wgpu_raster_lock(raster, level, lockMode);
}

inline void
rasterUnlock(Raster *raster, int32)
{
	revc_wgpu_raster_unlock(raster);
}

inline int32
rasterNumLevels(Raster *raster)
{
	return GETWGPURASTEREXT(raster)->numLevels;
}

inline bool32
imageFindRasterFormat(Image *img, int32 type, int32 *pWidth, int32 *pHeight, int32 *pDepth, int32 *pFormat)
{
	return revc_wgpu_image_find_raster_format(img, type, pWidth, pHeight, pDepth, pFormat);
}

// Image (any depth) into the raster's current level as RGBA8.
inline bool32
rasterFromImage(Raster *raster, Image *image)
{
	int32 r = revc_wgpu_raster_from_image(raster, image);
	if(r < 0)
		RWERROR_HDR((ERR_INVRASTER));
	return r > 0;
}

inline Image*
rasterToImage(Raster *raster)
{
	return revc_wgpu_raster_to_image(raster);
}

inline int32
dxtLevelSize(int32 dxt, int32 width, int32 height)
{
	return revc_wgpu_dxt_level_size(dxt, width, height);
}

inline void
allocateDXT(Raster *raster, int32 dxt, int32 numLevels, bool32 hasAlpha)
{
	revc_wgpu_allocate_dxt(raster, dxt, numLevels, hasAlpha);
}

inline uint32
uploadRaster(Raster *raster)
{
	return revc_wgpu_upload_raster(raster);
}

inline void registerNativeRaster(void) { revc_wgpu_register_native_raster(); }

}
}

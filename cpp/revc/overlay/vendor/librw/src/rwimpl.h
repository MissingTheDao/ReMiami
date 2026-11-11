// revc (docs/RENDERER_PORT.md, stage 2 step 7d2): librw's C++ methods are inline forwarders to
// the Rust port (rust/revc/src/rw*.rs); their .cpp files are gone. Included at the end of rw.h.
#pragma once
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stddef.h>
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

// ---------------------------------------------------------------- Texture, TexDictionary (rwtexture.rs)
namespace rw {



struct TextureGlobals
{
	TexDictionary *initialTexDict;
	TexDictionary *currentTexDict;
	// load textures from files
	bool32 loadTextures;
	// create dummy textures to store just names
	bool32 makeDummies;
	bool32 mipmapping;
	bool32 autoMipmapping;
	LinkList texDicts;

	LinkList textures;
};

#define TEXTUREGLOBAL(v) (revc_tex_globals()->v)
#define TEXGLOBALS (revc_tex_globals())

// revc (docs/RENDERER_PORT.md, stage 2 step 5a): the texture object model is in Rust
// (rust/revc/src/rwtexture.rs): textures and dictionaries (allocated there), reference counts,
// the lists, find and Texture::read, checked against tests/golden/rwtexture.txt; the module, the default callbacks and the
// anisotropy plugin too (step 7d2).
extern "C" {
TextureGlobals *revc_tex_globals(void);
void revc_texture_register_module(void);
void revc_register_anisotropy_plugin(void);
TexDictionary *revc_texdict_create(TextureGlobals *g);
void revc_texdict_destroy(TextureGlobals *g, TexDictionary *d);
void revc_texdict_add(TexDictionary *d, Texture *t);
void revc_texdict_add_front(TexDictionary *d, Texture *t);
void revc_texdict_remove(TexDictionary *d, Texture *t);
Texture *revc_texdict_find(TexDictionary *d, const char *name);
Texture *revc_tex_create(TextureGlobals *g, Raster *raster);
void revc_tex_destroy(Texture *t);
Texture *revc_tex_default_find(TextureGlobals *g, const char *name);
Texture *revc_tex_read(TextureGlobals *g, const char *name, const char *mask,
                       Texture *(*find)(const char *), Texture *(*read)(const char *, const char *));
}
static_assert(sizeof(Texture) == 120 && offsetof(Texture, inGlobalList) == 104 && sizeof(TexDictionary) == 48,
              "texture layouts (rwtexture.rs)");
inline void Texture::registerModule(void) { revc_texture_register_module(); }

inline void
Texture::setLoadTextures(bool32 b)
{
	TEXTUREGLOBAL(loadTextures) = b;
}

inline void
Texture::setCreateDummies(bool32 b)
{
	TEXTUREGLOBAL(makeDummies) = b;
}

inline void Texture::setMipmapping(bool32 b) { TEXTUREGLOBAL(mipmapping) = b; }
inline void Texture::setAutoMipmapping(bool32 b) { TEXTUREGLOBAL(autoMipmapping) = b; }
inline bool32 Texture::getMipmapping(void) { return TEXTUREGLOBAL(mipmapping); }
inline bool32 Texture::getAutoMipmapping(void) { return TEXTUREGLOBAL(autoMipmapping); }

//
// TexDictionary
//

inline TexDictionary*
TexDictionary::create(void)
{
	TexDictionary *dict = revc_texdict_create(TEXGLOBALS);
	if(dict == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return dict;
}

inline void
TexDictionary::destroy(void)
{
	revc_texdict_destroy(TEXGLOBALS, this);
}

inline void
TexDictionary::add(Texture *t)
{
	revc_texdict_add(this, t);
}

inline void
TexDictionary::remove(Texture *t)
{
	revc_texdict_remove(this, t);
}

inline void
TexDictionary::addFront(Texture *t)
{
	revc_texdict_add_front(this, t);
}

inline Texture*
TexDictionary::find(const char *name)
{
	return revc_texdict_find(this, name);
}


inline void
TexDictionary::setCurrent(TexDictionary *txd)
{
	revc_tex_globals()->currentTexDict = txd;
}

inline TexDictionary*
TexDictionary::getCurrent(void)
{
	return revc_tex_globals()->currentTexDict;
}

//
// Texture
//

inline Texture*
Texture::create(Raster *raster)
{
	Texture *tex = revc_tex_create(TEXGLOBALS, raster);
	if(tex == nil)
		RWERROR_HDR((ERR_ALLOC, s_plglist.size));
	return tex;
}

inline void
Texture::destroy(void)
{
	revc_tex_destroy(this);
}

inline Texture*
Texture::read(const char *name, const char *mask)
{
	return revc_tex_read(TEXGLOBALS, name, mask, Texture::findCB, Texture::readCB);
}


inline void registerAnisotropyPlugin(void) { revc_register_anisotropy_plugin(); }

inline void
Texture::setMaxAnisotropy(int32 maxaniso)
{
	if(anisotOffset > 0)
		*GETANISOTROPYEXT(this) = maxaniso;
}

inline int32
Texture::getMaxAnisotropy(void)
{
	if(anisotOffset > 0)
		return *GETANISOTROPYEXT(this);
	return 1;
}

inline int32
getMaxSupportedMaxAnisotropy(void)
{
#ifdef RW_WGPU
	return 16;	// wgpu's anisotropy clamp limit
#endif
	return 1;
}

}

// ---------------------------------------------------------------- HAnim, Skin, MatFX plugins (rwplugins.rs, rwhanim.rs)
namespace rw {


// revc (docs/RENDERER_PORT.md, stage 2 step 4b): the bone matrices and the HAnim keyframe
// callbacks are in Rust (rust/revc/src/rwhanim.rs), checked against tests/golden/rwhanim.txt.
extern "C" {
void revc_hanim_update_matrices(HAnimHierarchy *h);
void revc_hanim_callbacks(AnimInterpolatorInfo::ApplyCB *apply, AnimInterpolatorInfo::InterpCB *interp);
}
static_assert(sizeof(HAnimKeyFrame) == 40 && sizeof(HAnimInterpFrame) == 48 && sizeof(HAnimNodeInfo) == 24 &&
              sizeof(HAnimHierarchy) == 56, "HAnim layouts (rwhanim.rs)");

inline HAnimHierarchy*
HAnimHierarchy::create(int32 numNodes, int32 *nodeFlags, int32 *nodeIDs,
                       int32 flags, int32 maxKeySize)
{
	HAnimHierarchy *hier = revc_hier_create(numNodes, nodeFlags, nodeIDs, flags, maxKeySize);
	if(hier == nil)
		RWERROR_HDR((ERR_ALLOC, sizeof(*hier)));
	return hier;
}

inline void
HAnimHierarchy::destroy(void)
{
	revc_hier_destroy(this);
}

inline void
HAnimHierarchy::attachByIndex(int32 idx)
{
	revc_hier_attach_by_index(this, idx);
}

inline void
HAnimHierarchy::attach(void)
{
	revc_hier_attach(this);
}

inline int32
HAnimHierarchy::getIndex(int32 id)
{
	return revc_hier_get_index_id(this, id);
}

inline int32
HAnimHierarchy::getIndex(Frame *f)
{
	return revc_hier_get_index_frame(this, f);
}

inline HAnimHierarchy*
HAnimHierarchy::get(Frame *f)
{
	return HAnimData::get(f)->hierarchy;
}

inline HAnimHierarchy*
HAnimHierarchy::find(Frame *f)
{
	return revc_hier_find(f);
}

inline void
HAnimHierarchy::updateMatrices(void)
{
	revc_hanim_update_matrices(this);
}

inline HAnimData*
HAnimData::get(Frame *f)
{
	return PLUGINOFFSET(HAnimData, f, hAnimOffset);
}

inline void*
createHAnim(void *object, int32 offset, int32)
{
	revc_hanim_create(object);
	return object;
}

inline void*
destroyHAnim(void *object, int32 offset, int32)
{
	revc_hanim_destroy(object);
	return object;
}

inline void*
copyHAnim(void *dst, void *src, int32 offset, int32)
{
	revc_hanim_copy(dst, src);
	return dst;
}


inline void*
hanimOpen(void *object, int32 offset, int32 size)
{
	AnimInterpolatorInfo *info = rwNewT(AnimInterpolatorInfo, 1, MEMDUR_GLOBAL | ID_HANIM);
	info->id = 1;
	info->interpKeyFrameSize = sizeof(HAnimInterpFrame);
	info->animKeyFrameSize = sizeof(HAnimKeyFrame);
	info->customDataSize = 0;
	revc_hanim_callbacks(&info->applyCB, &info->interpCB);
	info->blendCB = nil;
	info->addCB = nil;
	info->mulRecipCB = nil;
	// revc: animations aren't streamed through librw (the game reads its own .ifp)
	info->streamRead = nil;
	info->streamWrite = nil;
	info->streamGetSize = nil;
	AnimInterpolatorInfo::registerInterp(info);
	return object;
}

inline void*
hanimClose(void *object, int32 offset, int32 size)
{
	AnimInterpolatorInfo::unregisterInterp(AnimInterpolatorInfo::find(1));
	return object;
}


// the hooks for rwplugins.rs: librw's allocator and the plugin offsets
extern "C" {
struct RevcPluginsHooks {
	uint8 *(*alloc)(size_t size);
	void (*free)(uint8 *p);
	int32 hanimOffset, skinGeoOffset, matfxMaterialOffset, matfxAtomicOffset;
};
void revc_plugins_set_hooks(RevcPluginsHooks h);
}
static_assert(sizeof(HAnimData) == 16 && sizeof(Skin) == 112 && sizeof(MatFX) == 88, "plugin layouts (rwplugins.rs)");
inline uint8 *pluginAlloc(size_t size) { return (uint8*)rwMalloc(size, MEMDUR_EVENT); }
inline void pluginFree(uint8 *p) { rwFree(p); }
inline void
revcPluginsUpdateHooks(void)
{
	RevcPluginsHooks h = { pluginAlloc, pluginFree, hAnimOffset, skinGlobals.geoOffset,
	                       matFXGlobals.materialOffset, matFXGlobals.atomicOffset };
	revc_plugins_set_hooks(h);
}

inline void
registerHAnimPlugin(void)
{
	Engine::registerPlugin(0, ID_HANIM, hanimOpen, hanimClose);
	hAnimOffset = Frame::registerPlugin(sizeof(HAnimData), ID_HANIM,
	                                    createHAnim,
	                                    destroyHAnim, copyHAnim);
	// revc: read in Rust (rw/DffRead.cpp); models aren't written
	revcPluginsUpdateHooks();
}

}
namespace rw {


inline void*
createSkin(void *object, int32 offset, int32)
{
	revc_skin_create(object);
	return object;
}

inline void*
destroySkin(void *object, int32 offset, int32)
{
	revc_skin_destroy(object);
	return object;
}

inline void*
copySkin(void *dst, void *src, int32 offset, int32)
{
	revc_skin_copy(dst, src);
	return dst;
}

// revc: the pre-3.3 skin in the atomic's extension isn't read: none of the game's 4,621 DFFs
// has one (all 275 skins are in the geometry's extension, read in Rust by rw/DffRead.cpp).

inline void
skinRights(void *object, int32, int32, uint32)
{
	Skin::setPipeline((Atomic*)object, 1);
}

inline void
skinAlways(void *object, int32, int32)
{
	Atomic *atomic = (Atomic*)object;
	Geometry *geo = atomic->geometry;
	if(geo == nil) return;
	Skin *skin = Skin::get(geo);
	if(skin == nil) return;
	Skin::setPipeline((Atomic*)object, 1);
}

inline void*
createSkinAtm(void *object, int32 offset, int32)
{
	*PLUGINOFFSET(void*, object, offset) = nil;
	return object;
}

inline void*
destroySkinAtm(void *object, int32 offset, int32)
{
	return object;
}

inline void*
copySkinAtm(void *dst, void *src, int32 offset, int32)
{
	*PLUGINOFFSET(void*, dst, offset) = *PLUGINOFFSET(void*, src, offset);
	return dst;
}

inline void*
skinOpen(void *o, int32, int32)
{
	// init dummy pipelines
	skinGlobals.dummypipe = ObjPipeline::create();
	skinGlobals.dummypipe->pluginID = ID_SKIN;
	skinGlobals.dummypipe->pluginData = 1;
	for(uint i = 0; i < nelem(skinGlobals.pipelines); i++)
		skinGlobals.pipelines[i] = skinGlobals.dummypipe;
	return o;
}

inline void*
skinClose(void *o, int32, int32)
{
	for(uint i = 0; i < nelem(skinGlobals.pipelines); i++)
		if(skinGlobals.pipelines[i] == skinGlobals.dummypipe)
			matFXGlobals.pipelines[i] = nil;
	skinGlobals.dummypipe->destroy();
	skinGlobals.dummypipe = nil;
	return o;
}

inline void
registerSkinPlugin(void)
{
	Driver::registerPlugin(PLATFORM_NULL, 0, ID_SKIN,
	                       skinOpen, skinClose);
	wgpu::initSkin();

	int32 o;
	o = Geometry::registerPlugin(sizeof(Skin*), ID_SKIN,
	                             createSkin, destroySkin, copySkin);
	Geometry::registerPluginStream(ID_SKIN,
	                               nil /* revc: read in Rust (rw/DffRead.cpp) */, nil, nil);
	skinGlobals.geoOffset = o;
	o = Atomic::registerPlugin(sizeof(HAnimHierarchy*),ID_SKIN,
	                           createSkinAtm, destroySkinAtm, copySkinAtm);
	skinGlobals.atomicOffset = o;
		Atomic::setStreamRightsCallback(ID_SKIN, skinRights);
	Atomic::setStreamAlwaysCallback(ID_SKIN, skinAlways);
	revcPluginsUpdateHooks();
}

inline void
Skin::init(int32 numBones, int32 numUsedBones, int32 numVertices)
{
	revc_skin_init(this, numBones, numUsedBones, numVertices);
}



//static_assert(sizeof(Skin::RLEcount) == 2, "RLEcount size");
//static_assert(sizeof(Skin::RLE) == 2, "RLE size");

inline void
Skin::findNumWeights(int32 numVertices)
{
	revc_skin_find_num_weights(this, numVertices);
}

inline void
Skin::findUsedBones(int32 numVertices)
{
	revc_skin_find_used_bones(this, numVertices);
}

inline void
Skin::setPipeline(Atomic *a, int32 type)
{
	(void)type;
	a->pipeline = skinGlobals.pipelines[rw::platform];
}

}
namespace rw {



// Atomic

inline void*
createAtomicMatFX(void *object, int32 offset, int32)
{
	revc_matfx_atomic_create(object);
	return object;
}

inline void*
copyAtomicMatFX(void *dst, void *src, int32 offset, int32)
{
	// don't call seteffects, it will override the pipeline
	revc_matfx_atomic_copy(dst, src);
	return dst;
}

// Material


// TODO: Frames and Matrices?
inline void
MatFX::setEffects(Material *mat, uint32 type)
{
	revc_matfx_set_effects(mat, type);
}

inline uint32
MatFX::getEffects(const Material *m)
{
	return revc_matfx_get_effects(m);
}

inline MatFX*
MatFX::get(const Material *m)
{
	return *PLUGINOFFSET(MatFX*, m, matFXGlobals.materialOffset);
}

inline int32
MatFX::getEffectIndex(uint32 type)
{
	return revc_matfx_effect_index(this, type);
}

inline void
MatFX::setBumpTexture(Texture *t)
{
	revc_matfx_set_bump_texture(this, t);
}

inline void
MatFX::setBumpCoefficient(float32 coef)
{
	revc_matfx_set_bump_coefficient(this, coef);
}

inline Texture*
MatFX::getBumpTexture(void)
{
	return revc_matfx_get_bump_texture(this);
}

inline float32
MatFX::getBumpCoefficient(void)
{
	return revc_matfx_get_bump_coefficient(this);
}

inline void
MatFX::setEnvTexture(Texture *t)
{
	revc_matfx_set_env_texture(this, t);
}

inline void
MatFX::setEnvFrame(Frame *f)
{
	revc_matfx_set_env_frame(this, f);
}

inline void
MatFX::setEnvCoefficient(float32 coef)
{
	revc_matfx_set_env_coefficient(this, coef);
}

inline void
MatFX::setEnvFBAlpha(bool32 useFBAlpha)
{
	revc_matfx_set_env_fb_alpha(this, useFBAlpha);
}

inline Texture*
MatFX::getEnvTexture(void)
{
	return revc_matfx_get_env_texture(this);
}

inline Frame*
MatFX::getEnvFrame(void)
{
	return revc_matfx_get_env_frame(this);
}

inline float32
MatFX::getEnvCoefficient(void)
{
	return revc_matfx_get_env_coefficient(this);
}

inline bool32
MatFX::getEnvFBAlpha(void)
{
	return revc_matfx_get_env_fb_alpha(this);
}


inline void
MatFX::setDualTexture(Texture *t)
{
	revc_matfx_set_dual_texture(this, t);
}

inline void
MatFX::setDualSrcBlend(int32 blend)
{
	revc_matfx_set_dual_src_blend(this, blend);
}

inline void
MatFX::setDualDestBlend(int32 blend)
{
	revc_matfx_set_dual_dest_blend(this, blend);
}

inline Texture*
MatFX::getDualTexture(void)
{
	return revc_matfx_get_dual_texture(this);
}

inline int32
MatFX::getDualSrcBlend(void)
{
	return revc_matfx_get_dual_src_blend(this);
}

inline int32
MatFX::getDualDestBlend(void)
{
	return revc_matfx_get_dual_dest_blend(this);
}

inline void
MatFX::setUVTransformMatrices(Matrix *base, Matrix *dual)
{
	revc_matfx_set_uv_transform(this, base, dual);
}

inline void
MatFX::getUVTransformMatrices(Matrix **base, Matrix **dual)
{
	revc_matfx_get_uv_transform(this, base, dual);
}

inline void*
createMaterialMatFX(void *object, int32 offset, int32)
{
	revc_matfx_material_create(object);
	return object;
}

inline void*
destroyMaterialMatFX(void *object, int32 offset, int32)
{
	revc_matfx_material_destroy(object);
	return object;
}

inline void*
copyMaterialMatFX(void *dst, void *src, int32 offset, int32)
{
	revc_matfx_material_copy(dst, src);
	return dst;
}

inline void
MatFX::enableEffects(Atomic *atomic)
{
	*PLUGINOFFSET(int32, atomic, matFXGlobals.atomicOffset) = 1;
	atomic->pipeline = matFXGlobals.pipelines[rw::platform];
}

// This prevents setting the pipeline on clone
inline void
MatFX::disableEffects(Atomic *atomic)
{
	*PLUGINOFFSET(int32, atomic, matFXGlobals.atomicOffset) = 0;
}

inline bool32
MatFX::getEffects(Atomic *atomic)
{
	return *PLUGINOFFSET(int32, atomic, matFXGlobals.atomicOffset);
}

inline void*
matfxOpen(void *o, int32, int32)
{
	// init dummy pipelines
	matFXGlobals.dummypipe = ObjPipeline::create();
	matFXGlobals.dummypipe->pluginID = 0; //ID_MATFX;
	matFXGlobals.dummypipe->pluginData = 0;
	for(uint i = 0; i < nelem(matFXGlobals.pipelines); i++)
		matFXGlobals.pipelines[i] = matFXGlobals.dummypipe;
	return o;
}

inline void*
matfxClose(void *o, int32, int32)
{
	for(uint i = 0; i < nelem(matFXGlobals.pipelines); i++)
		if(matFXGlobals.pipelines[i] == matFXGlobals.dummypipe)
			matFXGlobals.pipelines[i] = nil;
	matFXGlobals.dummypipe->destroy();
	matFXGlobals.dummypipe = nil;
	return o;
}

inline void
registerMatFXPlugin(void)
{
	Driver::registerPlugin(PLATFORM_NULL, 0, ID_MATFX,
	                       matfxOpen, matfxClose);
	wgpu::initMatFX();

	matFXGlobals.atomicOffset =
	Atomic::registerPlugin(sizeof(int32), ID_MATFX,
	                       createAtomicMatFX, nil, copyAtomicMatFX);

	matFXGlobals.materialOffset =
	Material::registerPlugin(sizeof(MatFX*), ID_MATFX,
	                         createMaterialMatFX, destroyMaterialMatFX,
	                         copyMaterialMatFX);
	revcPluginsUpdateHooks();
}

}

// ---------------------------------------------------------------- vectors, matrices, streams (math.rs, rwmatrix.rs, rwplugin.rs, rwimagefile.rs)
#define PSEP_C '/'
#define PSEP_S "/"
#ifdef __unix__
#endif


namespace rw {



inline Matrix identMat = {
	{ 1.0f, 0.0f, 0.0f }, Matrix::IDENTITY|Matrix::TYPEORTHONORMAL,
	{ 0.0f, 1.0f, 0.0f }, 0,
	{ 0.0f, 0.0f, 1.0f }, 0,
	{ 0.0f, 0.0f, 0.0f }, 0
};

// lazy implementation


// revc (stage 2 step 4b): the Quat functions are in Rust (rust/revc/src/rwhanim.rs)
extern "C" {
void revc_rwquat_mult(const Quat *q, const Quat *p, Quat *out);
void revc_rwquat_lerp(const Quat *q, const Quat *p, float32 r, Quat *out);
void revc_rwquat_slerp(const Quat *q, const Quat *p, float32 a, Quat *out);
}

inline Quat
mult(const Quat &q, const Quat &p)
{
	Quat r;
	revc_rwquat_mult(&q, &p, &r);
	return r;
}


// revc (stage 2 step 7d1): in Rust (rust/revc/src/rwhanim.rs), checked against tests/golden/rwquat.txt
extern "C" void revc_rwquat_rotate(Quat *q, const V3d *axis, float32 angle, int32 op);
inline Quat*
Quat::rotate(const V3d *axis, float32 angle, CombineOp op)
{
	revc_rwquat_rotate(this, axis, angle, op);
	return this;
}

inline Quat
lerp(const Quat &q, const Quat &p, float32 r)
{
	Quat res;
	revc_rwquat_lerp(&q, &p, r, &res);
	return res;
};

inline Quat
slerp(const Quat &q, const Quat &p, float32 a)
{
	Quat res;
	revc_rwquat_slerp(&q, &p, a, &res);
	return res;
}

//
// V3d
//

inline V3d
cross(const V3d &a, const V3d &b)
{
	return makeV3d(a.y*b.z - a.z*b.y,
	               a.z*b.x - a.x*b.z,
	               a.x*b.y - a.y*b.x);
}

// revc (stage 2 step 4c): in Rust (rust/revc/src/rwgeometry.rs)
extern "C" {
void revc_v3d_transform_points(V3d *out, const V3d *in, int32 n, const Matrix *m);
void revc_v3d_transform_vectors(V3d *out, const V3d *in, int32 n, const Matrix *m);
}

inline void
V3d::transformPoints(V3d *out, const V3d *in, int32 n, const Matrix *m)
{
	revc_v3d_transform_points(out, in, n, m);
}

inline void
V3d::transformVectors(V3d *out, const V3d *in, int32 n, const Matrix *m)
{
	revc_v3d_transform_vectors(out, in, n, m);
}


//
// Matrix
//

inline Matrix::Tolerance matrixDefaultTolerance = { 0.01f, 0.01f, 0.01f };

// revc (docs/RENDERER_PORT.md, stage 2 step 3a): the Matrix functions are in Rust
// (rust/revc/src/rwmatrix.rs), bit-identical to the code that was here (tests/golden/rwmatrix.txt).
extern "C" {
void revc_rwm_optimize(Matrix *m, Matrix::Tolerance *tol);
void revc_rwm_mult(Matrix *d, const Matrix *a, const Matrix *b);
void revc_rwm_mult_(Matrix *d, const Matrix *a, const Matrix *b);
void revc_rwm_invert(Matrix *d, const Matrix *s);
void revc_rwm_invert_orthonormal(Matrix *d, const Matrix *s);
void revc_rwm_invert_general(Matrix *d, const Matrix *s);
void revc_rwm_transpose(Matrix *d, const Matrix *s);
void revc_rwm_rotate_axis(Matrix *m, const V3d *axis, float32 angle, int32 op);
void revc_rwm_rotate_quat(Matrix *m, const Quat *q, int32 op);
void revc_rwm_translate(Matrix *m, const V3d *t, int32 op);
void revc_rwm_scale(Matrix *m, const V3d *s, int32 op);
void revc_rwm_transform(Matrix *m, const Matrix *with, int32 op);
void revc_rwm_get_rotation(const Matrix *m, Quat *out);
void revc_rwm_look_at(Matrix *m, const V3d *dir, const V3d *up);
void revc_rwm_make_rotation_axis(Matrix *d, const V3d *axis, float32 angle);
void revc_rwm_make_rotation_quat(Matrix *d, const Quat *q);
float32 revc_rwm_normal_error(const Matrix *m);
float32 revc_rwm_orthogonal_error(const Matrix *m);
float32 revc_rwm_identity_error(const Matrix *m);
}

inline Matrix*
Matrix::create(void)
{
	Matrix *m = (Matrix*)rwMalloc(sizeof(Matrix), MEMDUR_EVENT | ID_MATRIX);
	m->setIdentity();
	return m;
}

inline void
Matrix::destroy(void)
{
	rwFree(this);
}

inline void
Matrix::setIdentity(void)
{
	*this = identMat;
}

inline void
Matrix::optimize(Tolerance *tolerance)
{
	revc_rwm_optimize(this, tolerance);
}

inline Matrix*
Matrix::mult(Matrix *dst, const Matrix *src1, const Matrix *src2)
{
	revc_rwm_mult(dst, src1, src2);
	return dst;
}

inline Matrix*
Matrix::invert(Matrix *dst, const Matrix *src)
{
	revc_rwm_invert(dst, src);
	return dst;
}

// transpose the 3x3 submatrix, pos is set to 0
inline Matrix*
Matrix::transpose(Matrix *dst, const Matrix *src)
{
	revc_rwm_transpose(dst, src);
	return dst;
}

inline Matrix*
Matrix::rotate(const V3d *axis, float32 angle, CombineOp op)
{
	revc_rwm_rotate_axis(this, axis, angle, op);
	return this;
}

inline Matrix*
Matrix::rotate(const Quat &q, CombineOp op)
{
	revc_rwm_rotate_quat(this, &q, op);
	return this;
}
inline Matrix*
Matrix::translate(const V3d *translation, CombineOp op)
{
	revc_rwm_translate(this, translation, op);
	return this;
}

inline Matrix*
Matrix::scale(const V3d *scale, CombineOp op)
{
	revc_rwm_scale(this, scale, op);
	return this;
}

inline Matrix*
Matrix::transform(const Matrix *mat, CombineOp op)
{
	revc_rwm_transform(this, mat, op);
	return this;
}

inline Quat
Matrix::getRotation(void)
{
	Quat q;
	revc_rwm_get_rotation(this, &q);
	return q;
}

inline void
Matrix::lookAt(const V3d &dir, const V3d &up)
{
	revc_rwm_look_at(this, &dir, &up);
}

/* For a row-major representation, this calculates src1 * src2.
 * For column-major src2 * src1.
 * i.e. a vector is first xformed by src1, then by src2
 */
inline void
Matrix::mult_(Matrix *dst, const Matrix *src1, const Matrix *src2)
{
	revc_rwm_mult_(dst, src1, src2);
}

inline void
Matrix::invertOrthonormal(Matrix *dst, const Matrix *src)
{
	revc_rwm_invert_orthonormal(dst, src);
}

inline Matrix*
Matrix::invertGeneral(Matrix *dst, const Matrix *src)
{
	revc_rwm_invert_general(dst, src);
	return dst;
}

inline void
Matrix::makeRotation(Matrix *dst, const V3d *axis, float32 angle)
{
	revc_rwm_make_rotation_axis(dst, axis, angle);
}

/* q must be normalized */
inline void
Matrix::makeRotation(Matrix *dst, const Quat &q)
{
	revc_rwm_make_rotation_quat(dst, &q);
}

inline float32
Matrix::normalError(void)
{
	return revc_rwm_normal_error(this);
}

inline float32
Matrix::orthogonalError(void)
{
	return revc_rwm_orthogonal_error(this);
}

inline float32
Matrix::identityError(void)
{
	return revc_rwm_identity_error(this);
}

// revc (stage 2 step 5b): makePath (with the case correction) is in Rust
// (rust/revc/src/rwimagefile.rs), checked against tests/golden/rwimagefile.txt
extern "C" void revc_make_path(char *filename);

inline void
makePath(char *filename)
{
	revc_make_path(filename);
}

// Streams: the memory stream, the 32/16-bit transfers and the chunk headers
// live in Rust (rust/revc/src/rwplugin.rs), checked against tests/golden/rwplugin.txt.
// Rust reaches the virtual stream methods through these trampolines.
struct StreamHooks {
	uint32 (*read8)(Stream *s, void *data, uint32 n);
	uint32 (*write8)(Stream *s, const void *data, uint32 n);
	void (*seek)(Stream *s, int32 off, int32 whence);
	uint32 (*tell)(Stream *s);
	int32 (*eof)(Stream *s);
	int32 (*version)(void);
	int32 (*build)(void);
};
static_assert(offsetof(StreamMemory, data) == 8 && sizeof(StreamMemory) == 32, "StreamMemory layout");
static_assert(sizeof(ChunkHeaderInfo) == 16, "ChunkHeaderInfo layout");

extern "C" {
void revc_stream_set_hooks(StreamHooks h);
uint32 revc_sm_write8(StreamMemory *s, const void *d, uint32 n);
uint32 revc_sm_read8(StreamMemory *s, void *d, uint32 n);
void revc_sm_seek(StreamMemory *s, int32 off, int32 whence);
void revc_sm_open(StreamMemory *s, uint8 *d, uint32 len, uint32 cap);
uint32 revc_stream_read32(Stream *s, void *d, uint32 n);
uint32 revc_stream_write32(Stream *s, const void *d, uint32 n);
int32 revc_write_chunk_header(Stream *s, int32 type, int32 size);
int32 revc_read_chunk_header_info(Stream *s, ChunkHeaderInfo *h);
int32 revc_find_chunk(Stream *s, uint32 type, uint32 *len, uint32 *ver);
}

inline uint32 hRead8(Stream *s, void *d, uint32 n) { return s->read8(d, n); }
inline uint32 hWrite8(Stream *s, const void *d, uint32 n) { return s->write8(d, n); }
inline void hSeek(Stream *s, int32 off, int32 whence) { s->seek(off, whence); }
inline uint32 hTell(Stream *s) { return s->tell(); }
inline int32 hEof(Stream *s) { return s->eof(); }
inline int32 hVersion(void) { return version; }
inline int32 hBuild(void) { return build; }
inline struct StreamHooksInit {
	StreamHooksInit(void) { revc_stream_set_hooks({ hRead8, hWrite8, hSeek, hTell, hEof, hVersion, hBuild }); }
} streamHooksInit;

inline uint32 Stream::write32(const void *data, uint32 length) { return revc_stream_write32(this, data, length); }
inline uint32 Stream::write16(const void *data, uint32 length) { return revc_stream_write32(this, data, length); }
inline uint32 Stream::read32(void *data, uint32 length) { return revc_stream_read32(this, data, length); }
inline uint32 Stream::read16(void *data, uint32 length) { return revc_stream_read32(this, data, length); }

inline int32 Stream::writeI8(int8 val) { return write8(&val, 1); }
inline int32 Stream::writeU8(uint8 val) { return write8(&val, 1); }
inline int32 Stream::writeI16(int16 val) { return write16(&val, 2); }
inline int32 Stream::writeU16(uint16 val) { return write16(&val, 2); }
inline int32 Stream::writeI32(int32 val) { return write32(&val, 4); }
inline int32 Stream::writeU32(uint32 val) { return write32(&val, 4); }
inline int32 Stream::writeF32(float32 val) { return write32(&val, 4); }
// zero-initialised so a read past the end gives 0 instead of stack garbage
inline int8 Stream::readI8(void) { int8 t = 0; read8(&t, 1); return t; }
inline uint8 Stream::readU8(void) { uint8 t = 0; read8(&t, 1); return t; }
inline int16 Stream::readI16(void) { int16 t = 0; read16(&t, 2); return t; }
inline uint16 Stream::readU16(void) { uint16 t = 0; read16(&t, 2); return t; }
inline int32 Stream::readI32(void) { int32 t = 0; read32(&t, 4); return t; }
inline uint32 Stream::readU32(void) { uint32 t = 0; read32(&t, 4); return t; }
inline float32 Stream::readF32(void) { float32 t = 0; read32(&t, 4); return t; }

inline void StreamMemory::close(void) {}
inline uint32 StreamMemory::write8(const void *data, uint32 len) { return revc_sm_write8(this, data, len); }
inline uint32 StreamMemory::read8(void *data, uint32 len) { return revc_sm_read8(this, data, len); }
inline void StreamMemory::seek(int32 offset, int32 whence) { revc_sm_seek(this, offset, whence); }
inline uint32 StreamMemory::tell(void) { return this->position; }
inline bool StreamMemory::eof(void) { return this->position == S_EOF; }
inline StreamMemory *StreamMemory::open(uint8 *data, uint32 length, uint32 capacity) { revc_sm_open(this, data, length, capacity); return this; }
inline uint32 StreamMemory::getLength(void) { return this->length; }


inline StreamFile*
StreamFile::open(const char *path, const char *mode)
{
	assert(this->file == nil);
	this->file = fopen(path, mode);
	if(this->file == nil){
		RWERROR_HDR((ERR_FILE, path));
		return nil;
	}
	return this;
}

inline void
StreamFile::close(void)
{
	assert(this->file);
	fclose(this->file);
	this->file = nil;
}

inline uint32
StreamFile::write8(const void *data, uint32 length)
{
	return (uint32)fwrite(data, 1, length, this->file);
}

inline uint32
StreamFile::read8(void *data, uint32 length)
{
	return (uint32)fread(data, 1, length, this->file);
}

inline void
StreamFile::seek(int32 offset, int32 whence)
{
	fseek(this->file, offset, whence);
}

inline uint32
StreamFile::tell(void)
{
	return ftell(this->file);
}

inline bool
StreamFile::eof(void)
{
	return ( feof(this->file) != 0 );
}

inline bool writeChunkHeader(Stream *s, int32 type, int32 size) { return revc_write_chunk_header(s, type, size) != 0; }
inline bool readChunkHeaderInfo(Stream *s, ChunkHeaderInfo *header) { return revc_read_chunk_header_info(s, header) != 0; }
inline bool findChunk(Stream *s, uint32 type, uint32 *length, uint32 *version) { return revc_find_chunk(s, type, length, version) != 0; }


}

// ---------------------------------------------------------------- debug font (rwcharset.rs)
#ifndef RW_NULL

#ifdef RWHALFPIXEL
#define HALFPX (0.5f)
#else
#define HALFPX (0.0f)
#endif

// The debug font is Rust (rust/revc/src/rwcharset.rs, checked against tests/golden/rwcharset.txt);
// it reaches rasters, render states and Im2D through these hooks. The font bitmap (vgafont.inc)
// is read by the Rust side.
namespace rw {

static_assert(sizeof(Charset) == 40, "Charset layout (rwcharset.rs)");

extern "C" {
struct CharsetHooks {
	Raster *(*rasterFromFont)(const uint8 *pixels, const uint8 *palette);
	void (*rasterDestroy)(Raster *r);
	void (*setRenderState)(int32 state, uint32 value);
	void (*setRenderStatePtr)(int32 state, void *value);
	uint32 (*getRenderState)(int32 state);
	float32 (*nearZ)(void);
	float32 (*nearPlane)(void);
	void (*renderIndexed)(uint32 prim, void *vertices, int32 numVertices, void *indices, int32 numIndices);
};
void revc_charset_set_hooks(CharsetHooks h);
int32 revc_charset_open(void);
void revc_charset_close(void);
Charset *revc_charset_create(const RGBA *fg, const RGBA *bg);
void revc_charset_destroy(Charset *cs);
Charset *revc_charset_set_colors(Charset *cs, const RGBA *fg, const RGBA *bg);
void revc_charset_print(Charset *cs, const char *s, int32 x, int32 y, int32 hide);
void revc_charset_print_buffered(Charset *cs, const char *s, int32 x, int32 y, int32 hide);
void revc_charset_flush(void);
}

inline Raster*
hRasterFromFont(const uint8 *pixels, const uint8 *palette)
{
	Image *img = Image::create(256, 256, 8);
	if(img == nil)
		return nil;
	img->pixels = (uint8*)pixels;
	img->stride = 256;
	img->allocate();
	memcpy(img->palette, palette, 8);
	Raster *r = Raster::createFromImage(img);
	img->destroy();
	return r;
}
inline void hRasterDestroy(Raster *r) { r->destroy(); }
inline void hSetRenderState(int32 s, uint32 v) { SetRenderState(s, v); }
inline void hSetRenderStatePtr(int32 s, void *v) { SetRenderStatePtr(s, v); }
inline uint32 hGetRenderState(int32 s) { return GetRenderState(s); }
inline float32 hNearZ(void) { return im2d::GetNearZ(); }
inline float32 hNearPlane(void) { return ((Camera*)engine->currentCamera)->nearPlane; }
inline void
hRenderIndexed(uint32 prim, void *v, int32 nv, void *ix, int32 ni)
{
	im2d::RenderIndexedPrimitive((PrimitiveType)prim, v, nv, ix, ni);
}
inline struct CharsetInit {
	CharsetInit(void) {
		revc_charset_set_hooks({ hRasterFromFont, hRasterDestroy, hSetRenderState, hSetRenderStatePtr,
			hGetRenderState, hNearZ, hNearPlane, hRenderIndexed });
	}
} charsetInit;

inline bool32 Charset::open(void) { return revc_charset_open(); }
inline void Charset::close(void) { revc_charset_close(); }
inline Charset *Charset::create(const RGBA *fg, const RGBA *bg) { return revc_charset_create(fg, bg); }
inline void Charset::destroy(void) { revc_charset_destroy(this); }
inline Charset *Charset::setColors(const RGBA *fg, const RGBA *bg) { return revc_charset_set_colors(this, fg, bg); }
inline void Charset::flushBuffer(void) { revc_charset_flush(); }
inline void Charset::print(const char *str, int32 x, int32 y, bool32 hideSpaces) { revc_charset_print(this, str, x, y, hideSpaces); }
inline void Charset::printBuffered(const char *str, int32 x, int32 y, bool32 hideSpaces) { revc_charset_print_buffered(this, str, x, y, hideSpaces); }

}

#endif

// ---------------------------------------------------------------- Engine (rwengine.rs)
// on windows
//#ifdef DEBUG
//#include <crtdbg.h>
//#define free(p) _free_dbg(p, _NORMAL_BLOCK);
//#define malloc(sz) _malloc_dbg(sz, _NORMAL_BLOCK, __FILE__, __LINE__)
//#define realloc(p, sz) _realloc_dbg(p, sz, _NORMAL_BLOCK, __FILE__, __LINE__)
//#endif

// The engine core is Rust (rust/revc/src/rwengine.rs): the state machine, the default memory
// functions, the driver tables and the errors. The C++ statics the headers name stay here, and
// the per-class modules and plugin lists that are still C++ are reached through hooks.
namespace rw {


static_assert(sizeof(Engine) == 288 && sizeof(Driver) == 88 && sizeof(MemoryFunctions) == 40, "engine layout (rwengine.rs)");
static_assert(NUM_PLATFORMS == 14, "rwengine.rs NUM_PLATFORMS");
// the plugin lists start at these sizes (rwplugin.rs class_sizes)
static_assert(sizeof(Frame) == 200 && sizeof(Raster) == 96 && sizeof(Texture) == 120 && sizeof(TexDictionary) == 48 &&
              sizeof(Geometry) == 168 && sizeof(Material) == 40 && sizeof(Atomic) == 136 && sizeof(Clump) == 88 &&
              sizeof(Light) == 120 && sizeof(Camera) == 600 && sizeof(World) == 64, "class sizes (rwplugin.rs)");

extern "C" {
struct EngineHooks {
	void (*registerModules)(void);
	void (*registerImageFormats)(void);
	ObjPipeline *(*pipelineCreate)(void);
	void (*pipelineDestroy)(ObjPipeline *pipe);
};
void revc_engine_set_hooks(EngineHooks h);
int32 revc_engine_init(MemoryFunctions *memfuncs);
int32 revc_engine_open(EngineOpenParams *p);
int32 revc_engine_start(void);
void revc_engine_stop(void);
void revc_engine_close(void);
void revc_engine_term(void);
int32 revc_engine_system(int32 req, void *arg, int32 n);
}

inline void
registerModules(void)
{
	// core plugin attach here
	Frame::registerModule();
	Image::registerModule();
	Raster::registerModule();
	Texture::registerModule();

	Frame::numAllocated = 0;
	Image::numAllocated = 0;
	Raster::numAllocated = 0;
	Texture::numAllocated = 0;
	TexDictionary::numAllocated = 0;
	Geometry::numAllocated = 0;
	Material::numAllocated = 0;
	Atomic::numAllocated = 0;
	Light::numAllocated = 0;
	Camera::numAllocated = 0;
	Clump::numAllocated = 0;
	World::numAllocated = 0;

	// driver plugin attach
	wgpu::registerPlatformPlugins();
}

inline void
registerImageFormats(void)
{
	Image::registerFileFormat("tga", readTGA, writeTGA);
	Image::registerFileFormat("bmp", readBMP, writeBMP);
	Image::registerFileFormat("png", readPNG, writePNG);
}

inline ObjPipeline *pipelineCreate(void) { return ObjPipeline::create(); }
inline void pipelineDestroy(ObjPipeline *pipe) { pipe->destroy(); }

inline struct EngineHooksInit {
	EngineHooksInit(void) {
		revc_engine_set_hooks({ registerModules, registerImageFormats, pipelineCreate, pipelineDestroy });
	}
} engineHooksInit;

inline char *strdup_LOC(const char *s, uint32 hint, const char *here) {
	char *t;
	size_t sz = strlen(s)+1;
	t = (char*)malloc_LOC(sz, hint, here);
	if(t)
		memcpy(t, s, sz);
	return t;
}

inline bool32 Engine::init(MemoryFunctions *memfuncs) { return revc_engine_init(memfuncs); }
inline bool32 Engine::open(EngineOpenParams *p) { return revc_engine_open(p); }
inline bool32 Engine::start(void) { return revc_engine_start(); }
inline void Engine::term(void) { revc_engine_term(); }
inline void Engine::close(void) { revc_engine_close(); }
inline void Engine::stop(void) { revc_engine_stop(); }

inline int32 Engine::getNumSubSystems(void) { return revc_engine_system(DEVICEGETNUMSUBSYSTEMS, nil, 0); }
inline int32 Engine::getCurrentSubSystem(void) { return revc_engine_system(DEVICEGETCURRENTSUBSYSTEM, nil, 0); }
inline bool32 Engine::setSubSystem(int32 subsys) { return revc_engine_system(DEVICESETSUBSYSTEM, nil, subsys); }
inline SubSystemInfo*
Engine::getSubSystemInfo(SubSystemInfo *info, int32 subsys)
{
	return revc_engine_system(DEVICEGETSUBSSYSTEMINFO, info, subsys) ? info : nil;
}
inline int32 Engine::getNumVideoModes(void) { return revc_engine_system(DEVICEGETNUMVIDEOMODES, nil, 0); }
inline int32 Engine::getCurrentVideoMode(void) { return revc_engine_system(DEVICEGETCURRENTVIDEOMODE, nil, 0); }
inline bool32 Engine::setVideoMode(int32 mode) { return revc_engine_system(DEVICESETVIDEOMODE, nil, mode); }
inline VideoMode*
Engine::getVideoModeInfo(VideoMode *info, int32 mode)
{
	return revc_engine_system(DEVICEGETVIDEOMODEINFO, info, mode) ? info : nil;
}
inline uint32 Engine::getMaxMultiSamplingLevels(void) { return revc_engine_system(DEVICEGETMAXMULTISAMPLINGLEVELS, nil, 0); }
inline uint32 Engine::getMultiSamplingLevels(void) { return revc_engine_system(DEVICEGETMULTISAMPLINGLEVELS, nil, 0); }
inline bool32 Engine::setMultiSamplingLevels(uint32 levels) { return revc_engine_system(DEVICESETMULTISAMPLINGLEVELS, nil, levels); }

}

// ---------------------------------------------------------------- the wgpu device (rwdevice.rs)
// WGPU platform device: the driver plugin and the C++ face of the Rust device
// (rust/revc/src/rwdevice.rs); models go through wgpupipe.cpp.



#ifdef RW_WGPU

namespace rw {
namespace wgpu {

Raster *rasterCreate(Raster *raster);
uint8 *rasterLock(Raster *raster, int32 level, int32 lockMode);
void rasterUnlock(Raster *raster, int32 level);
int32 rasterNumLevels(Raster *raster);
bool32 imageFindRasterFormat(Image *img, int32 type, int32 *pWidth, int32 *pHeight, int32 *pDepth, int32 *pFormat);
bool32 rasterFromImage(Raster *raster, Image *image);
Image *rasterToImage(Raster *raster);
void registerNativeRaster(void);

// The device itself is Rust (rust/revc/src/rwdevice.rs); the Device table is filled from it and
// the names the pipelines and the game's custom pipelines use forward to it.
extern "C" {
struct DeviceHooks {
	Camera **(*currentCamera)(void);
	Raster *(*currentContext)(void);
	int32 (*maxAnisotropy)(Texture *tex);
	int32 (*antialiasing)(void);
};
void revc_device_set_hooks(DeviceHooks h);
void revc_device_evict_raster(Raster *r);
void revc_device_set_texture_stage0(Texture *t);
void revc_device_set_im2d_override(uint32 shader, const float32 *params);
void revc_device_current_draw_state(DrawState *s);
const float32 *revc_device_camera_xform(void);
void revc_device_begin_overlay(void);
void revc_device_im2d_uv2(PrimitiveType prim, void *v, int32 nv, void *i, int32 ni, Texture *tex1);
}

static_assert(sizeof(Device) == 136, "Device layout (rwdevice.rs)");
static_assert(sizeof(DrawState) == 120, "DrawState layout (revc_rw::frame::DrawState)");


inline Camera **hCurrentCamera(void) { return &engine->currentCamera; }
inline Raster *hCurrentContext(void) { return Raster::getCurrentContext(); }
inline int32 hMaxAnisotropy(Texture *t) { return t->getMaxAnisotropy(); }
inline int32 hAntialiasing(void) { return antialiasing; }
inline struct DeviceInit {
	DeviceInit(void) {
		revc_device_set_hooks({ hCurrentCamera, hCurrentContext, hMaxAnisotropy, hAntialiasing });
	}
} deviceInit;

inline void evictRaster(Raster *raster) { revc_device_evict_raster(raster); }
inline void setTextureStage0(Texture *tex) { revc_device_set_texture_stage0(tex); }
inline void setIm2DOverride(uint32 shader, const float32 *params) { revc_device_set_im2d_override(shader, params); }
inline void currentDrawState(DrawState *s) { revc_device_current_draw_state(s); }
inline const float32 *cameraXform(void) { return revc_device_camera_xform(); }
inline void beginOverlay(void) { revc_device_begin_overlay(); }
inline void
im2DRenderIndexedPrimitiveUV2(PrimitiveType primType, void *vertices, int32 numVertices,
	void *indices, int32 numIndices, Texture *tex1)
{
	revc_device_im2d_uv2(primType, vertices, numVertices, indices, numIndices, tex1);
}

inline void*
driverOpen(void *o, int32, int32)
{
	engine->driver[PLATFORM_WGPU]->defaultPipeline = makeDefaultPipeline();
	engine->driver[PLATFORM_WGPU]->rasterNativeOffset = nativeRasterOffset;
	engine->driver[PLATFORM_WGPU]->rasterCreate = rasterCreate;
	engine->driver[PLATFORM_WGPU]->rasterLock = rasterLock;
	engine->driver[PLATFORM_WGPU]->rasterUnlock = rasterUnlock;
	engine->driver[PLATFORM_WGPU]->rasterNumLevels = rasterNumLevels;
	engine->driver[PLATFORM_WGPU]->imageFindRasterFormat = imageFindRasterFormat;
	engine->driver[PLATFORM_WGPU]->rasterFromImage = rasterFromImage;
	engine->driver[PLATFORM_WGPU]->rasterToImage = rasterToImage;
	return o;
}

inline void*
driverClose(void *o, int32, int32)
{
	return o;
}

inline void
registerPlatformPlugins(void)
{
	Driver::registerPlugin(PLATFORM_WGPU, 0, PLATFORM_WGPU, driverOpen, driverClose);
	registerNativeRaster();
}

}
}

#else

namespace rw {
namespace wgpu {
inline void registerPlatformPlugins(void) {}
}
}

#endif

// ---------------------------------------------------------------- the wgpu pipelines (rwpipe.rs)
// WGPU platform atomic pipelines: the C++ face of rust/revc/src/rwpipe.rs (instancing, lighting,
// MatFX env maps, skinning) and the default/MatFX/skin pipeline objects.



#ifdef RW_WGPU

namespace rw {
namespace wgpu {

// The pipeline's work is Rust (rust/revc/src/rwpipe.rs); the pipeline objects, the driver
// plugins that make them, and the names the game's custom pipelines use stay here.
extern "C" {
struct EnvSettings { int32 flipU, applyLight, useMatColor; RGBA color; };
struct PipeHooks {
	World *(*currentWorld)(void);
	int32 (*skinAtomicOffset)(void);
	void (*envSettings)(EnvSettings *out);
};
void revc_pipe_set_hooks(PipeHooks h);
void revc_pipe_destroy_native_data(Geometry *g);
void revc_pipe_instance(uint32 pluginID, Atomic *a);
void revc_pipe_render(uint32 pluginID, Atomic *a);
InstanceDataHeader *revc_pipe_instance_atomic(uint32 pluginID, Atomic *a);
void revc_pipe_begin_model(Atomic *a, InstanceDataHeader *h, ModelState *m);
void revc_pipe_set_model_material(ModelState *m, const RGBA *c, const SurfaceProperties *sp);
void revc_pipe_draw_mesh(InstanceDataHeader *h, InstanceData *i, const ModelState *m, uint32 shader, int32 useTex1, Texture *tex1);
void revc_pipe_render_default(uint32 pluginID, Atomic *a, InstanceDataHeader *h);
const float32 *revc_pipe_last_light_direction(void);
}
static_assert(sizeof(InstanceDataHeader) == 32 && sizeof(InstanceData) == 24, "instance data layout (rwpipe.rs)");
static_assert(sizeof(ModelState) == 232*4, "ModelState layout (revc_rw::frame::ModelState)");

inline World *hCurrentWorld(void) { return (World*)engine->currentWorld; }
inline int32 hSkinAtomicOffset(void) { return skinGlobals.atomicOffset; }
inline void
hEnvSettings(EnvSettings *out)
{
	out->flipU = MatFX::envMapFlipU;
	out->applyLight = MatFX::envMapApplyLight;
	out->useMatColor = MatFX::envMapUseMatColor;
	out->color = MatFX::envMapColor;
}
inline struct PipeInit {
	PipeInit(void) { revc_pipe_set_hooks({ hCurrentWorld, hSkinAtomicOffset, hEnvSettings }); }
} pipeInit;

inline void *destroyNativeData(void *object, int32, int32) { revc_pipe_destroy_native_data((Geometry*)object); return object; }
inline void instance(rw::ObjPipeline *pipe, Atomic *atomic) { revc_pipe_instance(pipe->pluginID, atomic); }
inline void uninstance(rw::ObjPipeline*, Atomic*) { assert(0 && "can't uninstance"); }
inline void render(rw::ObjPipeline *pipe, Atomic *atomic) { revc_pipe_render(pipe->pluginID, atomic); }
inline InstanceDataHeader *instanceAtomic(rw::ObjPipeline *pipe, Atomic *atomic) { return revc_pipe_instance_atomic(pipe->pluginID, atomic); }
inline void beginModel(Atomic *atomic, InstanceDataHeader *header, ModelState *model) { revc_pipe_begin_model(atomic, header, model); }
inline void setModelMaterial(ModelState *model, const RGBA &col, const SurfaceProperties &sp) { revc_pipe_set_model_material(model, &col, &sp); }
inline void setTexture(Texture *tex) { setTextureStage0(tex); }
inline const float32 *lastLightDirection(void) { return revc_pipe_last_light_direction(); }
inline void
drawMesh(InstanceDataHeader *header, InstanceData *inst, const ModelState *model, uint32 shader,
	bool32 useTex1, Texture *tex1)
{
	revc_pipe_draw_mesh(header, inst, model, shader, useTex1, tex1);
}
inline void renderDefault(rw::ObjPipeline *pipe, Atomic *atomic, InstanceDataHeader *header) { revc_pipe_render_default(pipe->pluginID, atomic, header); }

inline void*
matfxOpen(void *o, int32, int32)
{
	rw::ObjPipeline *pipe = makeDefaultPipeline();
	pipe->pluginID = ID_MATFX;
	pipe->pluginData = 0;
	matFXGlobals.pipelines[PLATFORM_WGPU] = pipe;
	return o;
}

inline void*
matfxClose(void *o, int32, int32)
{
	if(matFXGlobals.pipelines[PLATFORM_WGPU]){
		matFXGlobals.pipelines[PLATFORM_WGPU]->destroy();
		matFXGlobals.pipelines[PLATFORM_WGPU] = nil;
	}
	return o;
}

inline void
initMatFX(void)
{
	Driver::registerPlugin(PLATFORM_WGPU, 0, ID_MATFX, matfxOpen, matfxClose);
}

inline void*
skinOpen(void *o, int32, int32)
{
	rw::ObjPipeline *pipe = makeDefaultPipeline();
	pipe->pluginID = ID_SKIN;
	pipe->pluginData = 1;
	skinGlobals.pipelines[PLATFORM_WGPU] = pipe;
	return o;
}

inline void*
skinClose(void *o, int32, int32)
{
	if(skinGlobals.pipelines[PLATFORM_WGPU]){
		skinGlobals.pipelines[PLATFORM_WGPU]->destroy();
		skinGlobals.pipelines[PLATFORM_WGPU] = nil;
	}
	return o;
}

inline void
initSkin(void)
{
	Driver::registerPlugin(PLATFORM_WGPU, 0, ID_SKIN, skinOpen, skinClose);
}

inline rw::ObjPipeline*
makeDefaultPipeline(void)
{
	rw::ObjPipeline *pipe = rw::ObjPipeline::create();
	pipe->init(PLATFORM_WGPU);
	pipe->impl.instance = instance;
	pipe->impl.uninstance = uninstance;
	pipe->impl.render = render;
	return pipe;
}

}
}

#else

namespace rw {
namespace wgpu {
inline void *destroyNativeData(void *object, int32, int32) { return object; }
inline void initMatFX(void) { }
inline void initSkin(void) { }
}
}

#endif

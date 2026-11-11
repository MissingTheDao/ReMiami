// revc (docs/RENDERER_PORT.md, stage 2 step 7d2): librw's C++ methods are inline forwarders to
// the Rust port (rust/revc/src/rw*.rs); their .cpp files are gone. Included at the end of rw.h.
#pragma once
#include <assert.h>
#include <stdio.h>
#include "rwerror.h"
#include "rwscene_ffi.h"

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

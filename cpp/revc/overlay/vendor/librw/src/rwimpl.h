// revc (docs/RENDERER_PORT.md, stage 2 step 7d2): librw's C++ methods are inline forwarders to
// the Rust port (rust/revc/src/rw*.rs); their .cpp files are gone. Included at the end of rw.h.
#pragma once
#include <assert.h>

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

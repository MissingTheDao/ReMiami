//! librw's scene objects, stage 2 step 6b: clump.cpp (Clump create/clone/destroy and its atomic,
//! light and camera lists, render; Atomic create/clone/destroy/setGeometry and the world
//! bounding sphere, the atomic sync callbacks), light.cpp (Light create/destroy, angle, colour),
//! world.cpp (World create/destroy, adding and removing clumps, atomics, lights and cameras,
//! render, enumerateLights) and camera.cpp's Camera create/clone/destroy. Objects keep librw's
//! layouts; memory, plugin data, the camera's device callbacks and the render callbacks stay C++
//! (hooks or the objects' own function pointers).
//!
//! The local model drafted the functions from the C++; the draft was rewritten where it was
//! wrong (circular list walks, setFrame, the clone's frames, the world-sphere dirty test, flag
//! values). Checked against tests/golden/rwscene.txt, recorded from the original C++
//! (tests/golden/recorders/rwscene).

use std::ffi::c_void;
use crate::math::{RwMatrix, V3};
use crate::rwcamera::{BBox, FrustumPlane, Sphere};
use crate::rwframe::{self, Frame, LLLink, LinkList, Object, ObjectWithFrame, link_remove, list_add, list_init};
use crate::rwgeomobj::{Geometry, geo_destroy};
use crate::rwgeometry::transform_point;
use crate::rwmatrix;

pub type Sync = unsafe extern "C" fn(*mut ObjectWithFrame);           // ObjectWithFrame::Sync (sync_cb field type: Option<Sync>)
pub type RenderCb = unsafe extern "C" fn(*mut Atomic);
pub type CameraCb = unsafe extern "C" fn(*mut Camera);

#[repr(C)] #[derive(Clone, Copy)] pub struct RgbaF { pub red: f32, pub green: f32, pub blue: f32, pub alpha: f32 }
#[repr(C)] pub struct Atomic {
    pub object: ObjectWithFrame, pub geometry: *mut Geometry, pub bounding_sphere: Sphere, pub world_bounding_sphere: Sphere,
    pub clump: *mut Clump, pub in_clump: LLLink, pub pipeline: *mut c_void, pub render_cb: Option<RenderCb>,
    pub world: *mut World, pub original_sync: Option<Sync>,
}
#[repr(C)] pub struct Clump { pub object: Object, pub atomics: LinkList, pub lights: LinkList, pub cameras: LinkList, pub world: *mut World, pub in_world: LLLink }
#[repr(C)] pub struct Light {
    pub object: ObjectWithFrame, pub radius: f32, pub color: RgbaF, pub minus_cos_angle: f32, pub in_world: LLLink,
    pub clump: *mut Clump, pub in_clump: LLLink, pub world: *mut World, pub original_sync: Option<Sync>,
}
#[repr(C)] pub struct World { pub object: Object, pub local_lights: LinkList, pub global_lights: LinkList, pub clumps: LinkList }
#[repr(C)] pub struct WorldLights { pub num_ambients: i32, pub ambient: RgbaF, pub num_directionals: i32, pub directionals: *mut *mut Light, pub num_locals: i32, pub locals: *mut *mut Light }
#[repr(C)] pub struct Camera {
    pub object: ObjectWithFrame, pub begin_update_cb: Option<CameraCb>, pub end_update_cb: Option<CameraCb>,
    pub view_window: [f32; 2], pub view_offset: [f32; 2], pub near_plane: f32, pub far_plane: f32, pub fog_plane: f32, pub projection: i32,
    pub view_matrix: RwMatrix, pub z_scale: f32, pub z_shift: f32, pub frustum_planes: [FrustumPlane; 6], pub frustum_corners: [V3; 8],
    pub frustum_bound_box: BBox, pub frame_buffer: *mut c_void, pub z_buffer: *mut c_void, pub dev_view: [f32; 16], pub dev_proj: [f32; 16],
    pub clump: *mut Clump, pub in_clump: LLLink, pub world: *mut World, pub original_sync: Option<Sync>,
    pub original_begin_update: Option<CameraCb>, pub original_end_update: Option<CameraCb>,
}
// object ids and flags
pub const ATOMIC_ID: u8 = 1; pub const CLUMP_ID: u8 = 2; pub const LIGHT_ID: u8 = 3; pub const CAMERA_ID: u8 = 4; pub const WORLD_ID: u8 = 7;
pub const COLLISIONTEST: u8 = 0x01; pub const RENDER: u8 = 0x04; pub const WORLDBOUNDDIRTY: u8 = 0x01; pub const SAMEBOUNDINGSPHERE: u32 = 0x01;
pub const DIRECTIONAL: i32 = 1; pub const AMBIENT: i32 = 2; pub const POINT: i32 = 0x80; pub const LIGHTATOMICS: u8 = 1; pub const LIGHTWORLD: u8 = 2;
pub const PERSPECTIVE: i32 = 1;
/// kinds for the per-type hooks: 0 atomic, 1 clump, 2 light, 3 camera, 4 world
#[repr(C)] pub struct Hooks {
    pub alloc: unsafe extern "C" fn(usize) -> *mut u8,   // rwMalloc (not zeroed)
    pub free: unsafe extern "C" fn(*mut u8),             // rwFree
    pub size: unsafe extern "C" fn(kind: i32) -> usize,  // s_plglist.size
    pub construct: unsafe extern "C" fn(kind: i32, *mut c_void),
    pub destruct: unsafe extern "C" fn(kind: i32, *mut c_void),
    pub copy: unsafe extern "C" fn(kind: i32, dst: *mut c_void, src: *mut c_void),
    pub allocated: unsafe extern "C" fn(kind: i32, delta: i32), // numAllocated += delta
    pub camera_sync: Sync,                       // camera.cpp's cameraSync
    pub default_begin_update: CameraCb, pub default_end_update: CameraCb,   // defaultBeginUpdateCB / defaultEndUpdateCB
    pub world_begin_update: CameraCb, pub world_end_update: CameraCb,       // worldBeginUpdateCB / worldEndUpdateCB
    pub dirty_list: unsafe extern "C" fn() -> *mut LinkList, // &engine->frameDirtyList
}
static mut HOOKS: Option<Hooks> = None;
fn hooks() -> &'static Hooks { unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().unwrap_or(&DEFAULT_HOOKS) } }

// The game's hooks (stage 2 step 7d2, from camera.cpp): librw's allocator, the classes' plugin
// lists and counters (rwplugin.rs), the camera's sync and update callbacks.
mod defaults {
    use super::*;
    use crate::rwplugin::{self as pl, class_list};
    fn class(kind: i32) -> usize {
        [pl::CLASS_ATOMIC, pl::CLASS_CLUMP, pl::CLASS_LIGHT, pl::CLASS_CAMERA, pl::CLASS_WORLD][kind as usize]
    }
    fn engine() -> *mut crate::rwengine::Engine {
        unsafe { *std::ptr::addr_of!(crate::rwengine::revc_engine) }
    }
    pub(super) unsafe extern "C" fn alloc(size: usize) -> *mut u8 {
        unsafe { crate::rwengine::rw_malloc(size, 0x30000) as *mut u8 }
    }
    pub(super) unsafe extern "C" fn free(p: *mut u8) {
        unsafe { crate::rwengine::rw_free(p as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn size(kind: i32) -> usize {
        unsafe { (*class_list(class(kind))).size as usize }
    }
    pub(super) unsafe extern "C" fn construct(kind: i32, o: *mut c_void) {
        unsafe { pl::pl_construct(class_list(class(kind)), o) }
    }
    pub(super) unsafe extern "C" fn destruct(kind: i32, o: *mut c_void) {
        unsafe { pl::pl_destruct(class_list(class(kind)), o) }
    }
    pub(super) unsafe extern "C" fn copy(kind: i32, d: *mut c_void, s: *mut c_void) {
        unsafe { pl::pl_copy(class_list(class(kind)), d, s) }
    }
    pub(super) unsafe extern "C" fn allocated(kind: i32, delta: i32) {
        unsafe { *pl::alloc_count(class(kind)) += delta }
    }
    /// camera.cpp's cameraSync: the view matrix and the frustum from the frame's LTM.
    pub(super) unsafe extern "C" fn camera_sync(o: *mut ObjectWithFrame) {
        unsafe {
            let cam = o as *mut Camera;
            let ltm = crate::rwframe::get_ltm((*cam).object.object.parent as *mut Frame);
            crate::rwcamera::revc_camera_sync(
                ltm,
                std::ptr::addr_of!((*cam).view_window) as *const crate::rwcamera::CamParams,
                &mut (*cam).view_matrix,
                (*cam).frustum_planes.as_mut_ptr(),
                (*cam).frustum_corners.as_mut_ptr(),
                &mut (*cam).frustum_bound_box,
            );
        }
    }
    pub(super) unsafe extern "C" fn default_begin_update(cam: *mut Camera) {
        unsafe {
            let e = engine();
            (*e).current_camera = cam as *mut c_void;
            crate::rwframe::sync_dirty(&mut (*e).frame_dirty_list);
            ((*e).device.begin_update)(cam);
        }
    }
    pub(super) unsafe extern "C" fn default_end_update(cam: *mut Camera) {
        unsafe { ((*engine()).device.end_update)(cam) }
    }
    pub(super) unsafe extern "C" fn world_begin_update(cam: *mut Camera) {
        unsafe {
            (*engine()).current_world = (*cam).world as *mut c_void;
            ((*cam).original_begin_update.expect("originalBeginUpdate"))(cam);
        }
    }
    pub(super) unsafe extern "C" fn world_end_update(cam: *mut Camera) {
        unsafe { ((*cam).original_end_update.expect("originalEndUpdate"))(cam) }
    }
    pub(super) unsafe extern "C" fn dirty_list() -> *mut LinkList {
        unsafe { &mut (*engine()).frame_dirty_list }
    }
}
static DEFAULT_HOOKS: Hooks = Hooks {
    alloc: defaults::alloc,
    free: defaults::free,
    size: defaults::size,
    construct: defaults::construct,
    destruct: defaults::destruct,
    copy: defaults::copy,
    allocated: defaults::allocated,
    camera_sync: defaults::camera_sync,
    default_begin_update: defaults::default_begin_update,
    default_end_update: defaults::default_end_update,
    world_begin_update: defaults::world_begin_update,
    world_end_update: defaults::world_end_update,
    dirty_list: defaults::dirty_list,
};

const NORMALS: u32 = crate::rwgeomobj::NORMALS;

macro_rules! container_of {
    ($link:expr, $t:ty, $field:ident) => {
        ($link as *mut u8).sub(std::mem::offset_of!($t, $field)) as *mut $t
    };
}

/// FORLIST: every element of a circular list; the next link is read before the body runs, so
/// the body may unlink (or free) the current one.
unsafe fn for_list(list: *mut LinkList, mut f: impl FnMut(*mut LLLink)) {
    unsafe {
        let end: *mut LLLink = &mut (*list).link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            f(lnk);
            lnk = next;
        }
    }
}

unsafe fn list_append(list: *mut LinkList, link: *mut LLLink) {
    unsafe {
        (*link).next = &mut (*list).link;
        (*link).prev = (*list).link.prev;
        (*(*list).link.prev).next = link;
        (*list).link.prev = link;
    }
}

fn link_init(l: &mut LLLink) {
    l.next = std::ptr::null_mut();
    l.prev = std::ptr::null_mut();
}

fn object_init(kind: u8, sub_type: u8) -> Object {
    Object { kind, sub_type, flags: 0, private_flags: 0, parent: std::ptr::null_mut() }
}

/// Object::copy (the parent isn't copied)
fn object_copy(o: &Object) -> Object {
    Object { kind: o.kind, sub_type: o.sub_type, flags: o.flags, private_flags: o.private_flags, parent: std::ptr::null_mut() }
}

unsafe fn alloc<T>(kind: i32) -> *mut T {
    unsafe {
        let h = hooks();
        let p = (h.alloc)((h.size)(kind)) as *mut T;
        if !p.is_null() {
            (h.allocated)(kind, 1);
        }
        p
    }
}

unsafe fn free<T>(kind: i32, p: *mut T) {
    unsafe {
        let h = hooks();
        (h.free)(p as *mut u8);
        (h.allocated)(kind, -1);
    }
}

fn frame_of(o: &ObjectWithFrame) -> *mut Frame {
    o.object.parent as *mut Frame
}

/// ObjectWithFrame::setFrame
///
/// # Safety
/// A valid object and frame (or null).
pub unsafe fn set_frame(o: *mut ObjectWithFrame, f: *mut Frame) {
    unsafe {
        if !(*o).object.parent.is_null() {
            link_remove(&mut (*o).in_frame);
        }
        (*o).object.parent = f as *mut c_void;
        if !f.is_null() {
            list_add(&mut (*f).object_list, &mut (*o).in_frame);
            rwframe::update_objects(f, (hooks().dirty_list)());
        }
    }
}

/// Atomic::setFrame: also marks the world bounding sphere dirty.
///
/// # Safety
/// As set_frame.
pub unsafe fn atomic_set_frame(a: *mut Atomic, f: *mut Frame) {
    unsafe {
        set_frame(&mut (*a).object, f);
        (*a).object.object.private_flags |= WORLDBOUNDDIRTY;
    }
}

const ATOMIC: i32 = 0;
const CLUMP: i32 = 1;
const LIGHT: i32 = 2;
const CAMERA: i32 = 3;
const WORLD: i32 = 4;

// ---------------------------------------------------------------- Clump

/// Clump::create
///
/// # Safety
/// Hooks set.
pub unsafe fn clump_create() -> *mut Clump {
    unsafe {
        let c: *mut Clump = alloc(CLUMP);
        if c.is_null() {
            return c;
        }
        (*c).object = object_init(CLUMP_ID, 0);
        list_init(&mut (*c).atomics);
        list_init(&mut (*c).lights);
        list_init(&mut (*c).cameras);
        (*c).world = std::ptr::null_mut();
        link_init(&mut (*c).in_world);
        (hooks().construct)(CLUMP, c as *mut c_void);
        c
    }
}

/// Clump::clone: the frame hierarchy and the atomics (on the cloned frames); joins the world.
///
/// # Safety
/// A clump with a root frame whose atomics hang in that hierarchy.
pub unsafe fn clump_clone(src: *mut Clump) -> *mut Clump {
    unsafe {
        let c = clump_create();
        let frame = (*src).object.parent as *mut Frame;
        let root = rwframe::clone_and_link(frame, (hooks().dirty_list)());
        (*c).object.parent = root as *mut c_void;
        for_list(&mut (*src).atomics, |lnk| {
            let a = container_of!(lnk, Atomic, in_clump);
            let atomic = atomic_clone(a);
            atomic_set_frame(atomic, (*frame_of(&(*a).object)).root);
            clump_add_atomic(c, atomic);
        });
        rwframe::purge_clone(frame);
        if !(*src).world.is_null() {
            world_add_clump((*src).world, c);
        }
        (hooks().copy)(CLUMP, c as *mut c_void, src as *mut c_void);
        c
    }
}

/// Clump::destroy: its atomics, lights, cameras and frame hierarchy go too.
///
/// # Safety
/// A clump outside any world.
pub unsafe fn clump_destroy(c: *mut Clump) {
    unsafe {
        (hooks().destruct)(CLUMP, c as *mut c_void);
        for_list(&mut (*c).atomics, |lnk| {
            let a = container_of!(lnk, Atomic, in_clump);
            clump_remove_atomic(c, a);
            atomic_destroy(a);
        });
        for_list(&mut (*c).lights, |lnk| {
            let l = container_of!(lnk, Light, in_clump);
            clump_remove_light(c, l);
            light_destroy(l);
        });
        for_list(&mut (*c).cameras, |lnk| {
            let k = container_of!(lnk, Camera, in_clump);
            clump_remove_camera(c, k);
            camera_destroy(k);
        });
        let f = (*c).object.parent as *mut Frame;
        if !f.is_null() {
            rwframe::destroy_hierarchy(f);
        }
        debug_assert!((*c).world.is_null());
        free(CLUMP, c);
    }
}

/// Clump::addAtomic
///
/// # Safety
/// Live objects; the atomic in no clump.
pub unsafe fn clump_add_atomic(c: *mut Clump, a: *mut Atomic) {
    unsafe {
        debug_assert!((*a).clump.is_null());
        (*a).clump = c;
        list_append(&mut (*c).atomics, &mut (*a).in_clump);
    }
}

/// Clump::removeAtomic
///
/// # Safety
/// The atomic in this clump.
pub unsafe fn clump_remove_atomic(c: *mut Clump, a: *mut Atomic) {
    unsafe {
        debug_assert!((*a).clump == c);
        link_remove(&mut (*a).in_clump);
        (*a).clump = std::ptr::null_mut();
    }
}

/// Clump::addLight
///
/// # Safety
/// Live objects; the light in no clump.
pub unsafe fn clump_add_light(c: *mut Clump, l: *mut Light) {
    unsafe {
        debug_assert!((*l).clump.is_null());
        (*l).clump = c;
        list_append(&mut (*c).lights, &mut (*l).in_clump);
    }
}

/// Clump::removeLight
///
/// # Safety
/// The light in this clump.
pub unsafe fn clump_remove_light(c: *mut Clump, l: *mut Light) {
    unsafe {
        debug_assert!((*l).clump == c);
        link_remove(&mut (*l).in_clump);
        (*l).clump = std::ptr::null_mut();
    }
}

/// Clump::addCamera
///
/// # Safety
/// Live objects; the camera in no clump.
pub unsafe fn clump_add_camera(c: *mut Clump, k: *mut Camera) {
    unsafe {
        debug_assert!((*k).clump.is_null());
        (*k).clump = c;
        list_append(&mut (*c).cameras, &mut (*k).in_clump);
    }
}

/// Clump::removeCamera
///
/// # Safety
/// The camera in this clump.
pub unsafe fn clump_remove_camera(c: *mut Clump, k: *mut Camera) {
    unsafe {
        debug_assert!((*k).clump == c);
        link_remove(&mut (*k).in_clump);
        (*k).clump = std::ptr::null_mut();
    }
}

/// Clump::render: every atomic with the RENDER flag, through its render callback.
///
/// # Safety
/// A live clump.
pub unsafe fn clump_render(c: *mut Clump) {
    unsafe {
        for_list(&mut (*c).atomics, |lnk| {
            let a = container_of!(lnk, Atomic, in_clump);
            if (*a).object.object.flags & RENDER != 0 {
                ((*a).render_cb.expect("atomic without render callback"))(a);
            }
        });
    }
}

// ---------------------------------------------------------------- Atomic

/// atomicSync: the frame moved, so the world bounding sphere is stale.
///
/// # Safety
/// An atomic.
pub unsafe extern "C" fn atomic_sync(obj: *mut ObjectWithFrame) {
    unsafe { (*obj).object.private_flags |= WORLDBOUNDDIRTY }
}

/// worldAtomicSync
///
/// # Safety
/// An atomic.
pub unsafe extern "C" fn world_atomic_sync(obj: *mut ObjectWithFrame) {
    unsafe { ((*(obj as *mut Atomic)).original_sync.expect("atomic without sync"))(obj) }
}

/// Atomic::create (the render callback is set by the C++ wrapper).
///
/// # Safety
/// Hooks set.
pub unsafe fn atomic_create() -> *mut Atomic {
    unsafe {
        let a: *mut Atomic = alloc(ATOMIC);
        if a.is_null() {
            return a;
        }
        let at = &mut *a;
        at.object.object = object_init(ATOMIC_ID, 0);
        at.object.sync_cb = atomic_sync;
        at.geometry = std::ptr::null_mut();
        at.bounding_sphere = Sphere { center: V3 { x: 0.0, y: 0.0, z: 0.0 }, radius: 0.0 };
        at.world_bounding_sphere = Sphere { center: V3 { x: 0.0, y: 0.0, z: 0.0 }, radius: 0.0 };
        atomic_set_frame(a, std::ptr::null_mut());
        let at = &mut *a;
        at.object.object.private_flags |= WORLDBOUNDDIRTY;
        at.clump = std::ptr::null_mut();
        link_init(&mut at.in_clump);
        at.pipeline = std::ptr::null_mut();
        at.render_cb = None;
        at.object.object.flags = COLLISIONTEST | RENDER;
        at.world = std::ptr::null_mut();
        at.original_sync = Some(at.object.sync_cb);
        at.object.sync_cb = world_atomic_sync;
        (hooks().construct)(ATOMIC, a as *mut c_void);
        a
    }
}

/// Atomic::clone: same flags, geometry (another reference), render callback and pipeline;
/// no frame, clump or world.
///
/// # Safety
/// A live atomic.
pub unsafe fn atomic_clone(src: *mut Atomic) -> *mut Atomic {
    unsafe {
        let a = atomic_create();
        if a.is_null() {
            return a;
        }
        (*a).object.object = object_copy(&(*src).object.object);
        (*a).object.object.private_flags |= WORLDBOUNDDIRTY;
        if !(*src).geometry.is_null() {
            atomic_set_geometry(a, (*src).geometry, 0);
        }
        (*a).render_cb = (*src).render_cb;
        (*a).pipeline = (*src).pipeline;
        (hooks().copy)(ATOMIC, a as *mut c_void, src as *mut c_void);
        a
    }
}

/// Atomic::destroy
///
/// # Safety
/// A live atomic outside any clump and world.
pub unsafe fn atomic_destroy(a: *mut Atomic) {
    unsafe {
        (hooks().destruct)(ATOMIC, a as *mut c_void);
        if !(*a).geometry.is_null() {
            geo_destroy((*a).geometry);
        }
        debug_assert!((*a).clump.is_null());
        debug_assert!((*a).world.is_null());
        atomic_set_frame(a, std::ptr::null_mut());
        free(ATOMIC, a);
    }
}

/// Atomic::setGeometry: takes a reference, and (unless SAMEBOUNDINGSPHERE) the first morph
/// target's bounding sphere.
///
/// # Safety
/// A live atomic; `geo` live or null.
pub unsafe fn atomic_set_geometry(a: *mut Atomic, geo: *mut Geometry, flags: u32) {
    unsafe {
        if !(*a).geometry.is_null() {
            geo_destroy((*a).geometry);
        }
        if !geo.is_null() {
            (*geo).ref_count += 1;
        }
        (*a).geometry = geo;
        if flags & SAMEBOUNDINGSPHERE != 0 {
            return;
        }
        if !geo.is_null() {
            (*a).bounding_sphere = (*(*geo).morph_targets).bounding_sphere;
            let f = frame_of(&(*a).object);
            if !f.is_null() {
                rwframe::update_objects(f, (hooks().dirty_list)());
            }
        }
    }
}

/// Atomic::getWorldBoundingSphere: recomputed (centre through the LTM, no scaling) when the
/// frame hierarchy or the sphere is dirty.
///
/// # Safety
/// A live atomic with a frame.
pub unsafe fn atomic_get_world_bounding_sphere(a: *mut Atomic) -> *mut Sphere {
    unsafe {
        let s: *mut Sphere = &mut (*a).world_bounding_sphere;
        let f = frame_of(&(*a).object);
        let dirty = (*(*f).root).object.private_flags & rwframe::HIERARCHYSYNC != 0;
        if !dirty && (*a).object.object.private_flags & WORLDBOUNDDIRTY == 0 {
            return s;
        }
        let ltm = rwframe::get_ltm(f);
        (*s).center = transform_point(&(*a).bounding_sphere.center, &*ltm);
        (*s).radius = (*a).bounding_sphere.radius;
        (*a).object.object.private_flags &= !WORLDBOUNDDIRTY;
        s
    }
}

// ---------------------------------------------------------------- Light

/// lightSync: nothing.
///
/// # Safety
/// None.
pub unsafe extern "C" fn light_sync(_: *mut ObjectWithFrame) {}

/// worldLightSync
///
/// # Safety
/// A light.
pub unsafe extern "C" fn world_light_sync(obj: *mut ObjectWithFrame) {
    unsafe { ((*(obj as *mut Light)).original_sync.expect("light without sync"))(obj) }
}

/// Light::create: white, radius 0, lights atomics and the world.
///
/// # Safety
/// Hooks set.
pub unsafe fn light_create(kind: i32) -> *mut Light {
    unsafe {
        let l: *mut Light = alloc(LIGHT);
        if l.is_null() {
            return l;
        }
        let li = &mut *l;
        li.object.object = object_init(LIGHT_ID, kind as u8);
        li.object.sync_cb = light_sync;
        li.radius = 0.0;
        li.color = RgbaF { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 };
        li.minus_cos_angle = 1.0;
        li.object.object.private_flags = 1;
        li.object.object.flags = LIGHTATOMICS | LIGHTWORLD;
        link_init(&mut li.in_world);
        li.clump = std::ptr::null_mut();
        link_init(&mut li.in_clump);
        li.world = std::ptr::null_mut();
        li.original_sync = Some(li.object.sync_cb);
        li.object.sync_cb = world_light_sync;
        (hooks().construct)(LIGHT, l as *mut c_void);
        l
    }
}

/// Light::destroy
///
/// # Safety
/// A live light outside any clump and world.
pub unsafe fn light_destroy(l: *mut Light) {
    unsafe {
        (hooks().destruct)(LIGHT, l as *mut c_void);
        debug_assert!((*l).clump.is_null());
        debug_assert!((*l).world.is_null());
        set_frame(&mut (*l).object, std::ptr::null_mut());
        free(LIGHT, l);
    }
}

/// Light::setAngle
///
/// # Safety
/// A live light.
pub unsafe fn light_set_angle(l: *mut Light, angle: f32) {
    unsafe { (*l).minus_cos_angle = -angle.cos() }
}

/// Light::getAngle
///
/// # Safety
/// A live light.
pub unsafe fn light_get_angle(l: *const Light) -> f32 {
    unsafe { (-(*l).minus_cos_angle).acos() }
}

/// Light::setColor: the private flag says whether the colour is grey.
///
/// # Safety
/// A live light.
pub unsafe fn light_set_color(l: *mut Light, r: f32, g: f32, b: f32) {
    unsafe {
        (*l).color.red = r;
        (*l).color.green = g;
        (*l).color.blue = b;
        (*l).object.object.private_flags = (r == g && r == b) as u8;
    }
}

// ---------------------------------------------------------------- World

/// World::create
///
/// # Safety
/// Hooks set.
pub unsafe fn world_create() -> *mut World {
    unsafe {
        let w: *mut World = alloc(WORLD);
        if w.is_null() {
            return w;
        }
        (*w).object = object_init(WORLD_ID, 0);
        list_init(&mut (*w).local_lights);
        list_init(&mut (*w).global_lights);
        list_init(&mut (*w).clumps);
        (hooks().construct)(WORLD, w as *mut c_void);
        w
    }
}

/// World::destroy (whatever is in it keeps pointing at it, as in librw)
///
/// # Safety
/// A live world.
pub unsafe fn world_destroy(w: *mut World) {
    unsafe {
        (hooks().destruct)(WORLD, w as *mut c_void);
        free(WORLD, w);
    }
}

/// World::addLight: ambient and directional lights are global; the others local (and synced).
///
/// # Safety
/// Live objects; the light in no world.
pub unsafe fn world_add_light(w: *mut World, l: *mut Light) {
    unsafe {
        debug_assert!((*l).world.is_null());
        (*l).world = w;
        if ((*l).object.object.sub_type as i32) < POINT {
            list_append(&mut (*w).global_lights, &mut (*l).in_world);
        } else {
            list_append(&mut (*w).local_lights, &mut (*l).in_world);
            let f = frame_of(&(*l).object);
            if !f.is_null() {
                rwframe::update_objects(f, (hooks().dirty_list)());
            }
        }
    }
}

/// World::removeLight
///
/// # Safety
/// The light in this world.
pub unsafe fn world_remove_light(w: *mut World, l: *mut Light) {
    unsafe {
        debug_assert!((*l).world == w);
        link_remove(&mut (*l).in_world);
        (*l).world = std::ptr::null_mut();
    }
}

/// World::addCamera
///
/// # Safety
/// Live objects; the camera in no world.
pub unsafe fn world_add_camera(w: *mut World, k: *mut Camera) {
    unsafe {
        debug_assert!((*k).world.is_null());
        (*k).world = w;
        let f = frame_of(&(*k).object);
        if !f.is_null() {
            rwframe::update_objects(f, (hooks().dirty_list)());
        }
    }
}

/// World::removeCamera
///
/// # Safety
/// The camera in this world.
pub unsafe fn world_remove_camera(w: *mut World, k: *mut Camera) {
    unsafe {
        debug_assert!((*k).world == w);
        (*k).world = std::ptr::null_mut();
    }
}

/// World::addAtomic
///
/// # Safety
/// Live objects; the atomic in no world.
pub unsafe fn world_add_atomic(w: *mut World, a: *mut Atomic) {
    unsafe {
        debug_assert!((*a).world.is_null());
        (*a).world = w;
        let f = frame_of(&(*a).object);
        if !f.is_null() {
            rwframe::update_objects(f, (hooks().dirty_list)());
        }
    }
}

/// World::removeAtomic
///
/// # Safety
/// The atomic in this world.
pub unsafe fn world_remove_atomic(w: *mut World, a: *mut Atomic) {
    unsafe {
        debug_assert!((*a).world == w);
        (*a).world = std::ptr::null_mut();
    }
}

/// World::addClump: at the front of the world's clumps, with its atomics, lights and cameras;
/// the clump frame's matrix is re-optimized and synced.
///
/// # Safety
/// Live objects; the clump in no world.
pub unsafe fn world_add_clump(w: *mut World, c: *mut Clump) {
    unsafe {
        debug_assert!((*c).world.is_null());
        (*c).world = w;
        list_add(&mut (*w).clumps, &mut (*c).in_world);
        for_list(&mut (*c).atomics, |lnk| world_add_atomic(w, container_of!(lnk, Atomic, in_clump)));
        for_list(&mut (*c).lights, |lnk| world_add_light(w, container_of!(lnk, Light, in_clump)));
        for_list(&mut (*c).cameras, |lnk| world_add_camera(w, container_of!(lnk, Camera, in_clump)));
        let f = (*c).object.parent as *mut Frame;
        if !f.is_null() {
            rwmatrix::optimize(&mut (*f).matrix, std::ptr::null());
            rwframe::update_objects(f, (hooks().dirty_list)());
        }
    }
}

/// World::removeClump
///
/// # Safety
/// The clump in this world.
pub unsafe fn world_remove_clump(w: *mut World, c: *mut Clump) {
    unsafe {
        debug_assert!((*c).world == w);
        link_remove(&mut (*c).in_world);
        for_list(&mut (*c).atomics, |lnk| world_remove_atomic(w, container_of!(lnk, Atomic, in_clump)));
        for_list(&mut (*c).lights, |lnk| world_remove_light(w, container_of!(lnk, Light, in_clump)));
        for_list(&mut (*c).cameras, |lnk| world_remove_camera(w, container_of!(lnk, Camera, in_clump)));
        (*c).world = std::ptr::null_mut();
    }
}

/// World::render: every clump, newest first.
///
/// # Safety
/// A live world.
pub unsafe fn world_render(w: *mut World) {
    unsafe { for_list(&mut (*w).clumps, |lnk| clump_render(container_of!(lnk, Clump, in_world))) }
}

/// World::enumerateLights: the ambient sum, the directional lights (when the geometry has
/// normals) and, for an atomic in this world, the local lights whose spheres touch its world
/// bounding sphere; at most as many as the arrays' counts on entry.
///
/// # Safety
/// A live world and atomic with a geometry; arrays as big as the counts.
pub unsafe fn world_enumerate_lights(w: *mut World, a: *mut Atomic, ld: *mut WorldLights) {
    unsafe {
        let ld = &mut *ld;
        let max_directionals = ld.num_directionals;
        let max_locals = ld.num_locals;
        ld.num_directionals = 0;
        ld.num_locals = 0;
        ld.num_ambients = 0;
        ld.ambient = RgbaF { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 };
        let normals = (*(*a).geometry).flags & NORMALS != 0;
        for_list(&mut (*w).global_lights, |lnk| {
            let l = &*container_of!(lnk, Light, in_world);
            if l.object.object.flags & LIGHTATOMICS == 0 {
                return;
            }
            let kind = l.object.object.sub_type as i32;
            if kind == AMBIENT {
                ld.ambient.red += l.color.red;
                ld.ambient.green += l.color.green;
                ld.ambient.blue += l.color.blue;
                ld.num_ambients += 1;
            } else if normals && kind == DIRECTIONAL && ld.num_directionals < max_directionals {
                *ld.directionals.add(ld.num_directionals as usize) = l as *const Light as *mut Light;
                ld.num_directionals += 1;
            }
        });
        if (*a).world != w || !normals {
            return;
        }
        let end: *mut LLLink = &mut (*w).local_lights.link;
        let mut lnk = (*end).next;
        while lnk != end {
            if ld.num_locals >= max_locals {
                return;
            }
            let l = container_of!(lnk, Light, in_world);
            lnk = (*lnk).next;
            if (*l).object.object.flags & LIGHTATOMICS == 0 {
                continue;
            }
            let s = &*atomic_get_world_bounding_sphere(a);
            let pos = (*rwframe::get_ltm(frame_of(&(*l).object))).pos;
            let d = V3 { x: pos.x - s.center.x, y: pos.y - s.center.y, z: pos.z - s.center.z };
            if (d.x * d.x + d.y * d.y + d.z * d.z).sqrt() < s.radius + (*l).radius {
                *ld.locals.add(ld.num_locals as usize) = l;
                ld.num_locals += 1;
            }
        }
    }
}

// ---------------------------------------------------------------- Camera

/// worldCameraSync
///
/// # Safety
/// A camera.
pub unsafe extern "C" fn world_camera_sync(obj: *mut ObjectWithFrame) {
    unsafe { ((*(obj as *mut Camera)).original_sync.expect("camera without sync"))(obj) }
}

/// Camera::create: 1x1 view window, near 0.05, far 10, fog 5, perspective; the device's
/// update callbacks wrapped by the world's.
///
/// # Safety
/// Hooks set.
pub unsafe fn camera_create() -> *mut Camera {
    unsafe {
        let h = hooks();
        let k: *mut Camera = alloc(CAMERA);
        if k.is_null() {
            return k;
        }
        let c = &mut *k;
        c.object.object = object_init(CAMERA_ID, 0);
        c.object.sync_cb = h.camera_sync;
        c.begin_update_cb = Some(h.default_begin_update);
        c.end_update_cb = Some(h.default_end_update);
        c.view_window = [1.0, 1.0];
        c.view_offset = [0.0, 0.0];
        c.near_plane = 0.05;
        c.far_plane = 10.0;
        c.fog_plane = 5.0;
        c.projection = PERSPECTIVE;
        c.frame_buffer = std::ptr::null_mut();
        c.z_buffer = std::ptr::null_mut();
        c.clump = std::ptr::null_mut();
        link_init(&mut c.in_clump);
        c.world = std::ptr::null_mut();
        c.original_sync = Some(c.object.sync_cb);
        c.original_begin_update = c.begin_update_cb;
        c.original_end_update = c.end_update_cb;
        c.object.sync_cb = world_camera_sync;
        c.begin_update_cb = Some(h.world_begin_update);
        c.end_update_cb = Some(h.world_end_update);
        (h.construct)(CAMERA, k as *mut c_void);
        k
    }
}

/// Camera::clone: same frame, view, planes, projection and buffers; joins the world.
///
/// # Safety
/// A live camera.
pub unsafe fn camera_clone(src: *mut Camera) -> *mut Camera {
    unsafe {
        let k = camera_create();
        if k.is_null() {
            return k;
        }
        (*k).object.object = object_copy(&(*src).object.object);
        set_frame(&mut (*k).object, frame_of(&(*src).object));
        let (c, s) = (&mut *k, &*src);
        c.view_window = s.view_window;
        c.view_offset = s.view_offset;
        c.near_plane = s.near_plane;
        c.far_plane = s.far_plane;
        c.fog_plane = s.fog_plane;
        c.projection = s.projection;
        c.frame_buffer = s.frame_buffer;
        c.z_buffer = s.z_buffer;
        if !s.world.is_null() {
            world_add_camera(s.world, k);
        }
        (hooks().copy)(CAMERA, k as *mut c_void, src as *mut c_void);
        k
    }
}

/// Camera::destroy
///
/// # Safety
/// A live camera outside any clump and world.
pub unsafe fn camera_destroy(k: *mut Camera) {
    unsafe {
        (hooks().destruct)(CAMERA, k as *mut c_void);
        debug_assert!((*k).clump.is_null());
        debug_assert!((*k).world.is_null());
        set_frame(&mut (*k).object, std::ptr::null_mut());
        free(CAMERA, k);
    }
}

// ---------------------------------------------------------------- C ABI

/// # Safety
/// Called once at startup, before any of these objects exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_scene_set_hooks(h: Hooks) {
    unsafe { *std::ptr::addr_of_mut!(HOOKS) = Some(h) };
}

macro_rules! ffi {
    ($($name:ident ( $($a:ident : $t:ty),* ) $(-> $r:ty)? => $body:expr;)*) => {
        $(
            /// # Safety
            /// As the Rust function it forwards to.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name($($a: $t),*) $(-> $r)? { unsafe { $body } }
        )*
    };
}

ffi! {
    revc_clump_create() -> *mut Clump => clump_create();
    revc_clump_clone(c: *mut Clump) -> *mut Clump => clump_clone(c);
    revc_clump_destroy(c: *mut Clump) => clump_destroy(c);
    revc_clump_add_atomic(c: *mut Clump, a: *mut Atomic) => clump_add_atomic(c, a);
    revc_clump_remove_atomic(c: *mut Clump, a: *mut Atomic) => clump_remove_atomic(c, a);
    revc_clump_add_light(c: *mut Clump, l: *mut Light) => clump_add_light(c, l);
    revc_clump_remove_light(c: *mut Clump, l: *mut Light) => clump_remove_light(c, l);
    revc_clump_add_camera(c: *mut Clump, k: *mut Camera) => clump_add_camera(c, k);
    revc_clump_remove_camera(c: *mut Clump, k: *mut Camera) => clump_remove_camera(c, k);
    revc_clump_render(c: *mut Clump) => clump_render(c);
    revc_atomic_create() -> *mut Atomic => atomic_create();
    revc_atomic_clone(a: *mut Atomic) -> *mut Atomic => atomic_clone(a);
    revc_atomic_destroy(a: *mut Atomic) => atomic_destroy(a);
    revc_atomic_set_geometry(a: *mut Atomic, g: *mut Geometry, flags: u32) => atomic_set_geometry(a, g, flags);
    revc_atomic_get_world_bounding_sphere(a: *mut Atomic) -> *mut Sphere => atomic_get_world_bounding_sphere(a);
    revc_light_create(kind: i32) -> *mut Light => light_create(kind);
    revc_light_destroy(l: *mut Light) => light_destroy(l);
    revc_light_set_angle(l: *mut Light, a: f32) => light_set_angle(l, a);
    revc_light_get_angle(l: *const Light) -> f32 => light_get_angle(l);
    revc_light_set_color(l: *mut Light, r: f32, g: f32, b: f32) => light_set_color(l, r, g, b);
    revc_world_create() -> *mut World => world_create();
    revc_world_destroy(w: *mut World) => world_destroy(w);
    revc_world_add_light(w: *mut World, l: *mut Light) => world_add_light(w, l);
    revc_world_remove_light(w: *mut World, l: *mut Light) => world_remove_light(w, l);
    revc_world_add_camera(w: *mut World, k: *mut Camera) => world_add_camera(w, k);
    revc_world_remove_camera(w: *mut World, k: *mut Camera) => world_remove_camera(w, k);
    revc_world_add_atomic(w: *mut World, a: *mut Atomic) => world_add_atomic(w, a);
    revc_world_remove_atomic(w: *mut World, a: *mut Atomic) => world_remove_atomic(w, a);
    revc_world_add_clump(w: *mut World, c: *mut Clump) => world_add_clump(w, c);
    revc_world_remove_clump(w: *mut World, c: *mut Clump) => world_remove_clump(w, c);
    revc_world_render(w: *mut World) => world_render(w);
    revc_world_enumerate_lights(w: *mut World, a: *mut Atomic, ld: *mut WorldLights) => world_enumerate_lights(w, a, ld);
    revc_camera_create() -> *mut Camera => camera_create();
    revc_camera_clone(k: *mut Camera) -> *mut Camera => camera_clone(k);
    revc_camera_destroy(k: *mut Camera) => camera_destroy(k);
    revc_set_frame(o: *mut ObjectWithFrame, f: *mut Frame) => set_frame(o, f);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwscene.txt (recorded from librw's C++, see the recorder).
    use super::*;
    use crate::rwgeomobj::{Hooks as GeoHooks, Material, geo_calculate_bounding_sphere, geo_create, revc_geometry_set_hooks};
    use std::cell::RefCell;

    type Ids = RefCell<Vec<*mut c_void>>;
    thread_local! {
        // frames, geometries, atomics, clumps, lights, cameras, worlds
        static IDS: [Ids; 7] = const { [const { RefCell::new(Vec::new()) }; 7] };
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static DIRTY: RefCell<*mut LinkList> = const { RefCell::new(std::ptr::null_mut()) };
    }
    const F: usize = 0;
    const G: usize = 1;
    const A: usize = 2;
    const C: usize = 3;
    const L: usize = 4;
    const K: usize = 5;
    const W: usize = 6;
    const TAG: [&str; 7] = ["f", "g", "a", "c", "l", "k", "w"];

    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn id<T>(t: usize, p: *mut T) -> i32 {
        if p.is_null() {
            return -1;
        }
        IDS.with(|v| v[t].borrow().iter().position(|&q| q == p as *mut c_void).map_or(-2, |i| i as i32))
    }
    fn get<T>(t: usize, i: &str) -> *mut T {
        IDS.with(|v| v[t].borrow()[i.parse::<usize>().unwrap()] as *mut T)
    }
    fn live(t: usize) -> Vec<(usize, *mut c_void)> {
        IDS.with(|v| v[t].borrow().iter().copied().enumerate().filter(|(_, p)| !p.is_null()).collect())
    }
    fn reg(t: usize, p: *mut c_void) {
        let n = IDS.with(|v| {
            v[t].borrow_mut().push(p);
            v[t].borrow().len() - 1
        });
        log(format!(" c{}{n}", TAG[t]));
    }
    fn unreg(t: usize, p: *mut c_void) {
        let i = id(t, p);
        log(format!(" d{}{i}", TAG[t]));
        if i >= 0 {
            IDS.with(|v| v[t].borrow_mut()[i as usize] = std::ptr::null_mut());
        }
    }

    // librw's allocator as the recorder has it (zeroed, size in front)
    unsafe extern "C" fn alloc_(n: usize) -> *mut u8 {
        unsafe {
            let p = std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(n + 16, 16).unwrap());
            *(p as *mut usize) = n;
            p.add(16)
        }
    }
    unsafe extern "C" fn free_(p: *mut u8) {
        if p.is_null() {
            return;
        }
        unsafe {
            let b = p.sub(16);
            std::alloc::dealloc(b, std::alloc::Layout::from_size_align(*(b as *mut usize) + 16, 16).unwrap());
        }
    }
    unsafe extern "C" fn realloc_(p: *mut u8, n: usize) -> *mut u8 {
        unsafe {
            let q = alloc_(n);
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(p, q, (*(p.sub(16) as *mut usize)).min(n));
                free_(p);
            }
            q
        }
    }
    // frames
    unsafe extern "C" fn f_size() -> usize {
        size_of::<Frame>()
    }
    unsafe extern "C" fn f_construct(f: *mut Frame) {
        reg(F, f as _);
    }
    unsafe extern "C" fn f_destruct(f: *mut Frame) {
        unreg(F, f as _);
    }
    unsafe extern "C" fn f_copy(d: *mut Frame, s: *mut Frame) {
        log(format!(" yf{},{}", id(F, d), id(F, s)));
    }
    unsafe extern "C" fn f_allocated(_: i32) {}
    // geometries
    unsafe extern "C" fn g_size() -> usize {
        size_of::<Geometry>()
    }
    unsafe extern "C" fn m_size() -> usize {
        size_of::<Material>()
    }
    unsafe extern "C" fn g_construct(g: *mut Geometry) {
        reg(G, g as _);
    }
    unsafe extern "C" fn g_destruct(g: *mut Geometry) {
        unreg(G, g as _);
    }
    unsafe extern "C" fn m_hook(_: *mut Material) {}
    unsafe extern "C" fn m_copy(_: *mut Material, _: *mut Material) {}
    unsafe extern "C" fn g_allocated(_: i32, _: i32) {}
    unsafe extern "C" fn never() {
        panic!("unexpected geometry callback");
    }
    // scene objects: kinds 0 atomic, 1 clump, 2 light, 3 camera, 4 world
    const KIND: [usize; 5] = [A, C, L, K, W];
    unsafe extern "C" fn s_size(kind: i32) -> usize {
        [size_of::<Atomic>(), size_of::<Clump>(), size_of::<Light>(), size_of::<Camera>(), size_of::<World>()][kind as usize]
    }
    unsafe extern "C" fn s_construct(kind: i32, o: *mut c_void) {
        reg(KIND[kind as usize], o);
    }
    unsafe extern "C" fn s_destruct(kind: i32, o: *mut c_void) {
        unreg(KIND[kind as usize], o);
    }
    unsafe extern "C" fn s_copy(kind: i32, d: *mut c_void, s: *mut c_void) {
        let t = KIND[kind as usize];
        if t != L && t != W {
            log(format!(" y{}{},{}", TAG[t], id(t, d), id(t, s)));
        }
    }
    unsafe extern "C" fn s_allocated(_: i32, _: i32) {}
    /// camera.cpp's cameraSync reads the frame's LTM (which can sync a hierarchy left with stale
    /// flags by a clone); the matrices it computes aren't part of the hash
    unsafe extern "C" fn camera_sync(o: *mut ObjectWithFrame) {
        unsafe {
            rwframe::get_ltm((*o).object.parent as *mut Frame);
        }
    }
    unsafe extern "C" fn camera_cb(_: *mut Camera) {
        panic!("camera update callback");
    }
    unsafe extern "C" fn dirty_list() -> *mut LinkList {
        DIRTY.with(|d| *d.borrow())
    }
    unsafe extern "C" fn rec_render(a: *mut Atomic) {
        log(format!(" r{}", id(A, a)));
    }

    struct H(u64);
    impl H {
        fn b(&mut self, b: u8) {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
        fn i(&mut self, v: i32) {
            for k in 0..4 {
                self.b((v >> (8 * k)) as u8);
            }
        }
        fn bytes<T>(&mut self, p: &T, n: usize) {
            let p = p as *const T as *const u8;
            for k in 0..n {
                self.b(unsafe { *p.add(k) });
            }
        }
        fn obj(&mut self, o: &Object) {
            for b in [o.kind, o.sub_type, o.flags, o.private_flags] {
                self.b(b);
            }
        }
        fn m(&mut self, m: &RwMatrix) {
            self.bytes(&m.right, 12);
            self.i(m.flags as i32);
            self.bytes(&m.up, 12);
            self.bytes(&m.at, 12);
            self.bytes(&m.pos, 12);
        }
        fn owf(&mut self, o: &ObjectWithFrame) {
            self.obj(&o.object);
            self.i(id(F, o.object.parent));
            self.i(1); // syncCB != nil
        }
        unsafe fn list<T>(&mut self, list: *mut LinkList, off: usize, t: usize) {
            let mut n = 0;
            unsafe {
                for_list(list, |lnk| {
                    self.i(id(t, (lnk as *mut u8).sub(off) as *mut T));
                    n += 1;
                });
            }
            self.i(n);
        }
    }

    unsafe fn state() -> String {
        unsafe {
            let mut h = H(0xcbf29ce484222325);
            for (i, p) in live(F) {
                let f = &mut *(p as *mut Frame);
                h.i(i as i32);
                h.obj(&f.object);
                h.i(id(F, f.object.parent));
                h.i(id(F, f.child));
                h.i(id(F, f.next));
                h.i(id(F, f.root));
                h.m(&f.matrix);
                h.m(&f.ltm);
                let mut n = 0;
                for_list(&mut f.object_list, |lnk| {
                    let o = container_of!(lnk, ObjectWithFrame, in_frame);
                    let t = (*o).object.kind;
                    h.i(t as i32);
                    h.i(match t {
                        ATOMIC_ID => id(A, o),
                        LIGHT_ID => id(L, o),
                        CAMERA_ID => id(K, o),
                        _ => -3,
                    });
                    n += 1;
                });
                h.i(n);
            }
            for (i, p) in live(G) {
                h.i(i as i32);
                h.i((*(p as *mut Geometry)).ref_count);
            }
            for (i, p) in live(A) {
                let a = &*(p as *mut Atomic);
                h.i(i as i32);
                h.owf(&a.object);
                h.i(id(G, a.geometry));
                h.bytes(&a.bounding_sphere, 16);
                h.bytes(&a.world_bounding_sphere, 16);
                h.i(id(C, a.clump));
                h.i(!a.pipeline.is_null() as i32);
                h.i(a.render_cb.is_some_and(|f| f as usize == rec_render as usize) as i32);
                h.i(id(W, a.world));
                h.i(a.original_sync.is_some() as i32);
            }
            for (i, p) in live(C) {
                let c = &mut *(p as *mut Clump);
                h.i(i as i32);
                h.obj(&c.object);
                h.i(id(F, c.object.parent));
                h.i(id(W, c.world));
                h.list::<Atomic>(&mut c.atomics, std::mem::offset_of!(Atomic, in_clump), A);
                h.list::<Light>(&mut c.lights, std::mem::offset_of!(Light, in_clump), L);
                h.list::<Camera>(&mut c.cameras, std::mem::offset_of!(Camera, in_clump), K);
            }
            for (i, p) in live(L) {
                let l = &*(p as *mut Light);
                h.i(i as i32);
                h.owf(&l.object);
                h.bytes(&l.radius, 4);
                h.bytes(&l.color, 16);
                h.bytes(&l.minus_cos_angle, 4);
                h.i(id(C, l.clump));
                h.i(id(W, l.world));
                h.i(l.original_sync.is_some() as i32);
            }
            for (i, p) in live(K) {
                let k = &*(p as *mut Camera);
                h.i(i as i32);
                h.owf(&k.object);
                h.i(k.begin_update_cb.is_some() as i32);
                h.i(k.end_update_cb.is_some() as i32);
                h.bytes(&k.view_window, 8);
                h.bytes(&k.view_offset, 8);
                h.bytes(&k.near_plane, 4);
                h.bytes(&k.far_plane, 4);
                h.bytes(&k.fog_plane, 4);
                h.i(k.projection);
                h.i(!k.frame_buffer.is_null() as i32);
                h.i(!k.z_buffer.is_null() as i32);
                h.i(id(C, k.clump));
                h.i(id(W, k.world));
                h.i(k.original_sync.is_some() as i32);
                h.i(k.original_begin_update.is_some() as i32);
                h.i(k.original_end_update.is_some() as i32);
            }
            for (i, p) in live(W) {
                let w = &mut *(p as *mut World);
                h.i(i as i32);
                h.obj(&w.object);
                h.list::<Light>(&mut w.local_lights, std::mem::offset_of!(Light, in_world), L);
                h.list::<Light>(&mut w.global_lights, std::mem::offset_of!(Light, in_world), L);
                h.list::<Clump>(&mut w.clumps, std::mem::offset_of!(Clump, in_world), C);
            }
            let dirty = DIRTY.with(|d| *d.borrow());
            for_list(dirty, |lnk| h.i(id(F, container_of!(lnk, Frame, in_dirty_list))));
            let l = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            format!(" {:016x} |{l}", h.0)
        }
    }

    fn hexf(s: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(s, 16).unwrap())
    }
    fn hx(v: f32) -> String {
        format!("{:08x}", v.to_bits())
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwscene.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwscene.txt");
        let dirty: *mut LinkList = Box::into_raw(Box::new(unsafe { std::mem::zeroed() }));
        unsafe {
            list_init(dirty);
            DIRTY.with(|d| *d.borrow_mut() = dirty);
            rwframe::revc_frame_set_hooks(rwframe::Hooks { size: f_size, construct: f_construct, destruct: f_destruct, copy: f_copy, allocated: f_allocated });
            revc_geometry_set_hooks(GeoHooks {
                alloc: alloc_,
                realloc: realloc_,
                free: free_,
                geo_size: g_size,
                geo_construct: g_construct,
                geo_destruct: g_destruct,
                mat_size: m_size,
                mat_construct: m_hook,
                mat_destruct: m_hook,
                mat_copy: m_copy,
                allocated: g_allocated,
                native_warning: never,
                verify_failed: never,
            });
            revc_scene_set_hooks(Hooks {
                alloc: alloc_,
                free: free_,
                size: s_size,
                construct: s_construct,
                destruct: s_destruct,
                copy: s_copy,
                allocated: s_allocated,
                camera_sync,
                default_begin_update: camera_cb,
                default_end_update: camera_cb,
                world_begin_update: camera_cb,
                world_end_update: camera_cb,
                dirty_list: dirty_list,
            });
        }
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let (args, expect) = input.split_once(" =").map_or((input, None), |(a, r)| (a, Some(r.trim().to_string())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let opt_frame = |s: &str| if s == "-1" { std::ptr::null_mut() } else { get::<Frame>(F, s) };
            let res: Option<String> = unsafe {
                match w[0] {
                    "newframe" => {
                        let f = rwframe::create();
                        let v = V3 { x: hexf(w[1]), y: hexf(w[2]), z: hexf(w[3]) };
                        rwframe::revc_frame_modify(f, 1, &v, 0.0, std::ptr::null(), 0, dirty);
                        Some(id(F, f).to_string())
                    }
                    "addchild" => {
                        rwframe::add_child(get(F, w[1]), get(F, w[2]), false, dirty);
                        None
                    }
                    "move" => {
                        let v = V3 { x: hexf(w[2]), y: hexf(w[3]), z: hexf(w[4]) };
                        rwframe::revc_frame_modify(get(F, w[1]), 1, &v, 0.0, std::ptr::null(), 2, dirty);
                        None
                    }
                    "syncdirty" => {
                        rwframe::sync_dirty(dirty);
                        None
                    }
                    "newgeo" => {
                        let nv: i32 = w[1].parse().unwrap();
                        let g = geo_create(nv, 0, 0);
                        let mut s = w[2].parse::<u32>().unwrap() | 1;
                        let vs = (*(*g).morph_targets).vertices as *mut f32;
                        for i in 0..(nv * 3) as usize {
                            s ^= s << 13;
                            s ^= s >> 17;
                            s ^= s << 5;
                            *vs.add(i) = (s % 2001).wrapping_sub(1000) as i32 as f32 / 100.0;
                        }
                        geo_calculate_bounding_sphere(g);
                        Some(id(G, g).to_string())
                    }
                    "delgeo" => {
                        geo_destroy(get(G, w[1]));
                        None
                    }
                    "newatomic" => {
                        let a = atomic_create();
                        (*a).render_cb = Some(rec_render);
                        Some(id(A, a).to_string())
                    }
                    "atomicframe" => {
                        atomic_set_frame(get(A, w[1]), opt_frame(w[2]));
                        None
                    }
                    "setgeo" => {
                        let g = if w[2] == "-1" { std::ptr::null_mut() } else { get(G, w[2]) };
                        atomic_set_geometry(get(A, w[1]), g, w[3].parse().unwrap());
                        None
                    }
                    "cloneatomic" => Some(id(A, atomic_clone(get(A, w[1]))).to_string()),
                    "delatomic" => {
                        atomic_destroy(get(A, w[1]));
                        None
                    }
                    "wbs" => {
                        let s = &*atomic_get_world_bounding_sphere(get(A, w[1]));
                        Some(format!("{} {} {} {}", hx(s.center.x), hx(s.center.y), hx(s.center.z), hx(s.radius)))
                    }
                    "newclump" => Some(id(C, clump_create()).to_string()),
                    "clumpframe" => {
                        (*get::<Clump>(C, w[1])).object.parent = opt_frame(w[2]) as *mut c_void;
                        None
                    }
                    "addatomic" | "removeatomic" | "addlight" | "removelight" | "addcamera" | "removecamera" => {
                        let c = get(C, w[1]);
                        match w[0] {
                            "addatomic" => clump_add_atomic(c, get(A, w[2])),
                            "removeatomic" => clump_remove_atomic(c, get(A, w[2])),
                            "addlight" => clump_add_light(c, get(L, w[2])),
                            "removelight" => clump_remove_light(c, get(L, w[2])),
                            "addcamera" => clump_add_camera(c, get(K, w[2])),
                            _ => clump_remove_camera(c, get(K, w[2])),
                        }
                        None
                    }
                    "cloneclump" => Some(id(C, clump_clone(get(C, w[1]))).to_string()),
                    "delclump" => {
                        clump_destroy(get(C, w[1]));
                        None
                    }
                    "render" => {
                        clump_render(get(C, w[1]));
                        None
                    }
                    "newlight" => Some(id(L, light_create(w[1].parse().unwrap())).to_string()),
                    "lightframe" => {
                        set_frame(&mut (*get::<Light>(L, w[1])).object, opt_frame(w[2]));
                        None
                    }
                    "color" => {
                        light_set_color(get(L, w[1]), hexf(w[2]), hexf(w[3]), hexf(w[4]));
                        None
                    }
                    "angle" => {
                        light_set_angle(get(L, w[1]), hexf(w[2]));
                        None
                    }
                    "getangle" => Some(hx(light_get_angle(get(L, w[1])))),
                    "radius" => {
                        (*get::<Light>(L, w[1])).radius = hexf(w[2]);
                        None
                    }
                    "dellight" => {
                        light_destroy(get(L, w[1]));
                        None
                    }
                    "newcamera" => Some(id(K, camera_create()).to_string()),
                    "cameraframe" => {
                        set_frame(&mut (*get::<Camera>(K, w[1])).object, opt_frame(w[2]));
                        None
                    }
                    "clonecamera" => Some(id(K, camera_clone(get(K, w[1]))).to_string()),
                    "delcamera" => {
                        camera_destroy(get(K, w[1]));
                        None
                    }
                    "newworld" => Some(id(W, world_create()).to_string()),
                    "waddclump" => {
                        world_add_clump(get(W, w[1]), get(C, w[2]));
                        None
                    }
                    "wremoveclump" => {
                        world_remove_clump(get(W, w[1]), get(C, w[2]));
                        None
                    }
                    "waddlight" => {
                        world_add_light(get(W, w[1]), get(L, w[2]));
                        None
                    }
                    "wremovelight" => {
                        world_remove_light(get(W, w[1]), get(L, w[2]));
                        None
                    }
                    "waddatomic" => {
                        world_add_atomic(get(W, w[1]), get(A, w[2]));
                        None
                    }
                    "wremoveatomic" => {
                        world_remove_atomic(get(W, w[1]), get(A, w[2]));
                        None
                    }
                    "waddcamera" => {
                        world_add_camera(get(W, w[1]), get(K, w[2]));
                        None
                    }
                    "wremovecamera" => {
                        world_remove_camera(get(W, w[1]), get(K, w[2]));
                        None
                    }
                    "wrender" => {
                        world_render(get(W, w[1]));
                        None
                    }
                    "enumlights" => {
                        let mut dirs = [std::ptr::null_mut::<Light>(); 8];
                        let mut locs = [std::ptr::null_mut::<Light>(); 8];
                        let mut wl = WorldLights {
                            num_ambients: 0,
                            ambient: RgbaF { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 },
                            num_directionals: w[3].parse().unwrap(),
                            directionals: dirs.as_mut_ptr(),
                            num_locals: w[4].parse().unwrap(),
                            locals: locs.as_mut_ptr(),
                        };
                        world_enumerate_lights(get(W, w[1]), get(A, w[2]), &mut wl);
                        let mut o = format!("{} {} {} {} {} {}", wl.num_ambients, hx(wl.ambient.red), hx(wl.ambient.green), hx(wl.ambient.blue), hx(wl.ambient.alpha), wl.num_directionals);
                        for d in &dirs[..wl.num_directionals as usize] {
                            o += &format!(" {}", id(L, *d));
                        }
                        o += &format!(" {}", wl.num_locals);
                        for l in &locs[..wl.num_locals as usize] {
                            o += &format!(" {}", id(L, *l));
                        }
                        Some(o)
                    }
                    "delworld" => {
                        world_destroy(get(W, w[1]));
                        None
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            if res != expect && bad.len() < 6 {
                bad.push(format!("{input}: result {res:?}"));
            }
            let got = unsafe { state() };
            n += 1;
            if got != want && bad.len() < 6 {
                bad.push(format!("line {} {input}\n  want{want}\n  got {got}", n));
            }
        }
        assert!(n > 4000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}


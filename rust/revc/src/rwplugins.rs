//! librw's plugin data, stage 2 step 6c: hanim.cpp's HAnimHierarchy object (create, destroy,
//! attach, the index lookups, get and find) and its frame plugin (create, destroy, copy);
//! anim.cpp's AnimInterpolator create/destroy; skin.cpp's Skin data (init, findNumWeights,
//! findUsedBones, the geometry plugin); matfx.cpp's effect data (setEffects, every setter and
//! getter, the material and atomic plugins). The structures keep librw's layouts; memory comes
//! from librw's allocator and the plugin offsets from its plugin lists (hooks). Choosing the skin
//! and MatFX pipelines stays C++ with the pipelines (step 7).
//!
//! The local model drafted the hierarchy part (its skin and MatFX part ran out of tokens); the
//! module was written and checked by hand against tests/golden/rwplugins.txt, recorded from the
//! original C++ (tests/golden/recorders/rwplugins).

use std::ffi::c_void;

use crate::math::RwMatrix;
use crate::rwframe::Frame;
use crate::rwgeomobj::{Geometry, Material};
use crate::rwhanim::{AnimInterpolator, HAnimHierarchy};
use crate::rwtexture::{Texture, tex_destroy};

/// rw::HAnimData (the frame plugin)
#[repr(C)]
pub struct HAnimData {
    pub id: i32,
    pub hierarchy: *mut HAnimHierarchy,
}

/// rw::Skin
#[repr(C)]
pub struct Skin {
    pub num_bones: i32,
    pub num_used_bones: i32,
    pub num_weights: i32,
    pub used_bones: *mut u8,
    pub inverse_matrices: *mut f32,
    pub indices: *mut u8,
    pub weights: *mut f32,
    pub bone_limit: i32,
    pub num_meshes: i32,
    pub rle_size: i32,
    pub remap_indices: *mut i8,
    pub rle_count: *mut c_void,
    pub rle: *mut c_void,
    pub data: *mut u8,
    pub platform_data: *mut c_void,
    pub legacy_type: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bump {
    pub frame: *mut Frame,
    pub bumped_tex: *mut Texture,
    pub tex: *mut Texture,
    pub coefficient: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Env {
    pub frame: *mut Frame,
    pub tex: *mut Texture,
    pub coefficient: f32,
    pub fb_alpha: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dual {
    pub tex: *mut Texture,
    pub src_blend: i32,
    pub dst_blend: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UvTransform {
    pub base_transform: *mut RwMatrix,
    pub dual_transform: *mut RwMatrix,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union FxData {
    pub bump: Bump,
    pub env: Env,
    pub dual: Dual,
    pub uvtransform: UvTransform,
}

/// One of MatFX's two effects (`kind` is librw's fx[i].type).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fx {
    pub kind: u32,
    pub data: FxData,
}

/// rw::MatFX (`kind` is MatFX::type, the combination).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MatFX {
    pub fx: [Fx; 2],
    pub kind: u32,
}

pub const NOTHING: u32 = 0;
pub const BUMPMAP: u32 = 1;
pub const ENVMAP: u32 = 2;
pub const BUMPENVMAP: u32 = 3;
pub const DUAL: u32 = 4;
pub const UVTRANSFORM: u32 = 5;
pub const DUALUVTRANSFORM: u32 = 6;
pub const SUBHIERARCHY: i32 = 0x1;
pub const NOMATRICES: i32 = 0x2;

/// librw's allocator and the plugin offsets.
#[repr(C)]
pub struct Hooks {
    /// rwMalloc / rwNew (not zeroed)
    pub alloc: unsafe extern "C" fn(usize) -> *mut u8,
    /// rwFree (null allowed)
    pub free: unsafe extern "C" fn(*mut u8),
    /// hAnimOffset: HAnimData in a Frame
    pub hanim_offset: i32,
    /// skinGlobals.geoOffset: Skin* in a Geometry
    pub skin_geo_offset: i32,
    /// matFXGlobals.materialOffset: MatFX* in a Material
    pub matfx_material_offset: i32,
    /// matFXGlobals.atomicOffset: int32 in an Atomic
    pub matfx_atomic_offset: i32,
}

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set once when the plugins are registered, read-only afterwards.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().expect("revc_plugins_set_hooks not called") }
}

/// PLUGINOFFSET(T, object, offset)
unsafe fn plugin<T>(object: *const c_void, offset: i32) -> *mut T {
    unsafe { (object as *mut u8).offset(offset as isize) as *mut T }
}

unsafe fn alloc<T>(n: usize) -> *mut T {
    unsafe { (hooks().alloc)(n * size_of::<T>()) as *mut T }
}

unsafe fn free<T>(p: *mut T) {
    unsafe { (hooks().free)(p as *mut u8) }
}

fn align16<T>(p: *mut u8) -> *mut T {
    ((p as usize + 0xF) & !0xF) as *mut T
}

// ---------------------------------------------------------------- AnimInterpolator (anim.cpp)

/// AnimInterpolator::create: room for `num_nodes` interpolated frames of `max_frame_size` (+16
/// for pointers and padding on 64 bits); the callbacks are set by setCurrentAnim.
///
/// # Safety
/// Hooks set.
pub unsafe fn interp_create(num_nodes: i32, max_frame_size: i32) -> *mut AnimInterpolator {
    unsafe {
        let realsz = max_frame_size + if size_of::<*mut c_void>() > 4 { 16 } else { 0 };
        let it = (hooks().alloc)(size_of::<AnimInterpolator>() + (num_nodes * realsz) as usize) as *mut AnimInterpolator;
        if it.is_null() {
            return it;
        }
        (*it).current_anim = std::ptr::null_mut();
        (*it).current_time = 0.0;
        (*it).next_frame = std::ptr::null_mut();
        (*it).max_interp_key_frame_size = max_frame_size;
        (*it).current_interp_key_frame_size = max_frame_size;
        (*it).current_anim_key_frame_size = -1;
        (*it).num_nodes = num_nodes;
        it
    }
}

/// AnimInterpolator::destroy
///
/// # Safety
/// An interpolator from interp_create.
pub unsafe fn interp_destroy(it: *mut AnimInterpolator) {
    unsafe { free(it) }
}

// ---------------------------------------------------------------- HAnimHierarchy (hanim.cpp)

/// HAnimHierarchy::create: nodes with the given ids and flags (zero without), a 16-byte aligned
/// matrix per node unless NOMATRICES, and an interpolator.
///
/// # Safety
/// Hooks set; the arrays hold `num_nodes` entries or are null.
pub unsafe fn hier_create(num_nodes: i32, node_flags: *const i32, node_ids: *const i32, flags: i32, max_key_size: i32) -> *mut HAnimHierarchy {
    unsafe {
        let hier: *mut HAnimHierarchy = alloc(1);
        if hier.is_null() {
            return hier;
        }
        let h = &mut *hier;
        h.interpolator = interp_create(num_nodes, max_key_size);
        h.num_nodes = num_nodes;
        h.flags = flags;
        h.parent_frame = std::ptr::null_mut();
        h.parent_hierarchy = hier;
        if h.flags & NOMATRICES != 0 {
            h.matrices = std::ptr::null_mut();
            h.matrices_unaligned = std::ptr::null_mut();
        } else {
            let p = (hooks().alloc)((h.num_nodes * 64 + 0xF) as usize);
            h.matrices_unaligned = p as *mut c_void;
            h.matrices = align16(p);
        }
        h.node_info = alloc(h.num_nodes as usize);
        for i in 0..h.num_nodes as usize {
            let n = &mut *h.node_info.add(i);
            n.id = if node_ids.is_null() { 0 } else { *node_ids.add(i) };
            n.index = i as i32;
            n.flags = if node_flags.is_null() { 0 } else { *node_flags.add(i) };
            n.frame = std::ptr::null_mut();
        }
        hier
    }
}

/// HAnimHierarchy::destroy
///
/// # Safety
/// A hierarchy from hier_create.
pub unsafe fn hier_destroy(h: *mut HAnimHierarchy) {
    unsafe {
        interp_destroy((*h).interpolator);
        free((*h).matrices_unaligned);
        free((*h).node_info);
        free(h);
    }
}

/// HAnimData::get
///
/// # Safety
/// A frame.
pub unsafe fn hanim_data(f: *mut Frame) -> *mut HAnimData {
    unsafe { plugin(f as *const c_void, hooks().hanim_offset) }
}

/// findUnattachedById: depth first over siblings, then children, a frame with this id that no
/// node of the hierarchy uses yet.
unsafe fn find_unattached_by_id(h: *mut HAnimHierarchy, f: *mut Frame, id: i32) -> *mut Frame {
    unsafe {
        if f.is_null() {
            return f;
        }
        let d = &*hanim_data(f);
        if d.id >= 0 && d.id == id && hier_get_index_frame(h, f) == -1 {
            return f;
        }
        let ff = find_unattached_by_id(h, (*f).next, id);
        if !ff.is_null() {
            return ff;
        }
        find_unattached_by_id(h, (*f).child, id)
    }
}

/// HAnimHierarchy::attachByIndex
///
/// # Safety
/// A live hierarchy with a parent frame; `idx` below its node count.
pub unsafe fn hier_attach_by_index(h: *mut HAnimHierarchy, idx: i32) {
    unsafe {
        let n = &mut *(*h).node_info.add(idx as usize);
        let f = find_unattached_by_id(h, (*h).parent_frame, n.id);
        if !f.is_null() {
            n.frame = f;
        }
    }
}

/// HAnimHierarchy::attach
///
/// # Safety
/// As hier_attach_by_index.
pub unsafe fn hier_attach(h: *mut HAnimHierarchy) {
    unsafe {
        for i in 0..(*h).num_nodes {
            hier_attach_by_index(h, i);
        }
    }
}

/// HAnimHierarchy::getIndex(id)
///
/// # Safety
/// A live hierarchy.
pub unsafe fn hier_get_index_id(h: *const HAnimHierarchy, id: i32) -> i32 {
    unsafe { (0..(*h).num_nodes).find(|&i| (*(*h).node_info.add(i as usize)).id == id).unwrap_or(-1) }
}

/// HAnimHierarchy::getIndex(frame)
///
/// # Safety
/// A live hierarchy.
pub unsafe fn hier_get_index_frame(h: *const HAnimHierarchy, f: *mut Frame) -> i32 {
    unsafe { (0..(*h).num_nodes).find(|&i| (*(*h).node_info.add(i as usize)).frame == f).unwrap_or(-1) }
}

/// HAnimHierarchy::get
///
/// # Safety
/// A frame.
pub unsafe fn hier_get(f: *mut Frame) -> *mut HAnimHierarchy {
    unsafe { (*hanim_data(f)).hierarchy }
}

/// HAnimHierarchy::find: the first hierarchy depth first (siblings before children).
///
/// # Safety
/// A frame or null.
pub unsafe fn hier_find(f: *mut Frame) -> *mut HAnimHierarchy {
    unsafe {
        if f.is_null() {
            return std::ptr::null_mut();
        }
        let h = hier_get(f);
        if !h.is_null() {
            return h;
        }
        let h = hier_find((*f).next);
        if !h.is_null() {
            return h;
        }
        hier_find((*f).child)
    }
}

/// The frame plugin's constructor.
///
/// # Safety
/// A frame.
pub unsafe fn hanim_create(o: *mut c_void) {
    unsafe {
        let d = &mut *plugin::<HAnimData>(o, hooks().hanim_offset);
        d.id = -1;
        d.hierarchy = std::ptr::null_mut();
    }
}

/// The frame plugin's destructor: the hierarchy forgets its node frames, and goes with its
/// parent frame.
///
/// # Safety
/// A frame being destroyed.
pub unsafe fn hanim_destroy(o: *mut c_void) {
    unsafe {
        let d = &mut *plugin::<HAnimData>(o, hooks().hanim_offset);
        let h = d.hierarchy;
        if !h.is_null() {
            for i in 0..(*h).num_nodes as usize {
                (*(*h).node_info.add(i)).frame = std::ptr::null_mut();
            }
            if o as *mut Frame == (*h).parent_frame {
                hier_destroy(h);
            }
        }
        d.id = -1;
        d.hierarchy = std::ptr::null_mut();
    }
}

/// The frame plugin's copy: the id, and a new (unattached) hierarchy for a frame that owns one.
///
/// # Safety
/// Two frames.
pub unsafe fn hanim_copy(dst: *mut c_void, src: *mut c_void) {
    unsafe {
        let off = hooks().hanim_offset;
        let d = &mut *plugin::<HAnimData>(dst, off);
        let s = &*plugin::<HAnimData>(src, off);
        d.id = s.id;
        d.hierarchy = std::ptr::null_mut();
        let sh = s.hierarchy;
        if !sh.is_null() && (*sh).flags & SUBHIERARCHY == 0 {
            let dh = hier_create((*sh).num_nodes, std::ptr::null(), std::ptr::null(), (*sh).flags, (*(*sh).interpolator).max_interp_key_frame_size);
            for i in 0..(*dh).num_nodes as usize {
                let (dn, sn) = (&mut *(*dh).node_info.add(i), &*(*sh).node_info.add(i));
                dn.frame = std::ptr::null_mut();
                dn.flags = sn.flags;
                dn.index = sn.index;
                dn.id = sn.id;
            }
            d.hierarchy = dh;
            (*dh).parent_frame = dst as *mut Frame;
        }
    }
}

// ---------------------------------------------------------------- Skin (skin.cpp)

/// Skin::get
///
/// # Safety
/// A geometry.
pub unsafe fn skin_get(g: *const Geometry) -> *mut Skin {
    unsafe { *plugin::<*mut Skin>(g as *const c_void, hooks().skin_geo_offset) }
}

/// The geometry plugin's constructor: no skin.
///
/// # Safety
/// A geometry.
pub unsafe fn skin_create(g: *mut c_void) {
    unsafe { *plugin::<*mut Skin>(g, hooks().skin_geo_offset) = std::ptr::null_mut() }
}

/// The geometry plugin's destructor.
///
/// # Safety
/// A geometry being destroyed.
pub unsafe fn skin_destroy(g: *mut c_void) {
    unsafe {
        let s = *plugin::<*mut Skin>(g, hooks().skin_geo_offset);
        if !s.is_null() {
            free((*s).data);
            free((*s).remap_indices);
        }
        free(s);
    }
}

/// The geometry plugin's copy (librw asserts it isn't reached: geometries aren't cloned; this is
/// what its release build does).
///
/// # Safety
/// Two geometries with the same vertex count.
pub unsafe fn skin_copy(dst: *mut c_void, src: *mut c_void) {
    unsafe {
        let off = hooks().skin_geo_offset;
        let ss = *plugin::<*mut Skin>(src, off);
        if ss.is_null() {
            return;
        }
        let nv = (*(src as *mut Geometry)).num_vertices;
        let ds: *mut Skin = alloc(1);
        *plugin::<*mut Skin>(dst, off) = ds;
        (*ds).num_bones = (*ss).num_bones;
        (*ds).num_used_bones = (*ss).num_used_bones;
        (*ds).num_weights = (*ss).num_weights;
        skin_init(ds, (*ss).num_bones, (*ss).num_used_bones, nv);
        std::ptr::copy_nonoverlapping((*ss).used_bones, (*ds).used_bones, (*ss).num_used_bones as usize);
        std::ptr::copy_nonoverlapping((*ss).inverse_matrices, (*ds).inverse_matrices, ((*ss).num_bones * 16) as usize);
        std::ptr::copy_nonoverlapping((*ss).indices, (*ds).indices, (nv * 4) as usize);
        std::ptr::copy_nonoverlapping((*ss).weights, (*ds).weights, (nv * 4) as usize);
    }
}

/// Skin::init: one block for the used bones, the (16-byte aligned) inverse matrices, the
/// indices and the weights; no split data.
///
/// # Safety
/// A writable skin; hooks set.
pub unsafe fn skin_init(s: *mut Skin, num_bones: i32, num_used_bones: i32, num_vertices: i32) {
    unsafe {
        let sk = &mut *s;
        sk.num_bones = num_bones;
        sk.num_used_bones = num_used_bones;
        let size = sk.num_used_bones + sk.num_bones * 64 + num_vertices * (16 + 4) + 0xF;
        sk.data = (hooks().alloc)(size as usize);
        let mut p = sk.data;
        sk.used_bones = std::ptr::null_mut();
        if sk.num_used_bones != 0 {
            sk.used_bones = p;
            p = p.add(sk.num_used_bones as usize);
        }
        p = align16(p);
        sk.inverse_matrices = std::ptr::null_mut();
        if sk.num_bones != 0 {
            sk.inverse_matrices = p as *mut f32;
            p = p.add(64 * sk.num_bones as usize);
        }
        sk.indices = std::ptr::null_mut();
        if num_vertices != 0 {
            sk.indices = p;
            p = p.add(4 * num_vertices as usize);
        }
        sk.weights = std::ptr::null_mut();
        if num_vertices != 0 {
            sk.weights = p as *mut f32;
        }
        sk.bone_limit = 0;
        sk.num_meshes = 0;
        sk.rle_size = 0;
        sk.remap_indices = std::ptr::null_mut();
        sk.rle_count = std::ptr::null_mut();
        sk.rle = std::ptr::null_mut();
        sk.platform_data = std::ptr::null_mut();
        sk.legacy_type = 0;
    }
}

/// Skin::findNumWeights: the most weights a vertex uses (1 to 4), from the first vertex on,
/// stopping at 4.
///
/// # Safety
/// A skin with `num_vertices` weights.
pub unsafe fn skin_find_num_weights(s: *mut Skin, num_vertices: i32) {
    unsafe {
        let sk = &mut *s;
        sk.num_weights = 1;
        let mut w = sk.weights;
        for _ in 0..num_vertices {
            while *w.add(sk.num_weights as usize) != 0.0 {
                sk.num_weights += 1;
                if sk.num_weights == 4 {
                    return;
                }
            }
            w = w.add(4);
        }
    }
}

/// Skin::findUsedBones: the bones with a non-zero weight anywhere, in bone order.
///
/// # Safety
/// A skin with `num_vertices` indices and weights and room for every used bone.
pub unsafe fn skin_find_used_bones(s: *mut Skin, num_vertices: i32) {
    unsafe {
        let sk = &mut *s;
        let mut used = [0u8; 256];
        let (mut idx, mut w) = (sk.indices, sk.weights);
        for _ in 0..num_vertices {
            for i in 0..sk.num_weights as usize {
                if *w.add(i) == 0.0 {
                    continue;
                }
                let b = *idx.add(i) as usize;
                if used[b] == 0 {
                    used[b] += 1;
                }
            }
            idx = idx.add(4);
            w = w.add(4);
        }
        sk.num_used_bones = 0;
        for (i, &u) in used.iter().enumerate() {
            if u != 0 {
                *sk.used_bones.add(sk.num_used_bones as usize) = i as u8;
                sk.num_used_bones += 1;
            }
        }
    }
}

// ---------------------------------------------------------------- MatFX (matfx.cpp)

/// The atomic plugin's constructor: effects off.
///
/// # Safety
/// An atomic.
pub unsafe fn matfx_atomic_create(a: *mut c_void) {
    unsafe { *plugin::<i32>(a, hooks().matfx_atomic_offset) = 0 }
}

/// The atomic plugin's copy: on if the source's is (without setting the pipeline).
///
/// # Safety
/// Two atomics.
pub unsafe fn matfx_atomic_copy(dst: *mut c_void, src: *mut c_void) {
    unsafe {
        let off = hooks().matfx_atomic_offset;
        if *plugin::<i32>(src, off) != 0 {
            *plugin::<i32>(dst, off) = 1;
        }
    }
}

/// clearMatFX: releases the effects' textures and zeroes everything.
///
/// # Safety
/// A valid MatFX.
pub unsafe fn clear_matfx(fx: *mut MatFX) {
    unsafe {
        for e in &(*fx).fx {
            let tex: &[*mut Texture] = match e.kind {
                BUMPMAP => &[e.data.bump.bumped_tex, e.data.bump.tex],
                ENVMAP => &[e.data.env.tex],
                DUAL => &[e.data.dual.tex],
                _ => &[],
            };
            for &t in tex {
                if !t.is_null() {
                    tex_destroy(t);
                }
            }
        }
        std::ptr::write_bytes(fx, 0, 1);
    }
}

/// MatFX::get
///
/// # Safety
/// A material.
pub unsafe fn matfx_get(m: *const Material) -> *mut MatFX {
    unsafe { *plugin::<*mut MatFX>(m as *const c_void, hooks().matfx_material_offset) }
}

/// MatFX::setEffects: makes the material's MatFX (zeroed) if needed; a different combination
/// clears the old one first; then the two effect slots.
///
/// # Safety
/// A material.
pub unsafe fn matfx_set_effects(m: *mut Material, kind: u32) {
    unsafe {
        let mut fx = matfx_get(m);
        if fx.is_null() {
            fx = alloc(1);
            std::ptr::write_bytes(fx, 0, 1);
            *plugin::<*mut MatFX>(m as *const c_void, hooks().matfx_material_offset) = fx;
        }
        if (*fx).kind != 0 && (*fx).kind != kind {
            clear_matfx(fx);
        }
        (*fx).kind = kind;
        let slots = match kind {
            BUMPMAP | ENVMAP | DUAL | UVTRANSFORM => Some((kind, NOTHING)),
            BUMPENVMAP => Some((BUMPMAP, ENVMAP)),
            DUALUVTRANSFORM => Some((UVTRANSFORM, DUAL)),
            _ => None,
        };
        if let Some((a, b)) = slots {
            (*fx).fx[0].kind = a;
            (*fx).fx[1].kind = b;
        }
    }
}

/// MatFX::getEffects
///
/// # Safety
/// A material.
pub unsafe fn matfx_get_effects(m: *const Material) -> u32 {
    unsafe {
        let fx = matfx_get(m);
        if fx.is_null() { 0 } else { (*fx).kind }
    }
}

/// MatFX::getEffectIndex: the slot holding this effect, or -1.
///
/// # Safety
/// A valid MatFX.
pub unsafe fn matfx_effect_index(fx: *const MatFX, kind: u32) -> i32 {
    unsafe { (0..2).find(|&i| (*fx).fx[i].kind == kind).map_or(-1, |i| i as i32) }
}

/// The slot of an effect, for the setters and getters.
unsafe fn slot(fx: *mut MatFX, kind: u32) -> Option<&'static mut FxData> {
    unsafe {
        let i = matfx_effect_index(fx, kind);
        if i >= 0 { Some(&mut (*fx).fx[i as usize].data) } else { None }
    }
}

/// Replaces a texture reference: releases the old one, takes one to the new.
unsafe fn swap_texture(field: &mut *mut Texture, t: *mut Texture) {
    unsafe {
        if !field.is_null() {
            tex_destroy(*field);
        }
        *field = t;
        if !t.is_null() {
            (*t).ref_count += 1;
        }
    }
}

macro_rules! fx_api {
    ($( $(#[$m:meta])* fn $name:ident($($a:ident : $t:ty),*) $(-> $r:ty)? = $kind:ident, $d:ident => $body:expr, $default:expr; )*) => {
        $(
            $(#[$m])*
            /// # Safety
            /// A valid MatFX.
            pub unsafe fn $name(fx: *mut MatFX $(, $a: $t)*) $(-> $r)? {
                #[allow(unused_unsafe)]
                unsafe {
                    match slot(fx, $kind) {
                        Some($d) => $body,
                        None => $default,
                    }
                }
            }
        )*
    };
}

fx_api! {
    /// MatFX::setBumpTexture
    fn matfx_set_bump_texture(t: *mut Texture) = BUMPMAP, d => swap_texture(&mut d.bump.tex, t), ();
    /// MatFX::setBumpCoefficient
    fn matfx_set_bump_coefficient(c: f32) = BUMPMAP, d => d.bump.coefficient = c, ();
    /// MatFX::getBumpTexture
    fn matfx_get_bump_texture() -> *mut Texture = BUMPMAP, d => d.bump.tex, std::ptr::null_mut();
    /// MatFX::getBumpCoefficient
    fn matfx_get_bump_coefficient() -> f32 = BUMPMAP, d => d.bump.coefficient, 0.0;
    /// MatFX::setEnvTexture
    fn matfx_set_env_texture(t: *mut Texture) = ENVMAP, d => swap_texture(&mut d.env.tex, t), ();
    /// MatFX::setEnvFrame
    fn matfx_set_env_frame(f: *mut Frame) = ENVMAP, d => d.env.frame = f, ();
    /// MatFX::setEnvCoefficient
    fn matfx_set_env_coefficient(c: f32) = ENVMAP, d => d.env.coefficient = c, ();
    /// MatFX::setEnvFBAlpha
    fn matfx_set_env_fb_alpha(a: i32) = ENVMAP, d => d.env.fb_alpha = a, ();
    /// MatFX::getEnvTexture
    fn matfx_get_env_texture() -> *mut Texture = ENVMAP, d => d.env.tex, std::ptr::null_mut();
    /// MatFX::getEnvFrame
    fn matfx_get_env_frame() -> *mut Frame = ENVMAP, d => d.env.frame, std::ptr::null_mut();
    /// MatFX::getEnvCoefficient
    fn matfx_get_env_coefficient() -> f32 = ENVMAP, d => d.env.coefficient, 0.0;
    /// MatFX::getEnvFBAlpha
    fn matfx_get_env_fb_alpha() -> i32 = ENVMAP, d => d.env.fb_alpha, 0;
    /// MatFX::setDualTexture
    fn matfx_set_dual_texture(t: *mut Texture) = DUAL, d => swap_texture(&mut d.dual.tex, t), ();
    /// MatFX::setDualSrcBlend
    fn matfx_set_dual_src_blend(b: i32) = DUAL, d => d.dual.src_blend = b, ();
    /// MatFX::setDualDestBlend
    fn matfx_set_dual_dest_blend(b: i32) = DUAL, d => d.dual.dst_blend = b, ();
    /// MatFX::getDualTexture
    fn matfx_get_dual_texture() -> *mut Texture = DUAL, d => d.dual.tex, std::ptr::null_mut();
    /// MatFX::getDualSrcBlend
    fn matfx_get_dual_src_blend() -> i32 = DUAL, d => d.dual.src_blend, 0;
    /// MatFX::getDualDestBlend
    fn matfx_get_dual_dest_blend() -> i32 = DUAL, d => d.dual.dst_blend, 0;
    /// MatFX::setUVTransformMatrices
    fn matfx_set_uv_transform(base: *mut RwMatrix, dual: *mut RwMatrix) = UVTRANSFORM, d => { d.uvtransform.base_transform = base; d.uvtransform.dual_transform = dual; }, ();
}

/// MatFX::getUVTransformMatrices (null when there's no UV transform)
///
/// # Safety
/// A valid MatFX; the outputs writable or null.
pub unsafe fn matfx_get_uv_transform(fx: *mut MatFX, base: *mut *mut RwMatrix, dual: *mut *mut RwMatrix) {
    unsafe {
        let (b, d) = match slot(fx, UVTRANSFORM) {
            Some(s) => (s.uvtransform.base_transform, s.uvtransform.dual_transform),
            None => (std::ptr::null_mut(), std::ptr::null_mut()),
        };
        if !base.is_null() {
            *base = b;
        }
        if !dual.is_null() {
            *dual = d;
        }
    }
}

/// The material plugin's constructor: no MatFX.
///
/// # Safety
/// A material.
pub unsafe fn matfx_material_create(m: *mut c_void) {
    unsafe { *plugin::<*mut MatFX>(m, hooks().matfx_material_offset) = std::ptr::null_mut() }
}

/// The material plugin's destructor.
///
/// # Safety
/// A material being destroyed.
pub unsafe fn matfx_material_destroy(m: *mut c_void) {
    unsafe {
        let fx = *plugin::<*mut MatFX>(m, hooks().matfx_material_offset);
        if !fx.is_null() {
            clear_matfx(fx);
            free(fx);
        }
    }
}

/// The material plugin's copy: a copy of the MatFX with another reference to each texture.
///
/// # Safety
/// Two materials.
pub unsafe fn matfx_material_copy(dst: *mut c_void, src: *mut c_void) {
    unsafe {
        let off = hooks().matfx_material_offset;
        let sf = *plugin::<*mut MatFX>(src, off);
        if sf.is_null() {
            return;
        }
        let df: *mut MatFX = alloc(1);
        *plugin::<*mut MatFX>(dst, off) = df;
        std::ptr::copy_nonoverlapping(sf, df, 1);
        for e in &(*df).fx {
            let tex: &[*mut Texture] = match e.kind {
                BUMPMAP => &[e.data.bump.bumped_tex, e.data.bump.tex],
                ENVMAP => &[e.data.env.tex],
                DUAL => &[e.data.dual.tex],
                _ => &[],
            };
            for &t in tex {
                if !t.is_null() {
                    (*t).ref_count += 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- C ABI

/// # Safety
/// Called once after the plugins are registered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_plugins_set_hooks(h: Hooks) {
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
    revc_interp_create(n: i32, size: i32) -> *mut AnimInterpolator => interp_create(n, size);
    revc_interp_destroy(it: *mut AnimInterpolator) => interp_destroy(it);
    revc_hier_create(n: i32, flags: *const i32, ids: *const i32, hflags: i32, maxkey: i32) -> *mut HAnimHierarchy => hier_create(n, flags, ids, hflags, maxkey);
    revc_hier_destroy(h: *mut HAnimHierarchy) => hier_destroy(h);
    revc_hier_attach_by_index(h: *mut HAnimHierarchy, i: i32) => hier_attach_by_index(h, i);
    revc_hier_attach(h: *mut HAnimHierarchy) => hier_attach(h);
    revc_hier_get_index_id(h: *const HAnimHierarchy, id: i32) -> i32 => hier_get_index_id(h, id);
    revc_hier_get_index_frame(h: *const HAnimHierarchy, f: *mut Frame) -> i32 => hier_get_index_frame(h, f);
    revc_hier_find(f: *mut Frame) -> *mut HAnimHierarchy => hier_find(f);
    revc_hanim_create(o: *mut c_void) => hanim_create(o);
    revc_hanim_destroy(o: *mut c_void) => hanim_destroy(o);
    revc_hanim_copy(d: *mut c_void, s: *mut c_void) => hanim_copy(d, s);
    revc_skin_create(g: *mut c_void) => skin_create(g);
    revc_skin_destroy(g: *mut c_void) => skin_destroy(g);
    revc_skin_copy(d: *mut c_void, s: *mut c_void) => skin_copy(d, s);
    revc_skin_init(s: *mut Skin, nb: i32, nub: i32, nv: i32) => skin_init(s, nb, nub, nv);
    revc_skin_find_num_weights(s: *mut Skin, nv: i32) => skin_find_num_weights(s, nv);
    revc_skin_find_used_bones(s: *mut Skin, nv: i32) => skin_find_used_bones(s, nv);
    revc_matfx_atomic_create(a: *mut c_void) => matfx_atomic_create(a);
    revc_matfx_atomic_copy(d: *mut c_void, s: *mut c_void) => matfx_atomic_copy(d, s);
    revc_matfx_set_effects(m: *mut Material, kind: u32) => matfx_set_effects(m, kind);
    revc_matfx_get_effects(m: *const Material) -> u32 => matfx_get_effects(m);
    revc_matfx_effect_index(fx: *const MatFX, kind: u32) -> i32 => matfx_effect_index(fx, kind);
    revc_matfx_set_bump_texture(fx: *mut MatFX, t: *mut Texture) => matfx_set_bump_texture(fx, t);
    revc_matfx_set_bump_coefficient(fx: *mut MatFX, c: f32) => matfx_set_bump_coefficient(fx, c);
    revc_matfx_get_bump_texture(fx: *mut MatFX) -> *mut Texture => matfx_get_bump_texture(fx);
    revc_matfx_get_bump_coefficient(fx: *mut MatFX) -> f32 => matfx_get_bump_coefficient(fx);
    revc_matfx_set_env_texture(fx: *mut MatFX, t: *mut Texture) => matfx_set_env_texture(fx, t);
    revc_matfx_set_env_frame(fx: *mut MatFX, f: *mut Frame) => matfx_set_env_frame(fx, f);
    revc_matfx_set_env_coefficient(fx: *mut MatFX, c: f32) => matfx_set_env_coefficient(fx, c);
    revc_matfx_set_env_fb_alpha(fx: *mut MatFX, a: i32) => matfx_set_env_fb_alpha(fx, a);
    revc_matfx_get_env_texture(fx: *mut MatFX) -> *mut Texture => matfx_get_env_texture(fx);
    revc_matfx_get_env_frame(fx: *mut MatFX) -> *mut Frame => matfx_get_env_frame(fx);
    revc_matfx_get_env_coefficient(fx: *mut MatFX) -> f32 => matfx_get_env_coefficient(fx);
    revc_matfx_get_env_fb_alpha(fx: *mut MatFX) -> i32 => matfx_get_env_fb_alpha(fx);
    revc_matfx_set_dual_texture(fx: *mut MatFX, t: *mut Texture) => matfx_set_dual_texture(fx, t);
    revc_matfx_set_dual_src_blend(fx: *mut MatFX, b: i32) => matfx_set_dual_src_blend(fx, b);
    revc_matfx_set_dual_dest_blend(fx: *mut MatFX, b: i32) => matfx_set_dual_dest_blend(fx, b);
    revc_matfx_get_dual_texture(fx: *mut MatFX) -> *mut Texture => matfx_get_dual_texture(fx);
    revc_matfx_get_dual_src_blend(fx: *mut MatFX) -> i32 => matfx_get_dual_src_blend(fx);
    revc_matfx_get_dual_dest_blend(fx: *mut MatFX) -> i32 => matfx_get_dual_dest_blend(fx);
    revc_matfx_set_uv_transform(fx: *mut MatFX, b: *mut RwMatrix, d: *mut RwMatrix) => matfx_set_uv_transform(fx, b, d);
    revc_matfx_get_uv_transform(fx: *mut MatFX, b: *mut *mut RwMatrix, d: *mut *mut RwMatrix) => matfx_get_uv_transform(fx, b, d);
    revc_matfx_material_create(m: *mut c_void) => matfx_material_create(m);
    revc_matfx_material_destroy(m: *mut c_void) => matfx_material_destroy(m);
    revc_matfx_material_copy(d: *mut c_void, s: *mut c_void) => matfx_material_copy(d, s);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwplugins.txt (recorded from librw's C++, see the recorder).
    use super::*;
    use crate::rwframe::{self, LinkList, list_init};
    use crate::rwgeomobj::{Hooks as GeoHooks, geo_create, geo_destroy, mat_clone, mat_create, mat_destroy, mat_set_texture, revc_geometry_set_hooks};
    use crate::rwtexture::{Hooks as TexHooks, TexDictionary, TextureGlobals, revc_tex_set_hooks, tex_create};
    use std::cell::RefCell;

    type Ids = RefCell<Vec<*mut c_void>>;
    thread_local! {
        static IDS: [Ids; 4] = const { [const { RefCell::new(Vec::new()) }; 4] };
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static DIRTY: RefCell<*mut LinkList> = const { RefCell::new(std::ptr::null_mut()) };
    }
    const F: usize = 0;
    const G: usize = 1;
    const M: usize = 2;
    const T: usize = 3;
    const TAG: [&str; 4] = ["f", "g", "m", "t"];
    static mut UV_A: RwMatrix = crate::rwmatrix::IDENT;
    static mut UV_B: RwMatrix = crate::rwmatrix::IDENT;

    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn id<P>(t: usize, p: *const P) -> i32 {
        if p.is_null() {
            return -1;
        }
        IDS.with(|v| v[t].borrow().iter().position(|&q| q as *const P == p).map_or(-2, |i| i as i32))
    }
    fn get<P>(t: usize, s: &str) -> *mut P {
        let i: i32 = s.parse().unwrap();
        if i < 0 { std::ptr::null_mut() } else { IDS.with(|v| v[t].borrow()[i as usize] as *mut P) }
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
    fn uv_id(m: *const RwMatrix) -> i32 {
        unsafe {
            if m.is_null() {
                0
            } else if m == std::ptr::addr_of!(UV_A) {
                1
            } else if m == std::ptr::addr_of!(UV_B) {
                2
            } else {
                3
            }
        }
    }

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
    // frames carry HAnimData
    unsafe extern "C" fn f_size() -> usize {
        size_of::<Frame>() + size_of::<HAnimData>()
    }
    unsafe extern "C" fn f_construct(f: *mut Frame) {
        reg(F, f as _);
        unsafe { hanim_create(f as _) };
    }
    unsafe extern "C" fn f_destruct(f: *mut Frame) {
        unsafe { hanim_destroy(f as _) };
        unreg(F, f as _);
    }
    unsafe extern "C" fn f_copy(d: *mut Frame, s: *mut Frame) {
        unsafe { hanim_copy(d as _, s as _) };
    }
    unsafe extern "C" fn f_allocated(_: i32) {}
    // geometries carry a Skin*, materials a MatFX*
    unsafe extern "C" fn g_size() -> usize {
        size_of::<Geometry>() + 8
    }
    unsafe extern "C" fn m_size() -> usize {
        size_of::<Material>() + 8
    }
    unsafe extern "C" fn g_construct(g: *mut Geometry) {
        reg(G, g as _);
        unsafe { skin_create(g as _) };
    }
    unsafe extern "C" fn g_destruct(g: *mut Geometry) {
        unsafe { skin_destroy(g as _) };
        unreg(G, g as _);
    }
    unsafe extern "C" fn m_construct(m: *mut Material) {
        reg(M, m as _);
        unsafe { matfx_material_create(m as _) };
    }
    unsafe extern "C" fn m_destruct(m: *mut Material) {
        unsafe { matfx_material_destroy(m as _) };
        unreg(M, m as _);
    }
    unsafe extern "C" fn m_copy(d: *mut Material, s: *mut Material) {
        unsafe { matfx_material_copy(d as _, s as _) };
    }
    unsafe extern "C" fn g_allocated(_: i32, _: i32) {}
    unsafe extern "C" fn never() {
        panic!("unexpected geometry callback");
    }
    unsafe extern "C" fn t_size() -> usize {
        size_of::<Texture>() + 8
    }
    unsafe extern "C" fn t_construct(t: *mut Texture) {
        reg(T, t as _);
    }
    unsafe extern "C" fn t_destruct(t: *mut Texture) {
        unreg(T, t as _);
    }
    unsafe extern "C" fn d_size() -> usize {
        size_of::<TexDictionary>()
    }
    unsafe extern "C" fn d_hook(_: *mut TexDictionary) {}
    unsafe extern "C" fn r_destroy(_: *mut c_void) {}
    unsafe extern "C" fn r_dummy() -> *mut c_void {
        std::ptr::null_mut()
    }
    unsafe extern "C" fn t_allocated(_: i32, _: i32) {}

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
        unsafe fn bytes<P>(&mut self, p: *const P, n: usize) {
            let p = p as *const u8;
            for k in 0..n {
                self.b(unsafe { *p.add(k) });
            }
        }
        fn ptr<P>(&mut self, p: *const P, base: *const u8) {
            self.i(if p.is_null() { -1 } else { (p as isize - base as isize) as i32 });
        }
    }

    unsafe fn hash_hier(h: &mut H, hr: &HAnimHierarchy) {
        unsafe {
            h.i(hr.flags);
            h.i(hr.num_nodes);
            h.i(!hr.matrices.is_null() as i32);
            h.i(!hr.matrices_unaligned.is_null() as i32);
            h.i(if hr.matrices.is_null() { -1 } else { (hr.matrices as usize & 0xF) as i32 });
            h.i(id(F, hr.parent_frame));
            h.i((hr.parent_hierarchy == hr as *const _ as *mut _) as i32);
            for i in 0..hr.num_nodes as usize {
                let n = &*hr.node_info.add(i);
                h.i(n.id);
                h.i(n.index);
                h.i(n.flags);
                h.i(id(F, n.frame));
            }
            let it = &*hr.interpolator;
            h.i(!it.current_anim.is_null() as i32);
            h.bytes(&it.current_time, 4);
            h.i(!it.next_frame.is_null() as i32);
            h.i(it.max_interp_key_frame_size);
            h.i(it.current_interp_key_frame_size);
            h.i(it.current_anim_key_frame_size);
            h.i(it.num_nodes);
        }
    }

    unsafe fn state() -> String {
        unsafe {
            let mut h = H(0xcbf29ce484222325);
            for (i, p) in live(F) {
                let f = p as *mut Frame;
                let d = &*hanim_data(f);
                h.i(i as i32);
                h.i(id(F, (*f).object.parent as *mut Frame));
                h.i(id(F, (*f).child));
                h.i(id(F, (*f).next));
                h.i(id(F, (*f).root));
                h.i(d.id);
                h.i(!d.hierarchy.is_null() as i32);
                if !d.hierarchy.is_null() {
                    h.i(id(F, (*d.hierarchy).parent_frame));
                    if (*d.hierarchy).parent_frame == f {
                        hash_hier(&mut h, &*d.hierarchy);
                    }
                }
            }
            for (i, p) in live(G) {
                let g = p as *mut Geometry;
                let s = skin_get(g);
                h.i(i as i32);
                h.i((*g).num_vertices);
                h.i(!s.is_null() as i32);
                if s.is_null() {
                    continue;
                }
                let s = &*s;
                let nv = (*g).num_vertices as usize;
                h.i(s.num_bones);
                h.i(s.num_used_bones);
                h.i(s.num_weights);
                h.ptr(s.used_bones, s.data);
                h.ptr(s.inverse_matrices, s.data);
                h.ptr(s.indices, s.data);
                h.ptr(s.weights, s.data);
                h.i(if s.inverse_matrices.is_null() { -1 } else { (s.inverse_matrices as usize & 0xF) as i32 });
                if !s.used_bones.is_null() {
                    h.bytes(s.used_bones, s.num_used_bones as usize);
                }
                if !s.inverse_matrices.is_null() {
                    h.bytes(s.inverse_matrices, s.num_bones as usize * 64);
                }
                if !s.indices.is_null() {
                    h.bytes(s.indices, nv * 4);
                }
                if !s.weights.is_null() {
                    h.bytes(s.weights, nv * 16);
                }
                for v in [s.bone_limit, s.num_meshes, s.rle_size, !s.remap_indices.is_null() as i32, !s.rle_count.is_null() as i32, !s.rle.is_null() as i32, !s.platform_data.is_null() as i32, s.legacy_type] {
                    h.i(v);
                }
            }
            for (i, p) in live(M) {
                let m = p as *mut Material;
                let fx = matfx_get(m);
                h.i(i as i32);
                h.i((*m).ref_count);
                h.i(id(T, (*m).texture));
                h.i(!fx.is_null() as i32);
                h.i(matfx_get_effects(m) as i32);
                if fx.is_null() {
                    continue;
                }
                h.i((*fx).kind as i32);
                for e in &(*fx).fx {
                    h.i(e.kind as i32);
                    match e.kind {
                        BUMPMAP => {
                            h.i(id(F, e.data.bump.frame));
                            h.i(id(T, e.data.bump.bumped_tex));
                            h.i(id(T, e.data.bump.tex));
                            h.bytes(&e.data.bump.coefficient, 4);
                        }
                        ENVMAP => {
                            h.i(id(F, e.data.env.frame));
                            h.i(id(T, e.data.env.tex));
                            h.bytes(&e.data.env.coefficient, 4);
                            h.i(e.data.env.fb_alpha);
                        }
                        DUAL => {
                            h.i(id(T, e.data.dual.tex));
                            h.i(e.data.dual.src_blend);
                            h.i(e.data.dual.dst_blend);
                        }
                        UVTRANSFORM => {
                            h.i(uv_id(e.data.uvtransform.base_transform));
                            h.i(uv_id(e.data.uvtransform.dual_transform));
                        }
                        _ => {}
                    }
                }
            }
            for (i, p) in live(T) {
                h.i(i as i32);
                h.i((*(p as *mut Texture)).ref_count);
            }
            let l = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            format!(" {:016x} |{l}", h.0)
        }
    }

    struct Gen(u32);
    impl Gen {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
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
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwplugins.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwplugins.txt");
        let dirty: *mut LinkList = Box::into_raw(Box::new(unsafe { std::mem::zeroed() }));
        let tg: *mut TextureGlobals = Box::into_raw(Box::new(unsafe { std::mem::zeroed() }));
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
                mat_construct: m_construct,
                mat_destruct: m_destruct,
                mat_copy: m_copy,
                allocated: g_allocated,
                native_warning: never,
                verify_failed: never,
            });
            revc_tex_set_hooks(TexHooks {
                tex_size: t_size,
                tex_construct: t_construct,
                tex_destruct: t_destruct,
                dict_size: d_size,
                dict_construct: d_hook,
                dict_destruct: d_hook,
                raster_destroy: r_destroy,
                raster_create_dummy: r_dummy,
                allocated: t_allocated,
            });
            revc_plugins_set_hooks(Hooks {
                alloc: alloc_,
                free: free_,
                hanim_offset: size_of::<Frame>() as i32,
                skin_geo_offset: size_of::<Geometry>() as i32,
                matfx_material_offset: size_of::<Material>() as i32,
                matfx_atomic_offset: 0,
            });
            list_init(&mut (*tg).tex_dicts);
            list_init(&mut (*tg).textures);
            let d = crate::rwtexture::dict_create(tg);
            (*tg).initial_tex_dict = d;
            (*tg).current_tex_dict = d;
            (*tg).load_textures = 1;
        }
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let (args, expect) = input.split_once(" =").map_or((input, None), |(a, r)| (a, Some(r.trim().to_string())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let num = |i: usize| w[i].parse::<i64>().unwrap();
            let res: Option<String> = unsafe {
                match w[0] {
                    "newframe" => Some(id(F, rwframe::create()).to_string()),
                    "addchild" => {
                        rwframe::add_child(get(F, w[1]), get(F, w[2]), false, dirty);
                        None
                    }
                    "hanimid" => {
                        (*hanim_data(get(F, w[1]))).id = num(2) as i32;
                        None
                    }
                    "newhier" => {
                        let f: *mut Frame = get(F, w[1]);
                        let (nn, hflags, seed, maxkey, no_ids) = (num(2) as usize, num(3) as i32, num(4) as u32, num(5) as i32, num(6) != 0);
                        let mut g = Gen(seed | 1);
                        let (mut ids, mut flags) = ([0i32; 20], [0i32; 20]);
                        for i in 0..nn {
                            ids[i] = (g.next() % 16) as i32;
                            flags[i] = (g.next() % 4) as i32;
                        }
                        let (fp, ip) = if no_ids { (std::ptr::null(), std::ptr::null()) } else { (flags.as_ptr(), ids.as_ptr()) };
                        let x = hier_create(nn as i32, fp, ip, hflags, maxkey);
                        (*hanim_data(f)).hierarchy = x;
                        (*x).parent_frame = f;
                        None
                    }
                    "attach" => {
                        hier_attach(hier_get(get(F, w[1])));
                        None
                    }
                    "attachidx" => {
                        hier_attach_by_index(hier_get(get(F, w[1])), num(2) as i32);
                        None
                    }
                    "getindex" => Some(hier_get_index_id(hier_get(get(F, w[1])), num(2) as i32).to_string()),
                    "getindexframe" => Some(hier_get_index_frame(hier_get(get(F, w[1])), get(F, w[2])).to_string()),
                    "find" | "get" => {
                        let f: *mut Frame = get(F, w[1]);
                        let x = if w[0] == "find" { hier_find(f) } else { hier_get(f) };
                        Some((if x.is_null() { -1 } else { id(F, (*x).parent_frame) }).to_string())
                    }
                    "clone" => {
                        let f: *mut Frame = get(F, w[1]);
                        let c = rwframe::clone_and_link(f, dirty);
                        rwframe::purge_clone(f);
                        Some(id(F, c).to_string())
                    }
                    "delhier" => {
                        rwframe::destroy_hierarchy(get(F, w[1]));
                        None
                    }
                    "newgeo" => Some(id(G, geo_create(num(1) as i32, 0, 0)).to_string()),
                    "skin" => {
                        let g: *mut Geometry = get(G, w[1]);
                        let (nb, nub, seed) = (num(2) as i32, num(3) as i32, num(4) as u32);
                        let s: *mut Skin = alloc(1);
                        *plugin::<*mut Skin>(g as *const c_void, hooks().skin_geo_offset) = s;
                        let nv = (*g).num_vertices as usize;
                        skin_init(s, nb, nub, nv as i32);
                        let mut gg = Gen(seed | 1);
                        let sk = &mut *s;
                        if !sk.used_bones.is_null() {
                            for i in 0..nub as usize {
                                *sk.used_bones.add(i) = gg.next() as u8;
                            }
                        }
                        if !sk.inverse_matrices.is_null() {
                            for i in 0..(nb * 16) as usize {
                                *sk.inverse_matrices.add(i) = (gg.next() % 2001).wrapping_sub(1000) as i32 as f32 / 100.0;
                            }
                        }
                        if !sk.indices.is_null() {
                            for i in 0..nv * 4 {
                                *sk.indices.add(i) = if nb != 0 { (gg.next() % nb as u32) as u8 } else { 0 };
                            }
                        }
                        if !sk.weights.is_null() {
                            for i in 0..nv {
                                let used = gg.next() % 5;
                                for k in 0..4 {
                                    *sk.weights.add(i * 4 + k) = if (k as u32) < used { (gg.next() % 100) as f32 / 100.0 } else { 0.0 };
                                }
                            }
                        }
                        sk.num_weights = (gg.next() % 5) as i32;
                        None
                    }
                    "numweights" => {
                        let g: *mut Geometry = get(G, w[1]);
                        skin_find_num_weights(skin_get(g), (*g).num_vertices);
                        None
                    }
                    "usedbones" => {
                        let g: *mut Geometry = get(G, w[1]);
                        skin_find_used_bones(skin_get(g), (*g).num_vertices);
                        None
                    }
                    "delgeo" => {
                        geo_destroy(get(G, w[1]));
                        None
                    }
                    "newmat" => Some(id(M, mat_create()).to_string()),
                    "clonemat" => Some(id(M, mat_clone(get(M, w[1]))).to_string()),
                    "delmat" => {
                        mat_destroy(get(M, w[1]));
                        None
                    }
                    "effects" => {
                        matfx_set_effects(get(M, w[1]), num(2) as u32);
                        None
                    }
                    "geteffects" => Some(matfx_get_effects(get(M, w[1])).to_string()),
                    "newtex" => Some(id(T, tex_create(tg, std::ptr::null_mut())).to_string()),
                    "deltex" => {
                        tex_destroy(get(T, w[1]));
                        None
                    }
                    "mattex" => {
                        mat_set_texture(get(M, w[1]), get(T, w[2]));
                        None
                    }
                    op => {
                        let fx = matfx_get(get(M, w[1]));
                        match op {
                            "bumptex" => matfx_set_bump_texture(fx, get(T, w[2])),
                            "bumpcoef" => matfx_set_bump_coefficient(fx, hexf(w[2])),
                            "envtex" => matfx_set_env_texture(fx, get(T, w[2])),
                            "envframe" => matfx_set_env_frame(fx, get(F, w[2])),
                            "envcoef" => matfx_set_env_coefficient(fx, hexf(w[2])),
                            "envfbalpha" => matfx_set_env_fb_alpha(fx, num(2) as i32),
                            "dualtex" => matfx_set_dual_texture(fx, get(T, w[2])),
                            "dualblend" => {
                                matfx_set_dual_src_blend(fx, num(2) as i32);
                                matfx_set_dual_dest_blend(fx, num(3) as i32);
                            }
                            "uvxform" => {
                                let m = |k: i64| match k {
                                    1 => std::ptr::addr_of_mut!(UV_A),
                                    2 => std::ptr::addr_of_mut!(UV_B),
                                    _ => std::ptr::null_mut(),
                                };
                                matfx_set_uv_transform(fx, m(num(2)), m(num(3)));
                            }
                            "getters" => {}
                            other => panic!("unknown op {other}"),
                        }
                        if op == "getters" {
                            let (mut a, mut b) = (std::ptr::null_mut(), std::ptr::null_mut());
                            matfx_get_uv_transform(fx, &mut a, &mut b);
                            let mut o = format!(
                                "{} {} {} {} {} {} {} {} {} {} {}",
                                id(T, matfx_get_bump_texture(fx)),
                                hx(matfx_get_bump_coefficient(fx)),
                                id(T, matfx_get_env_texture(fx)),
                                id(F, matfx_get_env_frame(fx)),
                                hx(matfx_get_env_coefficient(fx)),
                                matfx_get_env_fb_alpha(fx),
                                id(T, matfx_get_dual_texture(fx)),
                                matfx_get_dual_src_blend(fx),
                                matfx_get_dual_dest_blend(fx),
                                uv_id(a),
                                uv_id(b)
                            );
                            for k in 0..7 {
                                o += &format!(" {}", matfx_effect_index(fx, k));
                            }
                            Some(o)
                        } else {
                            None
                        }
                    }
                }
            };
            if res != expect && bad.len() < 6 {
                bad.push(format!("line {} {input}: result {res:?}", n + 1));
            }
            let got = unsafe { state() };
            n += 1;
            if got != want && bad.len() < 6 {
                bad.push(format!("line {n} {input}\n  want{want}\n  got {got}"));
            }
        }
        assert!(n > 6000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

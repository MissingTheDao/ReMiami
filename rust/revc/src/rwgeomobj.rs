//! librw's geometry and material objects, stage 2 step 6a: Geometry create/destroy (reference
//! counted), lock/unlock, addMorphTargets, allocateData, calculateBoundingSphere,
//! hasColoredMaterial and buildMeshes/buildTristrips (with verifyMesh); the mesh header
//! (geoplg.cpp: allocateMeshes, setupIndices, guessNumTriangles); MaterialList and Material
//! create/clone/destroy/setTexture. Objects keep librw's layouts; memory comes from librw's
//! allocator and plugin data stays in C++ (hooks). The index building itself is rwgeometry's.
//!
//! A first draft of these functions came from the local model (bin/ask-local.py) and was
//! corrected by hand. Checked against tests/golden/rwgeomobj.txt, recorded from the original
//! C++ (tests/golden/recorders/rwgeomobj).

use std::ffi::c_void;

use crate::math::V3;
use crate::rwcamera::Sphere;
use crate::rwframe::Object;
use crate::rwgeometry::{Triangle, bounding_sphere, list_meshes, strip_meshes};
use crate::rwtexture::{Texture, tex_destroy};

/// rw::RGBA
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// rw::SurfaceProperties
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceProperties {
    pub ambient: f32,
    pub specular: f32,
    pub diffuse: f32,
}

/// rw::Material (plugin data follows it in memory).
#[repr(C)]
pub struct Material {
    pub texture: *mut Texture,
    pub color: Rgba,
    pub surface_props: SurfaceProperties,
    pub pipeline: *mut c_void,
    pub ref_count: i32,
}

/// rw::Mesh
#[repr(C)]
pub struct Mesh {
    pub indices: *mut u16,
    pub num_indices: u32,
    pub material: *mut Material,
}

/// rw::MeshHeader; the Mesh array follows it, then (unless allocated without) the indices.
#[repr(C)]
pub struct MeshHeader {
    pub flags: u32,
    pub num_meshes: u16,
    pub serial_num: u16,
    pub total_indices: u32,
    pub pad: u32,
}

/// rw::MorphTarget
#[repr(C)]
pub struct MorphTarget {
    pub parent: *mut Geometry,
    pub bounding_sphere: Sphere,
    pub vertices: *mut V3,
    pub normals: *mut V3,
}

/// rw::MaterialList
#[repr(C)]
pub struct MaterialList {
    pub materials: *mut *mut Material,
    pub num_materials: i32,
    pub space: i32,
}

/// rw::Geometry (plugin data follows it in memory).
#[repr(C)]
pub struct Geometry {
    pub object: Object,
    pub flags: u32,
    pub locked_since_inst: u16,
    pub num_triangles: i32,
    pub num_vertices: i32,
    pub num_morph_targets: i32,
    pub num_tex_coord_sets: i32,
    pub triangles: *mut Triangle,
    pub colors: *mut Rgba,
    pub tex_coords: [*mut [f32; 2]; 8],
    pub morph_targets: *mut MorphTarget,
    pub mat_list: MaterialList,
    pub mesh_header: *mut MeshHeader,
    pub inst_data: *mut c_void,
    pub ref_count: i32,
}

pub const TRISTRIP: u32 = 0x01;
pub const TEXTURED: u32 = 0x04;
pub const PRELIT: u32 = 0x08;
pub const NORMALS: u32 = 0x10;
pub const TEXTURED2: u32 = 0x80;
pub const NATIVE: u32 = 0x01000000;
pub const LOCKPOLYGONS: i32 = 0x0001;
pub const GEOMETRY_ID: u8 = 8;
pub const MESH_TRISTRIP: u32 = 1;

/// librw's allocator, plugin lists and counters (C++).
#[repr(C)]
pub struct Hooks {
    /// rwMalloc / rwNew (contents not zeroed)
    pub alloc: unsafe extern "C" fn(usize) -> *mut u8,
    /// rwResize / rwRealloc
    pub realloc: unsafe extern "C" fn(*mut u8, usize) -> *mut u8,
    /// rwFree (null allowed)
    pub free: unsafe extern "C" fn(*mut u8),
    pub geo_size: unsafe extern "C" fn() -> usize,
    pub geo_construct: unsafe extern "C" fn(*mut Geometry),
    pub geo_destruct: unsafe extern "C" fn(*mut Geometry),
    pub mat_size: unsafe extern "C" fn() -> usize,
    pub mat_construct: unsafe extern "C" fn(*mut Material),
    pub mat_destruct: unsafe extern "C" fn(*mut Material),
    pub mat_copy: unsafe extern "C" fn(dst: *mut Material, src: *mut Material),
    /// numAllocated += delta (kind 0 geometry, 1 material)
    pub allocated: unsafe extern "C" fn(kind: i32, delta: i32),
    /// buildMeshes on pre-instanced geometry: librw's warning
    pub native_warning: unsafe extern "C" fn(),
    /// verifyMesh found a mismatch: librw prints "TRISTRIP verify failed" and exits
    pub verify_failed: unsafe extern "C" fn(),
}

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set before use (tests), read-only afterwards; the game uses the defaults.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().unwrap_or(&DEFAULT_HOOKS) }
}

// The game's hooks (stage 2 step 7d2, from geometry.cpp): librw's allocator and Geometry's and
// Material's plugin lists and counters (rwplugin.rs).
mod defaults {
    use super::*;
    use crate::rwplugin::{self as pl, CLASS_GEOMETRY, CLASS_MATERIAL, class_list};
    const HINT: u32 = 0x30000 | 0x08; // MEMDUR_EVENT | ID_GEOMETRY
    pub(super) unsafe extern "C" fn alloc(size: usize) -> *mut u8 {
        unsafe { crate::rwengine::rw_malloc(size, HINT) as *mut u8 }
    }
    pub(super) unsafe extern "C" fn realloc(p: *mut u8, size: usize) -> *mut u8 {
        unsafe { crate::rwengine::rw_realloc(p as *mut c_void, size, HINT) as *mut u8 }
    }
    pub(super) unsafe extern "C" fn free(p: *mut u8) {
        unsafe { crate::rwengine::rw_free(p as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn geo_size() -> usize {
        unsafe { (*class_list(CLASS_GEOMETRY)).size as usize }
    }
    pub(super) unsafe extern "C" fn geo_construct(g: *mut Geometry) {
        unsafe { pl::pl_construct(class_list(CLASS_GEOMETRY), g as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn geo_destruct(g: *mut Geometry) {
        unsafe { pl::pl_destruct(class_list(CLASS_GEOMETRY), g as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn mat_size() -> usize {
        unsafe { (*class_list(CLASS_MATERIAL)).size as usize }
    }
    pub(super) unsafe extern "C" fn mat_construct(m: *mut Material) {
        unsafe { pl::pl_construct(class_list(CLASS_MATERIAL), m as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn mat_destruct(m: *mut Material) {
        unsafe { pl::pl_destruct(class_list(CLASS_MATERIAL), m as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn mat_copy(d: *mut Material, s: *mut Material) {
        unsafe { pl::pl_copy(class_list(CLASS_MATERIAL), d as *mut c_void, s as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn allocated(kind: i32, delta: i32) {
        unsafe { *pl::alloc_count(if kind == 0 { CLASS_GEOMETRY } else { CLASS_MATERIAL }) += delta }
    }
    pub(super) unsafe extern "C" fn native_warning() {
        eprintln!("WARNING: trying Geometry::buildMeshes() on pre-instanced geometry");
    }
    pub(super) unsafe extern "C" fn verify_failed() {
        eprintln!("TRISTRIP verify failed");
        std::process::exit(1);
    }
}
static DEFAULT_HOOKS: Hooks = Hooks {
    alloc: defaults::alloc,
    realloc: defaults::realloc,
    free: defaults::free,
    geo_size: defaults::geo_size,
    geo_construct: defaults::geo_construct,
    geo_destruct: defaults::geo_destruct,
    mat_size: defaults::mat_size,
    mat_construct: defaults::mat_construct,
    mat_destruct: defaults::mat_destruct,
    mat_copy: defaults::mat_copy,
    allocated: defaults::allocated,
    native_warning: defaults::native_warning,
    verify_failed: defaults::verify_failed,
};

/// geoplg.cpp's nextSerialNum
static mut NEXT_SERIAL: u16 = 1;

unsafe fn meshes(mh: *mut MeshHeader) -> *mut Mesh {
    unsafe { mh.add(1) as *mut Mesh }
}

fn alloc(n: usize) -> *mut u8 {
    unsafe { (hooks().alloc)(n) }
}

fn free<T>(p: *mut T) {
    unsafe { (hooks().free)(p as *mut u8) }
}

// ---------------------------------------------------------------- Geometry

/// Geometry::create: the attributes (triangles, prelight colours, tex coords) in one block,
/// one morph target, no materials, one reference.
///
/// # Safety
/// Hooks set.
pub unsafe fn geo_create(num_verts: i32, num_tris: i32, flags: u32) -> *mut Geometry {
    unsafe {
        let h = hooks();
        let geo = alloc((h.geo_size)()) as *mut Geometry;
        if geo.is_null() {
            return geo;
        }
        (h.allocated)(0, 1);
        let g = &mut *geo;
        g.object = Object { kind: GEOMETRY_ID, sub_type: 0, flags: 0, private_flags: 0, parent: std::ptr::null_mut() };
        g.flags = flags & 0xFF00FFFF;
        g.num_tex_coord_sets = ((flags & 0xFF0000) >> 16) as i32;
        if g.num_tex_coord_sets == 0 {
            g.num_tex_coord_sets = if g.flags & TEXTURED != 0 {
                1
            } else if g.flags & TEXTURED2 != 0 {
                2
            } else {
                0
            };
        }
        g.num_triangles = num_tris;
        g.num_vertices = num_verts;
        g.colors = std::ptr::null_mut();
        g.tex_coords = [std::ptr::null_mut(); 8];
        g.triangles = std::ptr::null_mut();
        if g.flags & NATIVE == 0 {
            let nt = g.num_triangles as usize;
            let nv = g.num_vertices as usize;
            let mut sz = nt * size_of::<Triangle>();
            if g.flags & PRELIT != 0 {
                sz += nv * size_of::<Rgba>();
            }
            sz += g.num_tex_coord_sets as usize * nv * 8;
            let mut data = alloc(sz);
            g.triangles = data as *mut Triangle;
            data = data.add(nt * size_of::<Triangle>());
            if g.flags & PRELIT != 0 && nv != 0 {
                g.colors = data as *mut Rgba;
                data = data.add(nv * size_of::<Rgba>());
            }
            if nv != 0 {
                for i in 0..g.num_tex_coord_sets as usize {
                    g.tex_coords[i] = data as *mut [f32; 2];
                    data = data.add(nv * 8);
                }
            }
            for i in 0..nt {
                (*g.triangles.add(i)).mat_id = 0xFFFF;
            }
        }
        g.num_morph_targets = 0;
        g.morph_targets = std::ptr::null_mut();
        geo_add_morph_targets(geo, 1);
        matlist_init(&mut (*geo).mat_list);
        let g = &mut *geo;
        g.locked_since_inst = 0;
        g.mesh_header = std::ptr::null_mut();
        g.inst_data = std::ptr::null_mut();
        g.ref_count = 1;
        (h.geo_construct)(geo);
        geo
    }
}

/// Geometry::destroy: drops a reference; the last frees the blocks and the material list.
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_destroy(geo: *mut Geometry) {
    unsafe {
        (*geo).ref_count -= 1;
        if (*geo).ref_count <= 0 {
            let h = hooks();
            (h.geo_destruct)(geo);
            free((*geo).triangles);
            free((*geo).morph_targets);
            free((*geo).mesh_header);
            matlist_deinit(&mut (*geo).mat_list);
            free(geo);
            (h.allocated)(0, -1);
        }
    }
}

/// Geometry::lock: locking the polygons drops the meshes (rebuilt by unlock).
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_lock(geo: *mut Geometry, lock_flags: i32) {
    unsafe {
        (*geo).locked_since_inst |= lock_flags as u16;
        if lock_flags & LOCKPOLYGONS != 0 {
            free((*geo).mesh_header);
            (*geo).mesh_header = std::ptr::null_mut();
        }
    }
}

/// Geometry::unlock
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_unlock(geo: *mut Geometry) {
    unsafe {
        if (*geo).mesh_header.is_null() {
            geo_build_meshes(geo);
        }
    }
}

/// Geometry::addMorphTargets: the morph targets and then their vertex data, in one block; the
/// existing targets' data moves up to make room for the new headers.
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_add_morph_targets(geo: *mut Geometry, n: i32) {
    unsafe {
        if n == 0 {
            return;
        }
        let g = &mut *geo;
        let old = g.num_morph_targets as usize;
        let n = n as usize + old;
        let nv = g.num_vertices as usize;
        let mut sz = size_of::<MorphTarget>();
        if g.flags & NATIVE == 0 {
            sz += nv * size_of::<V3>();
            if g.flags & NORMALS != 0 {
                sz += nv * size_of::<V3>();
            }
        }
        let mts: *mut MorphTarget;
        if old != 0 {
            mts = (hooks().realloc)(g.morph_targets as *mut u8, n * sz) as *mut MorphTarget;
            g.morph_targets = mts;
            // move the vertex data up, last byte first (the regions overlap)
            let mut src = (mts as *mut u8).add(sz * old);
            let mut dst = (mts as *mut u8).add(sz * n);
            for _ in 0..(sz - size_of::<MorphTarget>()) * old {
                dst = dst.sub(1);
                src = src.sub(1);
                *dst = *src;
            }
        } else {
            mts = alloc(n * sz) as *mut MorphTarget;
            g.morph_targets = mts;
        }
        let mut data = mts.add(n) as *mut V3;
        for i in 0..n {
            let m = &mut *mts.add(i);
            m.parent = geo;
            m.vertices = std::ptr::null_mut();
            m.normals = std::ptr::null_mut();
            if i >= old {
                m.bounding_sphere = Sphere { center: V3 { x: 0.0, y: 0.0, z: 0.0 }, radius: 0.0 };
            }
            if g.flags & NATIVE == 0 && nv != 0 {
                m.vertices = data;
                data = data.add(nv);
                if g.flags & NORMALS != 0 {
                    m.normals = data;
                    data = data.add(nv);
                }
            }
        }
        g.num_morph_targets = n as i32;
    }
}

/// Geometry::calculateBoundingSphere: every morph target's (librw's ±1e6 start box gives a
/// huge sphere when there are no vertices).
///
/// # Safety
/// A live geometry whose morph targets have vertices when it has any.
pub unsafe fn geo_calculate_bounding_sphere(geo: *mut Geometry) {
    unsafe {
        let g = &*geo;
        for i in 0..g.num_morph_targets as usize {
            let m = &mut *g.morph_targets.add(i);
            let verts: &[V3] = if g.num_vertices > 0 { std::slice::from_raw_parts(m.vertices, g.num_vertices as usize) } else { &[] };
            m.bounding_sphere = bounding_sphere(verts);
        }
    }
}

/// Geometry::hasColoredMaterial: any material colour other than opaque white.
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_has_colored_material(geo: *const Geometry) -> bool {
    unsafe {
        let ml = &(*geo).mat_list;
        (0..ml.num_materials as usize).any(|i| (**ml.materials.add(i)).color != Rgba { r: 255, g: 255, b: 255, a: 255 })
    }
}

/// One Xbox vertex of the lightmap sidecar: its key (revc_import::xboxlm::vertex_key) and second
/// texture coordinates.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KeyedUv {
    pub key: u32,
    pub uv: [f32; 2],
}

/// Adds the Xbox lightmap coordinates (core/XboxLightmaps.cpp): each vertex gets the coordinates
/// of the Xbox vertex with its key (position and first coordinates). The PC release moved a few
/// vertices of some models by up to ~0.3 m (oceanrda05_dy: 5 of 466); up to 1 in 20 vertices
/// without a key take the coordinates of a matched vertex they share a triangle with (preferring
/// one with the same first coordinates), as lightmaps change slowly. False, with nothing changed,
/// if more are missing or one has no matched neighbour (another model, or a mod's).
///
/// # Safety
/// A live geometry; `xbox` holds `n` entries (any alignment).
pub unsafe fn geo_add_lightmap_coords(geo: *mut Geometry, xbox: *const KeyedUv, n: i32) -> bool {
    unsafe {
        let g = &*geo;
        let nv = g.num_vertices.max(0) as usize;
        if nv == 0 || n <= 0 || g.num_tex_coord_sets < 1 || g.morph_targets.is_null() || g.flags & NATIVE != 0 {
            return false;
        }
        let pos = (*g.morph_targets).vertices;
        let uv0 = g.tex_coords[0];
        if pos.is_null() || uv0.is_null() {
            return false;
        }
        // (the sidecar's entries needn't be aligned)
        let mut map = std::collections::HashMap::with_capacity(n as usize);
        for i in 0..n as usize {
            let x = std::ptr::read_unaligned(xbox.add(i));
            map.entry(x.key).or_insert(x.uv);
        }
        let mut uv1: Vec<Option<[f32; 2]>> = (0..nv)
            .map(|i| {
                let p = *pos.add(i);
                map.get(&revc_import::xboxlm::vertex_key([p.x, p.y, p.z], *uv0.add(i))).copied()
            })
            .collect();
        let missing: Vec<usize> = (0..nv).filter(|&i| uv1[i].is_none()).collect();
        if missing.len() * 20 > nv {
            return false;
        }
        if !missing.is_empty() {
            let tris = std::slice::from_raw_parts(g.triangles, g.num_triangles.max(0) as usize);
            for &i in &missing {
                let mut pick: Option<usize> = None;
                for t in tris.iter().filter(|t| t.v.contains(&(i as u16))) {
                    for &k in &t.v {
                        let k = k as usize;
                        if k >= nv || k == i || missing.contains(&k) {
                            continue;
                        }
                        if *uv0.add(k) == *uv0.add(i) {
                            pick = Some(k);
                            break;
                        }
                        pick.get_or_insert(k);
                    }
                }
                match pick {
                    Some(k) => uv1[i] = uv1[k],
                    None => return false,
                }
            }
        }
        let uv1: Vec<[f32; 2]> = uv1.into_iter().map(|u| u.unwrap()).collect();
        geo_add_tex_coord_set(geo, uv1.as_ptr(), nv as i32)
    }
}

/// Adds a texture coordinate set (Xbox world lightmaps, geo_add_lightmap_coords): a new data
/// block with the triangles, colours and existing sets plus `uv`, the old one freed, and the
/// geometry marked for re-instancing. False (nothing changed) if the vertex count differs, the
/// geometry is native or already has 8 sets.
///
/// # Safety
/// A live geometry; `uv` holds `nv` pairs.
pub unsafe fn geo_add_tex_coord_set(geo: *mut Geometry, uv: *const [f32; 2], nv: i32) -> bool {
    unsafe {
        let g = &mut *geo;
        if nv != g.num_vertices || g.flags & NATIVE != 0 || g.num_tex_coord_sets >= 8 || g.triangles.is_null() {
            return false;
        }
        let nt = g.num_triangles as usize;
        let nv = nv as usize;
        let sets = g.num_tex_coord_sets as usize;
        let prelit = g.flags & PRELIT != 0 && !g.colors.is_null();
        let tsz = nt * size_of::<Triangle>();
        let csz = if prelit { nv * size_of::<Rgba>() } else { 0 };
        let old = g.triangles as *mut u8;
        let mut data = alloc(tsz + csz + (sets + 1) * nv * 8);
        std::ptr::copy_nonoverlapping(old, data, tsz);
        g.triangles = data as *mut Triangle;
        data = data.add(tsz);
        if prelit {
            std::ptr::copy_nonoverlapping(g.colors as *const u8, data, csz);
            g.colors = data as *mut Rgba;
            data = data.add(csz);
        }
        for i in 0..sets {
            std::ptr::copy_nonoverlapping(g.tex_coords[i] as *const u8, data, nv * 8);
            g.tex_coords[i] = data as *mut [f32; 2];
            data = data.add(nv * 8);
        }
        std::ptr::copy_nonoverlapping(uv as *const u8, data, nv * 8);
        g.tex_coords[sets] = data as *mut [f32; 2];
        g.num_tex_coord_sets = sets as i32 + 1;
        if g.num_tex_coord_sets >= 2 {
            g.flags |= TEXTURED2;
        }
        free(old);
        g.locked_since_inst |= 0x0FFF; // rw::Geometry::LOCKALL
        true
    }
}

/// Geometry::allocateData: the attribute and vertex blocks even when the geometry is native
/// (the bounding spheres are kept by the reallocation).
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_allocate_data(geo: *mut Geometry) {
    unsafe {
        let g = &mut *geo;
        let nt = g.num_triangles as usize;
        let nv = g.num_vertices as usize;
        let mut sz = nt * size_of::<Triangle>();
        if g.flags & PRELIT != 0 {
            sz += nv * size_of::<Rgba>();
        }
        sz += g.num_tex_coord_sets as usize * nv * 8;
        let mut data = alloc(sz);
        g.triangles = data as *mut Triangle;
        data = data.add(nt * size_of::<Triangle>());
        for i in 0..nt {
            (*g.triangles.add(i)).mat_id = 0xFFFF;
        }
        if g.flags & PRELIT != 0 {
            g.colors = data as *mut Rgba;
            data = data.add(nv * size_of::<Rgba>());
        }
        for i in 0..g.num_tex_coord_sets as usize {
            g.tex_coords[i] = data as *mut [f32; 2];
            data = data.add(nv * 8);
        }
        let mut sz = size_of::<MorphTarget>() + nv * size_of::<V3>();
        if g.flags & NORMALS != 0 {
            sz += nv * size_of::<V3>();
        }
        let nm = g.num_morph_targets as usize;
        let mt = (hooks().realloc)(g.morph_targets as *mut u8, sz * nm) as *mut MorphTarget;
        g.morph_targets = mt;
        let mut vdata = mt.add(nm) as *mut V3;
        for i in 0..nm {
            let m = &mut *mt.add(i);
            m.parent = geo;
            m.vertices = std::ptr::null_mut();
            m.normals = std::ptr::null_mut();
            if nv != 0 {
                m.vertices = vdata;
                vdata = vdata.add(nv);
                if g.flags & NORMALS != 0 {
                    m.normals = vdata;
                    vdata = vdata.add(nv);
                }
            }
        }
    }
}

unsafe fn triangles<'a>(g: &Geometry) -> &'a [Triangle] {
    unsafe { if g.num_triangles > 0 { std::slice::from_raw_parts(g.triangles, g.num_triangles as usize) } else { &[] } }
}

/// Geometry::buildMeshes: one mesh per material, as triangle lists or (TRISTRIP) strips.
///
/// # Safety
/// A live geometry whose triangles' material ids are below its material count.
pub unsafe fn geo_build_meshes(geo: *mut Geometry) {
    unsafe {
        if (*geo).flags & NATIVE != 0 {
            (hooks().native_warning)();
            return;
        }
        free((*geo).mesh_header);
        (*geo).mesh_header = std::ptr::null_mut();
        let num_meshes = (*geo).mat_list.num_materials;
        if (*geo).flags & TRISTRIP != 0 {
            geo_build_tristrips(geo);
            return;
        }
        let lists = list_meshes(triangles(&*geo), num_meshes.max(0) as usize);
        let mh = geo_allocate_meshes(geo, num_meshes, ((*geo).num_triangles * 3) as u32, false);
        let ms = meshes(mh);
        for (i, l) in lists.iter().enumerate() {
            (*ms.add(i)).material = *(*geo).mat_list.materials.add(i);
            (*ms.add(i)).num_indices = l.len() as u32;
        }
        header_setup_indices(mh);
        for (i, l) in lists.iter().enumerate() {
            std::ptr::copy_nonoverlapping(l.as_ptr(), (*ms.add(i)).indices, l.len());
        }
    }
}

/// Geometry::buildTristrips (tristrip.cpp), then verifyMesh.
///
/// # Safety
/// As geo_build_meshes, every material used.
pub unsafe fn geo_build_tristrips(geo: *mut Geometry) {
    unsafe {
        let n = (*geo).mat_list.num_materials;
        let strips = strip_meshes(triangles(&*geo), n.max(0) as usize);
        let total: u32 = strips.iter().map(|s| s.len() as u32).sum();
        // librw allocated an empty header first and the real one after the strips were made:
        // two serial numbers
        NEXT_SERIAL = NEXT_SERIAL.wrapping_add(1);
        let mh = geo_allocate_meshes(geo, n, total, false);
        (*mh).flags = MESH_TRISTRIP;
        let ms = meshes(mh);
        for (i, s) in strips.iter().enumerate() {
            (*ms.add(i)).material = *(*geo).mat_list.materials.add(i);
            (*ms.add(i)).num_indices = s.len() as u32;
        }
        header_setup_indices(mh);
        for (i, s) in strips.iter().enumerate() {
            std::ptr::copy_nonoverlapping(s.as_ptr(), (*ms.add(i)).indices, s.len());
        }
        verify_mesh(geo);
    }
}

/// tristrip.cpp's verifyMesh: every strip triangle is one of the geometry's (with its
/// material), and every triangle appears.
///
/// # Safety
/// A live geometry with meshes.
pub unsafe fn verify_mesh(geo: *mut Geometry) {
    unsafe {
        let g = &*geo;
        let tris = triangles(g);
        let mut seen = vec![false; tris.len()];
        let mh = &*g.mesh_header;
        let ms = meshes(g.mesh_header);
        let nv = g.num_vertices;
        for i in 0..mh.num_meshes as usize {
            let mesh = &*ms.add(i);
            let mat = matlist_find(&g.mat_list, mesh.material);
            let mut x = 0usize;
            let n = mesh.num_indices as usize;
            for j in 0..n.saturating_sub(2) {
                let a = *mesh.indices.add(j + x) as i32;
                x = 1 - x;
                let b = *mesh.indices.add(j + x) as i32;
                let c = *mesh.indices.add(j + 2) as i32;
                if a >= nv || b >= nv || c >= nv {
                    (hooks().verify_failed)();
                    return;
                }
                if a == b || a == c || b == c {
                    continue;
                }
                let found = tris.iter().enumerate().position(|(k, t)| {
                    let v = t.v.map(|v| v as i32);
                    !seen[k] && t.mat_id as i32 == mat && ((v[0] == a && v[1] == b && v[2] == c) || (v[1] == a && v[2] == b && v[0] == c) || (v[2] == a && v[0] == b && v[1] == c))
                });
                match found {
                    Some(k) => seen[k] = true,
                    None => {
                        (hooks().verify_failed)();
                        return;
                    }
                }
            }
        }
        if seen.iter().any(|&s| !s) {
            (hooks().verify_failed)();
        }
    }
}

// ---------------------------------------------------------------- the mesh header (geoplg.cpp)

/// Geometry::allocateMeshes: a header with `num_meshes` meshes (existing ones keep their
/// material and index count) and room for `num_indices` indices unless `no_indices`.
///
/// # Safety
/// A live geometry.
pub unsafe fn geo_allocate_meshes(geo: *mut Geometry, num_meshes: i32, num_indices: u32, no_indices: bool) -> *mut MeshHeader {
    unsafe {
        let mut sz = size_of::<MeshHeader>() + num_meshes as usize * size_of::<Mesh>();
        if !no_indices {
            sz += num_indices as usize * 2;
        }
        let mh: *mut MeshHeader;
        let old: i32;
        if !(*geo).mesh_header.is_null() {
            old = (*(*geo).mesh_header).num_meshes as i32;
            mh = (hooks().realloc)((*geo).mesh_header as *mut u8, sz) as *mut MeshHeader;
        } else {
            old = 0;
            mh = alloc(sz) as *mut MeshHeader;
            (*mh).flags = 0;
        }
        (*geo).mesh_header = mh;
        (*mh).num_meshes = num_meshes as u16;
        (*mh).serial_num = NEXT_SERIAL;
        NEXT_SERIAL = NEXT_SERIAL.wrapping_add(1);
        (*mh).total_indices = num_indices;
        let ms = meshes(mh);
        let mut indices = ms.add(num_meshes as usize) as *mut u16;
        for i in 0..(*mh).num_meshes as usize {
            let m = &mut *ms.add(i);
            if i as i32 >= old {
                m.material = std::ptr::null_mut();
                m.num_indices = 0;
            }
            if no_indices {
                m.indices = std::ptr::null_mut();
            } else {
                m.indices = indices;
                indices = indices.add(m.num_indices as usize);
            }
        }
        mh
    }
}

/// MeshHeader::setupIndices: the meshes' index pointers laid out from the first one's.
///
/// # Safety
/// A header with at least one mesh (librw reads the first).
pub unsafe fn header_setup_indices(mh: *mut MeshHeader) {
    unsafe {
        let ms = meshes(mh);
        let mut indices = (*ms).indices;
        if indices.is_null() {
            return;
        }
        for i in 0..(*mh).num_meshes as usize {
            (*ms.add(i)).indices = indices;
            indices = indices.add((*ms.add(i)).num_indices as usize);
        }
    }
}

/// MeshHeader::guessNumTriangles: the most triangles the indices can hold.
///
/// # Safety
/// A valid header.
pub unsafe fn header_guess_num_triangles(mh: *const MeshHeader) -> u32 {
    unsafe {
        let h = &*mh;
        if h.flags == MESH_TRISTRIP { h.total_indices.wrapping_sub(2 * h.num_meshes as u32) } else { h.total_indices / 3 }
    }
}

// ---------------------------------------------------------------- MaterialList, Material

/// MaterialList::init
///
/// # Safety
/// A writable list.
pub unsafe fn matlist_init(ml: *mut MaterialList) {
    unsafe {
        (*ml).materials = std::ptr::null_mut();
        (*ml).num_materials = 0;
        (*ml).space = 0;
    }
}

/// MaterialList::deinit: drops a reference to every entry.
///
/// # Safety
/// A valid list.
pub unsafe fn matlist_deinit(ml: *mut MaterialList) {
    unsafe {
        if !(*ml).materials.is_null() {
            for i in 0..(*ml).num_materials as usize {
                mat_destroy(*(*ml).materials.add(i));
            }
            free((*ml).materials);
        }
    }
}

/// MaterialList::appendMaterial: the new index (the list grows by 20), -1 when out of memory.
///
/// # Safety
/// A valid list and material.
pub unsafe fn matlist_append(ml: *mut MaterialList, mat: *mut Material) -> i32 {
    unsafe {
        let l = &mut *ml;
        if l.num_materials >= l.space {
            let space = l.space + 20;
            let bytes = space as usize * size_of::<*mut Material>();
            let p = if l.materials.is_null() { alloc(bytes) } else { (hooks().realloc)(l.materials as *mut u8, bytes) } as *mut *mut Material;
            if p.is_null() {
                return -1;
            }
            l.space = space;
            l.materials = p;
        }
        *l.materials.add(l.num_materials as usize) = mat;
        l.num_materials += 1;
        (*mat).ref_count += 1;
        l.num_materials - 1
    }
}

/// MaterialList::findIndex
///
/// # Safety
/// A valid list.
pub unsafe fn matlist_find(ml: *const MaterialList, mat: *mut Material) -> i32 {
    unsafe { (0..(*ml).num_materials).find(|&i| *(*ml).materials.add(i as usize) == mat).unwrap_or(-1) }
}

/// Material::create: white, surface properties 1/1/1, one reference.
///
/// # Safety
/// Hooks set.
pub unsafe fn mat_create() -> *mut Material {
    unsafe {
        let h = hooks();
        let mat = alloc((h.mat_size)()) as *mut Material;
        if mat.is_null() {
            return mat;
        }
        (h.allocated)(1, 1);
        (*mat).texture = std::ptr::null_mut();
        (*mat).color = Rgba { r: 255, g: 255, b: 255, a: 255 };
        (*mat).surface_props = SurfaceProperties { ambient: 1.0, specular: 1.0, diffuse: 1.0 };
        (*mat).pipeline = std::ptr::null_mut();
        (*mat).ref_count = 1;
        (h.mat_construct)(mat);
        mat
    }
}

/// Material::clone
///
/// # Safety
/// A live material.
pub unsafe fn mat_clone(m: *mut Material) -> *mut Material {
    unsafe {
        let mat = mat_create();
        if mat.is_null() {
            return mat;
        }
        (*mat).color = (*m).color;
        (*mat).surface_props = (*m).surface_props;
        if !(*m).texture.is_null() {
            mat_set_texture(mat, (*m).texture);
        }
        (*mat).pipeline = (*m).pipeline;
        (hooks().mat_copy)(mat, m);
        mat
    }
}

/// Material::destroy: drops a reference; the last releases the texture.
///
/// # Safety
/// A live material.
pub unsafe fn mat_destroy(m: *mut Material) {
    unsafe {
        (*m).ref_count -= 1;
        if (*m).ref_count <= 0 {
            let h = hooks();
            (h.mat_destruct)(m);
            if !(*m).texture.is_null() {
                tex_destroy((*m).texture);
            }
            free(m);
            (h.allocated)(1, -1);
        }
    }
}

/// Material::setTexture: releases the old texture, takes a reference to the new one.
///
/// # Safety
/// A live material; `tex` live or null.
pub unsafe fn mat_set_texture(m: *mut Material, tex: *mut Texture) {
    unsafe {
        if !(*m).texture.is_null() {
            tex_destroy((*m).texture);
        }
        if !tex.is_null() {
            (*tex).ref_count += 1;
        }
        (*m).texture = tex;
    }
}

// ---------------------------------------------------------------- C ABI (geometry.cpp, geoplg.cpp forward)

/// # Safety
/// Called once at startup, before any geometry exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_geometry_set_hooks(h: Hooks) {
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
    revc_geo_create(nv: i32, nt: i32, flags: u32) -> *mut Geometry => geo_create(nv, nt, flags);
    revc_geo_destroy(g: *mut Geometry) => geo_destroy(g);
    revc_geo_lock(g: *mut Geometry, f: i32) => geo_lock(g, f);
    revc_geo_unlock(g: *mut Geometry) => geo_unlock(g);
    revc_geo_add_morph_targets(g: *mut Geometry, n: i32) => geo_add_morph_targets(g, n);
    revc_geo_calculate_bounding_sphere(g: *mut Geometry) => geo_calculate_bounding_sphere(g);
    revc_geo_has_colored_material(g: *const Geometry) -> i32 => geo_has_colored_material(g) as i32;
    revc_geo_allocate_data(g: *mut Geometry) => geo_allocate_data(g);
    revc_geo_add_tex_coord_set(g: *mut Geometry, uv: *const [f32; 2], nv: i32) -> i32 => geo_add_tex_coord_set(g, uv, nv) as i32;
    revc_geo_add_lightmap_coords(g: *mut Geometry, xbox: *const KeyedUv, n: i32) -> i32 => geo_add_lightmap_coords(g, xbox, n) as i32;
    revc_geo_build_meshes_obj(g: *mut Geometry) => geo_build_meshes(g);
    revc_geo_build_tristrips(g: *mut Geometry) => geo_build_tristrips(g);
    revc_geo_allocate_meshes(g: *mut Geometry, n: i32, idx: u32, no: i32) -> *mut MeshHeader => geo_allocate_meshes(g, n, idx, no != 0);
    revc_mesh_setup_indices(mh: *mut MeshHeader) => header_setup_indices(mh);
    revc_mesh_guess_num_triangles(mh: *const MeshHeader) -> u32 => header_guess_num_triangles(mh);
    revc_matlist_init(ml: *mut MaterialList) => matlist_init(ml);
    revc_matlist_deinit(ml: *mut MaterialList) => matlist_deinit(ml);
    revc_matlist_append(ml: *mut MaterialList, m: *mut Material) -> i32 => matlist_append(ml, m);
    revc_matlist_find(ml: *const MaterialList, m: *mut Material) -> i32 => matlist_find(ml, m);
    revc_mat_create() -> *mut Material => mat_create();
    revc_mat_clone(m: *mut Material) -> *mut Material => mat_clone(m);
    revc_mat_destroy(m: *mut Material) => mat_destroy(m);
    revc_mat_set_texture(m: *mut Material, t: *mut Texture) => mat_set_texture(m, t);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwgeomobj.txt (recorded from librw's C++, see the recorder).
    use super::*;
    use crate::rwframe::list_init;
    use crate::rwtexture::{Hooks as TexHooks, TexDictionary, TextureGlobals, revc_tex_set_hooks, tex_create};
    use std::cell::RefCell;

    thread_local! {
        static GEOS: RefCell<Vec<*mut Geometry>> = const { RefCell::new(Vec::new()) };
        static MATS: RefCell<Vec<*mut Material>> = const { RefCell::new(Vec::new()) };
        static TEXS: RefCell<Vec<*mut Texture>> = const { RefCell::new(Vec::new()) };
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static ALLOC: RefCell<[i32; 2]> = const { RefCell::new([0; 2]) };
    }
    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn id_of<T>(v: &RefCell<Vec<*mut T>>, p: *mut T) -> i32 {
        if p.is_null() {
            return -1;
        }
        v.borrow().iter().position(|&q| q == p).map_or(-2, |i| i as i32)
    }
    fn gid(p: *mut Geometry) -> i32 {
        GEOS.with(|v| id_of(v, p))
    }
    fn mid(p: *mut Material) -> i32 {
        MATS.with(|v| id_of(v, p))
    }
    fn tid(p: *mut Texture) -> i32 {
        TEXS.with(|v| id_of(v, p))
    }
    fn add<T>(v: &'static std::thread::LocalKey<RefCell<Vec<*mut T>>>, p: *mut T) -> usize {
        v.with(|v| {
            v.borrow_mut().push(p);
            v.borrow().len() - 1
        })
    }
    fn remove<T>(v: &'static std::thread::LocalKey<RefCell<Vec<*mut T>>>, i: i32) {
        if i >= 0 {
            v.with(|v| v.borrow_mut()[i as usize] = std::ptr::null_mut());
        }
    }

    // the recorder's calloc/realloc/free: a size header before each block
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
    unsafe extern "C" fn geo_size() -> usize {
        size_of::<Geometry>()
    }
    unsafe extern "C" fn mat_size() -> usize {
        size_of::<Material>()
    }
    unsafe extern "C" fn geo_construct(g: *mut Geometry) {
        log(format!(" cg{}", add(&GEOS, g)));
    }
    unsafe extern "C" fn geo_destruct(g: *mut Geometry) {
        let i = gid(g);
        log(format!(" dg{i}"));
        remove(&GEOS, i);
    }
    unsafe extern "C" fn mat_construct(m: *mut Material) {
        log(format!(" cm{}", add(&MATS, m)));
    }
    unsafe extern "C" fn mat_destruct(m: *mut Material) {
        let i = mid(m);
        log(format!(" dm{i}"));
        remove(&MATS, i);
    }
    unsafe extern "C" fn mat_copy(d: *mut Material, s: *mut Material) {
        log(format!(" ym{},{}", mid(d), mid(s)));
    }
    unsafe extern "C" fn allocated(kind: i32, delta: i32) {
        ALLOC.with(|a| a.borrow_mut()[kind as usize] += delta);
    }
    unsafe extern "C" fn native_warning() {
        panic!("buildMeshes on a native geometry");
    }
    unsafe extern "C" fn verify_failed() {
        panic!("TRISTRIP verify failed");
    }
    unsafe extern "C" fn tex_size() -> usize {
        size_of::<Texture>() + 8
    }
    unsafe extern "C" fn tex_construct(t: *mut Texture) {
        log(format!(" ct{}", add(&TEXS, t)));
    }
    unsafe extern "C" fn tex_destruct(t: *mut Texture) {
        let i = tid(t);
        log(format!(" dt{i}"));
        remove(&TEXS, i);
    }
    unsafe extern "C" fn dict_size() -> usize {
        size_of::<TexDictionary>()
    }
    unsafe extern "C" fn dict_hook(_: *mut TexDictionary) {}
    unsafe extern "C" fn raster_destroy(_: *mut c_void) {}
    unsafe extern "C" fn raster_dummy() -> *mut c_void {
        std::ptr::null_mut()
    }
    unsafe extern "C" fn tex_allocated(_: i32, _: i32) {}

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
        unsafe fn bytes(&mut self, p: *const u8, n: usize) {
            for k in 0..n {
                self.b(unsafe { *p.add(k) });
            }
        }
        /// the recorder's hptr: -1 null, the offset from base when inside [base, base+len], else -2
        fn ptr<T>(&mut self, p: *const T, base: *const u8, len: i64) {
            if p.is_null() {
                return self.i(-1);
            }
            let d = p as i64 - base as i64;
            self.i(if !base.is_null() && d >= 0 && d < len + 1 { d as i32 } else { -2 });
        }
    }

    unsafe fn state() -> String {
        unsafe {
            let mut h = H(0xcbf29ce484222325);
            for (i, &g) in GEOS.with(|v| v.borrow().clone()).iter().enumerate() {
                if g.is_null() {
                    continue;
                }
                let g = &*g;
                h.i(i as i32);
                for b in [g.object.kind, g.object.sub_type, g.object.flags, g.object.private_flags] {
                    h.b(b);
                }
                for v in [g.flags as i32, g.locked_since_inst as i32, g.num_triangles, g.num_vertices, g.num_morph_targets, g.num_tex_coord_sets] {
                    h.i(v);
                }
                let nv = g.num_vertices as i64;
                let alen = g.num_triangles as i64 * 8 + if g.flags & PRELIT != 0 { nv * 4 } else { 0 } + g.num_tex_coord_sets as i64 * nv * 8;
                let ab = g.triangles as *const u8;
                h.i(!ab.is_null() as i32);
                if !ab.is_null() {
                    h.bytes(ab, alen as usize);
                }
                h.ptr(g.colors, ab, alen);
                for k in 0..8 {
                    h.ptr(g.tex_coords[k], ab, alen);
                }
                let mb = g.morph_targets as *const u8;
                let mut mlen = g.num_morph_targets as i64 * size_of::<MorphTarget>() as i64;
                if g.flags & NATIVE == 0 {
                    mlen += g.num_morph_targets as i64 * nv * 12 * if g.flags & NORMALS != 0 { 2 } else { 1 };
                }
                for k in 0..g.num_morph_targets as usize {
                    let m = &*g.morph_targets.add(k);
                    h.i(gid(m.parent));
                    h.bytes(&m.bounding_sphere as *const Sphere as *const u8, 16);
                    h.ptr(m.vertices, mb, mlen);
                    h.ptr(m.normals, mb, mlen);
                    if !m.vertices.is_null() {
                        h.bytes(m.vertices as *const u8, nv as usize * 12);
                    }
                    if !m.normals.is_null() {
                        h.bytes(m.normals as *const u8, nv as usize * 12);
                    }
                }
                h.i(g.mat_list.num_materials);
                h.i(g.mat_list.space);
                for k in 0..g.mat_list.num_materials as usize {
                    h.i(mid(*g.mat_list.materials.add(k)));
                }
                let mh = g.mesh_header;
                h.i(!mh.is_null() as i32);
                if !mh.is_null() {
                    let m = &*mh;
                    for v in [m.flags as i32, m.num_meshes as i32, m.serial_num as i32, m.total_indices as i32] {
                        h.i(v);
                    }
                    let ms = meshes(mh);
                    let ib = ms.add(m.num_meshes as usize) as *const u8;
                    for k in 0..m.num_meshes as usize {
                        let me = &*ms.add(k);
                        h.i(me.num_indices as i32);
                        h.i(mid(me.material));
                        h.ptr(me.indices, ib, m.total_indices as i64 * 2);
                        if !me.indices.is_null() {
                            h.bytes(me.indices as *const u8, me.num_indices as usize * 2);
                        }
                    }
                }
                h.i(!g.inst_data.is_null() as i32);
                h.i(g.ref_count);
            }
            for (i, &m) in MATS.with(|v| v.borrow().clone()).iter().enumerate() {
                if m.is_null() {
                    continue;
                }
                let m = &*m;
                h.i(i as i32);
                h.i(tid(m.texture));
                h.bytes(&m.color as *const Rgba as *const u8, 4);
                h.bytes(&m.surface_props as *const SurfaceProperties as *const u8, 12);
                h.i(!m.pipeline.is_null() as i32);
                h.i(m.ref_count);
            }
            for (i, &t) in TEXS.with(|v| v.borrow().clone()).iter().enumerate() {
                if t.is_null() {
                    continue;
                }
                h.i(i as i32);
                h.i((*t).ref_count);
            }
            let l = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            let a = ALLOC.with(|a| *a.borrow());
            format!(" {:016x} |{l} | {} {}", h.0, a[0], a[1])
        }
    }

    /// The recorder's fill: triangles, colours, tex coords, vertices, normals from a seed.
    unsafe fn fill(g: *mut Geometry, seed: u32) {
        unsafe {
            let g = &mut *g;
            let mut s = seed | 1;
            let mut nx = || {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                s
            };
            let nv = g.num_vertices as u32;
            let nm = g.mat_list.num_materials as u32;
            if !g.triangles.is_null() {
                for i in 0..g.num_triangles as usize {
                    let t = &mut *g.triangles.add(i);
                    if nv >= 3 {
                        t.v[0] = (nx() % nv) as u16;
                        loop {
                            t.v[1] = (nx() % nv) as u16;
                            if t.v[1] != t.v[0] {
                                break;
                            }
                        }
                        loop {
                            t.v[2] = (nx() % nv) as u16;
                            if t.v[2] != t.v[0] && t.v[2] != t.v[1] {
                                break;
                            }
                        }
                    }
                    t.mat_id = if nm != 0 { (nx() % nm) as u16 } else { 0xFFFF };
                }
            }
            if !g.colors.is_null() {
                for i in 0..(nv * 4) as usize {
                    *(g.colors as *mut u8).add(i) = nx() as u8;
                }
            }
            for k in 0..g.num_tex_coord_sets as usize {
                if !g.tex_coords[k].is_null() {
                    for i in 0..(nv * 2) as usize {
                        *(g.tex_coords[k] as *mut f32).add(i) = (nx() & 0xFFFF) as f32 / 256.0;
                    }
                }
            }
            for k in 0..g.num_morph_targets as usize {
                let m = &*g.morph_targets.add(k);
                if !m.vertices.is_null() {
                    for i in 0..(nv * 3) as usize {
                        *(m.vertices as *mut f32).add(i) = (nx() % 20001).wrapping_sub(10000) as i32 as f32 / 100.0;
                    }
                }
                if !m.normals.is_null() {
                    for i in 0..(nv * 3) as usize {
                        *(m.normals as *mut f32).add(i) = (nx() % 2001).wrapping_sub(1000) as i32 as f32 / 1000.0;
                    }
                }
            }
        }
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwgeomobj.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwgeomobj.txt");
        unsafe {
            revc_geometry_set_hooks(Hooks {
                alloc: alloc_,
                realloc: realloc_,
                free: free_,
                geo_size,
                geo_construct,
                geo_destruct,
                mat_size,
                mat_construct,
                mat_destruct,
                mat_copy,
                allocated,
                native_warning,
                verify_failed,
            });
            revc_tex_set_hooks(TexHooks {
                tex_size,
                tex_construct,
                tex_destruct,
                dict_size,
                dict_construct: dict_hook,
                dict_destruct: dict_hook,
                raster_destroy,
                raster_create_dummy: raster_dummy,
                allocated: tex_allocated,
            });
        }
        // textureOpen, as the recorder runs it
        let tg: *mut TextureGlobals = Box::into_raw(Box::new(unsafe { std::mem::zeroed() }));
        unsafe {
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
            let (args, expect) = input.split_once(" =").map_or((input, None), |(a, r)| (a, Some(r.trim().split_whitespace().next().unwrap_or("").to_string())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let num = |i: usize| w[i].parse::<i64>().unwrap();
            let geo = |i: usize| GEOS.with(|v| v.borrow()[num(i) as usize]);
            let mat = |i: usize| MATS.with(|v| v.borrow()[num(i) as usize]);
            let res: Option<String> = unsafe {
                match w[0] {
                    "newgeo" => Some(gid(geo_create(num(1) as i32, num(2) as i32, num(3) as u32)).to_string()),
                    "delgeo" => {
                        geo_destroy(geo(1));
                        None
                    }
                    "addrefgeo" => {
                        (*geo(1)).ref_count += 1;
                        None
                    }
                    "addmorph" => {
                        geo_add_morph_targets(geo(1), num(2) as i32);
                        None
                    }
                    "fill" => {
                        fill(geo(1), num(2) as u32);
                        None
                    }
                    "allocdata" => {
                        geo_allocate_data(geo(1));
                        (*geo(1)).flags &= !NATIVE;
                        None
                    }
                    "spheres" => {
                        geo_calculate_bounding_sphere(geo(1));
                        None
                    }
                    "colored" => Some((geo_has_colored_material(geo(1)) as i32).to_string()),
                    "lock" => {
                        geo_lock(geo(1), num(2) as i32);
                        None
                    }
                    "unlock" => {
                        geo_unlock(geo(1));
                        None
                    }
                    "append" => Some(matlist_append(&mut (*geo(1)).mat_list, mat(2)).to_string()),
                    "find" => Some(matlist_find(&(*geo(1)).mat_list, mat(2)).to_string()),
                    "allocmeshes" => {
                        geo_allocate_meshes(geo(1), num(2) as i32, num(3) as u32, num(4) != 0);
                        None
                    }
                    "setupidx" => {
                        header_setup_indices((*geo(1)).mesh_header);
                        None
                    }
                    "guess" => Some(header_guess_num_triangles((*geo(1)).mesh_header).to_string()),
                    "newmat" => {
                        let m = mat_create();
                        if let Some(c) = input.split(" color ").nth(1) {
                            let c = u32::from_str_radix(c.trim(), 16).unwrap().to_le_bytes();
                            (*m).color = Rgba { r: c[0], g: c[1], b: c[2], a: c[3] };
                        }
                        Some(mid(m).to_string())
                    }
                    "clonemat" => Some(mid(mat_clone(mat(1))).to_string()),
                    "delmat" => {
                        mat_destroy(mat(1));
                        None
                    }
                    "addrefmat" => {
                        (*mat(1)).ref_count += 1;
                        None
                    }
                    "newtex" => Some(tid(tex_create(tg, std::ptr::null_mut())).to_string()),
                    "settex" => {
                        let t = if num(2) < 0 { std::ptr::null_mut() } else { TEXS.with(|v| v.borrow()[num(2) as usize]) };
                        mat_set_texture(mat(1), t);
                        None
                    }
                    "deltex" => {
                        tex_destroy(TEXS.with(|v| v.borrow()[num(1) as usize]));
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
                bad.push(format!("{input}\n  want{want}\n  got {got}"));
            }
        }
        assert!(n > 2000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

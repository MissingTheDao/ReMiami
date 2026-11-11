//! The wgpu model pipeline (stage 2 step 7b): what wgpupipe.cpp did. Geometry is instanced
//! once into a renderer vertex/index buffer (rebuilt when the mesh or locked data change), and
//! every mesh is drawn with the gl3 default pipeline's lighting (prelit colour + ambient +
//! dynamic lights, times the material colour), MatFX environment maps and skinning; mirrors
//! gl3pipe.cpp / gl3render.cpp / gl3device.cpp setLights. The pipeline objects and the driver
//! plugins that create them stay in C++ for now and forward here (wgpupipe.cpp); the game's
//! custom (Neo) pipelines use the same functions by their C++ names.
//!
//! revc's own driver code (stage 1, task 12), checked by the scenario suites.

use crate::host::wgpu as host;
use crate::math::RwMatrix;
use crate::rwdevice::{self as dev, current_draw_state, get_render_state, set_render_state};
use crate::rwframe::{Frame, get_ltm};
use crate::rwgeomobj::{self as geo, Geometry, Material, Mesh, MeshHeader, Rgba, SurfaceProperties};
use crate::rwhanim::HAnimHierarchy;
use crate::rwmatrix;
use crate::rwplugins::{self as plg, Env};
use crate::rwscene::{Atomic, Light, World, WorldLights};
use crate::rwtexture::Texture;
use revc_rw::frame::{EnvMap, Mat4, MeshVertex, ModelState, SkinVertex, Xform};
use std::ffi::c_void;
use std::ptr::{addr_of, addr_of_mut, null, null_mut};

/// rw::wgpu::InstanceData
#[repr(C)]
pub struct InstanceData {
    /// triangle-list index range in the renderer geometry
    pub first: u32,
    pub count: u32,
    pub material: *mut Material,
    pub vertex_alpha: i32,
}

/// rw::wgpu::InstanceDataHeader (rw::InstanceDataHeader's platform first)
#[repr(C)]
pub struct InstanceDataHeader {
    pub platform: u32,
    pub serial_number: u32,
    pub num_meshes: i32,
    pub inst: *mut InstanceData,
    /// renderer geometry id
    pub geometry: u32,
    pub skinned: i32,
}
const _: () = assert!(size_of::<InstanceDataHeader>() == 32 && size_of::<InstanceData>() == 24 && size_of::<ModelState>() == 232 * 4);

/// MatFX's environment map settings (static members of rw::MatFX).
#[repr(C)]
pub struct EnvSettings {
    pub flip_u: i32,
    pub apply_light: i32,
    pub use_mat_color: i32,
    pub color: Rgba,
}

/// What stays in C++ until the engine moves.
#[repr(C)]
pub struct Hooks {
    /// engine->currentWorld
    pub current_world: unsafe extern "C" fn() -> *mut World,
    /// skinGlobals.atomicOffset (the atomic's hierarchy pointer)
    pub skin_atomic_offset: unsafe extern "C" fn() -> i32,
    /// MatFX::envMapFlipU, envMapApplyLight, envMapUseMatColor, envMapColor
    pub env_settings: unsafe extern "C" fn(*mut EnvSettings),
}
static mut HOOKS: Option<Hooks> = None;
fn hooks() -> &'static Hooks {
    unsafe { (*addr_of!(HOOKS)).as_ref().expect("pipeline hooks") }
}

/// # Safety
/// Called once at startup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_pipe_set_hooks(h: Hooks) {
    unsafe { *addr_of_mut!(HOOKS) = Some(h) };
}

const ID_SKIN: u32 = 0x116;
const ID_MATFX: u32 = 0x120;
const LIGHT: u32 = 0x20;
const MODULATE: u32 = 0x40;
const LIGHT_POINT: u8 = 0x80;
const LIGHT_SPOT: u8 = 0x81;
const LIGHT_SOFTSPOT: u8 = 0x82;
const LOCALSPACEMATRICES: i32 = 0x4000;

unsafe fn meshes(mh: *mut MeshHeader) -> *mut Mesh {
    unsafe { mh.add(1) as *mut Mesh }
}

unsafe fn frame_of<T>(o: *const T) -> *mut Frame {
    // ObjectWithFrame starts every framed object; its Object's parent is the frame
    unsafe { (*(o as *const crate::rwframe::ObjectWithFrame)).object.parent as *mut Frame }
}

unsafe fn free_instance_data(g: *mut Geometry) {
    unsafe {
        let header = (*g).inst_data as *mut InstanceDataHeader;
        if header.is_null() || (*header).platform != dev::PLATFORM_WGPU as u32 {
            return;
        }
        (*g).inst_data = null_mut();
        if (*header).geometry != 0 {
            host::revc_rw_geometry_destroy((*header).geometry);
        }
        let n = (*header).num_meshes.max(1) as usize;
        drop(Vec::from_raw_parts((*header).inst, n, n));
        drop(Box::from_raw(header));
    }
}

/// The geometry plugin's destructor: frees the instance data with the geometry.
///
/// # Safety
/// A geometry being destroyed.
pub unsafe fn destroy_native_data(g: *mut Geometry) {
    unsafe { free_instance_data(g) }
}

/// Builds the vertices (GL's defaults for attributes the geometry lacks: normal 0, colour
/// (0,0,0,1), texcoords 0) and triangle-list indices of every mesh, and uploads them.
unsafe fn instance_geometry(g: *mut Geometry, header: *mut InstanceDataHeader) {
    unsafe {
        let gm = &*g;
        let h = &mut *header;
        let mh = gm.mesh_header;
        let nv = gm.num_vertices.max(0) as usize;
        let prelit = gm.flags & geo::PRELIT != 0 && !gm.colors.is_null();
        let mt = &*gm.morph_targets;
        let normals = gm.flags & geo::NORMALS != 0 && !mt.normals.is_null();
        let mut verts = Vec::with_capacity(nv);
        for i in 0..nv {
            let p = *mt.vertices.add(i);
            let n = if normals { *mt.normals.add(i) } else { Default::default() };
            let c = if prelit { *gm.colors.add(i) } else { Rgba { r: 0, g: 0, b: 0, a: 255 } };
            let uv = if gm.num_tex_coord_sets > 0 { *gm.tex_coords[0].add(i) } else { [0.0; 2] };
            let uv2 = if gm.num_tex_coord_sets > 1 { *gm.tex_coords[1].add(i) } else { [0.0; 2] };
            verts.push(MeshVertex { pos: [p.x, p.y, p.z], normal: [n.x, n.y, n.z], color: [c.r, c.g, c.b, c.a], uv, uv2 });
        }

        let tristrip = (*mh).flags == geo::TRISTRIP;
        let ms = std::slice::from_raw_parts_mut(meshes(mh), h.num_meshes as usize);
        let insts = std::slice::from_raw_parts_mut(h.inst, h.num_meshes as usize);
        let mut indices: Vec<u16> = Vec::new();
        for (m, inst) in ms.iter().zip(insts.iter_mut()) {
            inst.first = indices.len() as u32;
            inst.material = m.material;
            let ix = if m.num_indices == 0 { &[][..] } else { std::slice::from_raw_parts(m.indices, m.num_indices as usize) };
            if tristrip {
                // strip to list with the strip's alternating winding; degenerate joins dropped
                for j in 0..ix.len().saturating_sub(2) {
                    let (mut a, mut b, c) = (ix[j], ix[j + 1], ix[j + 2]);
                    if a == b || b == c || a == c {
                        continue;
                    }
                    if j & 1 != 0 {
                        std::mem::swap(&mut a, &mut b);
                    }
                    indices.extend([a, b, c]);
                }
            } else {
                for t in ix.chunks_exact(3) {
                    indices.extend_from_slice(t);
                }
            }
            inst.count = indices.len() as u32 - inst.first;
            // as gl3 instColor: vertex alpha if any prelit colour of the mesh's vertex range
            // isn't opaque
            inst.vertex_alpha = 0;
            if prelit && !ix.is_empty() {
                let (lo, hi) = (*ix.iter().min().unwrap() as usize, *ix.iter().max().unwrap() as usize);
                inst.vertex_alpha = (lo..=hi).any(|k| (*gm.colors.add(k)).a != 0xFF) as i32;
            }
        }

        let skin = if h.skinned != 0 { plg::skin_get(g) } else { null_mut() };
        if !skin.is_null() {
            let s = &*skin;
            let sverts: Vec<SkinVertex> = verts
                .iter()
                .enumerate()
                .map(|(i, v)| SkinVertex { base: *v, weights: *(s.weights.add(i * 4) as *const [f32; 4]), indices: *(s.indices.add(i * 4) as *const [u8; 4]) })
                .collect();
            h.geometry = host::revc_rw_geometry_upload_skin(h.geometry, sverts.as_ptr(), nv as i32, indices.as_ptr(), indices.len() as i32);
        } else {
            h.geometry = host::revc_rw_geometry_upload(h.geometry, verts.as_ptr(), nv as i32, indices.as_ptr(), indices.len() as i32);
        }
    }
}

/// The pipeline's instance callback (`plugin_id` is the pipeline's pluginID).
///
/// # Safety
/// A live atomic with a geometry.
pub unsafe fn instance(plugin_id: u32, atomic: *mut Atomic) {
    unsafe {
        let g = (*atomic).geometry;
        // don't try to (re)instance native data
        if (*g).flags & geo::NATIVE != 0 {
            return;
        }
        let mut header = (*g).inst_data as *mut InstanceDataHeader;
        let wgpu = |h: *mut InstanceDataHeader| !h.is_null() && (*h).platform == dev::PLATFORM_WGPU as u32;
        if wgpu(header) && (*header).serial_number != (*(*g).mesh_header).serial_num as u32 {
            // mesh changed, so reinstance everything
            free_instance_data(g);
            header = null_mut();
        }
        if (*g).inst_data.is_null() {
            let n = (*(*g).mesh_header).num_meshes as i32;
            let mut inst: Vec<InstanceData> = (0..n.max(1)).map(|_| InstanceData { first: 0, count: 0, material: null_mut(), vertex_alpha: 0 }).collect();
            let inst_ptr = inst.as_mut_ptr();
            std::mem::forget(inst);
            header = Box::into_raw(Box::new(InstanceDataHeader {
                platform: dev::PLATFORM_WGPU as u32,
                serial_number: (*(*g).mesh_header).serial_num as u32,
                num_meshes: n,
                inst: inst_ptr,
                geometry: 0,
                skinned: (!plg::skin_get(g).is_null() && plugin_id == ID_SKIN) as i32,
            }));
            (*g).inst_data = header as *mut c_void;
            instance_geometry(g, header);
        } else if (*g).locked_since_inst != 0 && wgpu(header) {
            instance_geometry(g, header);
        }
        (*g).locked_since_inst = 0;
    }
}

/// Instances the atomic's geometry; null if there is nothing to draw.
///
/// # Safety
/// A live atomic with a geometry.
pub unsafe fn instance_atomic(plugin_id: u32, atomic: *mut Atomic) -> *mut InstanceDataHeader {
    unsafe {
        instance(plugin_id, atomic);
        let h = (*(*atomic).geometry).inst_data as *mut InstanceDataHeader;
        if h.is_null() || (*h).platform != dev::PLATFORM_WGPU as u32 || (*h).geometry == 0 {
            return null_mut();
        }
        h
    }
}

struct DrawGlobals {
    /// gl3's u_lightDirection[0], kept from the last light that set it
    light_direction0: [f32; 4],
    /// bone matrices of the skinned atomic being drawn
    bones: [Mat4; 64],
    num_bones: i32,
    /// environment map of the MatFX mesh being drawn
    env: Option<EnvMap>,
    /// Neo shader and its second texture
    shader: u32,
    use_tex1: bool,
    tex1: *mut Texture,
}
static mut DG: DrawGlobals = DrawGlobals { light_direction0: [0.0; 4], bones: [[0.0; 16]; 64], num_bones: 0, env: None, shader: 0, use_tex1: false, tex1: null_mut() };
#[allow(clippy::mut_from_ref)]
fn dg() -> &'static mut DrawGlobals {
    unsafe { &mut *addr_of_mut!(DG) }
}

/// Direction of light slot 0 as gl3's u_lightDirection[0] holds it (the Neo gloss shader).
pub fn last_light_direction() -> *const f32 {
    dg().light_direction0.as_ptr()
}

fn v3(v: crate::math::V3) -> [f32; 4] {
    [v.x, v.y, v.z, 0.0]
}

/// gl3 lightingCB + setLights.
unsafe fn set_lights(atomic: *mut Atomic, model: &mut ModelState) {
    unsafe {
        let mut directionals = [null_mut::<Light>(); 8];
        let mut locals = [null_mut::<Light>(); 8];
        let mut ld = WorldLights { num_ambients: 0, ambient: crate::rwscene::RgbaF { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 }, num_directionals: 8, directionals: directionals.as_mut_ptr(), num_locals: 8, locals: locals.as_mut_ptr() };
        let world = (hooks().current_world)();
        let flags = (*(*atomic).geometry).flags;
        if flags & LIGHT != 0 && !world.is_null() {
            crate::rwscene::world_enumerate_lights(world, atomic, &mut ld);
            if flags & geo::NORMALS == 0 {
                // get rid of lights that need normals when we don't have any
                ld.num_directionals = 0;
                ld.num_locals = 0;
            }
        } else {
            ld.num_directionals = 0;
            ld.num_locals = 0;
        }
        model.ambient = [ld.ambient.red, ld.ambient.green, ld.ambient.blue, ld.ambient.alpha];
        model.lights = [[0.0; 4]; 32];
        let col = |l: &Light| [l.color.red, l.color.green, l.color.blue, l.color.alpha];
        let mut n = 0;
        for &l in directionals.iter().take(ld.num_directionals.max(0) as usize) {
            if n >= 8 {
                break;
            }
            let ltm = &*get_ltm(frame_of(l));
            let d = &mut model.lights[n * 4..n * 4 + 4];
            d[0][0] = 1.0;
            d[2] = v3(ltm.at);
            d[3] = col(&*l);
            n += 1;
        }
        for &l in locals.iter().take(ld.num_locals.max(0) as usize) {
            if n >= 8 {
                break;
            }
            let lt = &*l;
            let ltm = &*get_ltm(frame_of(l));
            let d = &mut model.lights[n * 4..n * 4 + 4];
            match lt.object.object.sub_type {
                LIGHT_POINT => {
                    d[0][0] = 2.0;
                    d[0][1] = lt.radius;
                    d[1] = v3(ltm.pos);
                    d[3] = col(lt);
                    n += 1;
                }
                LIGHT_SPOT | LIGHT_SOFTSPOT => {
                    d[0] = [3.0, lt.radius, lt.minus_cos_angle, if lt.object.object.sub_type == LIGHT_SOFTSPOT { 0.0 } else { 1.0 }];
                    d[1] = v3(ltm.pos);
                    d[2] = v3(ltm.at);
                    d[3] = col(lt);
                    n += 1;
                }
                _ => {}
            }
        }
        // gl3 keeps u_lightDirection[0] from the last light that set it
        let t = model.lights[0][0];
        if t == 1.0 || t == 3.0 {
            dg().light_direction0 = model.lights[2];
        }
    }
}

fn store_matrix(m: &RwMatrix) -> Mat4 {
    [m.right.x, m.right.y, m.right.z, 0.0, m.up.x, m.up.y, m.up.z, 0.0, m.at.x, m.at.y, m.at.z, 0.0, m.pos.x, m.pos.y, m.pos.z, 1.0]
}

/// gl3 uploadSkinMatrices: inverse bind matrix times the bone's matrix (relative to the atomic
/// unless the hierarchy already holds local-space matrices); identity without a hierarchy.
unsafe fn compute_skin_matrices(a: *mut Atomic) {
    unsafe {
        let skin = &*plg::skin_get((*a).geometry);
        let hier = *((a as *mut u8).add((hooks().skin_atomic_offset)() as usize) as *mut *mut HAnimHierarchy);
        let d = dg();
        d.num_bones = skin.num_bones.min(64);
        if !hier.is_null() {
            let h = &*hier;
            let inv_mats = skin.inverse_matrices as *mut RwMatrix;
            let n = h.num_nodes.min(d.num_bones);
            let mut m = RwMatrix::default();
            if h.flags & LOCALSPACEMATRICES != 0 {
                for i in 0..n as usize {
                    (*inv_mats.add(i)).flags = 0;
                    rwmatrix::mult(&mut m, inv_mats.add(i), h.matrices.add(i));
                    d.bones[i] = store_matrix(&m);
                }
            } else {
                let mut inv_atm = RwMatrix::default();
                let mut tmp = RwMatrix::default();
                rwmatrix::invert(&mut inv_atm, get_ltm(frame_of(a)));
                for i in 0..n as usize {
                    (*inv_mats.add(i)).flags = 0;
                    rwmatrix::mult(&mut tmp, h.matrices.add(i), &inv_atm);
                    rwmatrix::mult(&mut m, inv_mats.add(i), &tmp);
                    d.bones[i] = store_matrix(&m);
                }
            }
            d.num_bones = n;
        } else {
            let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
            for i in 0..d.num_bones as usize {
                d.bones[i] = id;
            }
        }
    }
}

fn rs(state: i32) -> u32 {
    unsafe { get_render_state(state) as usize as u32 }
}
fn set_rs(state: i32, v: u32) {
    unsafe { set_render_state(state, v as usize as *mut c_void) }
}

unsafe fn draw_simple(header: &InstanceDataHeader, inst: &InstanceData, model: &ModelState) {
    unsafe {
        let mut s = current_draw_state();
        let d = dg();
        s.shader = d.shader;
        // as gl3 setTexture(1, tex): the texture's own filter and addressing; null is white
        let tex1 = if d.use_tex1 { Some(dev::tex_envmap(d.tex1)) } else { d.env };
        let skinned = header.skinned != 0;
        host::revc_rw_draw_mesh(
            &s,
            header.geometry,
            inst.first,
            inst.count,
            dev::camera_xform() as *const Xform,
            model,
            if skinned { d.bones.as_ptr() } else { null() },
            if skinned { d.num_bones } else { 0 },
            tex1.as_ref().map_or(null(), |e| e as *const EnvMap),
        );
    }
}

/// gl3 drawInst_GSemu: the PS2 GS alpha test, failing pixels still write colour but not depth.
unsafe fn draw_inst(header: &InstanceDataHeader, inst: &InstanceData, model: &ModelState) {
    unsafe {
        if rs(dev::GSALPHATEST) == 0 || current_draw_state().blend == 0 {
            draw_simple(header, inst, model);
            return;
        }
        let zwrite = rs(dev::ZWRITEENABLE);
        let alphafunc = rs(dev::ALPHATESTFUNC);
        if zwrite != 0 {
            let alpharef = rs(dev::ALPHATESTREF);
            let gsalpharef = rs(dev::GSALPHATESTREF);
            set_rs(dev::ALPHATESTFUNC, dev::ALPHAGREATEREQUAL);
            set_rs(dev::ALPHATESTREF, gsalpharef);
            draw_simple(header, inst, model);
            set_rs(dev::ALPHATESTFUNC, dev::ALPHALESS);
            set_rs(dev::ZWRITEENABLE, 0);
            draw_simple(header, inst, model);
            set_rs(dev::ZWRITEENABLE, 1);
            set_rs(dev::ALPHATESTFUNC, alphafunc);
            set_rs(dev::ALPHATESTREF, alpharef);
        } else {
            set_rs(dev::ALPHATESTFUNC, dev::ALPHAALWAYS);
            draw_simple(header, inst, model);
            set_rs(dev::ALPHATESTFUNC, alphafunc);
        }
    }
}

/// 4x4 row-major product (rw::RawMatrix::mult).
fn raw_mult(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut d = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            d[r * 4 + c] = a[r * 4] * b[c] + a[r * 4 + 1] * b[4 + c] + a[r * 4 + 2] * b[8 + c] + a[r * 4 + 3] * b[12 + c];
        }
    }
    d
}

/// gl3 uploadEnvMatrix: the env frame's (or camera's) inverse rotation, then normal to texcoords.
unsafe fn env_matrix(mut frame: *mut Frame, flip_u: bool) -> [f32; 16] {
    unsafe {
        if frame.is_null() {
            frame = frame_of(dev::current_camera());
        }
        let mut inv = RwMatrix::default();
        rwmatrix::invert(&mut inv, get_ltm(frame));
        // convMatrix with the position cleared
        let inv_mtx = [inv.right.x, inv.right.y, inv.right.z, 0.0, inv.up.x, inv.up.y, inv.up.z, 0.0, inv.at.x, inv.at.y, inv.at.z, 0.0, 0.0, 0.0, 0.0, 1.0];
        let n2t = [if flip_u { -0.5 } else { 0.5 }, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0];
        raw_mult(&inv_mtx, &n2t)
    }
}

/// gl3 matfxEnvRender (one pass: base texture plus the env map, premultiplied, source blend ONE).
unsafe fn draw_env(header: &InstanceDataHeader, inst: &InstanceData, model: &mut ModelState, env: &Env) -> bool {
    unsafe {
        if env.tex.is_null() || (*env.tex).raster.is_null() || env.coefficient == 0.0 {
            return false;
        }
        let mat = &*inst.material;
        let mut es = EnvSettings { flip_u: 0, apply_light: 0, use_mat_color: 0, color: Rgba::default() };
        (hooks().env_settings)(&mut es);
        let e = dev::tex_envmap(env.tex);
        model.tex_matrix = env_matrix(env.frame, es.flip_u != 0);
        model.fx_params = [env.coefficient, if env.fb_alpha != 0 { 0.0 } else { 1.0 }, 0.0, 0.0];
        // clamps the lit colour: PC style (env lit) or PS2 style (env at full strength)
        model.color_clamp = [if es.apply_light != 0 { 0.0 } else { 1.0 }; 4];
        let ec = if es.use_mat_color != 0 { mat.color } else { es.color };
        model.env_color = [ec.r as f32 / 255.0, ec.g as f32 / 255.0, ec.b as f32 / 255.0, ec.a as f32 / 255.0];

        dev::set_texture_stage0(mat.texture);
        set_rs(dev::VERTEXALPHA, 1);
        set_rs(dev::SRCBLEND, dev::BLENDONE);
        dg().env = Some(e);
        draw_inst(header, inst, model);
        dg().env = None;
        set_rs(dev::SRCBLEND, dev::BLENDSRCALPHA);
        true
    }
}

/// World matrix, lights (gl3 lightingCB) and, for skinned geometry, bone matrices.
///
/// # Safety
/// A live atomic and its instance data.
pub unsafe fn begin_model(atomic: *mut Atomic, header: *const InstanceDataHeader, model: *mut ModelState) {
    unsafe {
        std::ptr::write_bytes(model, 0, 1);
        let model = &mut *model;
        model.world = store_matrix(&*get_ltm(frame_of(atomic)));
        set_lights(atomic, model);
        if (*header).skinned != 0 {
            compute_skin_matrices(atomic);
        }
    }
}

/// gl3 setMaterial: colour and surface properties.
pub fn set_model_material(model: &mut ModelState, col: Rgba, sp: SurfaceProperties) {
    model.mat_color = [col.r as f32 / 255.0, col.g as f32 / 255.0, col.b as f32 / 255.0, col.a as f32 / 255.0];
    model.surf_props = [sp.ambient, sp.specular, sp.diffuse, 0.0];
}

/// Draws one mesh (with the PS2 alpha test emulation) using `shader` (0 = default lighting)
/// and, if `use_tex1`, `tex1` as second texture (null = white).
///
/// # Safety
/// Live instance data; `tex1` live or null.
pub unsafe fn draw_mesh(header: *const InstanceDataHeader, inst: *const InstanceData, model: *const ModelState, shader: u32, use_tex1: bool, tex1: *mut Texture) {
    unsafe {
        if (*inst).count == 0 {
            return;
        }
        let d = dg();
        d.shader = shader;
        d.use_tex1 = use_tex1;
        d.tex1 = tex1;
        draw_inst(&*header, &*inst, &*model);
        let d = dg();
        d.shader = 0;
        d.use_tex1 = false;
        d.tex1 = null_mut();
    }
}

/// The default/MatFX/skin pipeline's drawing of an instanced atomic (gl3 defaultRenderCB etc.).
///
/// # Safety
/// A live atomic and its instance data.
pub unsafe fn render_default(plugin_id: u32, atomic: *mut Atomic, header: *mut InstanceDataHeader) {
    unsafe {
        let g = &*(*atomic).geometry;
        let mut model: ModelState = std::mem::zeroed();
        begin_model(atomic, header, &mut model);
        let h = &*header;
        let white = Rgba { r: 255, g: 255, b: 255, a: 255 };
        for inst in std::slice::from_raw_parts(h.inst, h.num_meshes as usize) {
            let mat = &*inst.material;
            set_model_material(&mut model, if g.flags & MODULATE != 0 { mat.color } else { white }, mat.surface_props);
            if plugin_id == ID_MATFX && inst.count != 0 {
                let fx = plg::matfx_get(inst.material);
                if !fx.is_null() && (*fx).kind == plg::ENVMAP && draw_env(h, inst, &mut model, &(*fx).fx[0].data.env) {
                    continue;
                }
            }
            dev::set_texture_stage0(mat.texture);
            set_rs(dev::VERTEXALPHA, (inst.vertex_alpha != 0 || mat.color.a != 0xFF) as u32);
            if inst.count != 0 {
                draw_inst(h, inst, &model);
            }
        }
    }
}

/// The pipeline's render callback.
///
/// # Safety
/// A live atomic.
pub unsafe fn render(plugin_id: u32, atomic: *mut Atomic) {
    unsafe {
        let h = instance_atomic(plugin_id, atomic);
        if !h.is_null() {
            render_default(plugin_id, atomic, h);
        }
    }
}

// ---------------------------------------------------------------- C ABI (wgpupipe.cpp forwards)

macro_rules! ffi {
    ($($name:ident ( $($a:ident : $t:ty),* ) $(-> $r:ty)? => $body:expr;)*) => {
        $(
            /// # Safety
            /// As the Rust function it forwards to.
            #[unsafe(no_mangle)]
            #[allow(unused_unsafe)]
            pub unsafe extern "C" fn $name($($a: $t),*) $(-> $r)? { unsafe { $body } }
        )*
    };
}

ffi! {
    revc_pipe_destroy_native_data(g: *mut Geometry) => destroy_native_data(g);
    revc_pipe_instance(plugin_id: u32, a: *mut Atomic) => instance(plugin_id, a);
    revc_pipe_render(plugin_id: u32, a: *mut Atomic) => render(plugin_id, a);
    revc_pipe_instance_atomic(plugin_id: u32, a: *mut Atomic) -> *mut InstanceDataHeader => instance_atomic(plugin_id, a);
    revc_pipe_begin_model(a: *mut Atomic, h: *const InstanceDataHeader, m: *mut ModelState) => begin_model(a, h, m);
    revc_pipe_set_model_material(m: *mut ModelState, c: *const Rgba, sp: *const SurfaceProperties) => set_model_material(&mut *m, *c, *sp);
    revc_pipe_draw_mesh(h: *const InstanceDataHeader, i: *const InstanceData, m: *const ModelState, shader: u32, use_tex1: i32, tex1: *mut Texture) => draw_mesh(h, i, m, shader, use_tex1 != 0, tex1);
    revc_pipe_render_default(plugin_id: u32, a: *mut Atomic, h: *mut InstanceDataHeader) => render_default(plugin_id, a, h);
    revc_pipe_last_light_direction() -> *const f32 => last_light_direction();
}

//! The wgpu device (stage 2 step 7b): what wgpudevice.cpp did in front of the Rust renderer.
//! Window and video modes come from the host's HostWindowApi; render states are kept as GL3
//! keeps them (gl3device.cpp's rwStateCache and texture stage 0); camera matrices, clears,
//! Im2D and Im3D draws go to the renderer (host/wgpu.rs). The C++ Device table and the C++
//! names the game's custom pipelines call forward here (wgpudevice.cpp).
//!
//! This is revc's own driver code (written in stage 1, task 12), not a librw original, so it
//! is checked by the scenario suites (screenshots on Vulkan and GL) rather than a recording.

use crate::host::wgpu as host;
use crate::math::RwMatrix;
use crate::rwraster::{self, Raster, WgpuRaster};
use crate::rwscene::{Camera, PERSPECTIVE};
use crate::rwtexture::Texture;
use revc_rw::frame::{DrawState, EnvMap, Vertex2D, Vertex2DUV2, Vertex3D, Xform};
use std::ffi::{c_char, c_void};
use std::ptr::{addr_of, addr_of_mut, null, null_mut};

// ---------------------------------------------------------------- render states
// (render-state block drafted by the local model from wgpudevice.cpp, then corrected: the raster
// state returns the pointer, constants are typed as the fields, and the static is reached
// through rs() instead of references to a static mut)

// rw::RenderState
pub(crate) const TEXTURERASTER: i32 = 0;
const TEXTUREADDRESS: i32 = 1;
const TEXTUREADDRESSU: i32 = 2;
const TEXTUREADDRESSV: i32 = 3;
const TEXTUREFILTER: i32 = 4;
const VERTEXALPHA: i32 = 5;
const SRCBLEND: i32 = 6;
const DESTBLEND: i32 = 7;
const ZTESTENABLE: i32 = 8;
const ZWRITEENABLE: i32 = 9;
const FOGENABLE: i32 = 10;
const FOGCOLOR: i32 = 11;
const CULLMODE: i32 = 12;
const STENCILENABLE: i32 = 13;
const STENCILFAIL: i32 = 14;
const STENCILZFAIL: i32 = 15;
const STENCILPASS: i32 = 16;
const STENCILFUNCTION: i32 = 17;
const STENCILFUNCTIONMASK: i32 = 19;
const STENCILFUNCTIONWRITEMASK: i32 = 20;
const ALPHATESTFUNC: i32 = 21;
const ALPHATESTREF: i32 = 22;
const GSALPHATEST: i32 = 23;
const GSALPHATESTREF: i32 = 24;
// values
const ALPHAALWAYS: u32 = 0;
const ALPHAGREATEREQUAL: u32 = 1;
const STENCILKEEP: u32 = 1;
const STENCILALWAYS: u32 = 8;
const CULLNONE: u32 = 1;
const BLENDSRCALPHA: u32 = 5;
const BLENDINVSRCALPHA: u32 = 6;

/// Render states as GL3 keeps them (gl3device.cpp rwStateCache): texture stage 0 keeps its own
/// filter and addressing; blending and the alpha test are on while vertex alpha or the bound
/// raster's alpha is.
#[derive(Clone, Copy)]
struct RenderState {
    raster: *mut Raster,
    filter: u32,
    address_u: u32,
    address_v: u32,
    vertex_alpha: u32,
    srcblend: u32,
    destblend: u32,
    ztest: u32,
    zwrite: u32,
    fog_enable: u32,
    fog_color: u32,
    cullmode: u32,
    alpha_func: u32,
    alpha_ref: u32,
    gsalpha: u32,
    gsalpharef: u32,
    /// STENCILENABLE .. STENCILFUNCTIONWRITEMASK, kept but unused
    stencil: [u32; 8],
}
const RS_ZERO: RenderState = RenderState {
    raster: null_mut(),
    filter: 0,
    address_u: 0,
    address_v: 0,
    vertex_alpha: 0,
    srcblend: 0,
    destblend: 0,
    ztest: 0,
    zwrite: 0,
    fog_enable: 0,
    fog_color: 0,
    cullmode: 0,
    alpha_func: 0,
    alpha_ref: 0,
    gsalpha: 0,
    gsalpharef: 0,
    stencil: [0; 8],
};
static mut RS: RenderState = RS_ZERO;
#[allow(clippy::mut_from_ref)]
fn rs() -> &'static mut RenderState {
    unsafe { &mut *addr_of_mut!(RS) }
}

fn reset_render_state() {
    let s = rs();
    *s = RS_ZERO;
    s.alpha_func = ALPHAGREATEREQUAL;
    s.alpha_ref = 10;
    s.srcblend = BLENDSRCALPHA;
    s.destblend = BLENDINVSRCALPHA;
    s.zwrite = 1;
    s.ztest = 1;
    s.cullmode = CULLNONE;
    s.fog_color = 0xFFFFFFFF;
    s.gsalpharef = 128;
    let st = |i: i32| (i - STENCILENABLE) as usize;
    s.stencil[st(STENCILFUNCTION)] = STENCILALWAYS;
    s.stencil[st(STENCILFUNCTIONMASK)] = 0xFFFFFFFF;
    s.stencil[st(STENCILFUNCTIONWRITEMASK)] = 0xFFFFFFFF;
    s.stencil[st(STENCILFAIL)] = STENCILKEEP;
    s.stencil[st(STENCILZFAIL)] = STENCILKEEP;
    s.stencil[st(STENCILPASS)] = STENCILKEEP;
}

const PLATFORM_WGPU: i32 = 13;

/// The driver data of a wgpu raster; null for null or other platforms' rasters.
fn raster_ext(r: *mut Raster) -> *mut WgpuRaster {
    if r.is_null() || unsafe { (*r).platform } != PLATFORM_WGPU {
        return null_mut();
    }
    unsafe { rwraster::natras(r) }
}

// GL3's texture stage 0 logic (gl3device.cpp setFilterMode, setAddressU/V, setRasterStage,
// setRasterStageOnly, setTexture), with the GL texture object's sampler parameters kept in the
// raster: binding through the TEXTURERASTER render state writes the stage's state into the
// texture, binding through setTexture reads the texture's state into the stage, a filter change
// goes onto the bound texture (with the anisotropy), and an addressing change while a texture is
// bound only applies when it already has that mode (gl3 compares with ==). Draws sample with the
// texture's parameters, so they match GL3's exactly.
fn stage_filter_mode(filter: u32, max_aniso: i32) {
    let s = rs();
    if s.filter != filter {
        s.filter = filter;
        let n = raster_ext(s.raster);
        if !n.is_null() {
            let n = unsafe { &mut *n };
            n.filter_mode = filter as i32;
            n.max_anisotropy = max_aniso;
        }
    }
}

fn stage_address_u(addressing: u32) {
    let s = rs();
    if s.address_u != addressing {
        s.address_u = addressing;
        // gl3 only "changes" it when it already has that mode: a no-op, kept as in GL3
    }
}

fn stage_address_v(addressing: u32) {
    let s = rs();
    if s.address_v != addressing {
        s.address_v = addressing;
    }
}

fn stage_raster(raster: *mut Raster, only: bool) {
    let s = rs();
    if raster == s.raster {
        return;
    }
    s.raster = raster;
    let n = raster_ext(raster);
    if n.is_null() {
        return;
    }
    let n = unsafe { &mut *n };
    if only {
        s.filter = n.filter_mode as u32;
        s.address_u = n.address_u as u32;
        s.address_v = n.address_v as u32;
    } else {
        n.filter_mode = s.filter as i32;
        n.address_u = s.address_u as i32;
        n.address_v = s.address_v as i32;
    }
}

/// gl3 evictRaster: unbinds a raster that is being destroyed.
///
/// # Safety
/// Any raster pointer.
pub unsafe fn evict_raster(raster: *mut Raster) {
    if rs().raster == raster {
        stage_raster(null_mut(), false);
    }
}

/// # Safety
/// TEXTURERASTER takes a live raster or null.
pub unsafe fn set_render_state(state: i32, pvalue: *mut c_void) {
    let s = rs();
    let value = pvalue as usize as u32;
    match state {
        TEXTURERASTER => stage_raster(pvalue as *mut Raster, false),
        TEXTUREADDRESS => {
            stage_address_u(value);
            stage_address_v(value);
        }
        TEXTUREADDRESSU => stage_address_u(value),
        TEXTUREADDRESSV => stage_address_v(value),
        TEXTUREFILTER => stage_filter_mode(value, 1),
        VERTEXALPHA => s.vertex_alpha = value,
        SRCBLEND => s.srcblend = value,
        DESTBLEND => s.destblend = value,
        ZTESTENABLE => s.ztest = value,
        ZWRITEENABLE => s.zwrite = value,
        FOGENABLE => s.fog_enable = value,
        FOGCOLOR => s.fog_color = value,
        CULLMODE => s.cullmode = value,
        STENCILENABLE..=STENCILFUNCTIONWRITEMASK => s.stencil[(state - STENCILENABLE) as usize] = value,
        ALPHATESTFUNC => s.alpha_func = value,
        ALPHATESTREF => s.alpha_ref = value,
        GSALPHATEST => s.gsalpha = value,
        GSALPHATESTREF => s.gsalpharef = value,
        _ => {}
    }
}

/// # Safety
/// None beyond the device being set up.
pub unsafe fn get_render_state(state: i32) -> *mut c_void {
    let s = rs();
    let val = match state {
        TEXTURERASTER => return s.raster as *mut c_void,
        TEXTUREADDRESS => {
            if s.address_u == s.address_v {
                s.address_u
            } else {
                0
            }
        }
        TEXTUREADDRESSU => s.address_u,
        TEXTUREADDRESSV => s.address_v,
        TEXTUREFILTER => s.filter,
        VERTEXALPHA => s.vertex_alpha,
        SRCBLEND => s.srcblend,
        DESTBLEND => s.destblend,
        ZTESTENABLE => s.ztest,
        ZWRITEENABLE => s.zwrite,
        FOGENABLE => s.fog_enable,
        FOGCOLOR => s.fog_color,
        CULLMODE => s.cullmode,
        STENCILENABLE..=STENCILFUNCTIONWRITEMASK => s.stencil[(state - STENCILENABLE) as usize],
        ALPHATESTFUNC => s.alpha_func,
        ALPHATESTREF => s.alpha_ref,
        GSALPHATEST => s.gsalpha,
        GSALPHATESTREF => s.gsalpharef,
        _ => 0,
    };
    val as usize as *mut c_void
}

// ---------------------------------------------------------------- hooks (C++ side)

/// What stays in C++ until the engine moves (step 7b part 3).
#[repr(C)]
pub struct Hooks {
    /// &engine->currentCamera
    pub current_camera: unsafe extern "C" fn() -> *mut *mut Camera,
    /// Raster::getCurrentContext()
    pub current_context: unsafe extern "C" fn() -> *mut Raster,
    /// Texture::getMaxAnisotropy() (the anisotropy plugin)
    pub max_anisotropy: unsafe extern "C" fn(*mut Texture) -> i32,
    /// wgpu::antialiasing, the level the game's menu sets
    pub antialiasing: unsafe extern "C" fn() -> i32,
}
static mut HOOKS: Option<Hooks> = None;
fn hooks() -> &'static Hooks {
    unsafe { (*addr_of!(HOOKS)).as_ref().expect("device hooks") }
}

/// # Safety
/// Called once at startup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_set_hooks(h: Hooks) {
    unsafe { *addr_of_mut!(HOOKS) = Some(h) };
}

// ---------------------------------------------------------------- host window

/// rw::wgpu::HostVideoMode
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HostVideoMode {
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub refresh_rate: i32,
    pub flags: u32,
}

/// rw::wgpu::HostWindowApi
#[repr(C)]
pub struct HostWindowApi {
    pub user: *mut c_void,
    pub num_displays: unsafe extern "C" fn(*mut c_void) -> i32,
    pub display_name: unsafe extern "C" fn(*mut c_void, i32) -> *const c_char,
    pub num_video_modes: unsafe extern "C" fn(*mut c_void, i32) -> i32,
    pub get_video_mode: unsafe extern "C" fn(*mut c_void, i32, i32, *mut HostVideoMode) -> i32,
    pub create_window: unsafe extern "C" fn(*mut c_void, *const c_char, i32, *const HostVideoMode, i32, i32, i32) -> *mut c_void,
    pub destroy_window: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub get_drawable_size: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32),
}

/// rw::EngineOpenParams
#[repr(C)]
pub struct EngineOpenParams {
    pub host: *const HostWindowApi,
    pub window: *mut *mut c_void,
    pub width: i32,
    pub height: i32,
    pub windowtitle: *const c_char,
}

/// rw::VideoMode
#[repr(C)]
pub struct VideoMode {
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub flags: u32,
}

#[derive(Clone, Copy, Default)]
struct DisplayMode {
    mode: HostVideoMode,
    depth: i32,
    flags: u32,
}

struct Globals {
    host: *const HostWindowApi,
    p_window: *mut *mut c_void,
    window: *mut c_void,
    win_width: i32,
    win_height: i32,
    win_title: *const c_char,
    num_displays: i32,
    current_display: i32,
    modes: Vec<DisplayMode>,
    current_mode: i32,
    num_samples: u32,
    applied_antialiasing: i32,
    view: [f32; 16],
    proj: [f32; 16],
    /// projection, view, world (identity: models bring their own), revc_rw::frame::Xform
    mesh_xform: [f32; 48],
    im2d_override: u32,
    im2d_override_params: [f32; 8],
    im3d_vertices: *const c_void,
    im3d_num_vertices: i32,
    im3d_xform: [f32; 48],
}
static mut G: Globals = Globals {
    host: null(),
    p_window: null_mut(),
    window: null_mut(),
    win_width: 0,
    win_height: 0,
    win_title: null(),
    num_displays: 0,
    current_display: 0,
    modes: Vec::new(),
    current_mode: 0,
    num_samples: 0,
    applied_antialiasing: -1,
    view: [0.0; 16],
    proj: [0.0; 16],
    mesh_xform: [0.0; 48],
    im2d_override: 0,
    im2d_override_params: [0.0; 8],
    im3d_vertices: null(),
    im3d_num_vertices: 0,
    im3d_xform: [0.0; 48],
};
#[allow(clippy::mut_from_ref)]
fn g() -> &'static mut Globals {
    unsafe { &mut *addr_of_mut!(G) }
}

unsafe fn make_video_mode_list(display: i32) {
    let g = g();
    let h = unsafe { &*g.host };
    let num = unsafe { (h.num_video_modes)(h.user, display) }.max(1);
    g.modes.clear();
    for i in 0..num {
        let mut m = DisplayMode::default();
        if unsafe { (h.get_video_mode)(h.user, display, i, &mut m.mode) } == 0 {
            if i != 0 {
                continue;
            }
            // the host could not report a desktop mode: the window size
            m.mode = HostVideoMode { width: g.win_width, height: g.win_height, depth: 32, refresh_rate: 0, flags: 0 };
        }
        m.flags = m.mode.flags;
        m.depth = 1;
        while m.depth < m.mode.depth {
            m.depth <<= 1;
        }
        g.modes.push(m);
    }
    if g.current_mode >= g.modes.len() as i32 {
        g.current_mode = 0;
    }
}

unsafe fn open_host(p: *const EngineOpenParams) -> i32 {
    let g = g();
    let p = unsafe { &*p };
    g.win_width = p.width;
    g.win_height = p.height;
    g.win_title = p.windowtitle;
    g.p_window = p.window;
    g.host = p.host;
    if g.host.is_null() {
        eprintln!("[RW] EngineOpenParams.host is NULL");
        return 0;
    }
    g.num_displays = unsafe { ((*g.host).num_displays)((*g.host).user) };
    g.current_display = 0;
    unsafe { make_video_mode_list(0) };
    1
}

unsafe fn start_host() -> i32 {
    let g = g();
    // a new window gets a new renderer: apply the antialiasing again
    g.applied_antialiasing = -1;
    let h = unsafe { &*g.host };
    let mode = g.modes[g.current_mode as usize].mode;
    let win = unsafe { (h.create_window)(h.user, g.win_title, g.current_display, &mode, g.win_width, g.win_height, g.num_samples as i32) };
    if win.is_null() {
        eprintln!("[RW] host createWindow() failed");
        return 0;
    }
    g.window = win;
    unsafe { *g.p_window = win };
    1
}

unsafe fn stop_host() -> i32 {
    let g = g();
    if !g.window.is_null() {
        unsafe { ((*g.host).destroy_window)((*g.host).user, g.window) };
    }
    g.window = null_mut();
    1
}

// ---------------------------------------------------------------- draw state

fn tex_filter(t: &Texture) -> u32 {
    t.filter_addressing & 0xFF
}
fn tex_address_u(t: &Texture) -> u32 {
    (t.filter_addressing >> 8) & 0xF
}
fn tex_address_v(t: &Texture) -> u32 {
    (t.filter_addressing >> 12) & 0xF
}

/// gl3 setTexture(0, tex): the device side of the pipelines' setTexture.
///
/// # Safety
/// A live texture or null.
pub unsafe fn set_texture_stage0(tex: *mut Texture) {
    unsafe {
        if tex.is_null() || (*tex).raster.is_null() {
            stage_raster(null_mut(), false);
            return;
        }
        let t = &*tex;
        stage_raster(t.raster as *mut Raster, true);
        stage_filter_mode(tex_filter(t), (hooks().max_anisotropy)(tex));
        stage_address_u(tex_address_u(t));
        stage_address_v(tex_address_v(t));
    }
}

/// Like gl3::im2dOverrideShader: Im2D draws use this shader until it is reset to 0.
///
/// # Safety
/// `params` is null or holds 8 floats.
pub unsafe fn set_im2d_override(shader: u32, params: *const f32) {
    let g = g();
    g.im2d_override = shader;
    g.im2d_override_params = if params.is_null() { [0.0; 8] } else { unsafe { *(params as *const [f32; 8]) } };
}

/// The state for the next draw, uploading the bound raster if it changed.
///
/// # Safety
/// The engine is open.
pub unsafe fn current_draw_state() -> DrawState {
    let rs = rs();
    let mut s = DrawState { max_anisotropy: 1, ..Default::default() };
    let mut tex_alpha = false;
    let r = rs.raster;
    let natras = raster_ext(r);
    if !natras.is_null() {
        s.texture = unsafe { rwraster::upload_raster(r) };
        if s.texture != 0 {
            unsafe {
                tex_alpha = (*natras).has_alpha != 0;
                s.max_anisotropy = (*natras).max_anisotropy as u32;
            }
        }
    }
    let alpha = rs.vertex_alpha != 0 || tex_alpha;
    // the texture's own sampler parameters, as GL3 samples it (see stage_filter_mode)
    if s.texture != 0 {
        unsafe {
            s.filter = (*natras).filter_mode as u32;
            s.address_u = (*natras).address_u as u32;
            s.address_v = (*natras).address_v as u32;
        }
    } else {
        s.filter = rs.filter;
        s.address_u = rs.address_u;
        s.address_v = rs.address_v;
    }
    s.blend = alpha as u32;
    s.src_blend = rs.srcblend;
    s.dst_blend = rs.destblend;
    s.z_test = rs.ztest;
    s.z_write = rs.zwrite;
    s.cull = rs.cullmode;
    s.alpha_func = if alpha { rs.alpha_func } else { ALPHAALWAYS };
    s.alpha_ref = rs.alpha_ref as f32 / 255.0;
    s.fog = rs.fog_enable;
    let cam = unsafe { *(hooks().current_camera)() };
    let (mut w, mut h) = (640.0f32, 480.0f32);
    if !cam.is_null() {
        unsafe {
            s.fog_start = (*cam).fog_plane;
            s.fog_end = (*cam).far_plane;
            let fb = (*cam).frame_buffer as *mut Raster;
            if !fb.is_null() {
                w = (*fb).width as f32;
                h = (*fb).height as f32;
            }
        }
    }
    s.fog_color = rs.fog_color;
    s.xform = [2.0 / w, -2.0 / h, -1.0, 1.0];
    s
}

/// The camera's projection, view and an identity world (48 floats), as the last beginUpdate
/// set them up.
pub fn camera_xform() -> *const f32 {
    g().mesh_xform.as_ptr()
}

/// The render target of a camera: its frame buffer when that is a texture (render to texture,
/// e.g. the Neo env map camera), else the window's frame.
unsafe fn camera_target(cam: *mut Camera) -> u32 {
    unsafe {
        let fb = if cam.is_null() { null_mut() } else { (*cam).frame_buffer as *mut Raster };
        if !raster_ext(fb).is_null() && ((*fb).kind == rwraster::CAMERATEXTURE || (*fb).kind == rwraster::TEXTURE) {
            return rwraster::upload_raster(fb);
        }
        0
    }
}

// ---------------------------------------------------------------- the Device functions

/// Camera matrices as gl3 beginUpdate builds them (left-handed view, columns
/// right/up/at/pos), except that the projection maps depth to wgpu's 0..1 instead of GL's -1..1.
unsafe extern "C" fn begin_update(cam: *mut Camera) {
    unsafe {
        *(hooks().current_camera)() = cam;
        let g = g();
        let aa = (hooks().antialiasing)();
        if aa != g.applied_antialiasing {
            host::revc_rw_set_antialiasing(aa);
            g.applied_antialiasing = aa;
        }
        host::revc_rw_set_target(camera_target(cam));

        let c = &mut *cam;
        let mut inv = RwMatrix::default();
        let frame = c.object.object.parent as *mut crate::rwframe::Frame;
        crate::rwmatrix::invert(&mut inv, crate::rwframe::get_ltm(frame));
        g.view = [
            -inv.right.x, inv.right.y, inv.right.z, 0.0,
            -inv.up.x, inv.up.y, inv.up.z, 0.0,
            -inv.at.x, inv.at.y, inv.at.z, 0.0,
            -inv.pos.x, inv.pos.y, inv.pos.z, 1.0,
        ];
        c.dev_view = g.view;

        let invwx = 1.0 / c.view_window[0];
        let invwy = 1.0 / c.view_window[1];
        let invz = 1.0 / (c.far_plane - c.near_plane);
        let p = &mut g.proj;
        *p = [0.0; 16];
        p[0] = invwx;
        p[5] = invwy;
        p[8] = c.view_offset[0] * invwx;
        p[9] = c.view_offset[1] * invwy;
        p[12] = -p[8];
        p[13] = -p[9];
        if c.projection == PERSPECTIVE {
            p[10] = c.far_plane * invz;
            p[11] = 1.0;
            p[14] = -c.near_plane * c.far_plane * invz;
            p[15] = 0.0;
        } else {
            p[10] = invz;
            p[11] = 0.0;
            p[14] = -c.near_plane * invz;
            p[15] = 1.0;
        }
        c.dev_proj = g.proj;
        g.mesh_xform[..16].copy_from_slice(&g.proj);
        g.mesh_xform[16..32].copy_from_slice(&g.view);
        g.mesh_xform[32..].copy_from_slice(&IDENTITY);

        host::revc_rw_begin_frame(g.window);
    }
}

const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

unsafe extern "C" fn end_update(_: *mut Camera) {}

/// rw::RGBA
#[repr(C)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

unsafe extern "C" fn clear_camera(cam: *mut Camera, col: *const Rgba, mode: u32) {
    unsafe {
        host::revc_rw_set_target(camera_target(cam));
        let c = &*col;
        host::revc_rw_clear(c.red as f32 / 255.0, c.green as f32 / 255.0, c.blue as f32 / 255.0, c.alpha as f32 / 255.0, mode);
    }
}

const FLIPWAITVSYNCH: u32 = 1;

unsafe extern "C" fn show_raster(_: *mut Raster, flags: u32) {
    host::revc_rw_present((flags & FLIPWAITVSYNCH != 0) as i32);
}

/// gl3 rasterRenderFast: the camera's frame copied into the raster on top of the context stack
/// (CMBlur's frame copy for the colour filter and trails).
unsafe extern "C" fn raster_render_fast(src: *mut Raster, x: i32, y: i32) -> i32 {
    unsafe {
        let dst = (hooks().current_context)();
        let natras = raster_ext(dst);
        if natras.is_null() || (*src).kind != rwraster::CAMERA {
            return 0;
        }
        match (*dst).kind {
            rwraster::NORMAL | rwraster::TEXTURE | rwraster::CAMERATEXTURE => {
                let id = rwraster::upload_raster(dst);
                if id == 0 || (*natras).dxt != 0 {
                    return 0;
                }
                host::revc_rw_copy_frame(id, x.max(0) as u32, y.max(0) as u32);
                1
            }
            _ => 0,
        }
    }
}

unsafe extern "C" fn set_render_state_cb(state: i32, value: *mut c_void) {
    unsafe { set_render_state(state, value) }
}
unsafe extern "C" fn get_render_state_cb(state: i32) -> *mut c_void {
    unsafe { get_render_state(state) }
}

fn im2d_state() -> DrawState {
    let mut s = unsafe { current_draw_state() };
    let g = g();
    s.shader = g.im2d_override;
    s.shader_params = g.im2d_override_params;
    s
}

unsafe extern "C" fn im2d_render_primitive(prim: u32, vertices: *mut c_void, num_vertices: i32) {
    let s = im2d_state();
    unsafe { host::revc_rw_im2d(&s, prim, vertices as *const Vertex2D, num_vertices, null(), 0) };
}

unsafe extern "C" fn im2d_render_indexed_primitive(prim: u32, vertices: *mut c_void, num_vertices: i32, indices: *mut c_void, num_indices: i32) {
    let s = im2d_state();
    unsafe { host::revc_rw_im2d(&s, prim, vertices as *const Vertex2D, num_vertices, indices as *const u16, num_indices) };
}

const TEX_LINEAR: u32 = 2;
const TEX_WRAP: u32 = 1;

/// Im2D vertices with a second coordinate set, drawn with the bound raster times `tex1`
/// (null = white) at the second coordinates.
///
/// # Safety
/// As the Im2D draws; `tex1` is a live texture or null.
pub unsafe fn im2d_render_indexed_primitive_uv2(prim: u32, vertices: *mut c_void, num_vertices: i32, indices: *mut c_void, num_indices: i32, tex1: *mut Texture) {
    unsafe {
        let s = current_draw_state();
        let mut t = EnvMap { texture: 0, filter: TEX_LINEAR, address_u: TEX_WRAP, address_v: TEX_WRAP };
        if !tex1.is_null() && !raster_ext((*tex1).raster as *mut Raster).is_null() {
            let tx = &*tex1;
            t = EnvMap { texture: rwraster::upload_raster(tx.raster as *mut Raster), filter: tex_filter(tx), address_u: tex_address_u(tx), address_v: tex_address_v(tx) };
        }
        host::revc_rw_im2d_uv2(&s, prim, vertices as *const Vertex2DUV2, num_vertices, indices as *const u16, num_indices, &t);
    }
}

const PRIMTYPELINELIST: u32 = 1;
const PRIMTYPETRILIST: u32 = 3;
const IM2D_VERTEX_SIZE: usize = 28;

unsafe fn im2d_pick(vertices: *mut c_void, picks: &[i32]) -> [[u8; IM2D_VERTEX_SIZE]; 3] {
    let mut buf = [[0u8; IM2D_VERTEX_SIZE]; 3];
    for (i, &v) in picks.iter().enumerate() {
        buf[i] = unsafe { *(vertices as *const [u8; IM2D_VERTEX_SIZE]).add(v as usize) };
    }
    buf
}

unsafe extern "C" fn im2d_render_line(vertices: *mut c_void, _: i32, v1: i32, v2: i32) {
    unsafe {
        let mut buf = im2d_pick(vertices, &[v1, v2]);
        im2d_render_primitive(PRIMTYPELINELIST, buf.as_mut_ptr() as *mut c_void, 2);
    }
}

unsafe extern "C" fn im2d_render_triangle(vertices: *mut c_void, _: i32, v1: i32, v2: i32, v3: i32) {
    unsafe {
        let mut buf = im2d_pick(vertices, &[v1, v2, v3]);
        im2d_render_primitive(PRIMTYPETRILIST, buf.as_mut_ptr() as *mut c_void, 3);
    }
}

const IM3D_VERTEXUV: u32 = 1;

/// Im3D as gl3immed.cpp: the vertices stay with the caller between transform and end.
unsafe extern "C" fn im3d_transform(vertices: *mut c_void, num_vertices: i32, world: *const RwMatrix, flags: u32) {
    let g = g();
    let w = if world.is_null() {
        IDENTITY
    } else {
        let m = unsafe { &*world };
        [m.right.x, m.right.y, m.right.z, 0.0, m.up.x, m.up.y, m.up.z, 0.0, m.at.x, m.at.y, m.at.z, 0.0, m.pos.x, m.pos.y, m.pos.z, 1.0]
    };
    g.im3d_xform[..16].copy_from_slice(&g.proj);
    g.im3d_xform[16..32].copy_from_slice(&g.view);
    g.im3d_xform[32..].copy_from_slice(&w);
    // as gl3: without texture coordinates the draw is untextured (and stays so)
    if flags & IM3D_VERTEXUV == 0 {
        unsafe { set_render_state(TEXTURERASTER, null_mut()) };
    }
    g.im3d_vertices = vertices;
    g.im3d_num_vertices = num_vertices;
}

unsafe extern "C" fn im3d_render_primitive(prim: u32) {
    let s = unsafe { current_draw_state() };
    let g = g();
    unsafe { host::revc_rw_im3d(&s, prim, g.im3d_vertices as *const Vertex3D, g.im3d_num_vertices, null(), 0, g.im3d_xform.as_ptr() as *const Xform) };
}

unsafe extern "C" fn im3d_render_indexed_primitive(prim: u32, indices: *mut c_void, num_indices: i32) {
    let s = unsafe { current_draw_state() };
    let g = g();
    unsafe { host::revc_rw_im3d(&s, prim, g.im3d_vertices as *const Vertex3D, g.im3d_num_vertices, indices as *const u16, num_indices, g.im3d_xform.as_ptr() as *const Xform) };
}

unsafe extern "C" fn im3d_end() {
    let g = g();
    g.im3d_vertices = null();
    g.im3d_num_vertices = 0;
}

// rw::DeviceReq
const DEVICEINIT: i32 = 0;
const DEVICETERM: i32 = 1;
const DEVICEOPEN: i32 = 2;
const DEVICECLOSE: i32 = 3;
const DEVICEFINALIZE: i32 = 4;
const DEVICEGETNUMSUBSYSTEMS: i32 = 5;
const DEVICEGETCURRENTSUBSYSTEM: i32 = 6;
const DEVICESETSUBSYSTEM: i32 = 7;
const DEVICEGETSUBSSYSTEMINFO: i32 = 8;
const DEVICEGETNUMVIDEOMODES: i32 = 9;
const DEVICEGETCURRENTVIDEOMODE: i32 = 10;
const DEVICESETVIDEOMODE: i32 = 11;
const DEVICEGETVIDEOMODEINFO: i32 = 12;
const DEVICEGETMAXMULTISAMPLINGLEVELS: i32 = 13;
const DEVICEGETMULTISAMPLINGLEVELS: i32 = 14;
const DEVICESETMULTISAMPLINGLEVELS: i32 = 15;

unsafe extern "C" fn device_system(req: i32, arg: *mut c_void, n: i32) -> i32 {
    let g = g();
    unsafe {
        match req {
            DEVICEOPEN => open_host(arg as *const EngineOpenParams),
            DEVICECLOSE | DEVICEFINALIZE => 1,
            DEVICEINIT => {
                reset_render_state();
                start_host()
            }
            DEVICETERM => stop_host(),
            DEVICEGETNUMSUBSYSTEMS => g.num_displays,
            DEVICEGETCURRENTSUBSYSTEM => g.current_display,
            DEVICESETSUBSYSTEM => {
                if n >= g.num_displays {
                    return 0;
                }
                g.current_display = n;
                make_video_mode_list(n);
                1
            }
            DEVICEGETSUBSSYSTEMINFO => {
                if n >= g.num_displays {
                    return 0;
                }
                // SubSystemInfo { char name[80] }
                let out = std::slice::from_raw_parts_mut(arg as *mut u8, 80);
                let h = &*g.host;
                let name = (h.display_name)(h.user, n);
                let bytes: &[u8] = if name.is_null() { b"Display" } else { std::ffi::CStr::from_ptr(name).to_bytes() };
                let len = bytes.len().min(79);
                out.fill(0);
                out[..len].copy_from_slice(&bytes[..len]);
                1
            }
            DEVICEGETNUMVIDEOMODES => g.modes.len() as i32,
            DEVICEGETCURRENTVIDEOMODE => g.current_mode,
            DEVICESETVIDEOMODE => {
                if n >= g.modes.len() as i32 {
                    return 0;
                }
                g.current_mode = n;
                1
            }
            DEVICEGETVIDEOMODEINFO => {
                let m = g.modes[n as usize];
                *(arg as *mut VideoMode) = VideoMode { width: m.mode.width, height: m.mode.height, depth: m.depth, flags: m.flags };
                1
            }
            DEVICEGETMAXMULTISAMPLINGLEVELS => 8,
            DEVICEGETMULTISAMPLINGLEVELS => g.num_samples.max(1) as i32,
            DEVICESETMULTISAMPLINGLEVELS => {
                g.num_samples = n as u32;
                1
            }
            _ => 0,
        }
    }
}

/// rw::Device with its function pointers (rwengine.h); the C++ table is filled from this.
#[repr(C)]
pub struct Device {
    pub z_near: f32,
    pub z_far: f32,
    pub begin_update: unsafe extern "C" fn(*mut Camera),
    pub end_update: unsafe extern "C" fn(*mut Camera),
    pub clear_camera: unsafe extern "C" fn(*mut Camera, *const Rgba, u32),
    pub show_raster: unsafe extern "C" fn(*mut Raster, u32),
    pub raster_render_fast: unsafe extern "C" fn(*mut Raster, i32, i32) -> i32,
    pub set_render_state: unsafe extern "C" fn(i32, *mut c_void),
    pub get_render_state: unsafe extern "C" fn(i32) -> *mut c_void,
    pub im2d_render_line: unsafe extern "C" fn(*mut c_void, i32, i32, i32),
    pub im2d_render_triangle: unsafe extern "C" fn(*mut c_void, i32, i32, i32, i32),
    pub im2d_render_primitive: unsafe extern "C" fn(u32, *mut c_void, i32),
    pub im2d_render_indexed_primitive: unsafe extern "C" fn(u32, *mut c_void, i32, *mut c_void, i32),
    pub im3d_transform: unsafe extern "C" fn(*mut c_void, i32, *const RwMatrix, u32),
    pub im3d_render_primitive: unsafe extern "C" fn(u32),
    pub im3d_render_indexed_primitive: unsafe extern "C" fn(u32, *mut c_void, i32),
    pub im3d_end: unsafe extern "C" fn(),
    pub system: unsafe extern "C" fn(i32, *mut c_void, i32) -> i32,
}

/// The wgpu render device.
pub const RENDER_DEVICE: Device = Device {
    z_near: 0.0,
    z_far: 1.0,
    begin_update,
    end_update,
    clear_camera,
    show_raster,
    raster_render_fast,
    set_render_state: set_render_state_cb,
    get_render_state: get_render_state_cb,
    im2d_render_line,
    im2d_render_triangle,
    im2d_render_primitive,
    im2d_render_indexed_primitive,
    im3d_transform,
    im3d_render_primitive,
    im3d_render_indexed_primitive,
    im3d_end,
    system: device_system,
};

// ---------------------------------------------------------------- C ABI (wgpudevice.cpp forwards)

/// # Safety
/// `d` points at an rw::Device.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_get(d: *mut Device) {
    unsafe { d.write(RENDER_DEVICE) };
}

/// # Safety
/// As the Rust functions they forward to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_evict_raster(r: *mut Raster) {
    unsafe { evict_raster(r) }
}
/// # Safety
/// As set_texture_stage0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_set_texture_stage0(t: *mut Texture) {
    unsafe { set_texture_stage0(t) }
}
/// # Safety
/// As set_im2d_override.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_set_im2d_override(shader: u32, params: *const f32) {
    unsafe { set_im2d_override(shader, params) }
}
/// # Safety
/// `s` points at a DrawState.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_current_draw_state(s: *mut DrawState) {
    unsafe { s.write(current_draw_state()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_device_camera_xform() -> *const f32 {
    camera_xform()
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_device_begin_overlay() {
    host::revc_rw_begin_overlay();
}
/// # Safety
/// As im2d_render_indexed_primitive_uv2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_device_im2d_uv2(prim: u32, v: *mut c_void, nv: i32, i: *mut c_void, ni: i32, tex1: *mut Texture) {
    unsafe { im2d_render_indexed_primitive_uv2(prim, v, nv, i, ni, tex1) }
}

const _: () = assert!(size_of::<Device>() == 136 && size_of::<DrawState>() == 120 && size_of::<EngineOpenParams>() == 32);

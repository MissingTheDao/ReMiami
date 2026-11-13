//! Host side of librw's WGPU platform (feature `wgpu`, plan task 12): the window has no GL
//! context; frames are presented by revc-rw on Vulkan or OpenGL (REVC_RENDERER_BACKEND=vulkan|gl, dx12 on Windows;
//! default vulkan). The device's camera clears, texture uploads and Im2D draws are recorded by
//! revc_rw::frame::Renderer and drawn when the frame is presented; Im3D and models come next.

use revc_rw::Backend;
use revc_rw::frame::{Antialiasing, DrawState, EnvMap, FrameSettings, MAX_BONES, Mat4, MeshExtras, MeshVertex, ModelState, Recorder, SkinVertex, TexData, Vertex2D, Vertex2DUV2, Vertex3D, WgpuBackend, Xform};
use revc_rw::present::{NativeWindow, Presenter};
use revc_rw::shadow::{Comparison, GATE, Shadow};
use revc_rw::thread::{Done, RenderThread};
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

/// Where recorded frames are drawn: on the game thread, or on the render thread
/// (REVC_RENDER_THREAD=1).
enum Out {
    Inline(Box<(Presenter, WgpuBackend)>),
    Thread(RenderThread),
}

/// The front end (records on the game thread) and where its frames go.
struct State {
    front: Recorder,
    out: Out,
    window: *mut SDL_Window,
    /// Output size at the last begin_frame.
    size: (u32, u32),
    /// supported_samples(wanted) for wanted 0..=16, so the game thread needs no device.
    samples: [u32; 17],
    /// REVC_RENDER_SHADOW=1: the retro-wgpu back end draws every frame off-screen too.
    shadow: Option<ShadowRun>,
}

/// Shadow mode: frames are compared every `every` frames and on every capture.
struct ShadowRun {
    shadow: Shadow,
    every: u64,
    n: u64,
    checked: u64,
    failed: u64,
    worst: f64,
    dir: Option<std::path::PathBuf>,
}

impl ShadowRun {
    fn report(&mut self, c: Comparison, capture: bool) {
        self.checked += 1;
        self.worst = self.worst.min(c.within);
        let pass = c.passed();
        if !pass {
            self.failed += 1;
        }
        if !pass || capture {
            eprintln!("revc: shadow frame {}{}: {:.3} % of pixels within 2 levels, max diff {} ({})", c.frame, if capture { " (capture)" } else { "" }, c.within * 100.0, c.max_diff, if pass { "pass" } else { "BELOW GATE" });
        }
        if let (Some(dir), Some((a, b))) = (&self.dir, &c.images)
            && self.failed <= 8
        {
            let _ = std::fs::create_dir_all(dir);
            for (name, img) in [("wgpu", a), ("retro", b)] {
                let path = dir.join(format!("shadow-{:06}-{name}.png", c.frame));
                if let Err(e) = png_write(&path, c.width, c.height, img) {
                    eprintln!("revc: shadow: {}: {e}", path.display());
                }
            }
        }
    }
}

impl Drop for ShadowRun {
    fn drop(&mut self) {
        eprintln!("revc: shadow summary: {} frames checked, {} below the gate ({:.1} % within 2 levels), worst {:.3} %", self.checked, self.failed, GATE * 100.0, self.worst * 100.0);
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_front<R>(f: impl FnOnce(&mut Recorder) -> R) -> Option<R> {
    STATE.with(|c| c.borrow_mut().as_mut().map(|s| f(&mut s.front)))
}

fn render_thread_wanted() -> bool {
    matches!(std::env::var("REVC_RENDER_THREAD").as_deref(), Ok("1") | Ok("on") | Ok("yes"))
}

/// rwCAMERACLEARIMAGE, rwCAMERACLEARZ
const CLEAR_IMAGE: u32 = 1;
const CLEAR_Z: u32 = 2;

fn backend() -> Backend {
    match std::env::var("REVC_RENDERER_BACKEND").as_deref() {
        Ok("gl") | Ok("opengl") => Backend::Gl,
        #[cfg(windows)]
        Ok("dx12") | Ok("d3d12") => Backend::Dx12,
        _ => Backend::Vulkan,
    }
}

fn native(window: *mut SDL_Window) -> Result<NativeWindow, String> {
    unsafe {
        let props = SDL_GetWindowProperties(window);
        let driver = CStr::from_ptr(SDL_GetCurrentVideoDriver()).to_string_lossy().into_owned();
        match driver.as_str() {
            "wayland" => {
                let display = SDL_GetPointerProperty(props, SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER, std::ptr::null_mut());
                let surface = SDL_GetPointerProperty(props, SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER, std::ptr::null_mut());
                Ok(NativeWindow::Wayland { display: NonNull::new(display).ok_or("no wl_display")?, surface: NonNull::new(surface).ok_or("no wl_surface")? })
            }
            "x11" => {
                let display = SDL_GetPointerProperty(props, SDL_PROP_WINDOW_X11_DISPLAY_POINTER, std::ptr::null_mut());
                let screen = SDL_GetNumberProperty(props, SDL_PROP_WINDOW_X11_SCREEN_NUMBER, 0) as i32;
                let window = SDL_GetNumberProperty(props, SDL_PROP_WINDOW_X11_WINDOW_NUMBER, 0) as u64;
                Ok(NativeWindow::X11 { display: NonNull::new(display), screen, window })
            }
            other => Err(format!("video driver {other} not supported by the wgpu renderer")),
        }
    }
}

fn pixel_size(window: *mut SDL_Window) -> (u32, u32) {
    let (mut w, mut h) = (0, 0);
    unsafe { SDL_GetWindowSizeInPixels(window, &mut w, &mut h) };
    (w.max(1) as u32, h.max(1) as u32)
}

/// Sets up presenting to `window` (created without SDL_WINDOW_OPENGL), with `samples`-times
/// MSAA (the game's MultiSampling setting; clamped to what the GPU does).
pub fn create(window: *mut SDL_Window, samples: i32) -> Result<(), String> {
    let (w, h) = pixel_size(window);
    unsafe extern "C" {
        fn revc_cpp_prefs_vsync() -> i32;
    }
    let vsync = unsafe { revc_cpp_prefs_vsync() } != 0;
    let p = unsafe { Presenter::new(backend(), native(window)?, w, h, vsync)? };
    let bc = p.gpu.bc_textures();
    eprintln!("revc: wgpu renderer on {} ({:?}), {:?} {w}x{h}, DXT {}, models in {}", p.gpu.info.name, p.gpu.info.backend, p.format(), if bc { "on the GPU" } else { "decoded on the CPU" }, if p.gpu.model_storage { "a storage buffer" } else { "uniform blocks" });
    let back = WgpuBackend::with_samples(&p.gpu, p.format(), samples.max(1) as u32);
    let used = back.antialiasing().msaa;
    if used > 1 || samples > 1 {
        eprintln!("revc: MSAA {used}x (asked {samples}x)");
    }
    let table: [u32; 17] = std::array::from_fn(|n| WgpuBackend::supported_samples(&p.gpu, n.max(1) as u32));
    let shadow_wanted = matches!(std::env::var("REVC_RENDER_SHADOW").as_deref(), Ok("1") | Ok("on") | Ok("yes"));
    let shadow = (shadow_wanted && !render_thread_wanted()).then(|| {
        let every = std::env::var("REVC_RENDER_SHADOW_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(30u64).max(1);
        eprintln!("revc: shadow mode on: retro-wgpu draws every frame off-screen, compared every {every} frames and on captures");
        ShadowRun { shadow: Shadow::new(&p.gpu), every, n: 0, checked: 0, failed: 0, worst: 1.0, dir: std::env::var_os("REVC_RENDER_SHADOW_DIR").map(Into::into) }
    });
    if shadow_wanted && shadow.is_none() {
        eprintln!("revc: shadow mode needs the inline renderer (REVC_RENDER_THREAD off); not started");
    }
    let out = if render_thread_wanted() {
        eprintln!("revc: render thread on");
        Out::Thread(RenderThread::spawn(p, back)?)
    } else {
        Out::Inline(Box::new((p, back)))
    };
    STATE.with(|c| *c.borrow_mut() = Some(State { front: Recorder::new(used), out, window, size: (w, h), samples: table, shadow }));
    Ok(())
}

/// Before the window is destroyed.
pub fn destroy() {
    // Dropping a render thread waits for its frames.
    STATE.with(|c| *c.borrow_mut() = None);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_begin_frame(_window: *mut c_void) {
    STATE.with(|c| {
        if let Some(st) = c.borrow_mut().as_mut()
            && !st.window.is_null()
        {
            st.size = pixel_size(st.window);
            if let Out::Inline(b) = &mut st.out
                && b.0.size() != st.size
            {
                b.0.resize(st.size.0, st.size.1);
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_clear(r: f32, g: f32, b: f32, a: f32, mode: u32) {
    let color = (mode & CLEAR_IMAGE != 0).then_some([r as f64, g as f64, b as f64, a as f64]);
    with_front(|f| f.clear(color, mode & CLEAR_Z != 0));
}

/// Creates (id 0) or updates a texture from mip levels, RGBA8 (`dxt` 0) or DXT1/3/5 blocks;
/// returns its id, 0 without a window.
///
/// # Safety
/// `levels` points at `num_levels` buffers holding level i of a width x height texture: RGBA8
/// ((width >> i) * (height >> i) * 4 bytes, min 1x1) or whole DXT blocks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_texture_upload(id: u32, width: i32, height: i32, dxt: i32, num_levels: i32, levels: *const *const u8, autogen: i32) -> u32 {
    if levels.is_null() || width <= 0 || height <= 0 || num_levels <= 0 {
        return 0;
    }
    let (w, h) = (width as u32, height as u32);
    let slices: Vec<&[u8]> = (0..num_levels.min(16) as usize)
        .map_while(|i| {
            let p = unsafe { *levels.add(i) };
            let (lw, lh) = ((w >> i).max(1), (h >> i).max(1));
            let len = match dxt {
                0 => (lw * lh * 4) as usize,
                1 => (lw.div_ceil(4) * lh.div_ceil(4) * 8) as usize,
                _ => (lw.div_ceil(4) * lh.div_ceil(4) * 16) as usize,
            };
            (!p.is_null()).then(|| unsafe { std::slice::from_raw_parts(p, len) })
        })
        .collect();
    if slices.is_empty() {
        return 0;
    }
    with_front(|f| f.upload_texture(id, w, h, TexData::from_dxt(dxt as u32), &slices, autogen != 0)).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_texture_destroy(id: u32) {
    with_front(|f| f.destroy_texture(id));
}

/// # Safety
/// `state` is valid; `vertices` holds `num_vertices` Im2DVertex; `indices` is null or holds
/// `num_indices` u16.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_im2d(state: *const DrawState, prim: u32, vertices: *const Vertex2D, num_vertices: i32, indices: *const u16, num_indices: i32) {
    if state.is_null() || vertices.is_null() || num_vertices <= 0 {
        return;
    }
    let state = unsafe { &*state };
    let verts = unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) };
    let idx = (!indices.is_null() && num_indices > 0).then(|| unsafe { std::slice::from_raw_parts(indices, num_indices as usize) });
    with_front(|f| f.draw_2d(state, prim, verts, idx));
}

/// # Safety
/// As `revc_rw_im2d`, with Im3DVertex; `xform` points at an Xform (48 floats).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_im3d(state: *const DrawState, prim: u32, vertices: *const Vertex3D, num_vertices: i32, indices: *const u16, num_indices: i32, xform: *const Xform) {
    if state.is_null() || vertices.is_null() || num_vertices <= 0 || xform.is_null() {
        return;
    }
    let state = unsafe { &*state };
    let verts = unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) };
    let idx = (!indices.is_null() && num_indices > 0).then(|| unsafe { std::slice::from_raw_parts(indices, num_indices as usize) });
    let xf = unsafe { *xform };
    with_front(|f| f.draw_3d(state, prim, verts, idx, &xf));
}

/// Creates (id 0) or replaces a model geometry; returns its id, 0 without a window.
///
/// # Safety
/// `vertices` holds `num_vertices` MeshVertex, `indices` `num_indices` u16 (triangle list).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_geometry_upload(id: u32, vertices: *const MeshVertex, num_vertices: i32, indices: *const u16, num_indices: i32) -> u32 {
    let verts = if vertices.is_null() || num_vertices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) } };
    let idx = if indices.is_null() || num_indices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(indices, num_indices as usize) } };
    with_front(|f| f.upload_geometry(id, verts, idx)).unwrap_or(0)
}

/// As `revc_rw_geometry_upload`, for a skinned geometry.
///
/// # Safety
/// `vertices` holds `num_vertices` SkinVertex, `indices` `num_indices` u16 (triangle list).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_geometry_upload_skin(id: u32, vertices: *const SkinVertex, num_vertices: i32, indices: *const u16, num_indices: i32) -> u32 {
    let verts = if vertices.is_null() || num_vertices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) } };
    let idx = if indices.is_null() || num_indices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(indices, num_indices as usize) } };
    with_front(|f| f.upload_skinned_geometry(id, verts, idx)).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_geometry_destroy(id: u32) {
    with_front(|f| f.destroy_geometry(id));
}

/// # Safety
/// `state` and `model` (a ModelState) are valid; `xform` points at an Xform (projection, view; world unused); `bones` is null
/// (unskinned) or holds `num_bones` matrices of 16 floats; `env` is null or a MatFX env map.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn revc_rw_draw_mesh(state: *const DrawState, geometry: u32, first: u32, count: u32, xform: *const Xform, model: *const ModelState, bones: *const Mat4, num_bones: i32, env: *const EnvMap) {
    if state.is_null() || xform.is_null() || model.is_null() {
        return;
    }
    let (state, model) = unsafe { (&*state, &*model) };
    let xf = unsafe { *xform };
    let bones = (!bones.is_null()).then(|| unsafe { std::slice::from_raw_parts(bones, num_bones.clamp(0, MAX_BONES as i32) as usize) });
    let env = (!env.is_null()).then(|| unsafe { *env });
    with_front(|f| f.draw_mesh(state, geometry, first, count, &xf, model, &MeshExtras { bones, env }));
}

/// # Safety
/// As `revc_rw_im2d`, with Im2DVertex + u2/v2 vertices; `tex1` points at an EnvMap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_im2d_uv2(state: *const DrawState, prim: u32, vertices: *const Vertex2DUV2, num_vertices: i32, indices: *const u16, num_indices: i32, tex1: *const EnvMap) {
    if state.is_null() || vertices.is_null() || num_vertices <= 0 || tex1.is_null() {
        return;
    }
    let (state, tex1) = unsafe { (&*state, &*tex1) };
    let verts = unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) };
    let idx = (!indices.is_null() && num_indices > 0).then(|| unsafe { std::slice::from_raw_parts(indices, num_indices as usize) });
    with_front(|f| f.draw_2d_uv2(state, prim, verts, idx, tex1));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_set_antialiasing(level: i32) {
    STATE.with(|c| {
        if let Some(st) = c.borrow_mut().as_mut() {
            let (name, aa) = Antialiasing::LEVELS.get(level.max(0) as usize).copied().unwrap_or(Antialiasing::LEVELS[0]);
            let samples = st.samples[aa.msaa.min(16) as usize];
            let s = st.front.settings();
            st.front.set_settings(FrameSettings { samples, scale: aa.supersample, fxaa: aa.fxaa, smaa: aa.smaa, ..s });
            let s = st.front.settings();
            eprintln!("revc: antialiasing {name} (MSAA {}x, supersampling {}x, FXAA {}, SMAA {})", s.samples, s.scale, if s.fxaa { "on" } else { "off" }, if s.smaa { "on" } else { "off" });
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_begin_overlay() {
    with_front(|f| f.begin_overlay());
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_set_target(texture: u32) {
    with_front(|f| f.set_target(texture));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_copy_frame(texture: u32, x: u32, y: u32) {
    with_front(|f| f.copy_frame(texture, x, y));
}

/// Takes back drawn frames: recycles them and reports errors.
fn returned(front: &mut Recorder, done: Vec<Done>) -> Option<(u32, u32, Vec<u8>)> {
    let mut shot = None;
    for d in done {
        if let Some(e) = d.error {
            eprintln!("revc: present: {e}");
        }
        if d.capture {
            shot = d.shot;
        }
        front.recycle(d.frame);
    }
    shot
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_present(vsync: i32) {
    let capture = super::with(|h| std::mem::take(&mut h.capture_requested));
    // The game's VSync setting (rwRASTERFLIPWAITVSYNC), like GL3's swap interval.
    let vsync = vsync != 0;
    let shot = STATE.with(|c| {
        let mut b = c.borrow_mut();
        let st = b.as_mut()?;
        let mut frame = st.front.finish();
        match &mut st.out {
            Out::Inline(pb) => {
                let (p, back) = &mut **pb;
                p.set_vsync(vsync);
                let shot = p.present(back, &mut frame, capture).unwrap_or_else(|e| {
                    eprintln!("revc: present: {e}");
                    None
                });
                if let Some(sh) = st.shadow.as_mut() {
                    sh.n += 1;
                    let check = capture || sh.n % sh.every == 0;
                    if let Some(c) = sh.shadow.run(&p.gpu, back, &frame, p.size(), check) {
                        sh.report(c, capture);
                    }
                }
                st.front.recycle(frame);
                shot
            }
            Out::Thread(t) => {
                let done = t.submit(frame, st.size, vsync, capture);
                returned(&mut st.front, done)
            }
        }
    });
    if capture {
        super::with(|h| h.captured = shot);
    }
}

fn png_write(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().map_err(|e| e.to_string())?.write_image_data(rgba).map_err(|e| e.to_string())
}

//! Host side of librw's WGPU platform (feature `wgpu`, plan task 12): the window has no GL
//! context; frames are presented by revc-rw on Vulkan or OpenGL (REVC_RENDERER_BACKEND=vulkan|gl,
//! default vulkan). The device's camera clears, texture uploads and Im2D draws are recorded by
//! revc_rw::frame::Renderer and drawn when the frame is presented; Im3D and models come next.

use revc_rw::Backend;
use revc_rw::frame::{Antialiasing, DrawState, EnvMap, MAX_BONES, Mat4, MeshExtras, MeshVertex, ModelState, Renderer, SkinVertex, TexData, Vertex2D, Vertex2DUV2, Vertex3D};
use revc_rw::present::{NativeWindow, Presenter};
use sdl3_sys::everything::*;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

thread_local! {
    static PRESENTER: RefCell<Option<Presenter>> = const { RefCell::new(None) };
    static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
    static WINDOW: Cell<*mut SDL_Window> = const { Cell::new(std::ptr::null_mut()) };
}

/// rwCAMERACLEARIMAGE, rwCAMERACLEARZ
const CLEAR_IMAGE: u32 = 1;
const CLEAR_Z: u32 = 2;

fn backend() -> Backend {
    match std::env::var("REVC_RENDERER_BACKEND").as_deref() {
        Ok("gl") | Ok("opengl") => Backend::Gl,
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
    let p = unsafe { Presenter::new(backend(), native(window)?, w, h, true)? };
    let bc = p.gpu.bc_textures();
    eprintln!("revc: wgpu renderer on {} ({:?}), {:?} {w}x{h}, DXT {}", p.gpu.info.name, p.gpu.info.backend, p.format(), if bc { "on the GPU" } else { "decoded on the CPU" });
    let r = Renderer::with_samples(&p.gpu, p.format(), samples.max(1) as u32);
    let used = Renderer::supported_samples(&p.gpu, samples.max(1) as u32);
    if used > 1 || samples > 1 {
        eprintln!("revc: MSAA {used}x (asked {samples}x)");
    }
    RENDERER.with(|c| *c.borrow_mut() = Some(r));
    PRESENTER.with(|c| *c.borrow_mut() = Some(p));
    WINDOW.with(|c| c.set(window));
    Ok(())
}

/// Before the window is destroyed.
pub fn destroy() {
    RENDERER.with(|c| *c.borrow_mut() = None);
    PRESENTER.with(|c| *c.borrow_mut() = None);
    WINDOW.with(|c| c.set(std::ptr::null_mut()));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_begin_frame(_window: *mut c_void) {
    let window = WINDOW.with(|c| c.get());
    if window.is_null() {
        return;
    }
    let size = pixel_size(window);
    PRESENTER.with(|c| {
        if let Some(p) = c.borrow_mut().as_mut()
            && p.size() != size
        {
            p.resize(size.0, size.1);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_clear(r: f32, g: f32, b: f32, a: f32, mode: u32) {
    let color = (mode & CLEAR_IMAGE != 0).then_some([r as f64, g as f64, b as f64, a as f64]);
    RENDERER.with(|c| {
        if let Some(rd) = c.borrow_mut().as_mut() {
            rd.clear(color, mode & CLEAR_Z != 0);
        }
    });
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
    PRESENTER.with(|pc| {
        RENDERER.with(|rc| match (pc.borrow().as_ref(), rc.borrow_mut().as_mut()) {
            (Some(p), Some(r)) => r.upload_texture(&p.gpu, id, w, h, TexData::from_dxt(dxt as u32), &slices, autogen != 0),
            _ => 0,
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_texture_destroy(id: u32) {
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.destroy_texture(id);
        }
    });
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
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.draw_2d(state, prim, verts, idx);
        }
    });
}

/// # Safety
/// As `revc_rw_im2d`, with Im3DVertex; `mvp` points at 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_im3d(state: *const DrawState, prim: u32, vertices: *const Vertex3D, num_vertices: i32, indices: *const u16, num_indices: i32, mvp: *const f32) {
    if state.is_null() || vertices.is_null() || num_vertices <= 0 || mvp.is_null() {
        return;
    }
    let state = unsafe { &*state };
    let verts = unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) };
    let idx = (!indices.is_null() && num_indices > 0).then(|| unsafe { std::slice::from_raw_parts(indices, num_indices as usize) });
    let mvp: Mat4 = unsafe { *mvp.cast::<Mat4>() };
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.draw_3d(state, prim, verts, idx, &mvp);
        }
    });
}

/// Creates (id 0) or replaces a model geometry; returns its id, 0 without a window.
///
/// # Safety
/// `vertices` holds `num_vertices` MeshVertex, `indices` `num_indices` u16 (triangle list).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_geometry_upload(id: u32, vertices: *const MeshVertex, num_vertices: i32, indices: *const u16, num_indices: i32) -> u32 {
    let verts = if vertices.is_null() || num_vertices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) } };
    let idx = if indices.is_null() || num_indices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(indices, num_indices as usize) } };
    PRESENTER.with(|pc| {
        RENDERER.with(|rc| match (pc.borrow().as_ref(), rc.borrow_mut().as_mut()) {
            (Some(p), Some(r)) => r.upload_geometry(&p.gpu, id, verts, idx),
            _ => 0,
        })
    })
}

/// As `revc_rw_geometry_upload`, for a skinned geometry.
///
/// # Safety
/// `vertices` holds `num_vertices` SkinVertex, `indices` `num_indices` u16 (triangle list).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rw_geometry_upload_skin(id: u32, vertices: *const SkinVertex, num_vertices: i32, indices: *const u16, num_indices: i32) -> u32 {
    let verts = if vertices.is_null() || num_vertices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(vertices, num_vertices as usize) } };
    let idx = if indices.is_null() || num_indices <= 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(indices, num_indices as usize) } };
    PRESENTER.with(|pc| {
        RENDERER.with(|rc| match (pc.borrow().as_ref(), rc.borrow_mut().as_mut()) {
            (Some(p), Some(r)) => r.upload_skinned_geometry(&p.gpu, id, verts, idx),
            _ => 0,
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_geometry_destroy(id: u32) {
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.destroy_geometry(id);
        }
    });
}

/// # Safety
/// `state` and `model` (a ModelState) are valid; `view_proj` points at 16 floats; `bones` is null
/// (unskinned) or holds `num_bones` matrices of 16 floats; `env` is null or a MatFX env map.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn revc_rw_draw_mesh(state: *const DrawState, geometry: u32, first: u32, count: u32, view_proj: *const f32, model: *const ModelState, bones: *const Mat4, num_bones: i32, env: *const EnvMap) {
    if state.is_null() || view_proj.is_null() || model.is_null() {
        return;
    }
    let (state, model) = unsafe { (&*state, &*model) };
    let vp: Mat4 = unsafe { *view_proj.cast::<Mat4>() };
    let bones = (!bones.is_null()).then(|| unsafe { std::slice::from_raw_parts(bones, num_bones.clamp(0, MAX_BONES as i32) as usize) });
    let env = (!env.is_null()).then(|| unsafe { *env });
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.draw_mesh(state, geometry, first, count, &vp, model, &MeshExtras { bones, env });
        }
    });
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
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.draw_2d_uv2(state, prim, verts, idx, tex1);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_set_antialiasing(level: i32) {
    PRESENTER.with(|pc| {
        RENDERER.with(|rc| {
            if let (Some(p), Some(r)) = (pc.borrow().as_ref(), rc.borrow_mut().as_mut()) {
                let (name, _) = Antialiasing::LEVELS.get(level.max(0) as usize).copied().unwrap_or(Antialiasing::LEVELS[0]);
                r.set_antialiasing(&p.gpu, Antialiasing::level(level.max(0) as u32));
                let aa = r.antialiasing();
                eprintln!("revc: antialiasing {name} (MSAA {}x, supersampling {}x, FXAA {}, SMAA {})", aa.msaa, aa.supersample, if aa.fxaa { "on" } else { "off" }, if aa.smaa { "on" } else { "off" });
            }
        })
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_set_target(texture: u32) {
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.set_target(texture);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_copy_frame(texture: u32, x: u32, y: u32) {
    RENDERER.with(|c| {
        if let Some(r) = c.borrow_mut().as_mut() {
            r.copy_frame(texture, x, y);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_present(_vsync: i32) {
    let capture = super::with(|h| std::mem::take(&mut h.capture_requested));
    let shot = PRESENTER.with(|pc| {
        RENDERER.with(|rc| match (pc.borrow_mut().as_mut(), rc.borrow_mut().as_mut()) {
            (Some(p), Some(r)) => p.present_frame(r, capture).unwrap_or_else(|e| {
                eprintln!("revc: present: {e}");
                None
            }),
            _ => None,
        })
    });
    if capture {
        super::with(|h| h.captured = shot);
    }
}

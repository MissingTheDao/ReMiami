//! Host side of librw's WGPU platform (feature `wgpu`, plan task 12): the window has no GL
//! context; frames are presented by revc-rw on Vulkan or OpenGL (REVC_RENDERER_BACKEND=vulkan|gl,
//! default vulkan). First step of the port: the device clears and presents; drawing comes next.

use revc_rw::Backend;
use revc_rw::present::{NativeWindow, Presenter};
use sdl3_sys::everything::*;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

thread_local! {
    static PRESENTER: RefCell<Option<Presenter>> = const { RefCell::new(None) };
    static CLEAR: Cell<[f64; 4]> = const { Cell::new([0.0, 0.0, 0.0, 1.0]) };
    static WINDOW: Cell<*mut SDL_Window> = const { Cell::new(std::ptr::null_mut()) };
}

/// rwCAMERACLEARIMAGE
const CLEAR_IMAGE: u32 = 1;

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

/// Sets up presenting to `window` (created without SDL_WINDOW_OPENGL).
pub fn create(window: *mut SDL_Window) -> Result<(), String> {
    let (w, h) = pixel_size(window);
    let p = unsafe { Presenter::new(backend(), native(window)?, w, h, true)? };
    eprintln!("revc: wgpu renderer on {} ({:?}), {:?} {w}x{h}", p.gpu.info.name, p.gpu.info.backend, p.format());
    PRESENTER.with(|c| *c.borrow_mut() = Some(p));
    WINDOW.with(|c| c.set(window));
    Ok(())
}

/// Before the window is destroyed.
pub fn destroy() {
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
    if mode & CLEAR_IMAGE != 0 {
        CLEAR.with(|c| c.set([r as f64, g as f64, b as f64, a as f64]));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_rw_present(_vsync: i32) {
    let colour = CLEAR.with(|c| c.get());
    PRESENTER.with(|c| {
        if let Some(p) = c.borrow_mut().as_mut()
            && let Err(e) = p.clear_and_present(colour)
        {
            eprintln!("revc: present: {e}");
        }
    });
}

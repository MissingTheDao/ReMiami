//! Opens an SDL3 window without a GL context (as the game host will for the wgpu renderer) and
//! presents cleared frames through revc-rw on the chosen backend.
//! usage: cargo run -p revc-rw --example window_clear -- [vulkan|gl] [frames]
use revc_rw::Backend;
use revc_rw::present::{NativeWindow, Presenter};
use sdl3_sys::everything::*;
use std::ffi::CStr;
use std::ptr::NonNull;

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
            other => Err(format!("video driver {other} not supported")),
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let backend = match args.next().as_deref() {
        Some("gl") => Backend::Gl,
        _ => Backend::Vulkan,
    };
    let frames: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(120);
    unsafe {
        assert!(SDL_Init(SDL_INIT_VIDEO), "SDL_Init failed");
        let window = SDL_CreateWindow(c"revc-rw window_clear".as_ptr(), 640, 360, SDL_WINDOW_RESIZABLE);
        assert!(!window.is_null(), "SDL_CreateWindow failed");
        let nw = native(window).unwrap_or_else(|e| panic!("{e}"));
        let mut p = Presenter::new(backend, nw, 640, 360, true).unwrap_or_else(|e| panic!("{e}"));
        println!("{backend:?}: {} ({:?}), surface {:?} {:?}", p.gpu.info.name, p.gpu.info.backend, p.format(), p.size());
        let start = std::time::Instant::now();
        let mut ev = SDL_Event::default();
        for i in 0..frames {
            while SDL_PollEvent(&mut ev) {
                if SDL_EventType(ev.r#type) == SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED {
                    p.resize(ev.window.data1 as u32, ev.window.data2 as u32);
                }
            }
            let t = i as f64 / frames as f64;
            p.clear_and_present([0.1 + 0.6 * t, 0.2, 0.5 - 0.3 * t, 1.0]).unwrap_or_else(|e| panic!("frame {i}: {e}"));
        }
        println!("{frames} frames in {:.2}s", start.elapsed().as_secs_f64());
        drop(p);
        SDL_DestroyWindow(window);
        SDL_Quit();
    }
}

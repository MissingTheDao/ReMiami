//! The SDL3 platform host. Replaces GLFW for the C++ game:
//!  - `revc_host_window_api()` gives librw's HOST backend its window, GL context,
//!    buffer swaps and video modes (rw::gl3::HostWindowApi in vendor/librw/src/gl/rwgl3.h);
//!  - the `host*` functions implement skel/hostapi.h (mouse, window, joysticks, event pump).
//!
//! Everything runs on the game's main thread. State lives in a thread-local `RefCell`;
//! game callbacks (`revc_on_*`) are invoked only after that borrow is released, because
//! the C++ side calls back into `host*` from inside them.

mod geometry;
mod joystick;
mod keymap;

use geometry::Geometry;
use joystick::{HostGamepadState, Joysticks};
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_double, c_int, c_void};
use std::ptr::{null, null_mut};

// ---------------------------------------------------------------- C++ side

unsafe extern "C" {
    fn revc_on_key(key: c_int, down: c_int);
    fn revc_on_resize(width: c_int, height: c_int);
    fn revc_on_scroll(dx: c_double, dy: c_double);
    fn revc_on_cursor(x: c_double, y: c_double);
    fn revc_on_cursor_enter(entered: c_int);
    fn revc_on_focus(focused: c_int);
    fn revc_on_iconify(iconified: c_int);
    fn revc_on_joystick(jid: c_int, connected: c_int);
    fn revc_on_toggle_fullscreen();
}

const HOST_CURSOR_NORMAL: c_int = 0;
const HOST_CURSOR_HIDDEN: c_int = 1;
const HOST_CURSOR_DISABLED: c_int = 2;
const VIDEOMODEEXCLUSIVE: u32 = 1; // rw::VideoModeFlag

/// rw::gl3::HostVideoMode
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HostVideoMode {
    width: i32,
    height: i32,
    depth: i32,
    refresh_rate: i32,
    flags: u32,
}

/// rw::gl3::HostWindowApi
#[repr(C)]
pub struct HostWindowApi {
    user: *mut c_void,
    num_displays: unsafe extern "C" fn(*mut c_void) -> i32,
    display_name: unsafe extern "C" fn(*mut c_void, i32) -> *const c_char,
    num_video_modes: unsafe extern "C" fn(*mut c_void, i32) -> i32,
    get_video_mode: unsafe extern "C" fn(*mut c_void, i32, i32, *mut HostVideoMode) -> i32,
    create_window: unsafe extern "C" fn(
        *mut c_void, *const c_char, i32, *const HostVideoMode, i32, i32, i32, *mut i32, *mut i32,
    ) -> *mut c_void,
    destroy_window: unsafe extern "C" fn(*mut c_void, *mut c_void),
    swap_buffers: unsafe extern "C" fn(*mut c_void, *mut c_void, i32),
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_drawable_size: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32),
}
// SAFETY: contains only function pointers and a null user pointer.
unsafe impl Sync for HostWindowApi {}

// ---------------------------------------------------------------- state

struct Display {
    id: SDL_DisplayID,
    name: CString,
    /// Fullscreen modes, deduplicated by size (highest refresh rate kept). Mode index i+1.
    modes: Vec<SDL_DisplayMode>,
}

struct Host {
    initialised: bool,
    displays: Vec<Display>,
    window: *mut SDL_Window,
    gl: SDL_GLContext,
    swap_interval: Option<i32>,
    should_close: bool,
    cursor_mode: c_int,
    /// Virtual cursor in window coordinates. In DISABLED (relative) mode it is
    /// unbounded and fed by motion deltas, like GLFW's disabled cursor.
    cursor: (f64, f64),
    mouse_buttons: [bool; 5],
    focused: bool,
    joysticks: Joysticks,
    /// Windowed-mode geometry, remembered across launches.
    geometry: Geometry,
    geometry_dirty: bool,
    /// Regression harness: read back the next presented frame.
    capture_requested: bool,
    captured: Option<(u32, u32, Vec<u8>)>,
}

impl Default for Host {
    fn default() -> Self {
        Host {
            initialised: false,
            displays: Vec::new(),
            window: null_mut(),
            gl: null_mut(),
            swap_interval: None,
            should_close: false,
            cursor_mode: HOST_CURSOR_NORMAL,
            cursor: (0.0, 0.0),
            mouse_buttons: [false; 5],
            focused: true,
            joysticks: Joysticks::default(),
            geometry: Geometry::default(),
            geometry_dirty: false,
            capture_requested: false,
            captured: None,
        }
    }
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
}

fn with<R>(f: impl FnOnce(&mut Host) -> R) -> R {
    HOST.with(|h| f(&mut h.borrow_mut()))
}

pub fn sdl_error() -> String {
    unsafe { CStr::from_ptr(SDL_GetError()) }.to_string_lossy().into_owned()
}

/// Initialise SDL. Called from main() before the game starts.
pub fn init() -> Result<(), String> {
    unsafe {
        // Keep the game's unscaled window-coordinate mouse maths valid.
        SDL_SetHint(SDL_HINT_VIDEO_WAYLAND_SCALE_TO_DISPLAY, c"0".as_ptr());
        if !SDL_Init(SDL_INIT_VIDEO | SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD) {
            return Err(sdl_error());
        }
    }
    with(|h| {
        h.initialised = true;
        h.focused = true;
        h.displays = enumerate_displays();
        h.geometry = Geometry::load();
        // Open joysticks that are already connected, so _InputInitialiseJoys() sees them
        // (their JOYSTICK_ADDED events are then ignored as duplicates).
        let mut n: c_int = 0;
        let ids = unsafe { SDL_GetJoysticks(&mut n) };
        if !ids.is_null() {
            for i in 0..n as usize {
                h.joysticks.add(unsafe { *ids.add(i) });
            }
            unsafe { SDL_free(ids.cast()) };
        }
    });
    Ok(())
}

/// Tear down SDL objects. Called from main() after the game returns.
pub fn shutdown() {
    crate::audio::close_device();
    with(|h| {
        if h.geometry_dirty {
            h.geometry.save();
        }
        h.joysticks = Joysticks::default(); // closes devices
        unsafe {
            if !h.gl.is_null() {
                SDL_GL_DestroyContext(h.gl);
            }
            if !h.window.is_null() {
                SDL_DestroyWindow(h.window);
            }
            SDL_Quit();
        }
        h.gl = null_mut();
        h.window = null_mut();
        h.initialised = false;
    });
}

fn enumerate_displays() -> Vec<Display> {
    let mut out = Vec::new();
    let mut n: c_int = 0;
    let ids = unsafe { SDL_GetDisplays(&mut n) };
    if ids.is_null() {
        return out;
    }
    // Primary display first, so display 0 is what the game picks by default.
    let primary = unsafe { SDL_GetPrimaryDisplay() };
    let mut list: Vec<SDL_DisplayID> = (0..n as usize).map(|i| unsafe { *ids.add(i) }).collect();
    unsafe { SDL_free(ids.cast()) };
    list.sort_by_key(|id| *id != primary);

    for id in list {
        let name = unsafe { SDL_GetDisplayName(id) };
        let name = if name.is_null() {
            c"Display".to_owned()
        } else {
            unsafe { CStr::from_ptr(name) }.to_owned()
        };
        let mut modes: Vec<SDL_DisplayMode> = Vec::new();
        let mut count: c_int = 0;
        let list = unsafe { SDL_GetFullscreenDisplayModes(id, &mut count) };
        if !list.is_null() {
            for i in 0..count as usize {
                // SAFETY: SDL_DisplayMode is plain data.
                let m = unsafe { std::ptr::read(*list.add(i)) };
                match modes.iter_mut().find(|e| e.w == m.w && e.h == m.h) {
                    Some(e) if m.refresh_rate > e.refresh_rate => *e = m,
                    Some(_) => {}
                    None => modes.push(m),
                }
            }
            unsafe { SDL_free(list.cast()) };
        }
        out.push(Display { id, name, modes });
    }
    out
}

// ---------------------------------------------------------------- window API (librw)

/// Put a windowed window where it was last time (position is ignored on Wayland).
fn restore_windowed_placement(win: *mut SDL_Window, g: &Geometry) {
    unsafe {
        match g.pos {
            Some((x, y)) => SDL_SetWindowPosition(win, x, y),
            None => SDL_SetWindowPosition(win, SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED),
        };
        if g.maximized {
            SDL_MaximizeWindow(win);
        }
    }
}

/// Record windowed size/position/maximized state from the live window.
fn track_geometry(h: &mut Host) {
    if h.window.is_null() {
        return;
    }
    let flags = unsafe { SDL_GetWindowFlags(h.window) };
    if flags.0 & SDL_WINDOW_FULLSCREEN.0 != 0 || flags.0 & SDL_WINDOW_MINIMIZED.0 != 0 {
        return;
    }
    let maximized = flags.0 & SDL_WINDOW_MAXIMIZED.0 != 0;
    let mut g = h.geometry;
    g.maximized = maximized;
    if !maximized {
        let (mut w, mut ht, mut x, mut y) = (0, 0, 0, 0);
        unsafe {
            SDL_GetWindowSize(h.window, &mut w, &mut ht);
            if SDL_GetWindowPosition(h.window, &mut x, &mut y) && (x, y) != (0, 0) {
                g.pos = Some((x, y));
            }
        }
        if w > 0 && ht > 0 {
            g.size = Some((w, ht));
        }
    }
    if g != h.geometry {
        h.geometry = g;
        h.geometry_dirty = true;
    }
}

unsafe extern "C" fn api_num_displays(_: *mut c_void) -> i32 {
    with(|h| h.displays.len().max(1) as i32)
}

unsafe extern "C" fn api_display_name(_: *mut c_void, display: i32) -> *const c_char {
    with(|h| h.displays.get(display as usize).map_or(null(), |d| d.name.as_ptr()))
}

unsafe extern "C" fn api_num_video_modes(_: *mut c_void, display: i32) -> i32 {
    with(|h| h.displays.get(display as usize).map_or(1, |d| d.modes.len() as i32 + 1))
}

unsafe extern "C" fn api_get_video_mode(_: *mut c_void, display: i32, index: i32, out: *mut HostVideoMode) -> i32 {
    let mode = with(|h| {
        let d = h.displays.get(display as usize)?;
        if index == 0 {
            let m = unsafe { SDL_GetDesktopDisplayMode(d.id) };
            if m.is_null() {
                return None;
            }
            let m = unsafe { &*m };
            Some(HostVideoMode { width: m.w, height: m.h, depth: 32, refresh_rate: m.refresh_rate as i32, flags: 0 })
        } else {
            let m = d.modes.get(index as usize - 1)?;
            Some(HostVideoMode {
                width: m.w,
                height: m.h,
                depth: 32,
                refresh_rate: m.refresh_rate as i32,
                flags: VIDEOMODEEXCLUSIVE,
            })
        }
    });
    match mode {
        Some(m) => {
            unsafe { *out = m };
            1
        }
        None => 0,
    }
}

/// Same fallback order as librw's SDL2/GLFW backends.
const PROFILES: [(SDL_GLProfile, c_int, c_int); 4] = [
    (SDL_GL_CONTEXT_PROFILE_CORE, 3, 3),
    (SDL_GL_CONTEXT_PROFILE_COMPATIBILITY, 2, 1),
    (SDL_GL_CONTEXT_PROFILE_ES, 3, 1),
    (SDL_GL_CONTEXT_PROFILE_ES, 2, 0),
];

unsafe extern "C" fn api_create_window(
    _: *mut c_void,
    title: *const c_char,
    display: i32,
    mode: *const HostVideoMode,
    width: i32,
    height: i32,
    samples: i32,
    gles: *mut i32,
    glversion: *mut i32,
) -> *mut c_void {
    let mode = unsafe { *mode };
    let fullscreen = mode.flags & VIDEOMODEEXCLUSIVE != 0;
    // Points into Host::displays, which is not modified while the window exists.
    let sdl_mode: *const SDL_DisplayMode = with(|h| {
        h.displays
            .get(display as usize)
            .and_then(|d| d.modes.iter().find(|m| m.w == mode.width && m.h == mode.height))
            .map_or(null(), |m| m as *const _)
    });
    let remembered = with(|host| host.geometry);
    let (w, h) = if fullscreen { (mode.width, mode.height) } else { remembered.size.unwrap_or((width, height)) };

    unsafe {
        SDL_GL_SetAttribute(SDL_GL_DEPTH_SIZE, 24);
        SDL_GL_SetAttribute(SDL_GL_STENCIL_SIZE, 8);
        SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);
        SDL_GL_SetAttribute(SDL_GL_MULTISAMPLEBUFFERS, (samples > 0) as c_int);
        SDL_GL_SetAttribute(SDL_GL_MULTISAMPLESAMPLES, samples.max(0));
    }

    for (profile, major, minor) in PROFILES {
        unsafe {
            SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, profile.0);
            SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, major);
            SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, minor);
        }
        let win = unsafe { SDL_CreateWindow(title, w, h, SDL_WINDOW_OPENGL | SDL_WINDOW_RESIZABLE) };
        if win.is_null() {
            eprintln!("revc: SDL_CreateWindow failed: {}", sdl_error());
            continue;
        }
        let ctx = unsafe { SDL_GL_CreateContext(win) };
        if ctx.is_null() {
            unsafe { SDL_DestroyWindow(win) };
            continue;
        }
        unsafe { SDL_GL_MakeCurrent(win, ctx) };
        if !fullscreen {
            restore_windowed_placement(win, &remembered);
        }
        if fullscreen {
            unsafe {
                SDL_SetWindowFullscreenMode(win, sdl_mode);
                SDL_SetWindowFullscreen(win, true);
            }
        }
        unsafe {
            *gles = (profile == SDL_GL_CONTEXT_PROFILE_ES) as i32;
            *glversion = major * 10 + minor;
        }
        with(|host| {
            host.window = win;
            host.gl = ctx;
            host.swap_interval = None;
            host.cursor = (w as f64 / 2.0, h as f64 / 2.0);
        });
        return win.cast();
    }
    eprintln!("revc: could not create an OpenGL context: {}", sdl_error());
    null_mut()
}

unsafe extern "C" fn api_destroy_window(_: *mut c_void, win: *mut c_void) {
    with(|h| unsafe {
        if !h.gl.is_null() {
            SDL_GL_DestroyContext(h.gl);
            h.gl = null_mut();
        }
        SDL_DestroyWindow(win.cast());
        if h.window == win.cast() {
            h.window = null_mut();
        }
    });
}

unsafe extern "C" fn api_swap_buffers(_: *mut c_void, win: *mut c_void, vsync: i32) {
    with(|h| {
        if h.swap_interval != Some(vsync) {
            unsafe { SDL_GL_SetSwapInterval(vsync) };
            h.swap_interval = Some(vsync);
        }
        if h.capture_requested {
            h.capture_requested = false;
            h.captured = read_back_buffer(win.cast());
        }
    });
    unsafe { SDL_GL_SwapWindow(win.cast()) };
}

unsafe extern "C" fn api_get_proc_address(_: *mut c_void, name: *const c_char) -> *mut c_void {
    match unsafe { SDL_GL_GetProcAddress(name) } {
        Some(f) => f as *mut c_void,
        None => null_mut(),
    }
}

unsafe extern "C" fn api_get_drawable_size(_: *mut c_void, win: *mut c_void, w: *mut i32, h: *mut i32) {
    unsafe { SDL_GetWindowSizeInPixels(win.cast(), w, h) };
}

/// Reads the finished frame from the back buffer (before the swap) as top-down RGBA.
fn read_back_buffer(win: *mut SDL_Window) -> Option<(u32, u32, Vec<u8>)> {
    type ReadPixels = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
    type PixelStorei = unsafe extern "C" fn(u32, i32);
    type ReadBuffer = unsafe extern "C" fn(u32);
    const GL_BACK: u32 = 0x0405;
    const GL_PACK_ALIGNMENT: u32 = 0x0D05;
    const GL_RGBA: u32 = 0x1908;
    const GL_UNSIGNED_BYTE: u32 = 0x1401;
    unsafe {
        let read: ReadPixels = std::mem::transmute(SDL_GL_GetProcAddress(c"glReadPixels".as_ptr())?);
        let store: PixelStorei = std::mem::transmute(SDL_GL_GetProcAddress(c"glPixelStorei".as_ptr())?);
        let buffer: ReadBuffer = std::mem::transmute(SDL_GL_GetProcAddress(c"glReadBuffer".as_ptr())?);
        let (mut w, mut h) = (0, 0);
        SDL_GetWindowSizeInPixels(win, &mut w, &mut h);
        if w <= 0 || h <= 0 {
            return None;
        }
        let row = w as usize * 4;
        let mut px = vec![0u8; row * h as usize];
        buffer(GL_BACK);
        store(GL_PACK_ALIGNMENT, 1);
        read(0, 0, w, h, GL_RGBA, GL_UNSIGNED_BYTE, px.as_mut_ptr().cast());
        // GL rows are bottom-up.
        let mut flipped = vec![0u8; px.len()];
        for y in 0..h as usize {
            flipped[y * row..(y + 1) * row].copy_from_slice(&px[(h as usize - 1 - y) * row..(h as usize - y) * row]);
        }
        // The default framebuffer's alpha is meaningless here.
        flipped.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        Some((w as u32, h as u32, flipped))
    }
}

/// Regression harness: capture the next frame that is presented.
pub fn request_capture() {
    with(|h| h.capture_requested = true);
}

/// Regression harness: the captured frame, once available.
pub fn take_capture() -> Option<(u32, u32, Vec<u8>)> {
    with(|h| h.captured.take())
}

static WINDOW_API: HostWindowApi = HostWindowApi {
    user: null_mut(),
    num_displays: api_num_displays,
    display_name: api_display_name,
    num_video_modes: api_num_video_modes,
    get_video_mode: api_get_video_mode,
    create_window: api_create_window,
    destroy_window: api_destroy_window,
    swap_buffers: api_swap_buffers,
    get_proc_address: api_get_proc_address,
    get_drawable_size: api_get_drawable_size,
};

#[unsafe(no_mangle)]
pub extern "C" fn revc_host_window_api() -> *const HostWindowApi {
    &WINDOW_API
}

// ---------------------------------------------------------------- events

/// A game callback, queued while the host state is borrowed.
enum Cb {
    Key(c_int, bool),
    Resize(c_int, c_int),
    Scroll(f64, f64),
    Cursor(f64, f64),
    CursorEnter(bool),
    Focus(bool),
    Iconify(bool),
    Joystick(c_int, bool),
    ToggleFullscreen,
}

/// SDL mouse button number -> hostapi.h HOST_MOUSE_BUTTON_* index.
fn mouse_index(button: u8) -> Option<usize> {
    match button as i32 {
        1 => Some(0), // left
        3 => Some(1), // right
        2 => Some(2), // middle
        4 => Some(3), // x1
        5 => Some(4), // x2
        _ => None,
    }
}

fn pump(h: &mut Host, out: &mut Vec<Cb>) {
    let mut ev = SDL_Event::default();
    while unsafe { SDL_PollEvent(&mut ev) } {
        let ty = SDL_EventType(unsafe { ev.r#type });
        match ty {
            SDL_EVENT_QUIT | SDL_EVENT_WINDOW_CLOSE_REQUESTED => h.should_close = true,

            SDL_EVENT_KEY_DOWN | SDL_EVENT_KEY_UP => {
                let k = unsafe { ev.key };
                let enter = k.scancode == SDL_SCANCODE_RETURN || k.scancode == SDL_SCANCODE_KP_ENTER;
                if enter && (k.r#mod.0 & SDL_KMOD_ALT.0) != 0 {
                    // Alt+Enter toggles fullscreen; the Enter itself is swallowed so it
                    // doesn't reach the game (it is "enter vehicle" on foot).
                    if k.down && !k.repeat {
                        out.push(Cb::ToggleFullscreen);
                    }
                } else if !k.repeat {
                    out.push(Cb::Key(keymap::rs_key_from_scancode(k.scancode), k.down));
                }
            }

            SDL_EVENT_MOUSE_MOTION => {
                let m = unsafe { ev.motion };
                if h.cursor_mode == HOST_CURSOR_DISABLED {
                    h.cursor.0 += m.xrel as f64;
                    h.cursor.1 += m.yrel as f64;
                } else {
                    h.cursor = (m.x as f64, m.y as f64);
                }
                out.push(Cb::Cursor(h.cursor.0, h.cursor.1));
            }
            SDL_EVENT_MOUSE_BUTTON_DOWN | SDL_EVENT_MOUSE_BUTTON_UP => {
                let b = unsafe { ev.button };
                if let Some(i) = mouse_index(b.button) {
                    h.mouse_buttons[i] = b.down;
                }
            }
            SDL_EVENT_MOUSE_WHEEL => {
                let w = unsafe { ev.wheel };
                out.push(Cb::Scroll(w.x as f64, w.y as f64));
            }

            SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                let w = unsafe { ev.window };
                out.push(Cb::Resize(w.data1, w.data2));
            }
            SDL_EVENT_WINDOW_RESIZED | SDL_EVENT_WINDOW_MOVED | SDL_EVENT_WINDOW_MAXIMIZED => track_geometry(h),
            SDL_EVENT_WINDOW_MOUSE_ENTER => out.push(Cb::CursorEnter(true)),
            SDL_EVENT_WINDOW_MOUSE_LEAVE => {
                // A captured (relative) cursor never "leaves", same as GLFW's disabled cursor.
                if h.cursor_mode != HOST_CURSOR_DISABLED {
                    out.push(Cb::CursorEnter(false));
                }
            }
            SDL_EVENT_WINDOW_FOCUS_GAINED => {
                h.focused = true;
                out.push(Cb::Focus(true));
                if h.cursor_mode == HOST_CURSOR_DISABLED {
                    out.push(Cb::CursorEnter(true));
                }
            }
            SDL_EVENT_WINDOW_FOCUS_LOST => {
                h.focused = false;
                h.mouse_buttons = [false; 5];
                out.push(Cb::Focus(false));
            }
            SDL_EVENT_WINDOW_MINIMIZED => out.push(Cb::Iconify(true)),
            SDL_EVENT_WINDOW_RESTORED => {
                track_geometry(h);
                out.push(Cb::Iconify(false));
            }

            SDL_EVENT_JOYSTICK_ADDED => {
                let id = unsafe { ev.jdevice.which };
                if let Some(slot) = h.joysticks.add(id) {
                    out.push(Cb::Joystick(slot as c_int, true));
                }
            }
            SDL_EVENT_JOYSTICK_REMOVED => {
                let id = unsafe { ev.jdevice.which };
                if let Some(slot) = h.joysticks.remove(id) {
                    out.push(Cb::Joystick(slot as c_int, false));
                }
            }
            _ => {}
        }
    }
}

/// Sends a key press/release to the game exactly like a real key event (regression harness).
/// `key` is an RsKeyCodes value (see host/keymap.rs).
pub fn inject_key(key: c_int, down: bool) {
    // SAFETY: game callback on the main thread; no host borrow is held here.
    unsafe { revc_on_key(key, down as c_int) };
}

/// Processes window/input events and dispatches them to the game. Called by the main
/// loop before every revc_game_frame().
pub fn poll_events() {
    let mut cbs = Vec::new();
    with(|h| pump(h, &mut cbs));
    for cb in cbs {
        // SAFETY: plain C entry points in skel/host/host.cpp; no host borrow is held here.
        unsafe {
            match cb {
                Cb::Key(k, d) => revc_on_key(k, d as c_int),
                Cb::Resize(w, h) => revc_on_resize(w, h),
                Cb::Scroll(x, y) => revc_on_scroll(x, y),
                Cb::Cursor(x, y) => revc_on_cursor(x, y),
                Cb::CursorEnter(e) => revc_on_cursor_enter(e as c_int),
                Cb::Focus(f) => revc_on_focus(f as c_int),
                Cb::Iconify(i) => revc_on_iconify(i as c_int),
                Cb::Joystick(j, c) => revc_on_joystick(j, c as c_int),
                Cb::ToggleFullscreen => revc_on_toggle_fullscreen(),
            }
        }
    }
    crate::testdrive::tick();
}

#[unsafe(no_mangle)]
pub extern "C" fn hostWindowShouldClose() -> c_int {
    with(|h| h.should_close as c_int)
}

// ---------------------------------------------------------------- mouse / window

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetCursorPos(x: *mut c_double, y: *mut c_double) {
    let (cx, cy) = with(|h| h.cursor);
    unsafe {
        if !x.is_null() {
            *x = cx;
        }
        if !y.is_null() {
            *y = cy;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetCursorPos(x: c_double, y: c_double) {
    with(|h| {
        h.cursor = (x, y);
        // Only a visible-but-free cursor needs a real warp. In relative mode the position is
        // virtual (this is what makes mouse look work on Wayland, where warping is refused).
        if h.cursor_mode != HOST_CURSOR_DISABLED && h.focused && !h.window.is_null() {
            unsafe { SDL_WarpMouseInWindow(h.window, x as f32, y as f32) };
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostGetMouseButton(button: c_int) -> c_int {
    with(|h| h.mouse_buttons.get(button as usize).copied().unwrap_or(false) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetCursorMode(mode: c_int) {
    let entered = with(|h| {
        if h.window.is_null() {
            h.cursor_mode = mode;
            return false;
        }
        let was_relative = h.cursor_mode == HOST_CURSOR_DISABLED;
        h.cursor_mode = mode;
        unsafe {
            match mode {
                HOST_CURSOR_DISABLED => {
                    SDL_SetWindowRelativeMouseMode(h.window, true);
                }
                HOST_CURSOR_HIDDEN => {
                    SDL_SetWindowRelativeMouseMode(h.window, false);
                    SDL_HideCursor();
                }
                _ => {
                    SDL_SetWindowRelativeMouseMode(h.window, false);
                    SDL_ShowCursor();
                }
            }
        }
        if was_relative && mode != HOST_CURSOR_DISABLED {
            // Leave the virtual position where the real cursor is.
            let (mut x, mut y) = (0f32, 0f32);
            unsafe { SDL_GetMouseState(&mut x, &mut y) };
            h.cursor = (x as f64, y as f64);
        }
        mode == HOST_CURSOR_DISABLED && h.focused
    });
    if entered {
        // SAFETY: no host borrow held.
        unsafe { revc_on_cursor_enter(1) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetWindowSize(w: *mut c_int, ht: *mut c_int) {
    with(|h| unsafe {
        *w = 0;
        *ht = 0;
        if !h.window.is_null() {
            SDL_GetWindowSize(h.window, w, ht);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetWindowSize(w: c_int, ht: c_int) {
    with(|h| {
        if !h.window.is_null() {
            unsafe {
                // A maximized window ignores size requests.
                if SDL_GetWindowFlags(h.window).0 & SDL_WINDOW_MAXIMIZED.0 != 0 {
                    SDL_RestoreWindow(h.window);
                }
                SDL_SetWindowSize(h.window, w, ht);
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetFullscreen(fullscreen: c_int, width: c_int, height: c_int) -> c_int {
    with(|h| {
        if h.window.is_null() {
            return 0;
        }
        let win = h.window;
        let ok = unsafe {
            if fullscreen != 0 {
                // Exclusive mode of that size on the window's display, else borderless desktop.
                let display = SDL_GetDisplayForWindow(win);
                let mode: *const SDL_DisplayMode = h
                    .displays
                    .iter()
                    .find(|d| d.id == display)
                    .and_then(|d| d.modes.iter().find(|m| m.w == width && m.h == height))
                    .map_or(null(), |m| m as *const _);
                SDL_SetWindowFullscreenMode(win, mode) && SDL_SetWindowFullscreen(win, true)
            } else {
                let ok = SDL_SetWindowFullscreen(win, false);
                if ok {
                    let (w, ht) = h.geometry.size.unwrap_or((width, height));
                    SDL_SetWindowSize(win, w, ht);
                    restore_windowed_placement(win, &h.geometry);
                }
                ok
            }
        };
        if !ok {
            eprintln!("revc: fullscreen switch failed: {}", sdl_error());
            return 0;
        }
        unsafe { SDL_SyncWindow(win) };
        if h.geometry_dirty {
            h.geometry.save();
            h.geometry_dirty = false;
        }
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetDesktopMode(w: *mut c_int, ht: *mut c_int, hz: *mut c_int) {
    let m = unsafe { SDL_GetDesktopDisplayMode(SDL_GetPrimaryDisplay()) };
    let (mw, mh, mr) = if m.is_null() {
        (1280, 720, 60)
    } else {
        let m = unsafe { &*m };
        (m.w, m.h, m.refresh_rate as c_int)
    };
    unsafe {
        *w = mw;
        *ht = mh;
        *hz = mr;
    }
}

// ---------------------------------------------------------------- fixed-timestep clock

thread_local! {
    static SIM_CLOCK: RefCell<(revc_timing::FixedStep, Option<std::time::Instant>)> =
        RefCell::new((revc_timing::FixedStep::new(30, 4), None));
}

/// 30 Hz simulation steps due this frame (see revc_timing::FixedStep); `alpha` gets the
/// position between the last step and the next one, for render interpolation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostSimStepsDue(alpha: *mut f32) -> c_int {
    let (steps, a) = SIM_CLOCK.with(|c| {
        let mut c = c.borrow_mut();
        let now = std::time::Instant::now();
        let elapsed = c.1.map_or(std::time::Duration::ZERO, |t| now - t);
        c.1 = Some(now);
        c.0.advance(elapsed)
    });
    if !alpha.is_null() {
        unsafe { *alpha = a };
    }
    steps as c_int
}

// ---------------------------------------------------------------- joysticks

#[unsafe(no_mangle)]
pub extern "C" fn hostJoystickPresent(jid: c_int) -> c_int {
    with(|h| h.joysticks.present(jid) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostJoystickIsGamepad(jid: c_int) -> c_int {
    with(|h| h.joysticks.is_gamepad(jid) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostGetJoystickName(jid: c_int) -> *const c_char {
    with(|h| h.joysticks.name(jid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetJoystickButtons(jid: c_int, count: *mut c_int) -> *const u8 {
    let (p, n) = with(|h| h.joysticks.buttons(jid));
    if !count.is_null() {
        unsafe { *count = n };
    }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetJoystickAxes(jid: c_int, count: *mut c_int) -> *const f32 {
    let (p, n) = with(|h| h.joysticks.axes(jid));
    if !count.is_null() {
        unsafe { *count = n };
    }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetGamepadState(jid: c_int, state: *mut HostGamepadState) -> c_int {
    match with(|h| h.joysticks.gamepad_state(jid)) {
        Some(s) => {
            unsafe { *state = s };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostUpdateGamepadMappings(mappings: *const c_char) -> c_int {
    if mappings.is_null() {
        return 0;
    }
    let bytes = unsafe { CStr::from_ptr(mappings) }.to_bytes();
    let n = unsafe {
        let io = SDL_IOFromConstMem(bytes.as_ptr().cast(), bytes.len());
        SDL_AddGamepadMappingsFromIO(io, true)
    };
    with(|h| h.joysticks.refresh_gamepads());
    (n >= 0) as c_int
}

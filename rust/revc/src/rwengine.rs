//! The engine core (stage 2 step 7b): librw's Engine init/open/start/stop/close/term state
//! machine, the default memory functions, the driver tables, the subsystem and video mode
//! queries, and the last-error record with its message table (engine.cpp, error.cpp). The
//! per-class plugin lists and modules that are still C++ are reached through hooks
//! (engine.cpp); the device is rwdevice.rs.
//!
//! The order of every step is librw's (engine.cpp at 05e88f2); checked by the scenario
//! suites, which open, start, stop and close the engine on both backends.

use crate::rwdevice::{self, Device};
use crate::rwframe::LinkList;
use crate::rwplugin::{self, PluginList};
use std::ffi::{CString, c_char, c_void};
use std::ptr::{addr_of, addr_of_mut, null_mut};
use std::sync::OnceLock;

pub const NUM_PLATFORMS: usize = 14;

/// rw::MemoryFunctions
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryFunctions {
    pub rwmalloc: unsafe extern "C" fn(usize, u32) -> *mut c_void,
    pub rwrealloc: unsafe extern "C" fn(*mut c_void, usize, u32) -> *mut c_void,
    pub rwfree: unsafe extern "C" fn(*mut c_void),
    pub rwmustmalloc: Option<unsafe extern "C" fn(usize, u32) -> *mut c_void>,
    pub rwmustrealloc: Option<unsafe extern "C" fn(*mut c_void, usize, u32) -> *mut c_void>,
}

/// rw::Driver (driver plugin data follows it)
#[repr(C)]
pub struct Driver {
    pub default_pipeline: *mut c_void,
    pub raster_native_offset: i32,
    pub raster_create: *const c_void,
    pub raster_lock: *const c_void,
    pub raster_unlock: *const c_void,
    pub raster_lock_palette: *const c_void,
    pub raster_unlock_palette: *const c_void,
    pub raster_num_levels: *const c_void,
    pub image_find_raster_format: *const c_void,
    pub raster_from_image: *const c_void,
    pub raster_to_image: *const c_void,
}

/// rw::Engine (engine plugin data follows it)
#[repr(C)]
pub struct Engine {
    pub current_camera: *mut c_void,
    pub current_world: *mut c_void,
    pub frame_dirty_list: LinkList,
    pub driver: [*mut Driver; NUM_PLATFORMS],
    pub device: Device,
    pub dummy_default_pipeline: *mut c_void,
}
const _: () = assert!(size_of::<Engine>() == 288 && size_of::<Driver>() == 88 && size_of::<MemoryFunctions>() == 40);

/// What is still C++: the class plugin lists and modules, the pipeline objects, the image
/// format table.
#[repr(C)]
pub struct Hooks {
    /// &rw::engine
    pub engine: *mut *mut Engine,
    /// &Engine::memfuncs
    pub memfuncs: *mut MemoryFunctions,
    /// &Engine::s_plglist
    pub engine_plugins: *mut PluginList,
    /// Driver::s_plglist (NUM_PLATFORMS lists)
    pub driver_plugins: *mut PluginList,
    /// Frame/Image/Raster/Texture::registerModule, the numAllocated resets, then the wgpu
    /// driver's plugins (engine.cpp's "core plugin attach" and "driver plugin attach")
    pub register_modules: unsafe extern "C" fn(),
    /// tga, bmp and png
    pub register_image_formats: unsafe extern "C" fn(),
    /// ObjPipeline::create() / destroy()
    pub pipeline_create: unsafe extern "C" fn() -> *mut c_void,
    pub pipeline_destroy: unsafe extern "C" fn(*mut c_void),
}
static mut HOOKS: Option<Hooks> = None;
fn hooks() -> &'static Hooks {
    unsafe { (*addr_of!(HOOKS)).as_ref().expect("engine hooks") }
}

/// # Safety
/// Called once at startup with pointers to the C++ statics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_set_hooks(h: Hooks) {
    unsafe { *addr_of_mut!(HOOKS) = Some(h) };
}

// ---------------------------------------------------------------- errors (error.cpp)

/// rw::Error
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Error {
    pub plugin: u32,
    pub code: u32,
}
static mut ERROR: Error = Error { plugin: 0, code: 0 };

const ERR_GENERAL: u32 = 0x8000_0001;
const ERR_ENGINEINIT: u32 = 0x8000_0007;
const ERR_ENGINEOPEN: u32 = 0x8000_0008;
const ERR_ENGINESTART: u32 = 0x8000_0009;

/// The message formats, "No error" then base.err in order (the same file the C++ error enum
/// is generated from).
fn error_formats() -> &'static [CString] {
    static F: OnceLock<Vec<CString>> = OnceLock::new();
    F.get_or_init(|| {
        let src = include_str!("../../../cpp/re3-miami/vendor/librw/src/base.err");
        let mut v = vec![CString::new("No error").unwrap()];
        for entry in src.split("ECODE(").skip(1) {
            let s = entry.find('"').expect("base.err format");
            let e = entry[s + 1..].find("\")").expect("base.err format") + s + 1;
            v.push(CString::new(&entry[s + 1..e]).unwrap());
        }
        v
    })
}

/// RWERROR: records the error (librw prints it with its file and line before).
fn rw_error(plugin: u32, code: u32) {
    let f = error_formats().get((code & !0x8000_0000) as usize).map_or("?", |c| c.to_str().unwrap_or("?"));
    eprintln!("[RW] {f}");
    unsafe { *addr_of_mut!(ERROR) = Error { plugin, code } };
}

/// # Safety
/// `e` points at an Error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_set_error(e: *const Error) {
    unsafe { *addr_of_mut!(ERROR) = *e };
}

/// Returns the last error and clears it.
///
/// # Safety
/// `e` points at an Error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_get_error(e: *mut Error) -> *mut Error {
    unsafe {
        *e = *addr_of!(ERROR);
        *addr_of_mut!(ERROR) = Error::default();
    }
    e
}

/// The printf format of an error code (with or without the 0x80000000 bit).
#[unsafe(no_mangle)]
pub extern "C" fn revc_error_format(code: u32) -> *const c_char {
    error_formats().get((code & !0x8000_0000) as usize).map_or(c"Unknown error %d".as_ptr(), |c| c.as_ptr())
}

// ---------------------------------------------------------------- memory functions

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(p: *mut c_void, size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}

unsafe extern "C" fn malloc_h(sz: usize, _hint: u32) -> *mut c_void {
    if sz == 0 {
        return null_mut();
    }
    unsafe { malloc(sz) }
}
unsafe extern "C" fn realloc_h(p: *mut c_void, sz: usize, _hint: u32) -> *mut c_void {
    unsafe { realloc(p, sz) }
}
unsafe extern "C" fn free_h(p: *mut c_void) {
    unsafe { free(p) }
}

fn out_of_memory() -> ! {
    eprintln!("Error: out of memory");
    std::process::exit(1);
}
unsafe extern "C" fn mustmalloc_h(sz: usize, hint: u32) -> *mut c_void {
    let r = unsafe { ((*hooks().memfuncs).rwmalloc)(sz, hint) };
    if r.is_null() && sz != 0 {
        out_of_memory();
    }
    r
}
unsafe extern "C" fn mustrealloc_h(p: *mut c_void, sz: usize, hint: u32) -> *mut c_void {
    let r = unsafe { ((*hooks().memfuncs).rwrealloc)(p, sz, hint) };
    if r.is_null() && sz != 0 {
        out_of_memory();
    }
    r
}

/// librw's defaultMemfuncs.
pub const DEFAULT_MEMFUNCS: MemoryFunctions = MemoryFunctions { rwmalloc: malloc_h, rwrealloc: realloc_h, rwfree: free_h, rwmustmalloc: None, rwmustrealloc: None };

fn memfuncs() -> &'static MemoryFunctions {
    unsafe { &*hooks().memfuncs }
}
unsafe fn rw_new(sz: usize) -> *mut c_void {
    const MEMDUR_GLOBAL: u32 = 0x40000;
    unsafe { (memfuncs().rwmustmalloc.expect("memfuncs"))(sz, MEMDUR_GLOBAL) }
}

// ---------------------------------------------------------------- the null driver's raster functions

// librw asserts in these (no raster support without a driver); release builds return nothing.
unsafe extern "C" fn null_ptr1(_: *mut c_void) -> *mut c_void {
    eprintln!("[RW] raster function not implemented");
    null_mut()
}
unsafe extern "C" fn null_ptr2(_: *mut c_void, _: i32) -> *mut c_void {
    eprintln!("[RW] raster function not implemented");
    null_mut()
}
unsafe extern "C" fn null_ptr3(_: *mut c_void, _: i32, _: i32) -> *mut c_void {
    eprintln!("[RW] raster function not implemented");
    null_mut()
}
unsafe extern "C" fn null_find_format(_: *mut c_void, _: i32, _: *mut i32, _: *mut i32, _: *mut i32, _: *mut i32) -> i32 {
    eprintln!("[RW] imageFindRasterFormat not implemented");
    0
}
unsafe extern "C" fn null_from_image(_: *mut c_void, _: *mut c_void) -> i32 {
    eprintln!("[RW] rasterFromImage not implemented");
    0
}

// ---------------------------------------------------------------- the state machine

/// rw::Engine::State
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum State {
    Dead = 0,
    Initialized,
    Opened,
    Started,
}
static mut STATE: State = State::Dead;
fn state() -> State {
    unsafe { *addr_of!(STATE) }
}
fn set_state(s: State) {
    unsafe { *addr_of_mut!(STATE) = s };
}
fn engine() -> *mut Engine {
    unsafe { *hooks().engine }
}
fn driver_list(i: usize) -> *mut PluginList {
    unsafe { hooks().driver_plugins.add(i) }
}

unsafe fn system(req: i32, arg: *mut c_void, n: i32) -> i32 {
    unsafe { ((*engine()).device.system)(req, arg, n) }
}

/// Engine::init: registers the modules and plugins.
///
/// # Safety
/// `mf` null or valid.
pub unsafe fn init(mf: *const MemoryFunctions) -> bool {
    unsafe {
        if !engine().is_null() || state() != State::Dead {
            rw_error(0, ERR_ENGINEINIT);
            return false;
        }
        let m = &mut *hooks().memfuncs;
        *m = if mf.is_null() { DEFAULT_MEMFUNCS } else { *mf };
        if m.rwmustmalloc.is_none() {
            m.rwmustmalloc = Some(mustmalloc_h);
        }
        if m.rwmustrealloc.is_none() {
            m.rwmustrealloc = Some(mustrealloc_h);
        }

        rwplugin::pl_open();
        for i in 0..NUM_PLATFORMS {
            let l = &mut *driver_list(i);
            l.size = size_of::<Driver>() as i32;
            l.default_size = l.size;
            crate::rwframe::list_init(&mut l.plugins);
        }
        (hooks().register_modules)();
        set_state(State::Initialized);
        true
    }
}

/// Engine::open: allocates the engine and opens the device.
///
/// # Safety
/// After init; `p` as the device expects (EngineOpenParams).
pub unsafe fn open(p: *mut c_void) -> bool {
    unsafe {
        if !engine().is_null() || state() != State::Initialized {
            rw_error(0, ERR_ENGINEOPEN);
            return false;
        }
        let e = rw_new((*hooks().engine_plugins).size as usize) as *mut Engine;
        *hooks().engine = e;
        (*e).current_camera = null_mut();
        (*e).current_world = null_mut();
        std::ptr::addr_of_mut!((*e).device).write(rwdevice::RENDER_DEVICE);
        system(DEVICEOPEN, p, 0);

        (*e).dummy_default_pipeline = (hooks().pipeline_create)();
        for i in 0..NUM_PLATFORMS {
            let d = rw_new((*driver_list(i)).size as usize) as *mut Driver;
            (*e).driver[i] = d;
            (*d).default_pipeline = (*e).dummy_default_pipeline;
            (*d).raster_create = null_ptr1 as *const c_void;
            (*d).raster_lock = null_ptr3 as *const c_void;
            (*d).raster_unlock = null_ptr2 as *const c_void;
            (*d).raster_lock_palette = null_ptr2 as *const c_void;
            (*d).raster_unlock_palette = null_ptr1 as *const c_void;
            (*d).raster_num_levels = null_ptr1 as *const c_void;
            (*d).image_find_raster_format = null_find_format as *const c_void;
            (*d).raster_from_image = null_from_image as *const c_void;
            (*d).raster_to_image = null_ptr1 as *const c_void;
        }
        set_state(State::Opened);
        true
    }
}

/// Engine::start: creates the rendering device and runs the engine plugin constructors.
///
/// # Safety
/// After open.
pub unsafe fn start() -> bool {
    unsafe {
        let e = engine();
        if e.is_null() || state() != State::Opened {
            rw_error(0, ERR_ENGINESTART);
            return false;
        }
        system(DEVICEINIT, null_mut(), 0);
        rwplugin::pl_construct(hooks().engine_plugins, e as *mut c_void);
        for i in 0..NUM_PLATFORMS {
            rwplugin::pl_construct(driver_list(i), (*e).driver[i] as *mut c_void);
        }
        system(DEVICEFINALIZE, null_mut(), 0);
        (hooks().register_image_formats)();
        set_state(State::Started);
        true
    }
}

/// Engine::term
///
/// # Safety
/// After close.
pub unsafe fn term() {
    unsafe {
        if !engine().is_null() || state() != State::Initialized {
            rw_error(0, ERR_GENERAL);
            return;
        }
        rwplugin::pl_close();
        set_state(State::Dead);
    }
}

/// Engine::close
///
/// # Safety
/// After stop.
pub unsafe fn close() {
    unsafe {
        let e = engine();
        if e.is_null() || state() != State::Opened {
            rw_error(0, ERR_GENERAL);
            return;
        }
        system(DEVICECLOSE, null_mut(), 0);
        for i in 0..NUM_PLATFORMS {
            (memfuncs().rwfree)((*e).driver[i] as *mut c_void);
        }
        (hooks().pipeline_destroy)((*e).dummy_default_pipeline);
        (memfuncs().rwfree)(e as *mut c_void);
        *hooks().engine = null_mut();
        set_state(State::Initialized);
    }
}

/// Engine::stop
///
/// # Safety
/// After start.
pub unsafe fn stop() {
    unsafe {
        let e = engine();
        if e.is_null() || state() != State::Started {
            rw_error(0, ERR_GENERAL);
            return;
        }
        for i in 0..NUM_PLATFORMS {
            rwplugin::pl_destruct(driver_list(i), (*e).driver[i] as *mut c_void);
        }
        rwplugin::pl_destruct(hooks().engine_plugins, e as *mut c_void);
        system(DEVICETERM, null_mut(), 0);
        set_state(State::Opened);
    }
}

const DEVICEINIT: i32 = 0;
const DEVICETERM: i32 = 1;
const DEVICEOPEN: i32 = 2;
const DEVICECLOSE: i32 = 3;
const DEVICEFINALIZE: i32 = 4;

// ---------------------------------------------------------------- C ABI (engine.cpp forwards)

/// # Safety
/// As init.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_init(mf: *const MemoryFunctions) -> i32 {
    unsafe { init(mf) as i32 }
}
/// # Safety
/// As open.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_open(p: *mut c_void) -> i32 {
    unsafe { open(p) as i32 }
}
/// # Safety
/// As start.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_start() -> i32 {
    unsafe { start() as i32 }
}
/// # Safety
/// As stop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_stop() {
    unsafe { stop() }
}
/// # Safety
/// As close.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_close() {
    unsafe { close() }
}
/// # Safety
/// As term.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_term() {
    unsafe { term() }
}
/// The device's system request (subsystems, video modes, multisampling).
///
/// # Safety
/// The engine is open.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_engine_system(req: i32, arg: *mut c_void, n: i32) -> i32 {
    unsafe { system(req, arg, n) }
}
/// librw's defaultMemfuncs, for the C++ symbol.
///
/// # Safety
/// `out` points at a MemoryFunctions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_default_memfuncs(out: *mut MemoryFunctions) {
    unsafe { out.write(DEFAULT_MEMFUNCS) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn error_table_matches_base_err() {
        let f = super::error_formats();
        assert_eq!(f[0].to_str().unwrap(), "No error");
        assert_eq!(f[1].to_str().unwrap(), "Error: %s");
        assert_eq!(f[7].to_str().unwrap(), "Engine could not be initialized");
        assert_eq!(f[9].to_str().unwrap(), "Engine could not be started");
    }
}

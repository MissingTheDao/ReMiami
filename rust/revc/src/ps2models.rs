//! The consoles' own models in their UI styles (roadmap R4, "exact PS2 models", "exact Xbox
//! models"): an import converts the disc's map and cutscene models to ordinary geometry
//! (revc_import::ps2geom, xboxgeom) into `<import cache>/<ps2|xbox>/models-world/`, and
//! CStreaming::ConvertBufferToObject asks here for each model it streams. The PS2 versions carry
//! the PS2's own vertex lighting; the Xbox's were partly rebuilt and re-lit, with the second
//! coordinate set of the world lightmaps (XboxLightmaps.cpp attaches the lightmaps to them).

use std::ffi::{CStr, c_char, c_int};
use std::sync::atomic::{AtomicBool, Ordering};

/// [Graphics] Ps2WorldModels and XboxWorldModels (default on), set when the settings load.
static ON: AtomicBool = AtomicBool::new(true);
static XBOX_ON: AtomicBool = AtomicBool::new(true);

#[unsafe(no_mangle)]
pub extern "C" fn revc_ps2_models_set(on: c_int) {
    ON.store(on != 0, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_xbox_models_set(on: c_int) {
    XBOX_ON.store(on != 0, Ordering::Relaxed);
}

/// Whether the console style `style` (1 PS2, 2 Xbox) draws the console's own world models (its
/// option on); the map data per style follows it (mapstyle.rs).
pub(crate) fn world_models_on(style: i32) -> bool {
    match style {
        1 => ON.load(Ordering::Relaxed),
        2 => XBOX_ON.load(Ordering::Relaxed),
        _ => false,
    }
}

/// The converted console model for `name` (a model name, no extension) while the PS2 or Xbox
/// style and its option are on: its bytes (freed with revc_ps2_model_free) and their length; null otherwise.
///
/// # Safety
/// `name` a NUL-terminated string, `len` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_ps2_model(name: *const c_char, len: *mut usize) -> *mut u8 {
    let console = match crate::texmods::world_style() {
        1 if ON.load(Ordering::Relaxed) => "ps2",
        2 if XBOX_ON.load(Ordering::Relaxed) => "xbox",
        _ => return std::ptr::null_mut(),
    };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_ascii_lowercase();
    let path = crate::discimport::cache_dir().join(console).join("models-world").join(format!("{name}.dff"));
    crate::discimport::note_access(&path);
    match std::fs::read(&path) {
        Ok(d) => {
            let b = d.into_boxed_slice();
            unsafe { *len = b.len() };
            Box::into_raw(b) as *mut u8
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `p`/`len` from revc_ps2_model.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_ps2_model_free(p: *mut u8, len: usize) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len)) });
    }
}

/// The console's shared wheel model (`<import>/<console>/models-world/wheels.dff`) while the world
/// is in the PS2 or Xbox style with that console's world models on: 1 with its path in `out`
/// (NUL-terminated, `cap` bytes), else 0 and the PC's MODELS\GENERIC\WHEELS.DFF loads.
///
/// # Safety
/// `out` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_console_wheels(out: *mut c_char, cap: usize) -> c_int {
    unsafe { console_file("models-world/wheels.dff", out, cap) }
}

/// The console's `default.ide` (`<import>/<console>/maps/default.ide`: the PS2's four weapons drawn
/// to 50 m, the Xbox's ten wheels to 100 m) under the same conditions as revc_console_wheels: 1 with
/// its path in `out`, else 0 and the PC's DATA\DEFAULT.IDE loads.
///
/// # Safety
/// `out` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_console_default_ide(out: *mut c_char, cap: usize) -> c_int {
    unsafe { console_file("maps/default.ide", out, cap) }
}

/// 1 when the world style's console import may serve ped models too (an import that converts
/// skinned models or leaves them out: revc_import::SKINS_MARKER); 0 keeps the PC's peds.
#[unsafe(no_mangle)]
pub extern "C" fn revc_console_peds_ok() -> c_int {
    let console = match crate::texmods::world_style() {
        1 if ON.load(Ordering::Relaxed) => "ps2",
        2 if XBOX_ON.load(Ordering::Relaxed) => "xbox",
        _ => return 0,
    };
    crate::discimport::cache_dir().join(console).join("models-world").join(revc_import::SKINS_MARKER).is_file() as c_int
}

/// The console's `handling.cfg` (`<import>/<console>/data/handling.cfg`) under the same conditions
/// as revc_console_wheels: 1 with its path in `out`, else 0 and the PC's DATA\HANDLING.CFG loads.
///
/// # Safety
/// `out` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_console_handling(out: *mut c_char, cap: usize) -> c_int {
    unsafe { console_file("data/handling.cfg", out, cap) }
}

/// `rel` in the import of the world style's console, while its world models are on: 1 with the
/// path in `out`, else 0.
///
/// # Safety
/// `out` holds `cap` bytes.
unsafe fn console_file(rel: &str, out: *mut c_char, cap: usize) -> c_int {
    let console = match crate::texmods::world_style() {
        1 if crate::ps2models::ON.load(std::sync::atomic::Ordering::Relaxed) => "ps2",
        2 if crate::ps2models::XBOX_ON.load(std::sync::atomic::Ordering::Relaxed) => "xbox",
        _ => return 0,
    };
    let path = crate::discimport::cache_dir().join(console).join(rel);
    crate::discimport::note_access(&path);
    if !path.is_file() {
        return 0;
    }
    let s = path.to_string_lossy();
    if out.is_null() || s.len() + 1 > cap {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), out as *mut u8, s.len());
        *out.add(s.len()) = 0;
    }
    1
}

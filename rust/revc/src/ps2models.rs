//! The PS2's own models in the PS2 style (roadmap R4, "exact PS2 models"): a PS2 import converts
//! the disc's map and cutscene models to ordinary geometry (revc_import::ps2geom) into
//! `<import cache>/ps2/models-world/`, and CStreaming::ConvertBufferToObject asks here for each
//! model it streams. The PS2 versions carry the PS2's own vertex lighting (shadows baked into the
//! prelit colours differ from the PC's).

use std::ffi::{CStr, c_char, c_int};
use std::sync::atomic::{AtomicBool, Ordering};

/// [Graphics] Ps2WorldModels (default on), set when the settings load.
static ON: AtomicBool = AtomicBool::new(true);

#[unsafe(no_mangle)]
pub extern "C" fn revc_ps2_models_set(on: c_int) {
    ON.store(on != 0, Ordering::Relaxed);
}

/// The converted PS2 model for `name` (a model name, no extension) while the PS2 style and the
/// option are on: its bytes (freed with revc_ps2_model_free) and their length; null otherwise.
///
/// # Safety
/// `name` a NUL-terminated string, `len` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_ps2_model(name: *const c_char, len: *mut usize) -> *mut u8 {
    if crate::texmods::style() != 1 || !ON.load(Ordering::Relaxed) {
        return std::ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_ascii_lowercase();
    let path = crate::discimport::cache_dir().join("ps2").join("models-world").join(format!("{name}.dff"));
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

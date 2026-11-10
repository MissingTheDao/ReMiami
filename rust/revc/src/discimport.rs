//! Asking the player for their own console disc images and importing assets from them
//! (goal "Asset import from original discs"; revc-import does the reading). The Options menu entry
//! CONSOLE DISCS shows what is imported and opens a file dialog; the import runs on its own thread
//! and leaves its files in `userfiles/import` (the game folder is the working directory).

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use revc_import::{Disc, DiscKind};
use sdl3_sys::everything::*;

struct State {
    busy: bool,
    /// The last import's result, for the menu.
    message: String,
}

static STATE: Mutex<State> = Mutex::new(State { busy: false, message: String::new() });

/// The dialog's filters; SDL reads them until the callback runs, so they live in a static.
struct Filters([SDL_DialogFileFilter; 2]);
// SAFETY: pointers to static C strings, never written.
unsafe impl Sync for Filters {}
static FILTERS: Filters = Filters([
    SDL_DialogFileFilter { name: c"Disc images (Xbox, PS2)".as_ptr(), pattern: c"iso;xiso".as_ptr() },
    SDL_DialogFileFilter { name: c"All files".as_ptr(), pattern: c"*".as_ptr() },
]);

/// `userfiles/import`, or REVC_IMPORT_DIR (tests point it at an import made beforehand).
pub fn cache_dir() -> PathBuf {
    std::env::var_os("REVC_IMPORT_DIR").map_or_else(|| PathBuf::from("userfiles/import"), PathBuf::from)
}

/// Imports from the image at `path` into the cache; Ok(summary) or Err(reason).
pub fn import_image(path: &Path) -> Result<String, String> {
    let mut disc = Disc::open(path)?;
    let report = revc_import::import(&mut disc, path, &cache_dir())?;
    let what = match disc.kind {
        DiscKind::Xbox => "Xbox",
        DiscKind::Ps2 => "PS2",
    };
    Ok(format!("{what}: {} files imported", report.written.len()))
}

fn set_message(msg: String) {
    eprintln!("revc: disc import: {msg}");
    let mut s = STATE.lock().unwrap();
    s.message = msg;
    s.busy = false;
}

unsafe extern "C" fn on_file_chosen(_: *mut c_void, filelist: *const *const c_char, _: c_int) {
    // NULL: error; empty list: cancelled.
    let first = if filelist.is_null() { std::ptr::null() } else { unsafe { *filelist } };
    if first.is_null() {
        STATE.lock().unwrap().busy = false;
        return;
    }
    let path = PathBuf::from(unsafe { CStr::from_ptr(first) }.to_string_lossy().into_owned());
    std::thread::spawn(move || {
        let msg = match import_image(&path) {
            Ok(m) => m,
            Err(e) => format!("failed: {e}"),
        };
        set_message(msg);
    });
}

/// 1 if a complete import from this kind of disc (0 Xbox, 1 PS2) is in the cache.
#[unsafe(no_mangle)]
pub extern "C" fn revc_import_status(kind: c_int) -> c_int {
    let kind = if kind == 1 { DiscKind::Ps2 } else { DiscKind::Xbox };
    revc_import::imported(&cache_dir(), kind) as c_int
}

/// 1 while a file dialog or an import is running.
#[unsafe(no_mangle)]
pub extern "C" fn revc_import_busy() -> c_int {
    STATE.lock().unwrap().busy as c_int
}

/// Opens the file dialog for a disc image (Xbox or PS2 .iso); the import starts when one is chosen.
#[unsafe(no_mangle)]
pub extern "C" fn revc_import_prompt() {
    // Regression tests never open a dialog (nobody would answer it).
    if std::env::var_os("REVC_TEST").is_some() {
        return;
    }
    {
        let mut s = STATE.lock().unwrap();
        if s.busy {
            return;
        }
        s.busy = true;
    }
    unsafe { SDL_ShowOpenFileDialog(Some(on_file_chosen), std::ptr::null_mut(), std::ptr::null_mut(), FILTERS.0.as_ptr(), 2, std::ptr::null(), false) };
}

/// The last import's result message (empty if none).
#[unsafe(no_mangle)]
pub extern "C" fn revc_import_message(buf: *mut c_char, size: c_int) {
    if buf.is_null() || size <= 0 {
        return;
    }
    let s = STATE.lock().unwrap();
    let bytes = s.message.as_bytes();
    let n = bytes.len().min(size as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast(), buf, n);
        *buf.add(n) = 0;
    }
}

/// Decodes an imported image (`rel` inside the cache, e.g. "xbox/ui/r3_menu_336.dds") to RGBA8;
/// free the pixels with revc_texmod_free. 0 if it isn't there or can't be decoded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_import_image(rel: *const c_char, width: *mut c_int, height: *mut c_int, rgba: *mut *mut u8) -> c_int {
    if rel.is_null() || width.is_null() || height.is_null() || rgba.is_null() {
        return 0;
    }
    let rel = unsafe { CStr::from_ptr(rel) }.to_string_lossy().into_owned();
    match crate::texmods::decode(&cache_dir().join(rel)) {
        Ok((w, h, px)) => {
            unsafe {
                *width = w as c_int;
                *height = h as c_int;
                *rgba = Box::into_raw(px.into_boxed_slice()) as *mut u8;
            }
            1
        }
        Err(_) => 0,
    }
}

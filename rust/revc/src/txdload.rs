//! Texture natives read in Rust (plan: docs/RENDERER_PORT.md, stage 2 step 1a).
//!
//! The game's TXD reader (cpp/re3-miami/src/rw/TexRead.cpp, `RwTextureGtaStreamRead`) hands each
//! TEXTURENATIVE chunk body here instead of to librw's D3D8 reader. Rust parses it
//! (revc_formats::txd) and returns what the WGPU raster is built from: DXT blocks as stored, or
//! every level as RGBA8 (top row first, as librw's D3D `toImage` + `unpalettize` produce them).
//! The C++ side builds the raster the way `Raster::convertTexToCurrentPlatform` did.

use std::ffi::c_char;

use revc_formats::txd;

/// Mirrors `RevcNativeTex` in TexRead.cpp.
#[repr(C)]
pub struct NativeTex {
    pub name: [c_char; 32],
    pub mask: [c_char; 32],
    pub filter_addressing: u32,
    /// Raster format bits as stored (pixel format, PAL4/PAL8, MIPMAP, AUTOMIPMAP).
    pub format: i32,
    pub raster_type: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    /// Levels in `data`.
    pub num_levels: i32,
    /// 0, or 1/3/5: `data` holds DXT blocks (each level's size from its dimensions).
    pub dxt: i32,
    pub has_alpha: i32,
    pub data: *mut u8,
    pub size: usize,
}

fn copy_name(dst: &mut [c_char; 32], s: &str) {
    for (d, b) in dst.iter_mut().zip(s.bytes().take(31)) {
        *d = b as c_char;
    }
}

/// The DXT kinds `d3d_to_wgpu` keeps compressed; DXT2/4 are decoded like other formats.
fn dxt_kind(compression: u8) -> Option<i32> {
    match compression {
        1 | 3 | 5 => Some(compression as i32),
        _ => None,
    }
}

pub fn convert(body: &[u8]) -> Result<(NativeTex, Vec<u8>), String> {
    let tex = txd::parse_native_body(body).map_err(|e| e.to_string())?;
    let mut out = NativeTex {
        name: [0; 32],
        mask: [0; 32],
        filter_addressing: tex.filter_addressing,
        format: tex.raster_format as i32,
        raster_type: tex.raster_type as i32,
        width: tex.width as i32,
        height: tex.height as i32,
        depth: tex.depth as i32,
        num_levels: tex.levels.len() as i32,
        dxt: 0,
        has_alpha: tex.has_alpha as i32,
        data: std::ptr::null_mut(),
        size: 0,
    };
    copy_name(&mut out.name, &tex.name);
    copy_name(&mut out.mask, &tex.mask);
    let mut data = Vec::new();
    if let Some(kind) = dxt_kind(tex.compression) {
        out.dxt = kind;
        for l in &tex.levels {
            data.extend_from_slice(l);
        }
    } else {
        for i in 0..tex.levels.len() {
            data.extend(tex.rgba(i).map_err(|e| e.to_string())?);
        }
    }
    Ok((out, data))
}

/// # Safety
/// `body` must point to `len` readable bytes and `out` to a writable NativeTex. On success
/// (returns 1) `out.data` must be freed with revc_txd_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_txd_read_native(body: *const u8, len: usize, out: *mut NativeTex) -> i32 {
    let body = unsafe { std::slice::from_raw_parts(body, len) };
    match convert(body) {
        Ok((mut t, data)) => {
            let data = data.into_boxed_slice();
            t.size = data.len();
            t.data = Box::into_raw(data) as *mut u8;
            unsafe { out.write(t) };
            1
        }
        Err(e) => {
            eprintln!("revc: texture native: {e}");
            0
        }
    }
}

/// # Safety
/// `tex` must have been filled by revc_txd_read_native, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_txd_free(tex: *mut NativeTex) {
    let t = unsafe { &mut *tex };
    if !t.data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(t.data, t.size)) });
        t.data = std::ptr::null_mut();
        t.size = 0;
    }
}

/// Texture hashes reported by the C++ reader while REVC_TXD_GOLDEN is set (rw/TexRead.cpp
/// `TxdHash`), as "name hash" lines like tests/golden/textures.txt.
static GOLDEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// # Safety
/// `name` must be a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_txd_golden_line(name: *const c_char, hash: u64) {
    let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy();
    GOLDEN.lock().unwrap().push(format!("{name} {hash:016x}"));
}

/// Compares the reported textures with the golden file (as sets of lines: the same texture may be
/// loaded more than once). Returns (distinct textures seen, lines only in the file, lines only
/// seen).
pub fn golden_check(path: &std::path::Path) -> Result<(usize, Vec<String>, Vec<String>), String> {
    use std::collections::BTreeSet;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let want: BTreeSet<&str> = text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let seen = GOLDEN.lock().unwrap();
    let got: BTreeSet<&str> = seen.iter().map(|s| s.as_str()).collect();
    let missing = want.difference(&got).map(|s| s.to_string()).collect();
    let extra = got.difference(&want).map(|s| s.to_string()).collect();
    Ok((got.len(), missing, extra))
}

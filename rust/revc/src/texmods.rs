//! Texture mods: loose image files that replace textures when their dictionary loads.
//!
//! Layout under the game folder (names are case-insensitive):
//!   mods/textures/<txd>/<texture>.png|.dds   replaces <texture> in that dictionary only
//!   mods/textures/<texture>.png|.dds         replaces <texture> in any dictionary
//! `<txd>` is the dictionary's name: the model's TXD in gta3.img (e.g. `infernus`), or the file
//! name of a loose TXD (`hud`, `fonts`, `generic`, `wheels` ...). Deeper folders are allowed for
//! grouping; the folder directly containing the file names the dictionary.
//! Any size is accepted (non-square and larger than the original included). A file that can't
//! be read or decoded is reported once and the original texture is kept.
//!
//! The C++ side (skel/host/texmods.cpp) asks for each texture of a freshly loaded dictionary.

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_uchar};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_SIZE: u32 = 8192;

static INDEX: OnceLock<HashMap<(String, String), PathBuf>> = OnceLock::new();
static APPLIED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static COMPRESSED: AtomicUsize = AtomicUsize::new(0);
/// Files already reported as bad (each is reported once, though it may be asked for again when
/// its dictionary is streamed in again).
static REPORTED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn mods_dir() -> PathBuf {
    Path::new("mods").join("textures")
}

fn build_index(root: &Path) -> HashMap<(String, String), PathBuf> {
    let mut index = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if ext != "png" && ext != "dds" {
                continue;
            }
            let tex = p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
            let txd = if dir == root {
                String::new()
            } else {
                dir.file_name().unwrap().to_string_lossy().to_ascii_lowercase()
            };
            // Prefer PNG if both exist; otherwise first found.
            let key = (txd, tex);
            if ext == "png" || !index.contains_key(&key) {
                index.insert(key, p);
            }
        }
    }
    index
}

fn index() -> &'static HashMap<(String, String), PathBuf> {
    INDEX.get_or_init(|| {
        let idx = build_index(&mods_dir());
        if !idx.is_empty() {
            eprintln!("texmods: {} texture overrides in {}", idx.len(), mods_dir().display());
        }
        idx
    })
}

/// (textures replaced, files rejected) since start.
pub fn stats() -> (usize, usize) {
    (APPLIED.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed))
}

/// Replacements served as DXT blocks (BC textures on the GPU) rather than decoded to RGBA8.
pub fn compressed() -> usize {
    COMPRESSED.load(Ordering::Relaxed)
}

/// A DXT1/3/5 DDS that can be uploaded as it is: size a multiple of 4 (BC textures are whole
/// blocks), and mip levels in the file when the texture is sampled with mips (otherwise the
/// RGBA8 path keeps it mipmapped by generating them).
fn dds_blocks(path: &Path, want_mips: bool) -> Option<(u32, u32, u8, bool, Vec<Vec<u8>>)> {
    let data = std::fs::read(path).ok()?;
    if !data.starts_with(b"DDS ") {
        return None;
    }
    let b = revc_formats::dds::blocks(&data).ok()??;
    if b.width % 4 != 0 || b.height % 4 != 0 || b.width > MAX_SIZE || b.height > MAX_SIZE {
        return None;
    }
    if want_mips && b.levels.len() < 2 {
        return None;
    }
    let levels = if want_mips { b.levels.iter().map(|l| l.to_vec()).collect() } else { vec![b.levels[0].to_vec()] };
    Some((b.width, b.height, b.kind, b.has_alpha, levels))
}

/// RGBA8, top row first.
pub(crate) fn decode(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let (w, h, rgba) = if data.starts_with(b"DDS ") {
        let img = revc_formats::dds::decode(&data).map_err(|e| e.to_string())?;
        (img.width, img.height, img.rgba)
    } else {
        decode_png(&data)?
    };
    if w == 0 || h == 0 || w > MAX_SIZE || h > MAX_SIZE {
        return Err(format!("size {w}x{h} outside 1..={MAX_SIZE}"));
    }
    Ok((w, h, rgba))
}

fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    if rgba.len() != (w * h * 4) as usize {
        return Err(format!("unexpected PNG layout ({:?}, {} bytes)", info.color_type, px.len()));
    }
    Ok((w, h, rgba))
}

fn lookup(txd: &str, tex: &str) -> Option<&'static PathBuf> {
    let idx = index();
    idx.get(&(txd.to_ascii_lowercase(), tex.to_ascii_lowercase()))
        .or_else(|| idx.get(&(String::new(), tex.to_ascii_lowercase())))
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings; the out pointers must be valid.
/// If the override for txd/texture is a DXT DDS that can stay compressed, returns 1 with its
/// blocks: `levels` mip levels one after the other (largest first, each whole 4x4 blocks), `size`
/// bytes in all, freed with revc_texmod_free_blocks. `want_mips`: the texture samples mip levels.
/// Returns 0 otherwise (then revc_texmod_find decodes it to RGBA8).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_find_dxt(
    txd: *const c_char,
    texture: *const c_char,
    want_mips: c_int,
    width: *mut c_int,
    height: *mut c_int,
    kind: *mut c_int,
    levels: *mut c_int,
    has_alpha: *mut c_int,
    blocks: *mut *mut c_uchar,
    size: *mut usize,
) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    let Some(path) = lookup(&txd, &tex) else { return 0 };
    let Some((w, h, k, alpha, lv)) = dds_blocks(path, want_mips != 0) else { return 0 };
    let n = lv.len();
    let data = lv.concat().into_boxed_slice();
    unsafe {
        *width = w as c_int;
        *height = h as c_int;
        *kind = k as c_int;
        *levels = n as c_int;
        *has_alpha = alpha as c_int;
        *size = data.len();
        *blocks = Box::into_raw(data) as *mut c_uchar;
    }
    APPLIED.fetch_add(1, Ordering::Relaxed);
    COMPRESSED.fetch_add(1, Ordering::Relaxed);
    1
}

/// # Safety
/// `blocks` and `size` must come from revc_texmod_find_dxt, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_free_blocks(blocks: *mut c_uchar, size: usize) {
    if !blocks.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(blocks, size)) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_texmod_active() -> c_int {
    !index().is_empty() as c_int
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings; the out pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_find(
    txd: *const c_char,
    texture: *const c_char,
    width: *mut c_int,
    height: *mut c_int,
    rgba: *mut *mut c_uchar,
) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    let Some(path) = lookup(&txd, &tex) else { return 0 };
    match decode(path) {
        Ok((w, h, px)) => {
            let boxed = px.into_boxed_slice();
            unsafe {
                *width = w as c_int;
                *height = h as c_int;
                *rgba = Box::into_raw(boxed) as *mut c_uchar;
            }
            APPLIED.fetch_add(1, Ordering::Relaxed);
            1
        }
        Err(e) => {
            let mut reported = REPORTED.lock().unwrap();
            if !reported.contains(path) {
                eprintln!("texmods: {}: {e}; keeping the original {txd}/{tex}", path.display());
                reported.push(path.clone());
                FAILED.fetch_add(1, Ordering::Relaxed);
            }
            0
        }
    }
}

/// # Safety
/// `rgba` must come from `revc_texmod_find` with the same width and height.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_free(rgba: *mut c_uchar, width: c_int, height: c_int) {
    if rgba.is_null() {
        return;
    }
    let len = width as usize * height as usize * 4;
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(rgba, len)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_global_and_per_txd() {
        let root = std::env::temp_dir().join(format!("revc-texmods-{}", std::process::id()));
        std::fs::create_dir_all(root.join("HUD")).unwrap();
        std::fs::create_dir_all(root.join("group/infernus")).unwrap();
        for f in ["HUD/Fist.PNG", "radar.dds", "group/infernus/body.png", "notes.txt"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        let idx = build_index(&root);
        assert!(idx.contains_key(&("hud".into(), "fist".into())));
        assert!(idx.contains_key(&("".into(), "radar".into())));
        assert!(idx.contains_key(&("infernus".into(), "body".into())));
        assert_eq!(idx.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn decodes_png_and_rejects_garbage() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 2, 1);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
        }
        assert_eq!(decode_png(&out).unwrap(), (2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]));
        assert!(decode_png(b"not a png").is_err());
    }
}

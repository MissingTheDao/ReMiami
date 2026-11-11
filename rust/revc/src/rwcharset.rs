//! librw's debug font (stage 2 step 7c: charset.cpp, RtCharset): the 256-glyph VGA font as a
//! paletted raster in two colours, and text batched into Im2D quads (up to 100 characters per
//! draw, flushed when the raster changes or the batch is full) drawn with nearest filtering,
//! no culling and no depth test. Used by the debug menu and debug text.
//!
//! Checked against tests/golden/rwcharset.txt, recorded from the original charset.cpp
//! (tests/golden/recorders/rwcharset).

use crate::rwraster::Raster;
use std::ffi::{c_char, c_void};
use std::ptr::{addr_of, addr_of_mut, null_mut};
use std::sync::OnceLock;

/// rw::Charset::Desc
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Desc {
    /// number of characters
    pub count: i32,
    /// characters per row and column of the raster
    pub tile_width: i32,
    pub tile_height: i32,
    /// character cell
    pub width: i32,
    pub height: i32,
    /// cell including the border
    pub width_internal: i32,
    pub height_internal: i32,
}

/// rw::Charset
#[repr(C)]
pub struct Charset {
    pub desc: Desc,
    pub raster: *mut Raster,
}

/// rw::wgpu::Im2DVertex
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Im2DVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
    pub color: [u8; 4],
    pub u: f32,
    pub v: f32,
}
const _: () = assert!(size_of::<Im2DVertex>() == 28 && size_of::<Charset>() == 40);

/// What is outside: rasters (built from the font through librw's Image), render states, Im2D.
#[repr(C)]
pub struct Hooks {
    /// Image 256x256x8 over `pixels` (stride 256) with palette entries 0 and 1 from `palette`
    /// (8 bytes, background then foreground), then Raster::createFromImage; null on failure
    pub raster_from_font: unsafe extern "C" fn(pixels: *const u8, palette: *const u8) -> *mut Raster,
    pub raster_destroy: unsafe extern "C" fn(*mut Raster),
    pub set_render_state: unsafe extern "C" fn(state: i32, value: u32),
    pub set_render_state_ptr: unsafe extern "C" fn(state: i32, value: *mut c_void),
    pub get_render_state: unsafe extern "C" fn(state: i32) -> u32,
    /// im2d::GetNearZ()
    pub near_z: unsafe extern "C" fn() -> f32,
    /// engine->currentCamera->nearPlane
    pub near_plane: unsafe extern "C" fn() -> f32,
    /// im2d::RenderIndexedPrimitive(PRIMTYPETRILIST, ...)
    pub render_indexed: unsafe extern "C" fn(prim: u32, vertices: *mut Im2DVertex, num_vertices: i32, indices: *mut u16, num_indices: i32),
}
static mut HOOKS: Option<Hooks> = None;
fn hooks() -> &'static Hooks {
    unsafe { (*addr_of!(HOOKS)).as_ref().expect("charset hooks") }
}

/// # Safety
/// Called before the charset is used.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_set_hooks(h: Hooks) {
    unsafe { *addr_of_mut!(HOOKS) = Some(h) };
}

/// The VGA font: 256x256 bytes, 0 background, 1 foreground (librw's vgafont.inc).
pub fn font_bits() -> &'static [u8] {
    static F: OnceLock<Vec<u8>> = OnceLock::new();
    F.get_or_init(|| {
        let src = include_str!("../../../cpp/re3-miami/vendor/librw/src/vgafont.inc");
        let mut v: Vec<u8> = src.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        v.resize(256 * 256, 0);
        v
    })
}

const NUMCHARS: usize = 100;
const PRIMTYPETRILIST: u32 = 3;
const TEXTURERASTER: i32 = 0;
const TEXTUREADDRESS: i32 = 1;
const TEXTUREFILTER: i32 = 4;
const ZTESTENABLE: i32 = 8;
const CULLMODE: i32 = 12;
const TEX_NEAREST: u32 = 1;
const TEX_WRAP: u32 = 1;
const CULLNONE: u32 = 1;

struct Buffer {
    vertices: Vec<Im2DVertex>,
    indices: Vec<u16>,
    open: bool,
    num_chars: usize,
    last_raster: *mut Raster,
}
static mut BUF: Buffer = Buffer { vertices: Vec::new(), indices: Vec::new(), open: false, num_chars: 0, last_raster: null_mut() };
#[allow(clippy::mut_from_ref)]
fn buf() -> &'static mut Buffer {
    unsafe { &mut *addr_of_mut!(BUF) }
}

/// Charset::open: the batch buffers; false if already open.
pub fn open() -> bool {
    let b = buf();
    if b.open {
        return false;
    }
    b.num_chars = 0;
    b.last_raster = null_mut();
    b.vertices = vec![Im2DVertex::default(); NUMCHARS * 4];
    b.indices = vec![0; NUMCHARS * 6];
    b.open = true;
    true
}

/// Charset::close (what is still batched stays unflushed, as in librw).
pub fn close() {
    let b = buf();
    b.vertices = Vec::new();
    b.indices = Vec::new();
    b.open = false;
}

/// Charset::create
///
/// # Safety
/// The hooks are set.
pub unsafe fn create(fg: [u8; 4], bg: [u8; 4]) -> *mut Charset {
    unsafe {
        let cs = Box::into_raw(Box::new(Charset { desc: Desc::default(), raster: null_mut() }));
        if set_colors(cs, fg, bg).is_null() {
            destroy(cs);
            return null_mut();
        }
        (*cs).desc = Desc { count: 256, tile_width: 28, tile_height: 10, width: 9, height: 16, width_internal: 9, height_internal: 16 };
        cs
    }
}

/// Charset::destroy
///
/// # Safety
/// A charset from create.
pub unsafe fn destroy(cs: *mut Charset) {
    unsafe {
        if !(*cs).raster.is_null() {
            (hooks().raster_destroy)((*cs).raster);
        }
        drop(Box::from_raw(cs));
    }
}

/// Charset::setColors: a new raster in these colours (the old one is destroyed); null if the
/// raster can't be made.
///
/// # Safety
/// A live charset.
pub unsafe fn set_colors(cs: *mut Charset, fg: [u8; 4], bg: [u8; 4]) -> *mut Charset {
    unsafe {
        let pal = [bg[0], bg[1], bg[2], bg[3], fg[0], fg[1], fg[2], fg[3]];
        let r = (hooks().raster_from_font)(font_bits().as_ptr(), pal.as_ptr());
        if r.is_null() {
            return null_mut();
        }
        if !(*cs).raster.is_null() {
            (hooks().raster_destroy)((*cs).raster);
        }
        (*cs).raster = r;
        cs
    }
}

/// Charset::flushBuffer: draws what is batched.
///
/// # Safety
/// The buffers are open.
pub unsafe fn flush_buffer() {
    unsafe {
        let b = buf();
        let h = hooks();
        if b.num_chars != 0 {
            (h.set_render_state_ptr)(TEXTURERASTER, b.last_raster as *mut c_void);
            (h.set_render_state)(TEXTUREADDRESS, TEX_WRAP);
            (h.set_render_state)(TEXTUREFILTER, TEX_NEAREST);
            let cull = (h.get_render_state)(CULLMODE);
            let ztest = (h.get_render_state)(ZTESTENABLE);
            (h.set_render_state)(CULLMODE, CULLNONE);
            (h.set_render_state)(ZTESTENABLE, 0);
            (h.render_indexed)(PRIMTYPETRILIST, b.vertices.as_mut_ptr(), (b.num_chars * 4) as i32, b.indices.as_mut_ptr(), (b.num_chars * 6) as i32);
            (h.set_render_state)(CULLMODE, cull);
            (h.set_render_state)(ZTESTENABLE, ztest);
        }
        b.num_chars = 0;
        b.last_raster = null_mut();
    }
}

unsafe fn print_char(cs: *mut Charset, c: i32, x: i32, y: i32) {
    unsafe {
        let d = (*cs).desc;
        if c >= d.count {
            return;
        }
        let b = buf();
        if (*cs).raster != b.last_raster || b.num_chars >= NUMCHARS {
            flush_buffer();
        }
        let b = buf();
        b.last_raster = (*cs).raster;
        let h = hooks();
        let near = (h.near_plane)();
        let recip_z = 1.0 / near;
        let (rw, rh) = ((*(*cs).raster).width as f32, (*(*cs).raster).height as f32);
        let u = ((c % d.tile_width) * d.width_internal) as f32 / rw;
        let v = ((c / d.tile_width) * d.height_internal) as f32 / rh;
        let du = d.width_internal as f32 / rw;
        let dv = d.height_internal as f32 / rh;
        let z = (h.near_z)();
        // setCameraZ(near) then setRecipCameraZ(recipZ): w = 1/recipZ
        let w = 1.0 / recip_z;
        let corner = |sx: i32, sy: i32, u: f32, v: f32| Im2DVertex { x: sx as f32, y: sy as f32, z, w, color: [255; 4], u, v };
        let n = b.num_chars;
        b.vertices[n * 4] = corner(x, y, u, v);
        b.vertices[n * 4 + 1] = corner(x + d.width_internal, y, u + du, v);
        b.vertices[n * 4 + 2] = corner(x, y + d.height_internal, u, v + dv);
        b.vertices[n * 4 + 3] = corner(x + d.width_internal, y + d.height_internal, u + du, v + dv);
        let base = (n * 4) as u16;
        b.indices[n * 6..n * 6 + 6].copy_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        b.num_chars += 1;
    }
}

/// Charset::printBuffered: batches the string (hideSpaces skips spaces but still advances).
///
/// # Safety
/// A live charset, the buffers open, a current camera.
pub unsafe fn print_buffered(cs: *mut Charset, s: &[u8], mut x: i32, y: i32, hide_spaces: bool) {
    unsafe {
        for &c in s {
            if !hide_spaces || c != b' ' {
                print_char(cs, c as i32, x, y);
            }
            x += (*cs).desc.width;
        }
    }
}

/// Charset::print: batches and draws.
///
/// # Safety
/// As print_buffered.
pub unsafe fn print(cs: *mut Charset, s: &[u8], x: i32, y: i32, hide_spaces: bool) {
    unsafe {
        print_buffered(cs, s, x, y, hide_spaces);
        flush_buffer();
    }
}

// ---------------------------------------------------------------- C ABI (charset.cpp forwards)

unsafe fn cstr<'a>(s: *const c_char) -> &'a [u8] {
    unsafe { std::ffi::CStr::from_ptr(s).to_bytes() }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_charset_open() -> i32 {
    open() as i32
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_charset_close() {
    close()
}
/// # Safety
/// `fg`, `bg` point at RGBA.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_create(fg: *const [u8; 4], bg: *const [u8; 4]) -> *mut Charset {
    unsafe { create(*fg, *bg) }
}
/// # Safety
/// As destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_destroy(cs: *mut Charset) {
    unsafe { destroy(cs) }
}
/// # Safety
/// As set_colors; `fg`, `bg` point at RGBA.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_set_colors(cs: *mut Charset, fg: *const [u8; 4], bg: *const [u8; 4]) -> *mut Charset {
    unsafe { set_colors(cs, *fg, *bg) }
}
/// # Safety
/// As print; `s` is a C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_print(cs: *mut Charset, s: *const c_char, x: i32, y: i32, hide: i32) {
    unsafe { print(cs, cstr(s), x, y, hide != 0) }
}
/// # Safety
/// As print_buffered; `s` is a C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_print_buffered(cs: *mut Charset, s: *const c_char, x: i32, y: i32, hide: i32) {
    unsafe { print_buffered(cs, cstr(s), x, y, hide != 0) }
}
/// # Safety
/// As flush_buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_charset_flush() {
    unsafe { flush_buffer() }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwcharset.txt; the hooks log like the recorder's stubs.
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static RASTERS: RefCell<Vec<*mut Raster>> = const { RefCell::new(Vec::new()) };
        static STATES: RefCell<[u32; 32]> = const { RefCell::new([0; 32]) };
        static NEAR: RefCell<(f32, f32)> = const { RefCell::new((0.9, 0.0)) };
    }
    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn fnv(b: &[u8], mut h: u64) -> u64 {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x100000001b3);
        }
        h
    }
    fn rid(r: *mut Raster) -> i32 {
        if r.is_null() {
            return 0;
        }
        RASTERS.with(|v| v.borrow().iter().position(|&x| x == r).map_or(-1, |i| i as i32 + 1))
    }

    unsafe extern "C" fn raster_from_font(pixels: *const u8, pal: *const u8) -> *mut Raster {
        let r: *mut Raster = Box::into_raw(Box::new(unsafe { std::mem::zeroed() }));
        unsafe {
            (*r).width = 256;
            (*r).height = 256;
        }
        let n = RASTERS.with(|v| {
            v.borrow_mut().push(r);
            v.borrow().len()
        });
        let pal = unsafe { std::slice::from_raw_parts(pal, 8) };
        let px = unsafe { std::slice::from_raw_parts(pixels, 256 * 256) };
        log(format!(" raster{n}:256x256x8:s256:pal{}:{:016x}", pal.iter().map(|b| format!("{b:02x}")).collect::<String>(), fnv(px, 0xcbf29ce484222325)));
        r
    }
    unsafe extern "C" fn raster_destroy(r: *mut Raster) {
        log(format!(" rdestroy{}", rid(r)));
    }
    unsafe extern "C" fn set_rs(s: i32, v: u32) {
        STATES.with(|st| st.borrow_mut()[s as usize] = v);
        log(format!(" rs{s}={v}"));
    }
    unsafe extern "C" fn set_rs_ptr(s: i32, v: *mut c_void) {
        log(format!(" rsp{s}=r{}", rid(v as *mut Raster)));
    }
    unsafe extern "C" fn get_rs(s: i32) -> u32 {
        STATES.with(|st| st.borrow()[s as usize])
    }
    unsafe extern "C" fn near_z() -> f32 {
        NEAR.with(|n| n.borrow().1)
    }
    unsafe extern "C" fn near_plane() -> f32 {
        NEAR.with(|n| n.borrow().0)
    }
    unsafe extern "C" fn render(prim: u32, v: *mut Im2DVertex, nv: i32, ix: *mut u16, ni: i32) {
        let vb = unsafe { std::slice::from_raw_parts(v as *const u8, nv as usize * 28) };
        let ib = unsafe { std::slice::from_raw_parts(ix as *const u8, ni as usize * 2) };
        log(format!(" draw{prim}:{nv}:{ni}:{:016x}:{:016x}", fnv(vb, 0xcbf29ce484222325), fnv(ib, 0xcbf29ce484222325)));
    }

    fn rgba(u: u32) -> [u8; 4] {
        u.to_le_bytes()
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwcharset.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwcharset.txt");
        unsafe {
            revc_charset_set_hooks(Hooks { raster_from_font, raster_destroy, set_render_state: set_rs, set_render_state_ptr: set_rs_ptr, get_render_state: get_rs, near_z, near_plane, render_indexed: render });
        }
        close();
        let mut cs = [null_mut::<Charset>(); 8];
        let mut opened = false;
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let (args, expect) = input.split_once(" = ").map_or((input, None), |(a, r)| (a, Some(r.to_string())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let num = |i: usize| w[i].parse::<i64>().unwrap();
            let res: Option<String> = unsafe {
                match w[0] {
                    "open" => {
                        let r = open();
                        opened = opened || r;
                        Some((r as i32).to_string())
                    }
                    "close" => {
                        close();
                        opened = false;
                        None
                    }
                    "create" => {
                        let k = num(1) as usize;
                        if !cs[k].is_null() {
                            destroy(cs[k]);
                        }
                        cs[k] = create(rgba(num(2) as u32), rgba(num(3) as u32));
                        Some((!cs[k].is_null() as i32).to_string())
                    }
                    "destroy" => {
                        let k = num(1) as usize;
                        if !cs[k].is_null() {
                            destroy(cs[k]);
                            cs[k] = null_mut();
                        }
                        None
                    }
                    "setcolors" => {
                        let k = num(1) as usize;
                        if cs[k].is_null() { Some("-1000000".into()) } else { Some(((set_colors(cs[k], rgba(num(2) as u32), rgba(num(3) as u32)) == cs[k]) as i32).to_string()) }
                    }
                    "near" => {
                        NEAR.with(|x| x.borrow_mut().0 = f32::from_bits(num(1) as u32));
                        None
                    }
                    "nearz" => {
                        NEAR.with(|x| x.borrow_mut().1 = f32::from_bits(num(1) as u32));
                        None
                    }
                    "state" => {
                        STATES.with(|s| s.borrow_mut()[num(1) as usize] = num(2) as u32);
                        None
                    }
                    "flush" => {
                        if opened {
                            flush_buffer();
                        }
                        None
                    }
                    "print" | "printb" => {
                        let (k, x, y, hide) = (num(1) as usize, num(2) as i32, num(3) as i32, num(4) != 0);
                        let s: Vec<u8> = if w[6] == "-" { vec![] } else { (0..w[6].len() / 2).map(|i| u8::from_str_radix(&w[6][i * 2..i * 2 + 2], 16).unwrap()).collect() };
                        if opened && !cs[k].is_null() {
                            if w[0] == "printb" {
                                print_buffered(cs[k], &s, x, y, hide);
                            } else {
                                print(cs[k], &s, x, y, hide);
                            }
                        }
                        None
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            // the recorder prints setcolors without a charset as no result
            let res = res.filter(|r| r != "-1000000");
            if res != expect && bad.len() < 6 {
                bad.push(format!("line {} {input}: result {res:?} want {expect:?}", n + 1));
            }
            let mut h = 0xcbf29ce484222325u64;
            for c in cs {
                let mut v = [0i32; 9];
                if !c.is_null() {
                    let d = unsafe { (*c).desc };
                    v = [1, d.count, d.tile_width, d.tile_height, d.width, d.height, d.width_internal, d.height_internal, rid(unsafe { (*c).raster })];
                }
                h = fnv(&v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>(), h);
            }
            h = fnv(&[opened as u8], h);
            let lg = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            let got = format!(" {h:016x} |{lg}");
            n += 1;
            if got != want && bad.len() < 6 {
                bad.push(format!("line {n} {}\n  want{want}\n  got {got}", &input[..input.len().min(120)]));
            }
        }
        close();
        assert!(n >= 6000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

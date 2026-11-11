//! librw's texture object model (vendor/librw/src/texture.cpp), stage 2 step 5a: textures with
//! reference counts and the global texture list, texture dictionaries (create, destroy, add,
//! addFront, remove, find, the current one) and Texture::read's find, load and dummy logic.
//! Rust allocates textures and dictionaries on librw's layouts; plugin data and rasters stay in
//! C++ (hooks), and the find and read callbacks stay overridable (custompipes sets its own).
//!
//! Checked against tests/golden/rwtexture.txt, recorded from the original texture.cpp
//! (tests/golden/recorders/rwtexture).

use std::ffi::{c_char, c_void};

use crate::rwframe::{LLLink, LinkList, Object, link_remove, list_add, list_init};

/// The object owning `link` (offset of `field` in `T`), as LLLinkGetData.
macro_rules! container_of {
    ($link:expr, $t:ty, $field:ident) => {
        ($link as *mut u8).sub(std::mem::offset_of!($t, $field)) as *mut $t
    };
}

pub const TEXDICTIONARY_ID: u8 = 6;
const WRAP: u32 = 1;
const NEAREST: u32 = 1;

/// rw::Texture (plugin data follows it in memory).
#[repr(C)]
pub struct Texture {
    pub raster: *mut c_void,
    pub dict: *mut TexDictionary,
    pub in_dict: LLLink,
    pub name: [u8; 32],
    pub mask: [u8; 32],
    pub filter_addressing: u32,
    pub ref_count: i32,
    pub in_global_list: LLLink,
}

/// rw::TexDictionary (plugin data follows it in memory).
#[repr(C)]
pub struct TexDictionary {
    pub object: Object,
    pub textures: LinkList,
    pub in_global_list: LLLink,
}

/// texture.cpp's TextureGlobals (the texture module's engine plugin data).
#[repr(C)]
pub struct TextureGlobals {
    pub initial_tex_dict: *mut TexDictionary,
    pub current_tex_dict: *mut TexDictionary,
    pub load_textures: i32,
    pub make_dummies: i32,
    pub mipmapping: i32,
    pub auto_mipmapping: i32,
    pub tex_dicts: LinkList,
    pub textures: LinkList,
}

/// What stays in C++: plugin lists (sizes, constructors, destructors), rasters, the counters.
#[repr(C)]
pub struct Hooks {
    pub tex_size: unsafe extern "C" fn() -> usize,
    pub tex_construct: unsafe extern "C" fn(*mut Texture),
    pub tex_destruct: unsafe extern "C" fn(*mut Texture),
    pub dict_size: unsafe extern "C" fn() -> usize,
    pub dict_construct: unsafe extern "C" fn(*mut TexDictionary),
    pub dict_destruct: unsafe extern "C" fn(*mut TexDictionary),
    /// Raster::destroy
    pub raster_destroy: unsafe extern "C" fn(*mut c_void),
    /// Raster::create(0, 0, 0, Raster::DONTALLOCATE), for dummy textures
    pub raster_create_dummy: unsafe extern "C" fn() -> *mut c_void,
    /// Texture::numAllocated (kind 0) and TexDictionary::numAllocated (kind 1) += delta
    pub allocated: unsafe extern "C" fn(kind: i32, delta: i32),
}

pub type FindCb = unsafe extern "C" fn(name: *const c_char) -> *mut Texture;
pub type ReadCb = unsafe extern "C" fn(name: *const c_char, mask: *const c_char) -> *mut Texture;

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set before use (tests), read-only afterwards; the game uses the defaults.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().unwrap_or(&DEFAULT_HOOKS) }
}

// ---------------------------------------------------------------- the game's hooks, the texture module, the default callbacks, anisotropy (7d2, texture.cpp)

mod defaults {
    use super::*;
    use crate::rwplugin::{self as pl, CLASS_TEXDICTIONARY, CLASS_TEXTURE, class_list};
    pub(super) unsafe extern "C" fn tex_size() -> usize {
        unsafe { (*class_list(CLASS_TEXTURE)).size as usize }
    }
    pub(super) unsafe extern "C" fn tex_construct(t: *mut Texture) {
        unsafe { pl::pl_construct(class_list(CLASS_TEXTURE), t as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn tex_destruct(t: *mut Texture) {
        unsafe { pl::pl_destruct(class_list(CLASS_TEXTURE), t as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn dict_size() -> usize {
        unsafe { (*class_list(CLASS_TEXDICTIONARY)).size as usize }
    }
    pub(super) unsafe extern "C" fn dict_construct(d: *mut TexDictionary) {
        unsafe { pl::pl_construct(class_list(CLASS_TEXDICTIONARY), d as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn dict_destruct(d: *mut TexDictionary) {
        unsafe { pl::pl_destruct(class_list(CLASS_TEXDICTIONARY), d as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn raster_destroy(r: *mut c_void) {
        unsafe { crate::rwraster::destroy(r as *mut crate::rwraster::Raster) }
    }
    pub(super) unsafe extern "C" fn raster_create_dummy() -> *mut c_void {
        unsafe { crate::rwengine::raster_create(0, 0, 0, crate::rwraster::DONTALLOCATE, 0) as *mut c_void }
    }
    pub(super) unsafe extern "C" fn allocated(kind: i32, delta: i32) {
        unsafe { *pl::alloc_count(if kind == 0 { CLASS_TEXTURE } else { CLASS_TEXDICTIONARY }) += delta }
    }
}
static DEFAULT_HOOKS: Hooks = Hooks {
    tex_size: defaults::tex_size,
    tex_construct: defaults::tex_construct,
    tex_destruct: defaults::tex_destruct,
    dict_size: defaults::dict_size,
    dict_construct: defaults::dict_construct,
    dict_destruct: defaults::dict_destruct,
    raster_destroy: defaults::raster_destroy,
    raster_create_dummy: defaults::raster_create_dummy,
    allocated: defaults::allocated,
};

static mut TEXTURE_MODULE_OFFSET: i32 = 0;

/// The running engine's TextureGlobals (TEXGLOBALS).
#[unsafe(no_mangle)]
pub extern "C" fn revc_tex_globals() -> *mut TextureGlobals {
    unsafe { (*std::ptr::addr_of!(crate::rwengine::revc_engine) as *mut u8).add(*std::ptr::addr_of!(TEXTURE_MODULE_OFFSET) as usize) as *mut TextureGlobals }
}

/// textureOpen: the lists, the initial dictionary (current), textures loaded from files.
unsafe extern "C" fn texture_open(o: *mut c_void, off: i32, _: i32) -> *mut c_void {
    unsafe {
        *std::ptr::addr_of_mut!(TEXTURE_MODULE_OFFSET) = off;
        let g = revc_tex_globals();
        crate::rwframe::list_init(&mut (*g).tex_dicts);
        crate::rwframe::list_init(&mut (*g).textures);
        let d = dict_create(g);
        (*g).initial_tex_dict = d;
        (*g).current_tex_dict = d;
        (*g).load_textures = 1;
        (*g).make_dummies = 0;
        (*g).mipmapping = 0;
        (*g).auto_mipmapping = 0;
    }
    o
}

/// textureClose: every dictionary goes, then any texture still around (reported).
unsafe extern "C" fn texture_close(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
    unsafe {
        let g = revc_tex_globals();
        let head = std::ptr::addr_of_mut!((*g).tex_dicts.link);
        let mut l = (*head).next;
        while l != head {
            let next = (*l).next;
            dict_destroy(g, (l as *mut u8).sub(std::mem::offset_of!(TexDictionary, in_global_list)) as *mut TexDictionary);
            l = next;
        }
        (*g).initial_tex_dict = std::ptr::null_mut();
        (*g).current_tex_dict = std::ptr::null_mut();
        let head = std::ptr::addr_of_mut!((*g).textures.link);
        let mut l = (*head).next;
        while l != head {
            let next = (*l).next;
            let t = (l as *mut u8).sub(std::mem::offset_of!(Texture, in_global_list)) as *mut Texture;
            let name = std::ffi::CStr::from_ptr((*t).name.as_ptr() as *const c_char).to_string_lossy();
            let mask = std::ffi::CStr::from_ptr((*t).mask.as_ptr() as *const c_char).to_string_lossy();
            println!("Tex still allocated: {} {name} {mask}", (*t).ref_count);
            tex_destroy(t);
            l = next;
        }
    }
    o
}
const ID_TEXTUREMODULE: u32 = 0x408;

/// Texture::registerModule
///
/// # Safety
/// During Engine::init.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texture_register_module() {
    unsafe {
        crate::rwplugin::pl_register(
            crate::rwplugin::class_list(crate::rwplugin::CLASS_ENGINE),
            size_of::<TextureGlobals>() as i32,
            ID_TEXTUREMODULE,
            Some(texture_open),
            Some(texture_close),
            None,
        );
    }
}

/// Texture::findCB's default: the current dictionary.
///
/// # Safety
/// `name` a C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_tex_default_find_cb(name: *const c_char) -> *mut Texture {
    unsafe { default_find(revc_tex_globals(), name) }
}

/// Texture::readCB's default: the image file (with its mask) as a new texture of that name.
///
/// # Safety
/// `name` a C string, `mask` one or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_tex_default_read_cb(name: *const c_char, mask: *const c_char) -> *mut Texture {
    unsafe {
        let n = std::ffi::CStr::from_ptr(name).to_str().unwrap_or("");
        let m = if mask.is_null() { None } else { Some(std::ffi::CStr::from_ptr(mask).to_str().unwrap_or("")) };
        let img = crate::rwimagefile::read_masked(".", n, m);
        if img.is_null() {
            return std::ptr::null_mut();
        }
        let tex = tex_create(revc_tex_globals(), crate::rwengine::raster_create_from_image(img, 0) as *mut c_void);
        // strncpy(name, 32)
        let copy = |dst: &mut [u8; 32], src: *const c_char| {
            let b = std::ffi::CStr::from_ptr(src).to_bytes();
            dst.fill(0);
            let k = b.len().min(32);
            dst[..k].copy_from_slice(&b[..k]);
        };
        copy(&mut (*tex).name, name);
        if !mask.is_null() {
            copy(&mut (*tex).mask, mask);
        }
        crate::rwimage::image_destroy(img);
        tex
    }
}

/// The anisotropy plugin's offset in Texture (rw::anisotOffset; 0 = not registered).
#[unsafe(no_mangle)]
pub static mut revc_anisot_offset: i32 = 0;
unsafe extern "C" fn anisot_create(o: *mut c_void, off: i32, _: i32) -> *mut c_void {
    unsafe { *((o as *mut u8).add(off as usize) as *mut i32) = 1 };
    o
}
unsafe extern "C" fn anisot_copy(d: *mut c_void, s: *mut c_void, off: i32, _: i32) -> *mut c_void {
    unsafe { *((d as *mut u8).add(off as usize) as *mut i32) = *((s as *mut u8).add(off as usize) as *mut i32) };
    d
}
unsafe extern "C" fn anisot_read(st: *mut c_void, _: i32, o: *mut c_void, off: i32, _: i32) -> *mut c_void {
    unsafe { *((o as *mut u8).add(off as usize) as *mut i32) = i32::from_le_bytes(crate::rwplugin::stream_read_bytes::<4>(st)) };
    st
}
const ID_ANISOT: u32 = 0x127;

/// registerAnisotropyPlugin
///
/// # Safety
/// During plugin attach.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_register_anisotropy_plugin() {
    unsafe {
        let l = crate::rwplugin::class_list(crate::rwplugin::CLASS_TEXTURE);
        *std::ptr::addr_of_mut!(revc_anisot_offset) = crate::rwplugin::pl_register(l, 4, ID_ANISOT, Some(anisot_create), None, Some(anisot_copy));
        crate::rwplugin::pl_register_stream(l, ID_ANISOT, Some(anisot_read), None, None);
    }
}

fn layout<T>(size: usize) -> std::alloc::Layout {
    std::alloc::Layout::from_size_align(size.max(size_of::<T>()), 16).unwrap()
}

unsafe fn list_append(list: *mut LinkList, link: *mut LLLink) {
    unsafe {
        (*link).next = &mut (*list).link;
        (*link).prev = (*list).link.prev;
        (*(*list).link.prev).next = link;
        (*list).link.prev = link;
    }
}

/// strncpy(dst, src, 32): up to 32 bytes, zero padded, no terminator for 32 or more.
unsafe fn strncpy32(dst: &mut [u8; 32], src: *const c_char) {
    let mut ended = false;
    for (i, d) in dst.iter_mut().enumerate() {
        let c = if ended { 0 } else { unsafe { *src.add(i) as u8 } };
        ended |= c == 0;
        *d = c;
    }
}

/// strncmp_ci(name, s, 32) == 0 (ASCII case folding, as tolower in the C locale).
unsafe fn name_matches(name: &[u8; 32], s: *const c_char) -> bool {
    for (i, &a) in name.iter().enumerate() {
        let c1 = a.to_ascii_lowercase();
        let c2 = unsafe { *s.add(i) as u8 }.to_ascii_lowercase();
        if c1 != c2 {
            return false;
        }
        if c1 == 0 {
            return true;
        }
    }
    true
}

// ---------------------------------------------------------------- TexDictionary

/// TexDictionary::create: a new, empty dictionary at the front of the global list.
///
/// # Safety
/// `g`: the texture globals.
pub unsafe fn dict_create(g: *mut TextureGlobals) -> *mut TexDictionary {
    unsafe {
        let h = hooks();
        let d = std::alloc::alloc(layout::<TexDictionary>((h.dict_size)())) as *mut TexDictionary;
        if d.is_null() {
            return d;
        }
        (h.allocated)(1, 1);
        (*d).object = Object { kind: TEXDICTIONARY_ID, sub_type: 0, flags: 0, private_flags: 0, parent: std::ptr::null_mut() };
        list_init(&mut (*d).textures);
        list_add(&mut (*g).tex_dicts, &mut (*d).in_global_list);
        (h.dict_construct)(d);
        d
    }
}

/// TexDictionary::destroy: takes every texture out and destroys it (drops one reference).
///
/// # Safety
/// A live dictionary.
pub unsafe fn dict_destroy(g: *mut TextureGlobals, d: *mut TexDictionary) {
    unsafe {
        let h = hooks();
        if (*g).current_tex_dict == d {
            (*g).current_tex_dict = std::ptr::null_mut();
        }
        let end: *mut LLLink = &mut (*d).textures.link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            let t = container_of!(lnk, Texture, in_dict);
            dict_remove(d, t);
            tex_destroy(t);
            lnk = next;
        }
        (h.dict_destruct)(d);
        link_remove(&mut (*d).in_global_list);
        std::alloc::dealloc(d as *mut u8, layout::<TexDictionary>((h.dict_size)()));
        (h.allocated)(1, -1);
    }
}

/// TexDictionary::add: moves the texture to the end of this dictionary.
///
/// # Safety
/// Live objects.
pub unsafe fn dict_add(d: *mut TexDictionary, t: *mut Texture) {
    unsafe {
        if !(*t).dict.is_null() {
            link_remove(&mut (*t).in_dict);
        }
        (*t).dict = d;
        list_append(&mut (*d).textures, &mut (*t).in_dict);
    }
}

/// TexDictionary::addFront
///
/// # Safety
/// Live objects.
pub unsafe fn dict_add_front(d: *mut TexDictionary, t: *mut Texture) {
    unsafe {
        if !(*t).dict.is_null() {
            link_remove(&mut (*t).in_dict);
        }
        (*t).dict = d;
        list_add(&mut (*d).textures, &mut (*t).in_dict);
    }
}

/// TexDictionary::remove
///
/// # Safety
/// A texture in this dictionary.
pub unsafe fn dict_remove(d: *mut TexDictionary, t: *mut Texture) {
    unsafe {
        debug_assert!((*t).dict == d);
        link_remove(&mut (*t).in_dict);
        (*t).dict = std::ptr::null_mut();
    }
}

/// TexDictionary::find: the first texture whose name matches (case-insensitive, 32 chars).
///
/// # Safety
/// A live dictionary; `name` a C string.
pub unsafe fn dict_find(d: *mut TexDictionary, name: *const c_char) -> *mut Texture {
    unsafe {
        let end: *mut LLLink = &mut (*d).textures.link;
        let mut lnk = (*end).next;
        while lnk != end {
            let t = container_of!(lnk, Texture, in_dict);
            if name_matches(&(*t).name, name) {
                return t;
            }
            lnk = (*lnk).next;
        }
        std::ptr::null_mut()
    }
}

// ---------------------------------------------------------------- Texture

/// Texture::create: one reference, wrap/wrap/nearest, at the front of the global list.
///
/// # Safety
/// `g`: the texture globals.
pub unsafe fn tex_create(g: *mut TextureGlobals, raster: *mut c_void) -> *mut Texture {
    unsafe {
        let h = hooks();
        let t = std::alloc::alloc(layout::<Texture>((h.tex_size)())) as *mut Texture;
        if t.is_null() {
            return t;
        }
        (h.allocated)(0, 1);
        (*t).dict = std::ptr::null_mut();
        (*t).in_dict = LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() };
        (*t).name = [0; 32];
        (*t).mask = [0; 32];
        (*t).filter_addressing = (WRAP << 12) | (WRAP << 8) | NEAREST;
        (*t).raster = raster;
        (*t).ref_count = 1;
        list_add(&mut (*g).textures, &mut (*t).in_global_list);
        (h.tex_construct)(t);
        t
    }
}

/// Texture::destroy: drops a reference; the last one frees it (and its raster).
///
/// # Safety
/// A live texture.
pub unsafe fn tex_destroy(t: *mut Texture) {
    unsafe {
        (*t).ref_count -= 1;
        if (*t).ref_count <= 0 {
            let h = hooks();
            (h.tex_destruct)(t);
            if !(*t).dict.is_null() {
                link_remove(&mut (*t).in_dict);
            }
            if !(*t).raster.is_null() {
                (h.raster_destroy)((*t).raster);
            }
            link_remove(&mut (*t).in_global_list);
            std::alloc::dealloc(t as *mut u8, layout::<Texture>((h.tex_size)()));
            (h.allocated)(0, -1);
        }
    }
}

/// texture.cpp's defaultFindCB: the current dictionary only.
///
/// # Safety
/// `g`: the texture globals; `name` a C string.
pub unsafe fn default_find(g: *mut TextureGlobals, name: *const c_char) -> *mut Texture {
    unsafe {
        if (*g).current_tex_dict.is_null() {
            return std::ptr::null_mut();
        }
        dict_find((*g).current_tex_dict, name)
    }
}

/// Texture::read: a found texture gets another reference; otherwise it is loaded (when loading
/// is on) or, failing that, made as a named dummy (when dummies are on). New and found ones
/// move into the current dictionary.
///
/// # Safety
/// `g`: the texture globals; `name` a C string, `mask` one or null.
pub unsafe fn tex_read(g: *mut TextureGlobals, name: *const c_char, mask: *const c_char, find: FindCb, read: ReadCb) -> *mut Texture {
    unsafe {
        let mut tex = find(name);
        if !tex.is_null() {
            (*tex).ref_count += 1;
            return tex;
        }
        let dummy = if (*g).load_textures != 0 {
            tex = read(name, mask);
            tex.is_null()
        } else {
            true
        };
        if dummy && (*g).make_dummies != 0 {
            tex = tex_create(g, std::ptr::null_mut());
            if tex.is_null() {
                return tex;
            }
            strncpy32(&mut (*tex).name, name);
            if !mask.is_null() {
                strncpy32(&mut (*tex).mask, mask);
            }
            (*tex).raster = (hooks().raster_create_dummy)();
        }
        if !tex.is_null() && !(*g).current_tex_dict.is_null() {
            if !(*tex).dict.is_null() {
                link_remove(&mut (*tex).in_dict);
            }
            dict_add((*g).current_tex_dict, tex);
        }
        tex
    }
}

// ---------------------------------------------------------------- C ABI (texture.cpp forwards)

/// # Safety
/// Called once by Texture::registerModule, before any texture exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_tex_set_hooks(h: Hooks) {
    unsafe { *std::ptr::addr_of_mut!(HOOKS) = Some(h) };
}

macro_rules! ffi {
    ($($name:ident ( $($a:ident : $t:ty),* ) $(-> $r:ty)? => $body:expr;)*) => {
        $(
            /// # Safety
            /// As the Rust function it forwards to.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name($($a: $t),*) $(-> $r)? { unsafe { $body } }
        )*
    };
}

ffi! {
    revc_texdict_create(g: *mut TextureGlobals) -> *mut TexDictionary => dict_create(g);
    revc_texdict_destroy(g: *mut TextureGlobals, d: *mut TexDictionary) => dict_destroy(g, d);
    revc_texdict_add(d: *mut TexDictionary, t: *mut Texture) => dict_add(d, t);
    revc_texdict_add_front(d: *mut TexDictionary, t: *mut Texture) => dict_add_front(d, t);
    revc_texdict_remove(d: *mut TexDictionary, t: *mut Texture) => dict_remove(d, t);
    revc_texdict_find(d: *mut TexDictionary, name: *const c_char) -> *mut Texture => dict_find(d, name);
    revc_tex_create(g: *mut TextureGlobals, raster: *mut c_void) -> *mut Texture => tex_create(g, raster);
    revc_tex_destroy(t: *mut Texture) => tex_destroy(t);
    revc_tex_default_find(g: *mut TextureGlobals, name: *const c_char) -> *mut Texture => default_find(g, name);
    revc_tex_read(g: *mut TextureGlobals, name: *const c_char, mask: *const c_char, find: FindCb, read: ReadCb) -> *mut Texture => tex_read(g, name, mask, find, read);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwtexture.txt (recorded from librw's C++, see the recorder).
    use super::*;
    use std::cell::RefCell;
    use std::ffi::CString;

    thread_local! {
        static DICTS: RefCell<Vec<*mut TexDictionary>> = const { RefCell::new(Vec::new()) };
        static TEXS: RefCell<Vec<*mut Texture>> = const { RefCell::new(Vec::new()) };
        static RASTERS: RefCell<Vec<*mut c_void>> = const { RefCell::new(Vec::new()) };
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static ALLOC: RefCell<[i32; 2]> = const { RefCell::new([0; 2]) };
        static G: RefCell<*mut TextureGlobals> = const { RefCell::new(std::ptr::null_mut()) };
    }
    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn id_of<T>(v: &RefCell<Vec<*mut T>>, p: *mut T) -> i64 {
        if p.is_null() {
            return -1;
        }
        v.borrow().iter().position(|&q| q == p).map_or(-2, |i| i as i64)
    }
    fn did(p: *mut TexDictionary) -> i64 {
        DICTS.with(|v| id_of(v, p))
    }
    fn tid(p: *mut Texture) -> i64 {
        TEXS.with(|v| id_of(v, p))
    }
    fn rid(p: *mut c_void) -> i64 {
        RASTERS.with(|v| id_of(v, p))
    }
    fn new_raster() -> *mut c_void {
        let r = Box::into_raw(Box::new(0u64)) as *mut c_void;
        RASTERS.with(|v| v.borrow_mut().push(r));
        r
    }

    unsafe extern "C" fn tex_size() -> usize {
        size_of::<Texture>() + 8
    }
    unsafe extern "C" fn dict_size() -> usize {
        size_of::<TexDictionary>() + 8
    }
    unsafe extern "C" fn tex_construct(t: *mut Texture) {
        let n = TEXS.with(|v| {
            v.borrow_mut().push(t);
            v.borrow().len() - 1
        });
        log(format!(" ct{n}"));
    }
    unsafe extern "C" fn tex_destruct(t: *mut Texture) {
        let i = tid(t);
        log(format!(" dt{i}"));
        if i >= 0 {
            TEXS.with(|v| v.borrow_mut()[i as usize] = std::ptr::null_mut());
        }
    }
    unsafe extern "C" fn dict_construct(d: *mut TexDictionary) {
        let n = DICTS.with(|v| {
            v.borrow_mut().push(d);
            v.borrow().len() - 1
        });
        log(format!(" cd{n}"));
    }
    unsafe extern "C" fn dict_destruct(d: *mut TexDictionary) {
        let i = did(d);
        log(format!(" dd{i}"));
        if i >= 0 {
            DICTS.with(|v| v.borrow_mut()[i as usize] = std::ptr::null_mut());
        }
    }
    unsafe extern "C" fn raster_destroy(r: *mut c_void) {
        log(format!(" rd{}", rid(r)));
    }
    unsafe extern "C" fn raster_create_dummy() -> *mut c_void {
        let r = new_raster();
        log(format!(" rc{}", rid(r)));
        r
    }
    unsafe extern "C" fn allocated(kind: i32, delta: i32) {
        ALLOC.with(|a| a.borrow_mut()[kind as usize] += delta);
    }
    unsafe extern "C" fn find_cb(name: *const c_char) -> *mut Texture {
        unsafe { default_find(G.with(|g| *g.borrow()), name) }
    }
    /// The recorder's defaultReadCB with its stubbed Image::readMasked (names starting with f
    /// load) and Raster::createFromImage.
    unsafe extern "C" fn read_cb(name: *const c_char, mask: *const c_char) -> *mut Texture {
        unsafe {
            let n = std::ffi::CStr::from_ptr(name).to_bytes();
            log(format!(" read:{}", String::from_utf8_lossy(&n[..n.len().min(32)])));
            if !matches!(n.first(), Some(b'f' | b'F')) {
                return std::ptr::null_mut();
            }
            let r = new_raster();
            log(format!(" ri{}", rid(r)));
            let t = tex_create(G.with(|g| *g.borrow()), r);
            strncpy32(&mut (*t).name, name);
            if !mask.is_null() {
                strncpy32(&mut (*t).mask, mask);
            }
            t
        }
    }

    fn cstr32(b: &[u8; 32]) -> String {
        String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(32)]).into_owned()
    }

    unsafe fn state(g: *mut TextureGlobals) -> String {
        unsafe {
            let mut s = String::new();
            let end: *mut LLLink = &mut (*g).tex_dicts.link;
            let mut lnk = (*end).next;
            while lnk != end {
                let d = container_of!(lnk, TexDictionary, in_global_list);
                s += &format!(" D{} {} :", did(d), ((*g).current_tex_dict == d) as i32);
                let e2: *mut LLLink = &mut (*d).textures.link;
                let mut l2 = (*e2).next;
                while l2 != e2 {
                    s += &format!(" {}", tid(container_of!(l2, Texture, in_dict)));
                    l2 = (*l2).next;
                }
                lnk = (*lnk).next;
            }
            let end: *mut LLLink = &mut (*g).textures.link;
            let mut lnk = (*end).next;
            while lnk != end {
                let t = container_of!(lnk, Texture, in_global_list);
                s += &format!(" T{} {} {} {}|{} {:x} {}", tid(t), (*t).ref_count, did((*t).dict), cstr32(&(*t).name), cstr32(&(*t).mask), (*t).filter_addressing, rid((*t).raster));
                lnk = (*lnk).next;
            }
            let h = s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
            let l = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            let a = ALLOC.with(|a| *a.borrow());
            format!(" {h:016x} | log:{l} | allocated: {} {}", a[0], a[1])
        }
    }

    fn name(s: &str) -> CString {
        CString::new(if s == "~" { "" } else { s }).unwrap()
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwtexture.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwtexture.txt");
        unsafe {
            revc_tex_set_hooks(Hooks { tex_size, tex_construct, tex_destruct, dict_size, dict_construct, dict_destruct, raster_destroy, raster_create_dummy, allocated });
            let g: *mut TextureGlobals = Box::into_raw(Box::new(std::mem::zeroed()));
            G.with(|x| *x.borrow_mut() = g);
            // textureOpen
            list_init(&mut (*g).tex_dicts);
            list_init(&mut (*g).textures);
            let d = dict_create(g);
            (*g).initial_tex_dict = d;
            (*g).current_tex_dict = d;
            (*g).load_textures = 1;
            let dict = |i: &str| DICTS.with(|v| v.borrow()[i.parse::<usize>().unwrap()]);
            let tex = |i: &str| TEXS.with(|v| v.borrow()[i.parse::<usize>().unwrap()]);
            let (mut n, mut bad) = (0, Vec::new());
            for line in text.lines() {
                let (input, want) = line.split_once(" =>").unwrap();
                let w: Vec<&str> = input.split_whitespace().collect();
                let mut out = String::new();
                match w[0] {
                    "open" => {}
                    "newdict" => {
                        dict_create(g);
                    }
                    "deldict" => dict_destroy(g, dict(w[1])),
                    "newtex" => {
                        let r = if w[1] == "1" { new_raster() } else { std::ptr::null_mut() };
                        let t = tex_create(g, r);
                        strncpy32(&mut (*t).name, name(w[2]).as_ptr());
                        (*t).filter_addressing = u32::from_str_radix(w[3], 16).unwrap();
                    }
                    "deltex" => tex_destroy(tex(w[1])),
                    "addref" => (*tex(w[1])).ref_count += 1,
                    "add" => dict_add(dict(w[1]), tex(w[2])),
                    "addfront" => dict_add_front(dict(w[1]), tex(w[2])),
                    "remove" => {
                        let t = tex(w[1]);
                        dict_remove((*t).dict, t);
                    }
                    "find" => {
                        let r = dict_find(dict(w[1]), name(w[2]).as_ptr());
                        out = format!(" {}", tid(r));
                    }
                    "setcurrent" => (*g).current_tex_dict = if w[1] == "-1" { std::ptr::null_mut() } else { dict(w[1]) },
                    "setload" => (*g).load_textures = w[1].parse().unwrap(),
                    "setdummies" => (*g).make_dummies = w[1].parse().unwrap(),
                    "read" => {
                        let nm = name(w[1]);
                        let mk = (w[2] != "-").then(|| name(w[2]));
                        let r = tex_read(g, nm.as_ptr(), mk.as_ref().map_or(std::ptr::null(), |m| m.as_ptr()), find_cb, read_cb);
                        out = format!(" {}", tid(r));
                    }
                    other => panic!("unknown op {other}"),
                }
                // the recorder prints find/read results before " =>"
                let got_in = input.rsplit_once(" =").map(|(_, r)| format!(" {}", r.trim()));
                if let Some(gi) = got_in.filter(|_| !out.is_empty()) {
                    if gi != out && bad.len() < 5 {
                        bad.push(format!("{input}: result{out}"));
                    }
                }
                let got = state(g);
                n += 1;
                if got != want && bad.len() < 5 {
                    bad.push(format!("{input}\n  want{want}\n  got {got}"));
                }
            }
            assert!(n > 3000, "only {n} cases");
            assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
        }
    }
}

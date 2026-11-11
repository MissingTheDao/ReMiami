//! librw's plugin lists and streams, stage 2 step 7a: plg.cpp (PluginList: register,
//! registerStream, the rights and always callbacks, getPluginOffset, construct/destruct/copy,
//! streamRead/streamSkip/streamWrite/streamGetSize, assertRights) and base.cpp's streams
//! (StreamMemory, the typed read/write helpers, writeChunkHeader, readChunkHeaderInfo, findChunk).
//! C++ streams are virtual classes: Rust reaches a stream's read8/write8/seek/tell/eof through
//! small C++ trampolines (hooks); StreamMemory's own methods are here. StreamFile (stdio) stays
//! C++. Little-endian only, as the game builds.
//!
//! Checked against tests/golden/rwplugin.txt, recorded from the original C++
//! (tests/golden/recorders/rwplugin).

use std::ffi::c_void;

use crate::rwframe::{LLLink, LinkList, link_remove, list_add, list_init};

pub type Constructor = unsafe extern "C" fn(*mut c_void, i32, i32) -> *mut c_void;
pub type Destructor = unsafe extern "C" fn(*mut c_void, i32, i32) -> *mut c_void;
pub type CopyConstructor = unsafe extern "C" fn(*mut c_void, *mut c_void, i32, i32) -> *mut c_void;
pub type StreamRead = unsafe extern "C" fn(*mut c_void, i32, *mut c_void, i32, i32) -> *mut c_void;
pub type StreamWrite = unsafe extern "C" fn(*mut c_void, i32, *mut c_void, i32, i32) -> *mut c_void;
pub type StreamGetSize = unsafe extern "C" fn(*mut c_void, i32, i32) -> i32;
pub type RightsCallback = unsafe extern "C" fn(*mut c_void, i32, i32, u32);
pub type AlwaysCallback = unsafe extern "C" fn(*mut c_void, i32, i32);

/// rw::PluginList
#[repr(C)]
pub struct PluginList {
    pub size: i32,
    pub default_size: i32,
    pub plugins: LinkList,
}

/// rw::Plugin
#[repr(C)]
pub struct Plugin {
    pub offset: i32,
    pub size: i32,
    pub id: u32,
    pub constructor: Constructor,
    pub destructor: Destructor,
    pub copy: CopyConstructor,
    pub read: Option<StreamRead>,
    pub write: Option<StreamWrite>,
    pub get_size: Option<StreamGetSize>,
    pub rights_callback: Option<RightsCallback>,
    pub always_callback: Option<AlwaysCallback>,
    pub parent_list: *mut PluginList,
    pub in_parent_list: LLLink,
    pub in_global_list: LLLink,
}

/// rw::StreamMemory (a C++ class: the vtable pointer comes first).
#[repr(C)]
pub struct StreamMemory {
    pub vptr: *const c_void,
    pub data: *mut u8,
    pub length: u32,
    pub capacity: u32,
    pub position: u32,
}

pub const S_EOF: u32 = 0xFFFF_FFFF;
pub const ID_NAOBJECT: u32 = 0x00;
pub const ID_EXTENSION: u32 = 0x03;

/// rw::ChunkHeaderInfo
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ChunkHeaderInfo {
    pub kind: u32,
    pub length: u32,
    pub version: u32,
    pub build: u32,
}

/// A stream's virtual methods (C++ trampolines) and librw's version globals.
#[repr(C)]
pub struct Hooks {
    pub read8: unsafe extern "C" fn(s: *mut c_void, data: *mut u8, n: u32) -> u32,
    pub write8: unsafe extern "C" fn(s: *mut c_void, data: *const u8, n: u32) -> u32,
    pub seek: unsafe extern "C" fn(s: *mut c_void, offset: i32, whence: i32),
    pub tell: unsafe extern "C" fn(s: *mut c_void) -> u32,
    pub eof: unsafe extern "C" fn(s: *mut c_void) -> i32,
    /// rw::version and rw::build
    pub version: unsafe extern "C" fn() -> i32,
    pub build: unsafe extern "C" fn() -> i32,
}

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set once at startup (static initialiser in base.cpp), read-only afterwards.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().expect("revc_stream_set_hooks not called") }
}

macro_rules! container_of {
    ($link:expr, $t:ty, $field:ident) => {
        ($link as *mut u8).sub(std::mem::offset_of!($t, $field)) as *mut $t
    };
}

unsafe fn for_list(list: *mut LinkList, mut f: impl FnMut(*mut LLLink) -> bool) {
    unsafe {
        let end: *mut LLLink = &mut (*list).link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            if !f(lnk) {
                return;
            }
            lnk = next;
        }
    }
}

// ---------------------------------------------------------------- memory streams (base.cpp)

/// StreamMemory::write8: grows the length up to the capacity; a short write ends the stream.
///
/// # Safety
/// A valid memory stream; `data` `len` bytes.
pub unsafe fn sm_write8(s: *mut StreamMemory, data: *const u8, len: u32) -> u32 {
    unsafe {
        let m = &mut *s;
        if m.position == S_EOF {
            return 0;
        }
        let mut l = len;
        if m.position.wrapping_add(l) > m.length {
            if m.position.wrapping_add(l) > m.capacity {
                l = m.capacity.wrapping_sub(m.position);
            }
            m.length = m.position.wrapping_add(l);
        }
        std::ptr::copy(data, m.data.add(m.position as usize), l as usize);
        m.position = m.position.wrapping_add(l);
        if len != l {
            m.position = S_EOF;
        }
        l
    }
}

/// StreamMemory::read8: a short read ends the stream.
///
/// # Safety
/// A valid memory stream; `data` `len` bytes.
pub unsafe fn sm_read8(s: *mut StreamMemory, data: *mut u8, len: u32) -> u32 {
    unsafe {
        let m = &mut *s;
        if m.position == S_EOF {
            return 0;
        }
        let mut l = len;
        if m.position.wrapping_add(l) > m.length {
            l = m.length.wrapping_sub(m.position);
        }
        std::ptr::copy(m.data.add(m.position as usize), data, l as usize);
        m.position = m.position.wrapping_add(l);
        if len != l {
            m.position = S_EOF;
        }
        l
    }
}

/// StreamMemory::seek (0 from the start, 1 from here, otherwise back from the end); past the
/// length grows it up to the capacity, past that ends the stream.
///
/// # Safety
/// A valid memory stream.
pub unsafe fn sm_seek(s: *mut StreamMemory, offset: i32, whence: i32) {
    unsafe {
        let m = &mut *s;
        m.position = match whence {
            0 => offset as u32,
            1 => m.position.wrapping_add(offset as u32),
            _ => m.length.wrapping_sub(offset as u32),
        };
        if m.position > m.length {
            if m.position > m.capacity {
                m.position = S_EOF;
            } else {
                m.length = m.position;
            }
        }
    }
}

/// StreamMemory::open
///
/// # Safety
/// A writable memory stream.
pub unsafe fn sm_open(s: *mut StreamMemory, data: *mut u8, length: u32, capacity: u32) {
    unsafe {
        let m = &mut *s;
        m.data = data;
        m.capacity = capacity;
        m.length = length;
        if m.capacity < m.length {
            m.capacity = m.length;
        }
        m.position = 0;
    }
}

// ---------------------------------------------------------------- stream helpers

/// Stream::read8 etc. through the stream's own methods.
unsafe fn read8(s: *mut c_void, data: *mut u8, n: u32) -> u32 {
    unsafe { (hooks().read8)(s, data, n) }
}
unsafe fn write8(s: *mut c_void, data: *const u8, n: u32) -> u32 {
    unsafe { (hooks().write8)(s, data, n) }
}

/// Stream::read32 / read16 (little-endian: the bytes as they are)
///
/// # Safety
/// A valid stream; `data` `n` bytes.
pub unsafe fn stream_read32(s: *mut c_void, data: *mut u8, n: u32) -> u32 {
    unsafe { read8(s, data, n) }
}

/// Stream::write32 / write16
///
/// # Safety
/// A valid stream; `data` `n` bytes.
pub unsafe fn stream_write32(s: *mut c_void, data: *const u8, n: u32) -> u32 {
    unsafe { write8(s, data, n) }
}

/// Stream::readI8 .. readF32: the value's bytes as read (a short read leaves the rest zero;
/// librw left them uninitialised).
///
/// # Safety
/// A valid stream.
pub unsafe fn stream_read_bytes<const N: usize>(s: *mut c_void) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { read8(s, b.as_mut_ptr(), N as u32) };
    b
}

/// Stream::writeI8 .. writeF32: the number of bytes written.
///
/// # Safety
/// A valid stream.
pub unsafe fn stream_write_bytes(s: *mut c_void, b: &[u8]) -> i32 {
    unsafe { write8(s, b.as_ptr(), b.len() as u32) as i32 }
}

/// libraryIDPack
pub fn library_id_pack(version: i32, build: i32) -> u32 {
    if version <= 0x31000 {
        return (version >> 8) as u32;
    }
    ((((version - 0x30000) & 0x3FF00) << 14) | ((version & 0x3F) << 16) | (build & 0xFFFF)) as u32
}

/// libraryIDUnpackVersion
pub fn library_id_unpack_version(libid: u32) -> u32 {
    if libid & 0xFFFF0000 != 0 { (((libid >> 14) & 0x3FF00) + 0x30000) | ((libid >> 16) & 0x3F) } else { libid << 8 }
}

/// libraryIDUnpackBuild
pub fn library_id_unpack_build(libid: u32) -> u32 {
    if libid & 0xFFFF0000 != 0 { libid & 0xFFFF } else { 0 }
}

/// writeChunkHeader: type, size and the library id of rw::version and rw::build.
///
/// # Safety
/// A valid stream.
pub unsafe fn write_chunk_header(s: *mut c_void, kind: i32, size: i32) -> bool {
    unsafe {
        let h = hooks();
        let id = library_id_pack((h.version)(), (h.build)());
        let mut buf = [0u8; 12];
        buf[0..4].copy_from_slice(&kind.to_le_bytes());
        buf[4..8].copy_from_slice(&size.to_le_bytes());
        buf[8..12].copy_from_slice(&id.to_le_bytes());
        stream_write32(s, buf.as_ptr(), 12);
        true
    }
}

/// readChunkHeaderInfo: false at the end of the stream.
///
/// # Safety
/// A valid stream.
pub unsafe fn read_chunk_header_info(s: *mut c_void, header: &mut ChunkHeaderInfo) -> bool {
    unsafe {
        let mut buf = [0u8; 12];
        stream_read32(s, buf.as_mut_ptr(), 12);
        if (hooks().eof)(s) != 0 {
            return false;
        }
        let w = |i: usize| u32::from_le_bytes(buf[i * 4..i * 4 + 4].try_into().unwrap());
        header.kind = w(0);
        header.length = w(1);
        header.version = library_id_unpack_version(w(2));
        header.build = library_id_unpack_build(w(2));
        true
    }
}

/// findChunk: skips chunks up to one of this type (false at the end or at an NAOBJECT).
///
/// # Safety
/// A valid stream; the outputs writable or null.
pub unsafe fn find_chunk(s: *mut c_void, kind: u32, length: *mut u32, version: *mut u32) -> bool {
    unsafe {
        let mut h = ChunkHeaderInfo::default();
        while read_chunk_header_info(s, &mut h) {
            if h.kind == ID_NAOBJECT {
                return false;
            }
            if h.kind == kind {
                if !length.is_null() {
                    *length = h.length;
                }
                if !version.is_null() {
                    *version = h.version;
                }
                return true;
            }
            (hooks().seek)(s, h.length as i32, 1);
        }
        false
    }
}

// ---------------------------------------------------------------- plugin lists (plg.cpp)

unsafe extern "C" fn def_ctor(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
    o
}
unsafe extern "C" fn def_dtor(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
    o
}
unsafe extern "C" fn def_copy(d: *mut c_void, _: *mut c_void, _: i32, _: i32) -> *mut c_void {
    d
}

/// plg.cpp's allPlugins
static mut ALL_PLUGINS: LinkList = LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } };

fn all_plugins() -> *mut LinkList {
    std::ptr::addr_of_mut!(ALL_PLUGINS)
}

/// PluginList::open
///
/// # Safety
/// Before any plugin is registered.
pub unsafe fn pl_open() {
    unsafe { list_init(all_plugins()) }
}

/// PluginList::close: frees every plugin; lists left empty get their default size back.
///
/// # Safety
/// After the plugin lists are no longer used.
pub unsafe fn pl_close() {
    unsafe {
        for_list(all_plugins(), |lnk| {
            let p = container_of!(lnk, Plugin, in_global_list);
            let l = (*p).parent_list;
            link_remove(&mut (*p).in_parent_list);
            link_remove(&mut (*p).in_global_list);
            drop(Box::from_raw(p));
            if (*l).plugins.link.next == &mut (*l).plugins.link as *mut LLLink {
                (*l).size = (*l).default_size;
            }
            true
        });
    }
}

unsafe fn plugins(l: *mut PluginList, mut f: impl FnMut(&mut Plugin) -> bool) {
    unsafe { for_list(&mut (*l).plugins, |lnk| f(&mut *container_of!(lnk, Plugin, in_parent_list))) }
}

/// PluginList::construct: every plugin's constructor, newest first.
///
/// # Safety
/// A valid list; `o` an object of the list's size.
pub unsafe fn pl_construct(l: *mut PluginList, o: *mut c_void) {
    unsafe {
        plugins(l, |p| {
            (p.constructor)(o, p.offset, p.size);
            true
        })
    }
}

/// PluginList::destruct
///
/// # Safety
/// As pl_construct.
pub unsafe fn pl_destruct(l: *mut PluginList, o: *mut c_void) {
    unsafe {
        plugins(l, |p| {
            (p.destructor)(o, p.offset, p.size);
            true
        })
    }
}

/// PluginList::copy
///
/// # Safety
/// As pl_construct, two objects.
pub unsafe fn pl_copy(l: *mut PluginList, dst: *mut c_void, src: *mut c_void) {
    unsafe {
        plugins(l, |p| {
            (p.copy)(dst, src, p.offset, p.size);
            true
        })
    }
}

/// PluginList::streamRead: the extension chunk, each sub-chunk to the plugin with its id and a
/// reader (others skipped; a reader is trusted to consume its chunk), then every always callback.
///
/// # Safety
/// A valid list, stream and object.
pub unsafe fn pl_stream_read(l: *mut PluginList, s: *mut c_void, o: *mut c_void) -> bool {
    unsafe {
        let mut length: u32 = 0;
        if !find_chunk(s, ID_EXTENSION, &mut length, std::ptr::null_mut()) {
            return false;
        }
        let mut length = length as i32;
        while length > 0 {
            let mut h = ChunkHeaderInfo::default();
            if !read_chunk_header_info(s, &mut h) {
                return false;
            }
            length -= 12;
            let mut done = false;
            plugins(l, |p| {
                if p.id == h.kind
                    && let Some(r) = p.read
                {
                    r(s, h.length as i32, o, p.offset, p.size);
                    done = true;
                    return false;
                }
                true
            });
            if !done {
                (hooks().seek)(s, h.length as i32, 1);
            }
            length -= h.length as i32;
        }
        plugins(l, |p| {
            if let Some(a) = p.always_callback {
                a(o, p.offset, p.size);
            }
            true
        });
        true
    }
}

/// PluginList::streamGetSize: the extension chunk's contents (12 per sub-chunk plus its size).
///
/// # Safety
/// A valid list and object.
pub unsafe fn pl_stream_get_size(l: *mut PluginList, o: *mut c_void) -> i32 {
    unsafe {
        let mut size = 0;
        plugins(l, |p| {
            if let Some(g) = p.get_size {
                let n = g(o, p.offset, p.size);
                if n > 0 {
                    size += 12 + n;
                }
            }
            true
        });
        size
    }
}

/// PluginList::streamWrite
///
/// # Safety
/// A valid list, stream and object; a plugin with a size callback has a writer.
pub unsafe fn pl_stream_write(l: *mut PluginList, s: *mut c_void, o: *mut c_void) {
    unsafe {
        let size = pl_stream_get_size(l, o);
        write_chunk_header(s, ID_EXTENSION as i32, size);
        plugins(l, |p| {
            if let Some(g) = p.get_size {
                let n = g(o, p.offset, p.size);
                if n > 0 {
                    write_chunk_header(s, p.id as i32, n);
                    (p.write.expect("plugin with a size callback but no writer"))(s, n, o, p.offset, p.size);
                }
            }
            true
        });
    }
}

/// PluginList::streamSkip
///
/// # Safety
/// A valid stream.
pub unsafe fn pl_stream_skip(s: *mut c_void) {
    unsafe {
        let mut length: u32 = 0;
        if !find_chunk(s, ID_EXTENSION, &mut length, std::ptr::null_mut()) {
            return;
        }
        let mut length = length as i32;
        while length > 0 {
            let mut h = ChunkHeaderInfo::default();
            if !read_chunk_header_info(s, &mut h) {
                return;
            }
            (hooks().seek)(s, h.length as i32, 1);
            length -= 12 + h.length as i32;
        }
    }
}

/// PluginList::assertRights: the rights callback of the first plugin with this id.
///
/// # Safety
/// A valid list and object.
pub unsafe fn pl_assert_rights(l: *mut PluginList, o: *mut c_void, id: u32, data: u32) {
    unsafe {
        plugins(l, |p| {
            if p.id == id {
                if let Some(r) = p.rights_callback {
                    r(o, p.offset, p.size, data);
                }
                return false;
            }
            true
        })
    }
}

/// PluginList::registerPlugin: the plugin's offset; the list grows by its size, rounded up to a
/// pointer; the new plugin goes first.
///
/// # Safety
/// A valid list.
pub unsafe fn pl_register(l: *mut PluginList, size: i32, id: u32, ctor: Option<Constructor>, dtor: Option<Destructor>, copy: Option<CopyConstructor>) -> i32 {
    unsafe {
        let offset = (*l).size;
        let round = size_of::<*mut c_void>() as i32 - 1;
        (*l).size = ((*l).size + size + round) & !round;
        let p = Box::into_raw(Box::new(Plugin {
            offset,
            size,
            id,
            constructor: ctor.unwrap_or(def_ctor),
            destructor: dtor.unwrap_or(def_dtor),
            copy: copy.unwrap_or(def_copy),
            read: None,
            write: None,
            get_size: None,
            rights_callback: None,
            always_callback: None,
            parent_list: l,
            in_parent_list: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() },
            in_global_list: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() },
        }));
        list_add(&mut (*l).plugins, &mut (*p).in_parent_list);
        list_add(all_plugins(), &mut (*p).in_global_list);
        offset
    }
}

/// The first plugin with this id: its offset after `f` changed it, or -1.
unsafe fn with_plugin(l: *mut PluginList, id: u32, f: impl FnOnce(&mut Plugin)) -> i32 {
    unsafe {
        let mut f = Some(f);
        let mut r = -1;
        plugins(l, |p| {
            if p.id == id {
                (f.take().unwrap())(p);
                r = p.offset;
                return false;
            }
            true
        });
        r
    }
}

/// PluginList::registerStream
///
/// # Safety
/// A valid list.
pub unsafe fn pl_register_stream(l: *mut PluginList, id: u32, read: Option<StreamRead>, write: Option<StreamWrite>, get_size: Option<StreamGetSize>) -> i32 {
    unsafe {
        with_plugin(l, id, |p| {
            p.read = read;
            p.write = write;
            p.get_size = get_size;
        })
    }
}

/// PluginList::setStreamRightsCallback
///
/// # Safety
/// A valid list.
pub unsafe fn pl_set_rights(l: *mut PluginList, id: u32, cb: Option<RightsCallback>) -> i32 {
    unsafe { with_plugin(l, id, |p| p.rights_callback = cb) }
}

/// PluginList::setStreamAlwaysCallback
///
/// # Safety
/// A valid list.
pub unsafe fn pl_set_always(l: *mut PluginList, id: u32, cb: Option<AlwaysCallback>) -> i32 {
    unsafe { with_plugin(l, id, |p| p.always_callback = cb) }
}

/// PluginList::getPluginOffset
///
/// # Safety
/// A valid list.
pub unsafe fn pl_get_offset(l: *mut PluginList, id: u32) -> i32 {
    unsafe { with_plugin(l, id, |_| {}) }
}

// ---------------------------------------------------------------- C ABI (plg.cpp, base.cpp forward)

/// # Safety
/// Called once at startup, before any stream is used.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_stream_set_hooks(h: Hooks) {
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
    revc_sm_write8(s: *mut StreamMemory, d: *const u8, n: u32) -> u32 => sm_write8(s, d, n);
    revc_sm_read8(s: *mut StreamMemory, d: *mut u8, n: u32) -> u32 => sm_read8(s, d, n);
    revc_sm_seek(s: *mut StreamMemory, off: i32, whence: i32) => sm_seek(s, off, whence);
    revc_sm_open(s: *mut StreamMemory, d: *mut u8, len: u32, cap: u32) => sm_open(s, d, len, cap);
    revc_stream_read32(s: *mut c_void, d: *mut u8, n: u32) -> u32 => stream_read32(s, d, n);
    revc_stream_write32(s: *mut c_void, d: *const u8, n: u32) -> u32 => stream_write32(s, d, n);
    revc_write_chunk_header(s: *mut c_void, kind: i32, size: i32) -> i32 => write_chunk_header(s, kind, size) as i32;
    revc_read_chunk_header_info(s: *mut c_void, h: *mut ChunkHeaderInfo) -> i32 => read_chunk_header_info(s, &mut *h) as i32;
    revc_find_chunk(s: *mut c_void, kind: u32, len: *mut u32, ver: *mut u32) -> i32 => find_chunk(s, kind, len, ver) as i32;
    revc_pl_open() => pl_open();
    revc_pl_close() => pl_close();
    revc_pl_construct(l: *mut PluginList, o: *mut c_void) => pl_construct(l, o);
    revc_pl_destruct(l: *mut PluginList, o: *mut c_void) => pl_destruct(l, o);
    revc_pl_copy(l: *mut PluginList, d: *mut c_void, s: *mut c_void) => pl_copy(l, d, s);
    revc_pl_stream_read(l: *mut PluginList, s: *mut c_void, o: *mut c_void) -> i32 => pl_stream_read(l, s, o) as i32;
    revc_pl_stream_write(l: *mut PluginList, s: *mut c_void, o: *mut c_void) => pl_stream_write(l, s, o);
    revc_pl_stream_get_size(l: *mut PluginList, o: *mut c_void) -> i32 => pl_stream_get_size(l, o);
    revc_pl_stream_skip(s: *mut c_void) => pl_stream_skip(s);
    revc_pl_assert_rights(l: *mut PluginList, o: *mut c_void, id: u32, data: u32) => pl_assert_rights(l, o, id, data);
    revc_pl_register(l: *mut PluginList, size: i32, id: u32, c: Option<Constructor>, d: Option<Destructor>, y: Option<CopyConstructor>) -> i32 => pl_register(l, size, id, c, d, y);
    revc_pl_register_stream(l: *mut PluginList, id: u32, r: Option<StreamRead>, w: Option<StreamWrite>, g: Option<StreamGetSize>) -> i32 => pl_register_stream(l, id, r, w, g);
    revc_pl_set_rights(l: *mut PluginList, id: u32, cb: Option<RightsCallback>) -> i32 => pl_set_rights(l, id, cb);
    revc_pl_set_always(l: *mut PluginList, id: u32, cb: Option<AlwaysCallback>) -> i32 => pl_set_always(l, id, cb);
    revc_pl_get_offset(l: *mut PluginList, id: u32) -> i32 => pl_get_offset(l, id);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwplugin.txt (recorded from librw's C++, see the recorder).
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
    }
    static mut VERSION: i32 = 0x36003;
    static mut BUILD: i32 = 0xFFFF;
    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }

    // the stream hooks: every stream in this test is a memory stream
    unsafe extern "C" fn h_read8(s: *mut c_void, d: *mut u8, n: u32) -> u32 {
        unsafe { sm_read8(s as *mut StreamMemory, d, n) }
    }
    unsafe extern "C" fn h_write8(s: *mut c_void, d: *const u8, n: u32) -> u32 {
        unsafe { sm_write8(s as *mut StreamMemory, d, n) }
    }
    unsafe extern "C" fn h_seek(s: *mut c_void, off: i32, w: i32) {
        unsafe { sm_seek(s as *mut StreamMemory, off, w) }
    }
    unsafe extern "C" fn h_tell(s: *mut c_void) -> u32 {
        unsafe { (*(s as *mut StreamMemory)).position }
    }
    unsafe extern "C" fn h_eof(s: *mut c_void) -> i32 {
        unsafe { ((*(s as *mut StreamMemory)).position == S_EOF) as i32 }
    }
    unsafe extern "C" fn h_version() -> i32 {
        unsafe { VERSION }
    }
    unsafe extern "C" fn h_build() -> i32 {
        unsafe { BUILD }
    }

    // the recorder's eight callback sets
    struct CbSet {
        c: Constructor,
        d: Destructor,
        y: CopyConstructor,
        r: StreamRead,
        w: StreamWrite,
        g: StreamGetSize,
        rc: RightsCallback,
        ac: AlwaysCallback,
    }
    macro_rules! cbset {
        ($k:expr) => {{
            unsafe extern "C" fn c(o: *mut c_void, off: i32, size: i32) -> *mut c_void {
                log(format!(" c{}@{off}", $k));
                for i in 0..size {
                    unsafe { *(o as *mut u8).add((off + i) as usize) = ($k * 16 + i) as u8 };
                }
                o
            }
            unsafe extern "C" fn d(o: *mut c_void, off: i32, size: i32) -> *mut c_void {
                log(format!(" d{}@{off}", $k));
                for i in 0..size {
                    unsafe { *(o as *mut u8).add((off + i) as usize) ^= 0x5A };
                }
                o
            }
            unsafe extern "C" fn y(dst: *mut c_void, src: *mut c_void, off: i32, size: i32) -> *mut c_void {
                log(format!(" y{}@{off}", $k));
                unsafe { std::ptr::copy((src as *mut u8).add(off as usize), (dst as *mut u8).add(off as usize), size as usize) };
                dst
            }
            unsafe extern "C" fn r(st: *mut c_void, len: i32, o: *mut c_void, off: i32, size: i32) -> *mut c_void {
                let n = if $k % 2 == 1 { len / 2 } else { len };
                let mut buf = [0u8; 256];
                let got = unsafe { sm_read8(st as *mut StreamMemory, buf.as_mut_ptr(), n as u32) };
                log(format!(" r{}:{len}:{got}", $k));
                if size > 0 {
                    for i in 0..got as usize {
                        unsafe { *(o as *mut u8).add(off as usize + i % size as usize) ^= buf[i] };
                    }
                }
                st
            }
            unsafe extern "C" fn w(st: *mut c_void, len: i32, o: *mut c_void, off: i32, size: i32) -> *mut c_void {
                log(format!(" w{}:{len}", $k));
                for i in 0..len {
                    let b = if size != 0 { unsafe { *(o as *mut u8).add((off + i % size) as usize) } } else { $k as u8 };
                    unsafe { stream_write_bytes(st, &[b]) };
                }
                st
            }
            unsafe extern "C" fn g(_: *mut c_void, _: i32, size: i32) -> i32 {
                match $k % 4 {
                    0 => size,
                    1 => -1,
                    2 => 0,
                    _ => size + 3,
                }
            }
            unsafe extern "C" fn rc(_: *mut c_void, _: i32, _: i32, data: u32) {
                log(format!(" R{}:{data}", $k));
            }
            unsafe extern "C" fn ac(_: *mut c_void, _: i32, _: i32) {
                log(format!(" A{}", $k));
            }
            CbSet { c, d, y, r, w, g, rc, ac }
        }};
    }

    struct H(u64);
    impl H {
        fn b(&mut self, b: u8) {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
        fn i(&mut self, v: i32) {
            for k in 0..4 {
                self.b((v >> (8 * k)) as u8);
            }
        }
        fn bytes(&mut self, b: &[u8]) {
            for &x in b {
                self.b(x);
            }
        }
    }

    fn fill_obj(o: &mut [u8], seed: u32) {
        let mut s = seed | 1;
        for b in o.iter_mut() {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            *b = s as u8;
        }
    }
    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    fn mem_stream(data: &mut [u8], len: u32, cap: u32) -> StreamMemory {
        let mut s = StreamMemory { vptr: std::ptr::null(), data: std::ptr::null_mut(), length: 0, capacity: 0, position: 0 };
        unsafe { sm_open(&mut s, data.as_mut_ptr(), len, cap) };
        s
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwplugin.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwplugin.txt");
        unsafe {
            revc_stream_set_hooks(Hooks { read8: h_read8, write8: h_write8, seek: h_seek, tell: h_tell, eof: h_eof, version: h_version, build: h_build });
            pl_open();
        }
        let sets = [cbset!(0), cbset!(1), cbset!(2), cbset!(3), cbset!(4), cbset!(5), cbset!(6), cbset!(7)];
        let defs = [16, 24, 40, 64];
        let mut lists: Vec<Box<PluginList>> = defs
            .iter()
            .map(|&d| {
                let mut l = Box::new(PluginList { size: d, default_size: d, plugins: LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } } });
                unsafe { list_init(&mut l.plugins) };
                l
            })
            .collect();
        let mut memdata = vec![0u8; 1024];
        let mut mem = mem_stream(&mut memdata, 0, 1024);
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let (args, expect) = input.split_once(" =").map_or((input, None), |(a, r)| (a, Some(r.trim().to_string())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let num = |i: usize| w[i].parse::<i64>().unwrap();
            let mut obj = vec![0u8; 1024];
            let mut obj_len = 0usize;
            let ms: *mut c_void = &mut mem as *mut StreamMemory as *mut c_void;
            let res: Option<String> = unsafe {
                let l: *mut PluginList = if w[0].starts_with('m') || w[0] == "find" { std::ptr::null_mut() } else { &mut *lists[num(1) as usize] };
                match w[0] {
                    "reg" => {
                        let (size, id, mode, k) = (num(2) as i32, num(3) as u32, num(4), num(5) as usize);
                        let s = &sets[k];
                        Some(pl_register(l, size, id, (mode & 1 != 0).then_some(s.c), (mode & 2 != 0).then_some(s.d), (mode & 4 != 0).then_some(s.y)).to_string())
                    }
                    "regstream" => {
                        let (id, k, mode) = (num(2) as u32, num(3) as usize, num(4));
                        let s = &sets[k];
                        Some(pl_register_stream(l, id, (mode & 1 != 0).then_some(s.r), (mode & 2 != 0).then_some(s.w), (mode & 4 != 0).then_some(s.g)).to_string())
                    }
                    "setrights" => Some(pl_set_rights(l, num(2) as u32, Some(sets[num(3) as usize].rc)).to_string()),
                    "setalways" => Some(pl_set_always(l, num(2) as u32, Some(sets[num(3) as usize].ac)).to_string()),
                    "getoffset" => Some(pl_get_offset(l, num(2) as u32).to_string()),
                    "construct" | "destruct" | "rights" => {
                        obj_len = (*l).size as usize;
                        fill_obj(&mut obj[..obj_len], num(2) as u32);
                        let o = obj.as_mut_ptr() as *mut c_void;
                        match w[0] {
                            "construct" => pl_construct(l, o),
                            "destruct" => pl_destruct(l, o),
                            _ => pl_assert_rights(l, o, num(3) as u32, num(4) as u32),
                        }
                        None
                    }
                    "copy" => {
                        obj_len = (*l).size as usize;
                        let mut src = vec![0u8; obj_len];
                        fill_obj(&mut obj[..obj_len], num(2) as u32);
                        fill_obj(&mut src, num(3) as u32);
                        pl_copy(l, obj.as_mut_ptr() as _, src.as_mut_ptr() as _);
                        None
                    }
                    "read" => {
                        obj_len = (*l).size as usize;
                        fill_obj(&mut obj[..obj_len], num(2) as u32);
                        let mut b = unhex(w[3]);
                        let n = b.len() as u32;
                        let mut s = mem_stream(&mut b, n, n);
                        let ok = pl_stream_read(l, &mut s as *mut StreamMemory as _, obj.as_mut_ptr() as _);
                        Some(format!("{} {}", ok as i32, s.position))
                    }
                    "skip" => {
                        let mut b = unhex(w[2]);
                        let n = b.len() as u32;
                        let mut s = mem_stream(&mut b, n, n);
                        pl_stream_skip(&mut s as *mut StreamMemory as _);
                        Some(s.position.to_string())
                    }
                    "write" => {
                        obj_len = (*l).size as usize;
                        fill_obj(&mut obj[..obj_len], num(2) as u32);
                        let gs = pl_stream_get_size(l, obj.as_mut_ptr() as _);
                        let mut b = vec![0u8; 1024];
                        let mut s = mem_stream(&mut b, 0, 1024);
                        pl_stream_write(l, &mut s as *mut StreamMemory as _, obj.as_mut_ptr() as _);
                        Some(format!("{gs} {} {}", s.length, hex(&b[..s.length as usize])))
                    }
                    "mopen" => {
                        let (len, cap, seed) = (num(1) as u32, num(2) as u32, num(3) as u32);
                        let mut s = seed | 1;
                        for b in memdata.iter_mut() {
                            s ^= s << 13;
                            s ^= s >> 17;
                            s ^= s << 5;
                            *b = s as u8;
                        }
                        mem = mem_stream(&mut memdata, len, cap);
                        None
                    }
                    "mwrite8" => {
                        let b = if w.len() > 2 { unhex(w[2]) } else { vec![] };
                        Some(sm_write8(&mut mem, b.as_ptr(), num(1) as u32).to_string())
                    }
                    "mread8" => {
                        let mut b = [0u8; 32];
                        let r = sm_read8(&mut mem, b.as_mut_ptr(), num(1) as u32);
                        Some(format!("{r} {}", hex(&b[..r as usize])).trim_end().to_string())
                    }
                    "mseek" => {
                        sm_seek(&mut mem, num(1) as i32, num(2) as i32);
                        None
                    }
                    "mtell" => Some(mem.position.to_string()),
                    "meof" => Some(((mem.position == S_EOF) as i32).to_string()),
                    "mlen" => Some(mem.length.to_string()),
                    "mri8" => Some((stream_read_bytes::<1>(ms)[0] as i8).to_string()),
                    "mru8" => Some(stream_read_bytes::<1>(ms)[0].to_string()),
                    "mri16" => Some(i16::from_le_bytes(stream_read_bytes::<2>(ms)).to_string()),
                    "mru16" => Some(u16::from_le_bytes(stream_read_bytes::<2>(ms)).to_string()),
                    "mri32" => Some(i32::from_le_bytes(stream_read_bytes::<4>(ms)).to_string()),
                    "mru32" => Some(u32::from_le_bytes(stream_read_bytes::<4>(ms)).to_string()),
                    "mrf32" => Some(format!("{:08x}", u32::from_le_bytes(stream_read_bytes::<4>(ms)))),
                    "mwi8" | "mwu8" => Some(stream_write_bytes(ms, &[(num(1) as u32) as u8]).to_string()),
                    "mwi16" | "mwu16" => Some(stream_write_bytes(ms, &((num(1) as u32) as u16).to_le_bytes()).to_string()),
                    "mwi32" | "mwu32" | "mwf32" => Some(stream_write_bytes(ms, &(num(1) as u32).to_le_bytes()).to_string()),
                    "mread32" | "mread16" => {
                        let mut b = [0u8; 8];
                        let r = stream_read32(ms, b.as_mut_ptr(), num(1) as u32);
                        Some(format!("{r} {}", hex(&b[..r as usize])))
                    }
                    "mchunkw" => {
                        VERSION = num(1) as i32;
                        BUILD = num(2) as i32;
                        Some((write_chunk_header(ms, num(3) as u32 as i32, num(4) as i32) as i32).to_string())
                    }
                    "mchunkr" => {
                        let mut h = ChunkHeaderInfo::default();
                        let ok = read_chunk_header_info(ms, &mut h);
                        Some(format!("{} {} {} {} {}", ok as i32, h.kind, h.length, h.version, h.build))
                    }
                    "find" => {
                        let mut b = unhex(w[2]);
                        let nb = b.len() as u32;
                        let mut s = mem_stream(&mut b, nb, nb);
                        let (mut len, mut ver) = (7u32, 7u32);
                        let ok = find_chunk(&mut s as *mut StreamMemory as _, num(1) as u32, &mut len, &mut ver);
                        Some(format!("{} {len} {ver} {}", ok as i32, s.position))
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            if res != expect && bad.len() < 6 {
                bad.push(format!("line {} {}: result {res:?} want {expect:?}", n + 1, &input[..input.len().min(120)]));
            }
            // state: the lists, the object, the memory stream
            let mut h = H(0xcbf29ce484222325);
            for l in lists.iter_mut() {
                h.i(l.size);
                h.i(l.default_size);
                unsafe {
                    plugins(&mut **l, |p| {
                        for v in [p.offset, p.size, p.id as i32, p.read.is_some() as i32, p.write.is_some() as i32, p.get_size.is_some() as i32, p.rights_callback.is_some() as i32, p.always_callback.is_some() as i32] {
                            h.i(v);
                        }
                        true
                    });
                }
            }
            h.bytes(&obj[..obj_len]);
            h.i(mem.length as i32);
            h.i(mem.capacity as i32);
            h.i(mem.position as i32);
            h.bytes(&memdata);
            let lg = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            let got = format!(" {:016x} |{lg}", h.0);
            n += 1;
            if got != want && bad.len() < 6 {
                bad.push(format!("line {n} {}\n  want{want}\n  got {got}", &input[..input.len().min(120)]));
            }
        }
        assert!(n > 5000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

// ---------------------------------------------------------------- the class plugin lists (7d2)

/// Indices of the classes' plugin lists and allocation counters (rwplg.h REVC_CLASS_*); the
/// modules use them as their hooks move to Rust.
#[allow(dead_code)]
pub const CLASS_FRAME: usize = 0;
pub const CLASS_RASTER: usize = 1;
pub const CLASS_TEXTURE: usize = 2;
pub const CLASS_TEXDICTIONARY: usize = 3;
pub const CLASS_GEOMETRY: usize = 4;
pub const CLASS_MATERIAL: usize = 5;
pub const CLASS_ATOMIC: usize = 6;
pub const CLASS_CLUMP: usize = 7;
pub const CLASS_LIGHT: usize = 8;
pub const CLASS_CAMERA: usize = 9;
pub const CLASS_WORLD: usize = 10;
pub const CLASS_ENGINE: usize = 11;
/// Image has a counter but no plugin list.
pub const CLASS_IMAGE: usize = 12;
pub const NUM_CLASS_LISTS: usize = 12;
pub const NUM_PLATFORMS: usize = 14;

const EMPTY_LIST: PluginList = PluginList { size: 0, default_size: 0, plugins: LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } } };

/// Each class's PluginList (librw's `Class::s_plglist`; the C++ headers refer to these).
#[unsafe(no_mangle)]
pub static mut revc_plugin_lists: [PluginList; NUM_CLASS_LISTS] = [EMPTY_LIST; NUM_CLASS_LISTS];
/// Driver::s_plglist, one per platform.
#[unsafe(no_mangle)]
pub static mut revc_driver_plugin_lists: [PluginList; NUM_PLATFORMS] = [EMPTY_LIST; NUM_PLATFORMS];
/// Each class's numAllocated.
#[unsafe(no_mangle)]
pub static mut revc_alloc_counts: [i32; NUM_CLASS_LISTS + 1] = [0; NUM_CLASS_LISTS + 1];

/// The plugin list of class `i` (CLASS_*).
pub fn class_list(i: usize) -> *mut PluginList {
    unsafe { std::ptr::addr_of_mut!(revc_plugin_lists[i]) }
}
/// The driver plugin list of platform `i`.
pub fn driver_list(i: usize) -> *mut PluginList {
    unsafe { std::ptr::addr_of_mut!(revc_driver_plugin_lists[i]) }
}
/// numAllocated of class `i` (CLASS_*, or CLASS_IMAGE).
pub fn alloc_count(i: usize) -> *mut i32 {
    unsafe { std::ptr::addr_of_mut!(revc_alloc_counts[i]) }
}

/// The classes' sizes, which their plugin lists start from (librw's `s_plglist(sizeof(Class))`).
pub fn class_sizes() -> [i32; NUM_CLASS_LISTS] {
    [
        size_of::<crate::rwframe::Frame>() as i32,
        size_of::<crate::rwraster::Raster>() as i32,
        size_of::<crate::rwtexture::Texture>() as i32,
        size_of::<crate::rwtexture::TexDictionary>() as i32,
        size_of::<crate::rwgeomobj::Geometry>() as i32,
        size_of::<crate::rwgeomobj::Material>() as i32,
        size_of::<crate::rwscene::Atomic>() as i32,
        size_of::<crate::rwscene::Clump>() as i32,
        size_of::<crate::rwscene::Light>() as i32,
        size_of::<crate::rwscene::Camera>() as i32,
        size_of::<crate::rwscene::World>() as i32,
        size_of::<crate::rwengine::Engine>() as i32,
    ]
}

/// Sets up a list never used yet (librw built them as static objects): empty, at its default
/// size. Lists already set up keep their plugins.
unsafe fn init_list(l: *mut PluginList, size: i32) {
    unsafe {
        if (*l).plugins.link.next.is_null() {
            (*l).size = size;
            (*l).default_size = size;
            list_init(&mut (*l).plugins);
        }
    }
}

/// Sets up the class and driver plugin lists (Engine::init, before any registration).
///
/// # Safety
/// Not while plugins are being used.
pub unsafe fn init_class_lists() {
    unsafe {
        for (i, size) in class_sizes().into_iter().enumerate() {
            init_list(class_list(i), size);
        }
        for i in 0..NUM_PLATFORMS {
            init_list(driver_list(i), size_of::<crate::rwengine::Driver>() as i32);
        }
    }
}

const _: () = assert!(size_of::<crate::rwframe::Frame>() == 200 && size_of::<crate::rwraster::Raster>() == 96 && size_of::<crate::rwtexture::Texture>() == 120);

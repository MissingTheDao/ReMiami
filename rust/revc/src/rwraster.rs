//! librw's rasters (vendor/librw/src/raster.cpp) and revc's wgpu raster driver
//! (vendor/librw/src/wgpu/wgpuraster.cpp), stage 2 step 5c: raster creation and destruction,
//! sub-rasters, the level count and alpha helpers, the context stack; and the driver's raster
//! storage (RGBA8 levels or DXT blocks), level locking, image to raster and back (DXT through
//! rwimage), DXT allocation as TXD loading uses it, and the upload bookkeeping. The C++ objects
//! keep their layouts; plugin data, memory, images and the host calls go through hooks.
//!
//! Faithful to the C++, with three of its undefined cases made safe: setting an image into a
//! texture raster that has no levels (made with DONTALLOCATE) and isn't locked does nothing
//! (the driver wrote through a null pointer); copies to and from a locked mip level of an odd
//! width stop at the level's end (rasterLock halves width and stride separately, so the stride
//! no longer matches); and the temporary unpalettized image in rasterFromImage is freed (the
//! driver leaked it). None of these happen in the game.
//! Checked against tests/golden/rwraster.txt, recorded from the C++ (tests/golden/recorders/rwraster).


use crate::rwimage::{self, Image};
use std::ffi::c_void;

// Raster types, flags, formats, lock modes (rwobjects.h)
pub const NORMAL: i32 = 0x00;
pub const ZBUFFER: i32 = 0x01;
pub const CAMERA: i32 = 0x02;
pub const TEXTURE: i32 = 0x04;
pub const CAMERATEXTURE: i32 = 0x05;
pub const DONTALLOCATE: i32 = 0x80;
pub const C1555: i32 = 0x0100;
pub const C565: i32 = 0x0200;
pub const C4444: i32 = 0x0300;
pub const LUM8: i32 = 0x0400;
pub const C8888: i32 = 0x0500;
pub const C888: i32 = 0x0600;
pub const C555: i32 = 0x0A00;
pub const AUTOMIPMAP: i32 = 0x1000;
pub const PAL8: i32 = 0x2000;
pub const PAL4: i32 = 0x4000;
pub const MIPMAP: i32 = 0x8000;
pub const LOCKWRITE: i32 = 1;
pub const LOCKREAD: i32 = 2;
pub const LOCKNOFETCH: i32 = 4;

/// rw::Raster (plugin data follows it in memory).
#[repr(C)]
pub struct Raster {
    pub platform: i32,
    pub kind: i32,
    pub flags: i32,
    pub private_flags: i32,
    pub format: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub stride: i32,
    pub pixels: *mut u8,
    pub palette: *mut u8,
    pub original_pixels: *mut u8,
    pub original_width: i32,
    pub original_height: i32,
    pub original_stride: i32,
    pub parent: *mut Raster,
    pub offset_x: i32,
    pub offset_y: i32,
}

/// rw::wgpu::WgpuRaster (the driver's plugin data).
#[repr(C)]
pub struct WgpuRaster {
    pub num_levels: i32,
    pub levels: [*mut u8; 16],
    pub has_alpha: i32,
    pub base_width: i32,
    pub base_height: i32,
    pub dxt: i32,
    pub tex_id: u32,
    pub dirty: i32,
    pub autogen_mipmap: i32,
    pub max_anisotropy: i32,
    pub filter_mode: i32,
    pub address_u: i32,
    pub address_v: i32,
}

/// rw::Rect
#[repr(C)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// raster.cpp's RasterGlobals (the context stack).
#[repr(C)]
pub struct RasterGlobals {
    pub sp: i32,
    pub stack: [*mut Raster; 32],
}

pub type UploadFn = unsafe extern "C" fn(id: u32, w: i32, h: i32, dxt: i32, n: i32, levels: *const *const u8, autogen: i32) -> u32;

/// What stays outside: plugin lists, librw's allocator, the device and the host's textures.
#[repr(C)]
pub struct Hooks {
    /// Raster::s_plglist.size
    pub size: unsafe extern "C" fn() -> usize,
    pub construct: unsafe extern "C" fn(*mut Raster),
    pub destruct: unsafe extern "C" fn(*mut Raster),
    /// Raster::numAllocated += delta
    pub allocated: unsafe extern "C" fn(delta: i32),
    /// rwMalloc(size, MEMDUR_EVENT | ID_DRIVER)
    pub alloc: unsafe extern "C" fn(size: usize) -> *mut u8,
    /// rwFree
    pub free: unsafe extern "C" fn(*mut u8),
    /// wgpu::evictRaster (unbind from the device)
    pub evict: unsafe extern "C" fn(*mut Raster),
    /// the host's texture upload (revc_rw_texture_upload)
    pub upload: UploadFn,
    /// the host's texture release (revc_rw_texture_destroy)
    pub texture_destroy: unsafe extern "C" fn(id: u32),
    /// wgpu::nativeRasterOffset
    pub native_offset: i32,
}

static mut HOOKS: Option<Hooks> = None;

// ---------------------------------------------------------------- the driver's raster plugin and the module (7d2)

/// wgpu::nativeRasterOffset (the WgpuRaster in every raster)
#[unsafe(no_mangle)]
pub static mut revc_native_raster_offset: i32 = 0;

mod defaults {
    use super::*;
    use crate::rwplugin::{self as pl, CLASS_RASTER, class_list};
    pub(super) unsafe extern "C" fn size() -> usize {
        unsafe { (*class_list(CLASS_RASTER)).size as usize }
    }
    pub(super) unsafe extern "C" fn construct(r: *mut Raster) {
        unsafe { pl::pl_construct(class_list(CLASS_RASTER), r as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn destruct(r: *mut Raster) {
        unsafe { pl::pl_destruct(class_list(CLASS_RASTER), r as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn allocated(d: i32) {
        unsafe { *pl::alloc_count(CLASS_RASTER) += d }
    }
    pub(super) unsafe extern "C" fn alloc(size: usize) -> *mut u8 {
        unsafe { crate::rwengine::rw_malloc(size, 0x30000 | 0xB00) as *mut u8 } // MEMDUR_EVENT | ID_DRIVER
    }
    pub(super) unsafe extern "C" fn free(p: *mut u8) {
        unsafe { crate::rwengine::rw_free(p as *mut c_void) }
    }
    pub(super) unsafe extern "C" fn evict(r: *mut Raster) {
        unsafe { crate::rwdevice::evict_raster(r) }
    }
    pub(super) unsafe extern "C" fn native_create(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
        unsafe { native_clear(o as *mut Raster) };
        o
    }
    pub(super) unsafe extern "C" fn native_destroy_cb(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
        unsafe { native_destroy(o as *mut Raster) };
        o
    }
    /// a copied raster object starts without its own pixel storage
    pub(super) unsafe extern "C" fn native_copy(dst: *mut c_void, _: *mut c_void, _: i32, _: i32) -> *mut c_void {
        unsafe { native_clear(dst as *mut Raster) };
        dst
    }
}
const ID_RASTERWGPU: u32 = 0xA0D;

/// wgpu::registerNativeRaster: the WgpuRaster plugin, then the hooks with its offset.
///
/// # Safety
/// During Engine::init (driver plugin attach).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_wgpu_register_native_raster() {
    unsafe {
        let off = crate::rwplugin::pl_register(
            crate::rwplugin::class_list(crate::rwplugin::CLASS_RASTER),
            size_of::<WgpuRaster>() as i32,
            ID_RASTERWGPU,
            Some(defaults::native_create),
            Some(defaults::native_destroy_cb),
            Some(defaults::native_copy),
        );
        *std::ptr::addr_of_mut!(revc_native_raster_offset) = off;
        *std::ptr::addr_of_mut!(HOOKS) = Some(Hooks {
            size: defaults::size,
            construct: defaults::construct,
            destruct: defaults::destruct,
            allocated: defaults::allocated,
            alloc: defaults::alloc,
            free: defaults::free,
            evict: defaults::evict,
            upload: crate::host::wgpu::revc_rw_texture_upload,
            texture_destroy: crate::host::wgpu::revc_rw_texture_destroy,
            native_offset: off,
        });
    }
}

/// The raster module's offset in the engine (its RasterGlobals: the context stack).
static mut RASTER_MODULE_OFFSET: i32 = 0;
unsafe extern "C" fn raster_module_open(o: *mut c_void, off: i32, _: i32) -> *mut c_void {
    unsafe {
        *std::ptr::addr_of_mut!(RASTER_MODULE_OFFSET) = off;
        let g = (o as *mut u8).add(off as usize) as *mut RasterGlobals;
        (*g).sp = -1;
        (*g).stack = [std::ptr::null_mut(); 32];
    }
    o
}
unsafe extern "C" fn raster_module_close(o: *mut c_void, _: i32, _: i32) -> *mut c_void {
    o
}
const ID_RASTERMODULE: u32 = 0x407;

/// Raster::registerModule
///
/// # Safety
/// During Engine::init.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_raster_register_module() {
    unsafe {
        crate::rwplugin::pl_register(
            crate::rwplugin::class_list(crate::rwplugin::CLASS_ENGINE),
            size_of::<RasterGlobals>() as i32,
            ID_RASTERMODULE,
            Some(raster_module_open),
            Some(raster_module_close),
            None,
        );
    }
}

/// The context stack (RASTERGLOBALS) of the running engine.
#[unsafe(no_mangle)]
pub extern "C" fn revc_raster_globals() -> *mut RasterGlobals {
    unsafe { (*std::ptr::addr_of!(crate::rwengine::revc_engine) as *mut u8).add(*std::ptr::addr_of!(RASTER_MODULE_OFFSET) as usize) as *mut RasterGlobals }
}

fn hooks() -> &'static Hooks {
    // SAFETY: set once at startup (driver registration), read-only afterwards.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().expect("revc_raster_set_hooks not called") }
}

/// GETWGPURASTEREXT
pub unsafe fn natras(r: *mut Raster) -> *mut WgpuRaster {
    unsafe { (r as *mut u8).offset(hooks().native_offset as isize) as *mut WgpuRaster }
}

fn layout(size: usize) -> std::alloc::Layout {
    std::alloc::Layout::from_size_align(size.max(size_of::<Raster>()), 16).unwrap()
}

// ---------------------------------------------------------------- raster.cpp

/// Raster::create's object (the driver's rasterCreate follows, in C++'s dispatch).
///
/// # Safety
/// Hooks set.
pub unsafe fn create(width: i32, height: i32, depth: i32, format: i32, platform: i32) -> *mut Raster {
    unsafe {
        let h = hooks();
        let r = std::alloc::alloc(layout((h.size)())) as *mut Raster;
        assert!(!r.is_null());
        (h.allocated)(1);
        std::ptr::write(
            r,
            Raster {
                platform,
                kind: format & 0x7,
                flags: format & 0xF8,
                private_flags: 0,
                format: format & 0xFF00,
                width,
                height,
                depth,
                stride: 0,
                pixels: std::ptr::null_mut(),
                palette: std::ptr::null_mut(),
                original_pixels: std::ptr::null_mut(),
                original_width: 0,
                original_height: 0,
                original_stride: 0,
                parent: r,
                offset_x: 0,
                offset_y: 0,
            },
        );
        (h.construct)(r);
        r
    }
}

/// Raster::destroy
///
/// # Safety
/// A live raster.
pub unsafe fn destroy(r: *mut Raster) {
    unsafe {
        let h = hooks();
        (h.destruct)(r);
        std::alloc::dealloc(r as *mut u8, layout((h.size)()));
        (h.allocated)(-1);
    }
}

/// Raster::subRaster: only rasters made with DONTALLOCATE become windows into another.
///
/// # Safety
/// Live rasters.
pub unsafe fn sub_raster(r: *mut Raster, parent: *mut Raster, rect: &Rect) {
    unsafe {
        if (*r).flags & DONTALLOCATE == 0 {
            return;
        }
        (*r).width = rect.w;
        (*r).height = rect.h;
        (*r).offset_x = (*parent).offset_x + rect.x;
        (*r).offset_y = (*parent).offset_y + rect.y;
        (*r).parent = (*parent).parent;
    }
}

/// Raster::calculateNumLevels: halvings of the bigger side until 0.
pub fn calculate_num_levels(width: i32, height: i32) -> i32 {
    let mut size = width.max(height);
    let mut n = 0;
    while size != 0 {
        size /= 2;
        n += 1;
    }
    n
}

/// Raster::formatHasAlpha
pub fn format_has_alpha(format: i32) -> bool {
    matches!(format & 0xF00, C8888 | C1555 | C4444)
}

/// Raster::pushContext
///
/// # Safety
/// `g`: the raster globals.
pub unsafe fn push_context(g: *mut RasterGlobals, r: *mut Raster) -> *mut Raster {
    unsafe {
        let g = &mut *g;
        if g.sp >= 31 {
            return std::ptr::null_mut();
        }
        g.sp += 1;
        g.stack[g.sp as usize] = r;
        r
    }
}

/// Raster::popContext
///
/// # Safety
/// `g`: the raster globals.
pub unsafe fn pop_context(g: *mut RasterGlobals) -> *mut Raster {
    unsafe {
        let g = &mut *g;
        if g.sp < 0 {
            return std::ptr::null_mut();
        }
        g.sp -= 1;
        g.stack[(g.sp + 1) as usize]
    }
}

/// Raster::getCurrentContext
///
/// # Safety
/// `g`: the raster globals.
pub unsafe fn current_context(g: *const RasterGlobals) -> *mut Raster {
    unsafe {
        let g = &*g;
        if g.sp < 0 || g.sp >= 32 { std::ptr::null_mut() } else { g.stack[g.sp as usize] }
    }
}

// ---------------------------------------------------------------- the wgpu driver

fn level_count(r: &Raster) -> i32 {
    if r.format & (MIPMAP | AUTOMIPMAP) != 0 { calculate_num_levels(r.width, r.height) } else { 1 }
}

/// The size of a DXT level in bytes (whole 4x4 blocks).
pub fn dxt_level_size(dxt: i32, width: i32, height: i32) -> i32 {
    let bw = ((width + 3) / 4).max(1);
    let bh = ((height + 3) / 4).max(1);
    bw * bh * if dxt == 1 { 8 } else { 16 }
}

unsafe fn alloc_zeroed(size: usize) -> *mut u8 {
    unsafe {
        let p = (hooks().alloc)(size);
        std::ptr::write_bytes(p, 0, size);
        p
    }
}

/// wgpu::rasterCreate: textures are RGBA8 whatever was asked for (levels allocated unless
/// DONTALLOCATE; one with automatic mipmaps); the camera and z-buffer live in the host.
/// Null for unknown types.
///
/// # Safety
/// A raster from create.
pub unsafe fn wgpu_raster_create(r: *mut Raster) -> *mut Raster {
    unsafe {
        let n = &mut *natras(r);
        n.num_levels = 0;
        n.has_alpha = 0;
        n.levels = [std::ptr::null_mut(); 16];
        n.tex_id = 0;
        n.dirty = 0;
        n.autogen_mipmap = 0;
        n.dxt = 0;
        n.max_anisotropy = 1;
        n.filter_mode = 0;
        n.address_u = 0;
        n.address_v = 0;
        let ras = &mut *r;
        match ras.kind {
            NORMAL | TEXTURE | CAMERATEXTURE => {
                n.has_alpha = !matches!(ras.format & 0xF00, C888 | C565 | C555 | LUM8) as i32;
                n.autogen_mipmap = (ras.format & (MIPMAP | AUTOMIPMAP) == MIPMAP | AUTOMIPMAP) as i32;
                ras.format = (ras.format & !PAL8 & !PAL4 & !0xF00) | C8888;
                ras.depth = 32;
                ras.stride = ras.width * 4;
                n.base_width = ras.width;
                n.base_height = ras.height;
                if ras.flags & DONTALLOCATE != 0 {
                    return r;
                }
                n.num_levels = if n.autogen_mipmap != 0 { 1 } else { level_count(ras) };
                for i in 0..n.num_levels as usize {
                    let w = (ras.width >> i).max(1);
                    let h = (ras.height >> i).max(1);
                    n.levels[i] = alloc_zeroed((w * h * 4) as usize);
                }
                r
            }
            CAMERA | ZBUFFER => {
                ras.depth = 32;
                ras.stride = 0;
                r
            }
            _ => std::ptr::null_mut(),
        }
    }
}

/// wgpu::rasterLock: the level's pixels; the raster reports the level's size until unlock.
///
/// # Safety
/// A live, unlocked raster.
pub unsafe fn wgpu_raster_lock(r: *mut Raster, level: i32, lock_mode: i32) -> *mut u8 {
    unsafe {
        let n = &*natras(r);
        let ras = &mut *r;
        debug_assert!(ras.private_flags == 0);
        match ras.kind {
            NORMAL | TEXTURE | CAMERATEXTURE => {
                if level >= n.num_levels {
                    return std::ptr::null_mut();
                }
                for _ in 0..level {
                    if ras.width > 1 {
                        ras.width /= 2;
                        ras.stride /= 2;
                    }
                    if ras.height > 1 {
                        ras.height /= 2;
                    }
                }
                ras.pixels = n.levels[level as usize];
                ras.private_flags = lock_mode;
                ras.pixels
            }
            _ => std::ptr::null_mut(),
        }
    }
}

/// wgpu::rasterUnlock
///
/// # Safety
/// A live raster.
pub unsafe fn wgpu_raster_unlock(r: *mut Raster) {
    unsafe {
        let n = &mut *natras(r);
        let ras = &mut *r;
        if ras.private_flags & LOCKWRITE != 0 {
            n.dirty = 1;
        }
        ras.width = n.base_width;
        ras.height = n.base_height;
        ras.stride = ras.width * 4;
        ras.pixels = std::ptr::null_mut();
        ras.private_flags = 0;
    }
}

/// wgpu::imageFindRasterFormat: textures only, RGBA8 at the image's size.
pub fn image_find_raster_format(img: &Image, kind: i32) -> Option<(i32, i32, i32, i32)> {
    if kind & 0xF != TEXTURE {
        return None;
    }
    Some((img.width, img.height, 32, C8888 | kind))
}

/// The bytes of the locked (or base) level the copies may touch.
unsafe fn level_len(r: *mut Raster) -> usize {
    unsafe {
        let n = &*natras(r);
        let p = (*r).pixels;
        (0..n.num_levels as usize)
            .find(|&i| n.levels[i] == p)
            .map(|i| {
                let w = (n.base_width >> i).max(1);
                let h = (n.base_height >> i).max(1);
                (w * h * 4) as usize
            })
            .unwrap_or(0)
    }
}

/// wgpu::rasterFromImage: the image (any depth) into the raster's current level as RGBA8.
/// 1 done, 0 not a texture, -1 a DXT raster (librw reports ERR_INVRASTER).
///
/// # Safety
/// A live raster and a valid image.
pub unsafe fn wgpu_raster_from_image(r: *mut Raster, image: *mut Image) -> i32 {
    unsafe {
        if (*r).kind & 0xF != TEXTURE {
            return 0;
        }
        if (*natras(r)).dxt != 0 {
            return -1;
        }
        let ih = rwimage::hooks();
        let mut src = image;
        let mut tmp: *mut Image = std::ptr::null_mut();
        if (*image).depth <= 8 {
            tmp = (ih.create)((*image).width, (*image).height, (*image).depth);
            (*tmp).pixels = (*image).pixels;
            (*tmp).stride = (*image).stride;
            (*tmp).palette = (*image).palette;
            rwimage::unpalettize(tmp, false);
            src = tmp;
        }
        let mut unlock = false;
        if (*r).pixels.is_null() {
            wgpu_raster_lock(r, 0, LOCKWRITE | LOCKNOFETCH);
            unlock = true;
        }
        if !(*r).pixels.is_null() {
            (*natras(r)).has_alpha = rwimage::has_alpha(src) as i32;
            let s = &*src;
            let ras = &*r;
            let w = ras.width.min(s.width);
            let h = ras.height.min(s.height);
            let len = level_len(r) as isize;
            for y in 0..h {
                let mut inp = s.pixels.offset((y * s.stride) as isize);
                let mut out = (y * ras.stride) as isize;
                for _ in 0..w {
                    let px: [u8; 4] = match s.depth {
                        32 => {
                            let v = [*inp, *inp.add(1), *inp.add(2), *inp.add(3)];
                            inp = inp.add(4);
                            v
                        }
                        24 => {
                            let v = [*inp, *inp.add(1), *inp.add(2), 0xFF];
                            inp = inp.add(3);
                            v
                        }
                        16 => {
                            let v = *inp as u32 | (*inp.add(1) as u32) << 8;
                            inp = inp.add(2);
                            [(((v >> 10) & 0x1F) * 255 / 31) as u8, (((v >> 5) & 0x1F) * 255 / 31) as u8, ((v & 0x1F) * 255 / 31) as u8, if v & 0x8000 != 0 { 0xFF } else { 0 }]
                        }
                        _ => {
                            out += 4;
                            continue;
                        }
                    };
                    if out + 4 <= len {
                        std::ptr::copy_nonoverlapping(px.as_ptr(), ras.pixels.offset(out), 4);
                    }
                    out += 4;
                }
            }
        }
        if unlock {
            wgpu_raster_unlock(r);
        }
        if !tmp.is_null() {
            // the unpalettized pixels are tmp's own; the source image's palette isn't
            if (*tmp).flags & 1 == 0 {
                (*tmp).pixels = std::ptr::null_mut();
            }
            (*tmp).palette = std::ptr::null_mut();
            (*tmp).flags &= 1;
            (ih.destroy)(tmp);
        }
        1
    }
}

/// wgpu::rasterToImage: the current (or base) level as a 32-bit image, DXT decompressed.
///
/// # Safety
/// A live raster.
pub unsafe fn wgpu_raster_to_image(r: *mut Raster) -> *mut Image {
    unsafe {
        let n = &*natras(r);
        if n.num_levels == 0 {
            return std::ptr::null_mut();
        }
        let mut unlock = false;
        if (*r).pixels.is_null() {
            wgpu_raster_lock(r, 0, LOCKREAD);
            unlock = true;
        }
        let ih = rwimage::hooks();
        let ras = &*r;
        let image = (ih.create)(ras.width, ras.height, 32);
        (ih.allocate)(image);
        if n.dxt != 0 {
            rwimage::set_pixels_dxt(image, n.dxt, ras.pixels);
        } else {
            let len = level_len(r) as isize;
            let row = (ras.width * 4) as isize;
            for y in 0..ras.height {
                let from = (y * ras.stride) as isize;
                let take = row.min(len - from).max(0);
                std::ptr::copy_nonoverlapping(ras.pixels.offset(from), (*image).pixels.offset((y * (*image).stride) as isize), take as usize);
            }
        }
        if unlock {
            wgpu_raster_unlock(r);
        }
        image
    }
}

/// wgpu::allocateDXT: DXT levels for a texture raster made without levels (TXD loading).
///
/// # Safety
/// A live texture raster without levels.
pub unsafe fn allocate_dxt(r: *mut Raster, dxt: i32, num_levels: i32, has_alpha: bool) {
    unsafe {
        let n = &mut *natras(r);
        let ras = &mut *r;
        debug_assert!(n.num_levels == 0);
        n.dxt = dxt;
        n.has_alpha = has_alpha as i32;
        ras.stride = if dxt == 1 { ras.width / 2 } else { ras.width };
        ras.depth = 16;
        n.num_levels = if ras.format & MIPMAP != 0 { num_levels } else { 1 };
        n.autogen_mipmap = 0;
        if ras.format & (MIPMAP | AUTOMIPMAP) == MIPMAP | AUTOMIPMAP {
            n.num_levels = 1;
        }
        n.num_levels = n.num_levels.min(16);
        for i in 0..n.num_levels as usize {
            let w = (ras.width >> i).max(1);
            let h = (ras.height >> i).max(1);
            n.levels[i] = alloc_zeroed(dxt_level_size(dxt, w, h) as usize);
        }
        n.dirty = 1;
    }
}

/// wgpu::uploadRaster: the host texture id, uploading first when new or written.
///
/// # Safety
/// A live raster.
pub unsafe fn upload_raster(r: *mut Raster) -> u32 {
    unsafe {
        let n = &mut *natras(r);
        if n.num_levels == 0 {
            return 0;
        }
        if n.tex_id == 0 || n.dirty != 0 {
            let id = (hooks().upload)(n.tex_id, n.base_width, n.base_height, n.dxt, n.num_levels, n.levels.as_ptr() as *const *const u8, n.autogen_mipmap);
            if id != 0 {
                n.tex_id = id;
                n.dirty = 0;
            }
        }
        n.tex_id
    }
}

/// The native raster plugin's constructor and copy constructor (no storage of its own yet).
///
/// # Safety
/// A raster.
pub unsafe fn native_clear(r: *mut Raster) {
    unsafe { std::ptr::write_bytes(natras(r), 0, 1) }
}

/// The native raster plugin's destructor: unbinds, releases the host texture and the levels.
///
/// # Safety
/// A raster being destroyed.
pub unsafe fn native_destroy(r: *mut Raster) {
    unsafe {
        let h = hooks();
        (h.evict)(r);
        let n = &mut *natras(r);
        if n.tex_id != 0 {
            (h.texture_destroy)(n.tex_id);
        }
        for i in 0..n.num_levels as usize {
            (h.free)(n.levels[i]);
        }
        native_clear(r);
    }
}

// ---------------------------------------------------------------- C ABI

/// # Safety
/// Called once at driver registration, before any raster exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_raster_set_hooks(h: Hooks) {
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
    revc_raster_create(w: i32, h: i32, depth: i32, format: i32, platform: i32) -> *mut Raster => create(w, h, depth, format, platform);
    revc_raster_destroy(r: *mut Raster) => destroy(r);
    revc_raster_sub(r: *mut Raster, parent: *mut Raster, rect: *const Rect) => sub_raster(r, parent, &*rect);
    revc_raster_push_context(g: *mut RasterGlobals, r: *mut Raster) -> *mut Raster => push_context(g, r);
    revc_raster_pop_context(g: *mut RasterGlobals) -> *mut Raster => pop_context(g);
    revc_raster_current_context(g: *const RasterGlobals) -> *mut Raster => current_context(g);
    revc_wgpu_raster_create(r: *mut Raster) -> *mut Raster => wgpu_raster_create(r);
    revc_wgpu_raster_lock(r: *mut Raster, level: i32, mode: i32) -> *mut u8 => wgpu_raster_lock(r, level, mode);
    revc_wgpu_raster_unlock(r: *mut Raster) => wgpu_raster_unlock(r);
    revc_wgpu_raster_from_image(r: *mut Raster, img: *mut Image) -> i32 => wgpu_raster_from_image(r, img);
    revc_wgpu_raster_to_image(r: *mut Raster) -> *mut Image => wgpu_raster_to_image(r);
    revc_wgpu_allocate_dxt(r: *mut Raster, dxt: i32, n: i32, has_alpha: i32) => allocate_dxt(r, dxt, n, has_alpha != 0);
    revc_wgpu_upload_raster(r: *mut Raster) -> u32 => upload_raster(r);
    revc_wgpu_native_clear(r: *mut Raster) => native_clear(r);
    revc_wgpu_native_destroy(r: *mut Raster) => native_destroy(r);
}

/// # Safety
/// None.
#[unsafe(no_mangle)]
pub extern "C" fn revc_raster_calculate_num_levels(w: i32, h: i32) -> i32 {
    calculate_num_levels(w, h)
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_raster_format_has_alpha(format: i32) -> i32 {
    format_has_alpha(format) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_wgpu_dxt_level_size(dxt: i32, w: i32, h: i32) -> i32 {
    dxt_level_size(dxt, w, h)
}

/// # Safety
/// A valid image; the outputs writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_wgpu_image_find_raster_format(img: *const Image, kind: i32, w: *mut i32, h: *mut i32, depth: *mut i32, format: *mut i32) -> i32 {
    unsafe {
        match image_find_raster_format(&*img, kind) {
            Some((a, b, c, d)) => {
                *w = a;
                *h = b;
                *depth = c;
                *format = d;
                1
            }
            None => 0,
        }
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwraster.txt (recorded from the C++, see the recorder).
    use super::*;
    use crate::rwimage::testkit::{Gen, build, test_hooks};
    use std::cell::RefCell;

    const PLATFORM_WGPU: i32 = 13;

    thread_local! {
        static RAS: RefCell<Vec<*mut Raster>> = const { RefCell::new(Vec::new()) };
        static LOG: RefCell<String> = const { RefCell::new(String::new()) };
        static NEXT_TEX: RefCell<u32> = const { RefCell::new(1) };
    }
    fn log(s: String) {
        LOG.with(|l| l.borrow_mut().push_str(&s));
    }
    fn rid(r: *mut Raster) -> i64 {
        if r.is_null() {
            return -1;
        }
        RAS.with(|v| v.borrow().iter().position(|&q| q == r).map_or(-2, |i| i as i64))
    }

    unsafe extern "C" fn size() -> usize {
        size_of::<Raster>() + size_of::<WgpuRaster>()
    }
    unsafe extern "C" fn construct(r: *mut Raster) {
        let n = RAS.with(|v| {
            v.borrow_mut().push(r);
            v.borrow().len() - 1
        });
        log(format!(" c{n}"));
        unsafe { native_clear(r) };
    }
    unsafe extern "C" fn destruct(r: *mut Raster) {
        let i = rid(r);
        log(format!(" d{i}"));
        unsafe { native_destroy(r) };
        if i >= 0 {
            RAS.with(|v| v.borrow_mut()[i as usize] = std::ptr::null_mut());
        }
    }
    unsafe extern "C" fn allocated(_: i32) {}
    unsafe extern "C" fn alloc(n: usize) -> *mut u8 {
        Box::leak(vec![0u8; n.max(1)].into_boxed_slice()).as_mut_ptr()
    }
    unsafe extern "C" fn free(_: *mut u8) {}
    unsafe extern "C" fn evict(r: *mut Raster) {
        log(format!(" evict{}", rid(r)));
    }
    unsafe extern "C" fn upload(id: u32, w: i32, h: i32, dxt: i32, n: i32, _: *const *const u8, autogen: i32) -> u32 {
        log(format!(" up{id}:{w}x{h},{dxt},{n},{autogen}"));
        if id != 0 {
            return id;
        }
        NEXT_TEX.with(|t| {
            let v = *t.borrow();
            *t.borrow_mut() += 1;
            v
        })
    }
    unsafe extern "C" fn texture_destroy(id: u32) {
        log(format!(" tdel{id}"));
    }

    struct H(u64);
    impl H {
        fn b(&mut self, b: u8) {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
        fn i(&mut self, v: i64) {
            for k in 0..4 {
                self.b((v >> (8 * k)) as u8);
            }
        }
    }

    fn level_size(n: &WgpuRaster, i: usize) -> i32 {
        let w = (n.base_width >> i).max(1);
        let h = (n.base_height >> i).max(1);
        if n.dxt != 0 { dxt_level_size(n.dxt, w, h) } else { w * h * 4 }
    }

    unsafe fn state(g: *const RasterGlobals) -> String {
        unsafe {
            let mut h = H(0xcbf29ce484222325);
            let all = RAS.with(|v| v.borrow().clone());
            for (i, &r) in all.iter().enumerate() {
                if r.is_null() {
                    continue;
                }
                let n = &*natras(r);
                let ras = &*r;
                for v in [i as i32, ras.platform, ras.kind, ras.flags, ras.private_flags, ras.format, ras.width, ras.height, ras.depth, ras.stride] {
                    h.i(v as i64);
                }
                let pl = (0..n.num_levels as usize).filter(|&k| n.levels[k] == ras.pixels).last().map_or(-1, |k| k as i64);
                h.i(if ras.pixels.is_null() { -2 } else { pl });
                h.i(!ras.palette.is_null() as i64);
                h.i(rid(ras.parent));
                h.i(ras.offset_x as i64);
                h.i(ras.offset_y as i64);
                for v in [n.num_levels, n.has_alpha, n.base_width, n.base_height, n.dxt, n.tex_id as i32, n.dirty, n.autogen_mipmap, n.max_anisotropy, n.filter_mode, n.address_u, n.address_v] {
                    h.i(v as i64);
                }
                for k in 0..n.num_levels as usize {
                    let s = level_size(n, k);
                    h.i(s as i64);
                    for j in 0..s as usize {
                        h.b(*n.levels[k].add(j));
                    }
                }
            }
            h.i((*g).sp as i64);
            for i in 0..=(*g).sp.min(31) {
                if i >= 0 {
                    h.i(rid((*g).stack[i as usize]));
                }
            }
            let l = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
            format!(" {:016x} |{l}", h.0)
        }
    }

    unsafe fn image_hash(img: *mut Image) -> String {
        unsafe {
            if img.is_null() {
                return " nil".into();
            }
            let i = &*img;
            let mut h = H(0xcbf29ce484222325);
            for v in [i.width, i.height, i.depth, i.stride] {
                h.i(v as i64);
            }
            for y in 0..i.height {
                for x in 0..i.width * i.bpp {
                    h.b(*i.pixels.offset((y * i.stride + x) as isize));
                }
            }
            format!(" {:016x}", h.0)
        }
    }

    /// Raster::create with the wgpu driver
    unsafe fn rcreate(w: i32, h: i32, d: i32, f: i32) -> *mut Raster {
        unsafe { wgpu_raster_create(create(w, h, d, f, PLATFORM_WGPU)) }
    }

    #[test]
    fn matches_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwraster.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwraster.txt");
        unsafe {
            crate::rwimage::revc_image_set_hooks(test_hooks());
            revc_raster_set_hooks(Hooks { size, construct, destruct, allocated, alloc, free, evict, upload, texture_destroy, native_offset: size_of::<Raster>() as i32 });
        }
        let g: *mut RasterGlobals = Box::into_raw(Box::new(RasterGlobals { sp: -1, stack: [std::ptr::null_mut(); 32] }));
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let (args, result) = input.split_once(" =").map_or((input, None), |(a, r)| (a, Some(r.trim())));
            let w: Vec<&str> = args.split_whitespace().collect();
            let num = |s: &str| s.parse::<i64>().unwrap();
            let ras = |s: &str| RAS.with(|v| v.borrow()[s.parse::<usize>().unwrap()]);
            let p = |from: usize| -> Vec<u32> { w[from..from + 7].iter().map(|s| s.parse::<u32>().unwrap()).collect() };
            let res: Option<String> = unsafe {
                match w[0] {
                    "create" => Some(rid(rcreate(num(w[1]) as i32, num(w[2]) as i32, num(w[3]) as i32, num(w[4]) as i32)).to_string()),
                    "destroy" => {
                        destroy(ras(w[1]));
                        None
                    }
                    "sub" => {
                        let rect = Rect { x: num(w[3]) as i32, y: num(w[4]) as i32, w: num(w[5]) as i32, h: num(w[6]) as i32 };
                        sub_raster(ras(w[1]), ras(w[2]), &rect);
                        None
                    }
                    "lock" => Some((!wgpu_raster_lock(ras(w[1]), num(w[2]) as i32, num(w[3]) as i32).is_null() as i32).to_string()),
                    "unlock" => {
                        wgpu_raster_unlock(ras(w[1]));
                        None
                    }
                    "numlevels" => Some((*natras(ras(w[1]))).num_levels.to_string()),
                    "fromimage" => {
                        let img = build(&p(2));
                        Some(((wgpu_raster_from_image(ras(w[1]), Box::into_raw(img)) == 1) as i32).to_string())
                    }
                    "toimage" => Some(image_hash(wgpu_raster_to_image(ras(w[1]))).trim().to_string()),
                    "dxt" => {
                        let (wd, ht, fmt, dxt, nl, ha, seed) = (num(w[1]) as i32, num(w[2]) as i32, num(w[3]) as i32, num(w[4]) as i32, num(w[5]) as i32, num(w[6]), num(w[7]) as u32);
                        let x = rcreate(wd, ht, 16, fmt);
                        let id = rid(x);
                        allocate_dxt(x, dxt, nl, ha != 0);
                        let nr = &*natras(x);
                        let mut g2 = Gen(seed | 1);
                        for k in 0..nr.num_levels as usize {
                            for j in 0..level_size(nr, k) as usize {
                                *nr.levels[k].add(j) = g2.next() as u8;
                            }
                        }
                        Some(id.to_string())
                    }
                    "upload" => Some(upload_raster(ras(w[1])).to_string()),
                    "calc" => Some(format!("{} {}", calculate_num_levels(num(w[1]) as i32, num(w[2]) as i32), format_has_alpha(num(w[3]) as i32) as i32)),
                    "findfmt" => {
                        let img = build(&p(2));
                        Some(match image_find_raster_format(&img, num(w[1]) as i32) {
                            Some((a, b, c, d)) => format!("1 {a} {b} {c} {d}"),
                            None => "0 -1 -1 -1 -1".into(),
                        })
                    }
                    "push" => Some(rid(push_context(g, ras(w[1]))).to_string()),
                    "pop" => Some(rid(pop_context(g)).to_string()),
                    "current" => Some(rid(current_context(g)).to_string()),
                    other => panic!("unknown op {other}"),
                }
            };
            if res.as_deref() != result && bad.len() < 6 {
                bad.push(format!("{input}: result {res:?}"));
            }
            let got = unsafe { state(g) };
            n += 1;
            if got != want && bad.len() < 6 {
                bad.push(format!("{input}\n  want{want}\n  got {got}"));
            }
        }
        assert!(n > 2000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

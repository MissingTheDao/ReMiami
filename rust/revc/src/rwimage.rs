//! librw's image pixel functions (vendor/librw/src/image.cpp), stage 2 step 5b: DXT1/3/5
//! decompression (setPixelsDXT, used when a compressed raster is read back), hasAlpha,
//! convertTo32, unpalettize, palettize with librw's octree colour quantizer (ColorQuant), and
//! the mask functions readMasked uses (makeMask, applyMask) plus removeMask. rw::Image stays a
//! C++ object; its memory stays with librw's allocator (hooks).
//!
//! Bit-identical to librw: the same integer arithmetic, the quantizer's tree, leaf order and
//! reductions (including librw's habit of dropping the colours of deeper subtrees when it reduces
//! a node). One difference: palettize fills the palette entries past the last colour with zeros,
//! where librw copies whatever was on its stack; nothing refers to those entries.
//! Checked against tests/golden/rwimage.txt, recorded from the C++
//! (tests/golden/recorders/rwimage).

/// rw::Image
#[repr(C)]
pub struct Image {
    pub flags: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub bpp: i32,
    pub stride: i32,
    pub pixels: *mut u8,
    pub palette: *mut u8,
}

/// librw's memory functions for images.
#[repr(C)]
pub struct Hooks {
    /// rwNewT(uint8, size, MEMDUR_EVENT | ID_IMAGE)
    pub alloc: unsafe extern "C" fn(size: usize) -> *mut u8,
    /// Image::free
    pub free: unsafe extern "C" fn(img: *mut Image),
    /// Image::allocate
    pub allocate: unsafe extern "C" fn(img: *mut Image),
}

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set once at startup, read-only afterwards.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().expect("revc_image_set_hooks not called") }
}

fn bpp_of(depth: i32) -> i32 {
    if depth < 8 { 1 } else { depth / 8 }
}

unsafe fn set_pixels(img: *mut Image, p: *mut u8) {
    unsafe {
        (*img).pixels = p;
        (*img).flags |= 1;
    }
}

// ---------------------------------------------------------------- pixel converters (raster.cpp)

fn rgba_from_rgb888(i: &[u8]) -> [u8; 4] {
    [i[0], i[1], i[2], 0xFF]
}

fn rgba_from_argb1555(i: &[u8]) -> [u8; 4] {
    let a = ((i[1] >> 7) & 1) as u32;
    let r = ((i[1] >> 2) & 0x1F) as u32;
    let g = ((i[1] & 3) as u32) << 3 | ((i[0] >> 5) & 7) as u32;
    let b = (i[0] & 0x1F) as u32;
    [(r * 0xFF / 0x1f) as u8, (g * 0xFF / 0x1f) as u8, (b * 0xFF / 0x1f) as u8, (a * 0xFF) as u8]
}

/// The RGBA of the pixel at `p` (palette lookup for 4/8 bits).
unsafe fn pixel_rgba(img: &Image, p: *const u8) -> [u8; 4] {
    unsafe {
        match img.depth {
            4 | 8 => {
                let c = img.palette.add(*p as usize * 4);
                [*c, *c.add(1), *c.add(2), *c.add(3)]
            }
            32 => [*p, *p.add(1), *p.add(2), *p.add(3)],
            24 => rgba_from_rgb888(std::slice::from_raw_parts(p, 3)),
            16 => rgba_from_argb1555(std::slice::from_raw_parts(p, 2)),
            _ => panic!("invalid depth"),
        }
    }
}

// ---------------------------------------------------------------- DXT

fn color565(col: u32) -> [u32; 3] {
    [((col >> 11) & 0x1F) * 0xFF / 0x1F, ((col >> 5) & 0x3F) * 0xFF / 0x3F, (col & 0x1F) * 0xFF / 0x1F]
}

/// The four block colours; `four` = always the 4-colour mode (librw's DXT3 ignores the order).
fn block_colors(col0: u32, col1: u32, four: bool) -> [[u32; 4]; 4] {
    let a = color565(col0);
    let b = color565(col1);
    let mut c = [[0u32; 4]; 4];
    c[0] = [a[0], a[1], a[2], 0xFF];
    c[1] = [b[0], b[1], b[2], 0xFF];
    if four || col0 > col1 {
        for i in 0..3 {
            c[2][i] = (2 * c[0][i] + c[1][i]) / 3;
            c[3][i] = (c[0][i] + 2 * c[1][i]) / 3;
        }
        c[2][3] = 0xFF;
        c[3][3] = 0xFF;
    } else {
        for i in 0..3 {
            c[2][i] = (c[0][i] + c[1][i]) / 2;
        }
        c[2][3] = 0xFF;
        c[3] = [0, 0, 0, 0];
    }
    c
}

fn le16(s: &[u8], at: usize) -> u32 {
    u16::from_le_bytes([s[at], s[at + 1]]) as u32
}
fn le32(s: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(s[at..at + 4].try_into().unwrap())
}
fn le64(s: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(s[at..at + 8].try_into().unwrap())
}

/// decompressDXT1/3/5 into `dst` (w*h RGBA texels); w and h are multiples of 4, as librw needs.
pub fn decompress_dxt(kind: i32, dst: &mut [u8], w: i32, h: i32, src: &[u8]) {
    let (step, total) = match kind {
        1 => (8usize, (w * h / 2) as usize),
        3 | 5 => (16usize, (w * h) as usize),
        _ => return,
    };
    let (mut x, mut y) = (0i32, 0i32);
    let mut j = 0usize;
    while j < total {
        let cofs = if kind == 1 { j } else { j + 8 };
        let c = block_colors(le16(src, cofs), le16(src, cofs + 2), kind == 3);
        let mut indices = le32(src, cofs + 4);
        let mut idx = [0usize; 16];
        for k in idx.iter_mut() {
            *k = (indices & 3) as usize;
            indices >>= 2;
        }
        let mut alpha = [0u32; 16];
        match kind {
            3 => {
                let mut a = le64(src, j);
                for v in alpha.iter_mut() {
                    *v = ((a & 0xF) * 17) as u32;
                    a >>= 4;
                }
            }
            5 => {
                let (a0, a1) = (src[j] as u32, src[j + 1] as u32);
                let a = if a0 > a1 {
                    [a0, a1, (6 * a0 + a1) / 7, (5 * a0 + 2 * a1) / 7, (4 * a0 + 3 * a1) / 7, (3 * a0 + 4 * a1) / 7, (2 * a0 + 5 * a1) / 7, (a0 + 6 * a1) / 7]
                } else {
                    [a0, a1, (4 * a0 + a1) / 5, (3 * a0 + 2 * a1) / 5, (2 * a0 + 3 * a1) / 5, (a0 + 4 * a1) / 5, 0, 0xFF]
                };
                let mut bits = le64(src, j + 2);
                for v in alpha.iter_mut() {
                    *v = a[(bits & 7) as usize];
                    bits >>= 3;
                }
            }
            _ => {}
        }
        for l in 0..4 {
            for k in 0..4 {
                let t = l * 4 + k;
                let o = (((y + l as i32) * w + x + k as i32) * 4) as usize;
                let col = c[idx[t]];
                dst[o] = col[0] as u8;
                dst[o + 1] = col[1] as u8;
                dst[o + 2] = col[2] as u8;
                dst[o + 3] = if kind == 1 { col[3] as u8 } else { alpha[t] as u8 };
            }
        }
        x += 4;
        if x >= w {
            y += 4;
            x = 0;
        }
        j += step;
    }
}

// ---------------------------------------------------------------- Image functions

/// Image::setPixelsDXT: decompresses into the image's (allocated, 32-bit) pixels.
///
/// # Safety
/// An allocated 32-bit image; `src` holds the compressed level.
pub unsafe fn set_pixels_dxt(img: *mut Image, kind: i32, src: *const u8) {
    unsafe {
        let (w, h) = ((*img).width, (*img).height);
        let n = match kind {
            1 => w * h / 2,
            3 | 5 => w * h,
            _ => return,
        } as usize;
        let src = std::slice::from_raw_parts(src, n + if kind == 5 { 0 } else { 0 });
        let dst = std::slice::from_raw_parts_mut((*img).pixels, (w * h * 4) as usize);
        decompress_dxt(kind, dst, w, h, src);
    }
}

/// Image::hasAlpha: any texel not fully opaque (16 bits: any with the alpha bit clear).
///
/// # Safety
/// A valid image (with a palette for 4/8 bits).
pub unsafe fn has_alpha(img: *const Image) -> bool {
    unsafe {
        let i = &*img;
        let mut ret: u8 = 0xFF;
        let mut pixels = i.pixels;
        match i.depth {
            32 | 16 => {
                for _ in 0..i.height {
                    let mut line = pixels;
                    for _ in 0..i.width {
                        ret &= if i.depth == 32 { *line.add(3) } else { *line.add(1) & 0x80 };
                        line = line.add(i.bpp as usize);
                    }
                    pixels = pixels.offset(i.stride as isize);
                }
                if i.depth == 16 {
                    return ret != 0x80;
                }
            }
            24 => return false,
            d if d <= 8 => {
                for _ in 0..i.height {
                    let mut line = pixels;
                    for _ in 0..i.width {
                        ret &= *i.palette.add(*line as usize * 4 + 3);
                        line = line.add(i.bpp as usize);
                    }
                    pixels = pixels.offset(i.stride as isize);
                }
            }
            _ => {}
        }
        ret != 0xFF
    }
}

/// Image::unpalettize: 4/8 bits to 24 (or 32 when there is alpha or `force_alpha`).
///
/// # Safety
/// A valid image.
pub unsafe fn unpalettize(img: *mut Image, force_alpha: bool) {
    unsafe {
        let i = &mut *img;
        if i.depth > 8 {
            return;
        }
        let ndepth = if force_alpha || has_alpha(img) { 32 } else { 24 };
        let nstride = i.width * ndepth / 8;
        let npixels = (hooks().alloc)((nstride * i.height) as usize);
        let mut line = i.pixels;
        let mut nline = npixels;
        for _ in 0..i.height {
            let mut p = line;
            let mut np = nline;
            for _ in 0..i.width {
                let c = i.palette.add(*p as usize * 4);
                *np = *c;
                *np.add(1) = *c.add(1);
                *np.add(2) = *c.add(2);
                np = np.add(3);
                if ndepth == 32 {
                    *np = *c.add(3);
                    np = np.add(1);
                }
                p = p.add(1);
            }
            line = line.offset(i.stride as isize);
            nline = nline.offset(nstride as isize);
        }
        (hooks().free)(img);
        let i = &mut *img;
        i.depth = ndepth;
        i.bpp = bpp_of(ndepth);
        i.stride = nstride;
        set_pixels(img, npixels);
    }
}

/// Image::convertTo32
///
/// # Safety
/// A valid image with pixels.
pub unsafe fn convert_to32(img: *mut Image) {
    unsafe {
        let i = &*img;
        let conv: fn(&[u8]) -> [u8; 4] = match i.depth {
            4 | 8 => {
                unpalettize(img, true);
                return;
            }
            16 => rgba_from_argb1555,
            24 => rgba_from_rgb888,
            _ => return,
        };
        let newstride = i.width * 4;
        let newpixels = (hooks().alloc)((newstride * i.height) as usize);
        for y in 0..i.height {
            let line = i.pixels.offset((y * i.stride) as isize);
            let nl = newpixels.offset((y * newstride) as isize);
            for x in 0..i.width {
                let px = std::slice::from_raw_parts(line.add((x * i.bpp) as usize), i.bpp as usize);
                let c = conv(px);
                std::ptr::copy_nonoverlapping(c.as_ptr(), nl.add((x * 4) as usize), 4);
            }
        }
        (hooks().free)(img);
        let i = &mut *img;
        i.depth = 32;
        i.bpp = 4;
        i.stride = newstride;
        i.pixels = std::ptr::null_mut();
        i.palette = std::ptr::null_mut();
        set_pixels(img, newpixels);
    }
}

/// Image::makeMask: the brightest channel into alpha (palette or texels).
///
/// # Safety
/// A valid image.
pub unsafe fn make_mask(img: *mut Image) {
    unsafe {
        let d = (*img).depth;
        if d == 4 || d == 8 {
            let pal = (*img).palette;
            for i in 0..(1usize << d) {
                let c = pal.add(i * 4);
                *c.add(3) = (*c).max(*c.add(1)).max(*c.add(2));
            }
            return;
        }
        if d == 16 || d == 24 {
            convert_to32(img);
        }
        if d == 16 || d == 24 || d == 32 {
            let i = &*img;
            for y in 0..i.height {
                let mut p = i.pixels.offset((y * i.stride) as isize);
                for _ in 0..i.width {
                    *p.add(3) = (*p).max(*p.add(1)).max(*p.add(2));
                    p = p.add(i.bpp as usize);
                }
            }
        }
    }
}

/// Image::applyMask: the mask's alpha into this image (made 32-bit); same size only, and not
/// from 16 or 24-bit masks.
///
/// # Safety
/// Valid images.
pub unsafe fn apply_mask(img: *mut Image, mask: *const Image) {
    unsafe {
        let m = &*mask;
        if (*img).width != m.width || (*img).height != m.height {
            return;
        }
        if m.depth == 16 || m.depth == 24 {
            return;
        }
        convert_to32(img);
        let i = &*img;
        for y in 0..i.height {
            let mut p = i.pixels.offset((y * i.stride) as isize);
            let mut q = m.pixels.offset((y * m.stride) as isize) as *const u8;
            for _ in 0..i.width {
                if m.depth == 32 {
                    *p.add(3) = *q.add(3);
                } else if m.depth <= 8 {
                    *p.add(3) = *m.palette.add(*q as usize * 4 + 3);
                }
                p = p.add(i.bpp as usize);
                q = q.add(m.bpp as usize);
            }
        }
    }
}

/// Image::removeMask: everything opaque.
///
/// # Safety
/// A valid image.
pub unsafe fn remove_mask(img: *mut Image) {
    unsafe {
        let i = &*img;
        if i.depth <= 8 {
            let n = 4 * (1usize << i.depth);
            for k in (0..n).step_by(4) {
                *i.palette.add(k + 3) = 0xFF;
            }
            return;
        }
        if i.depth == 24 {
            return;
        }
        for y in 0..i.height {
            let mut p = i.pixels.offset((y * i.stride) as isize);
            for _ in 0..i.width {
                match i.depth {
                    16 => {
                        *p.add(1) |= 0x80;
                        p = p.add(2);
                    }
                    32 => {
                        *p.add(3) = 0xFF;
                        p = p.add(4);
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------- ColorQuant (octree)

const QUANTDEPTH: i32 = 8;

#[derive(Clone)]
struct QNode {
    r: u32,
    g: u32,
    b: u32,
    a: u32,
    num_pixels: i32,
    parent: Option<usize>,
    children: [Option<usize>; 16],
    linked: bool,
}

struct ColorQuant {
    nodes: Vec<QNode>,
    leaves: Vec<usize>,
    root: usize,
}

/// makeTreeAddr: one nibble per level (bit i of r, g, b, a), most significant bits first.
fn tree_addr(c: [u8; 4]) -> u32 {
    let mut addr = 0u32;
    let (mut r, mut g, mut b, mut a) = (1u32, 2u32, 4u32, 8u32);
    for i in 0..8 {
        let mask = 0x80u8 >> i;
        if c[0] & mask != 0 {
            addr |= r;
        }
        if c[1] & mask != 0 {
            addr |= g;
        }
        if c[2] & mask != 0 {
            addr |= b;
        }
        if c[3] & mask != 0 {
            addr |= a;
        }
        r <<= 4;
        g <<= 4;
        b <<= 4;
        a <<= 4;
    }
    addr
}

impl ColorQuant {
    fn new() -> ColorQuant {
        let mut q = ColorQuant { nodes: Vec::new(), leaves: Vec::new(), root: 0 };
        q.root = q.create_node(QUANTDEPTH);
        q
    }

    fn create_node(&mut self, level: i32) -> usize {
        let n = self.nodes.len();
        self.nodes.push(QNode { r: 0, g: 0, b: 0, a: 0, num_pixels: 0, parent: None, children: [None; 16], linked: false });
        if level == 0 {
            self.leaves.push(n);
            self.nodes[n].linked = true;
        }
        n
    }

    fn get_node(&mut self, mut node: usize, mut addr: u32, mut level: i32) -> usize {
        while level != 0 {
            let a = (addr & 0xF) as usize;
            let child = match self.nodes[node].children[a] {
                Some(c) => c,
                None => {
                    let c = self.create_node(level - 1);
                    self.nodes[node].children[a] = Some(c);
                    self.nodes[c].parent = Some(node);
                    c
                }
            };
            node = child;
            addr >>= 4;
            level -= 1;
        }
        node
    }

    fn find_node(&self, mut node: usize, mut addr: u32, mut level: i32) -> usize {
        while level != 0 {
            match self.nodes[node].children[(addr & 0xF) as usize] {
                Some(c) => node = c,
                None => return node,
            }
            addr >>= 4;
            level -= 1;
        }
        node
    }

    fn destroy(&mut self, n: usize) {
        for i in 0..16 {
            if let Some(c) = self.nodes[n].children[i] {
                self.destroy(c);
            }
        }
        if self.nodes[n].linked {
            let at = self.leaves.iter().position(|&l| l == n).unwrap();
            self.leaves.remove(at);
            self.nodes[n].linked = false;
        }
    }

    fn reduce_node(&mut self, n: usize) {
        for i in 0..16 {
            if let Some(c) = self.nodes[n].children[i] {
                let (r, g, b, a, p) = {
                    let k = &self.nodes[c];
                    (k.r, k.g, k.b, k.a, k.num_pixels)
                };
                let node = &mut self.nodes[n];
                node.r = node.r.wrapping_add(r);
                node.g = node.g.wrapping_add(g);
                node.b = node.b.wrapping_add(b);
                node.a = node.a.wrapping_add(a);
                node.num_pixels += p;
                self.destroy(c);
                self.nodes[n].children[i] = None;
            }
        }
        self.leaves.push(n);
        self.nodes[n].linked = true;
    }

    fn add_color(&mut self, c: [u8; 4]) {
        let n = self.get_node(self.root, tree_addr(c), QUANTDEPTH);
        let node = &mut self.nodes[n];
        node.r = node.r.wrapping_add(c[0] as u32);
        node.g = node.g.wrapping_add(c[1] as u32);
        node.b = node.b.wrapping_add(c[2] as u32);
        node.a = node.a.wrapping_add(c[3] as u32);
        node.num_pixels += 1;
    }

    fn find_color(&self, c: [u8; 4]) -> u8 {
        self.nodes[self.find_node(self.root, tree_addr(c), QUANTDEPTH)].num_pixels as u8
    }

    /// makePalette: merges the first leaf's parent until few enough leaves remain, then each
    /// leaf's average is a palette entry and its index is kept in numPixels.
    fn make_palette(&mut self, num_colors: usize, colors: &mut [[u8; 4]; 256]) {
        while self.leaves.len() > num_colors {
            let n = self.leaves[0];
            let p = self.nodes[n].parent.expect("quantizer reduced to the root");
            self.reduce_node(p);
        }
        for i in 0..self.leaves.len() {
            let n = &mut self.nodes[self.leaves[i]];
            let np = n.num_pixels as u32;
            n.r /= np;
            n.g /= np;
            n.b /= np;
            n.a /= np;
            colors[i] = [n.r as u8, n.g as u8, n.b as u8, n.a as u8];
            n.num_pixels = i as i32;
        }
    }
}

/// Image::palettize: down to `depth` (4 or 8) bits with the octree quantizer.
///
/// # Safety
/// A valid image with pixels (and a palette for 4/8 bits).
pub unsafe fn palettize(img: *mut Image, depth: i32) {
    unsafe {
        let i = &*img;
        let mut q = ColorQuant::new();
        let px = |x: i32, y: i32| pixel_rgba(i, i.pixels.offset((y * i.stride + x * i.bpp) as isize));
        for y in 0..i.height {
            for x in 0..i.width {
                q.add_color(px(x, y));
            }
        }
        let mut colors = [[0u8; 4]; 256];
        q.make_palette(1usize << depth, &mut colors);
        let newstride = i.width;
        let newpixels = (hooks().alloc)((newstride * i.height) as usize);
        for y in 0..i.height {
            for x in 0..i.width {
                *newpixels.offset((y * newstride + x) as isize) = q.find_color(px(x, y));
            }
        }
        (hooks().free)(img);
        let i = &mut *img;
        i.depth = depth;
        i.bpp = bpp_of(depth);
        i.stride = newstride;
        i.pixels = std::ptr::null_mut();
        i.palette = std::ptr::null_mut();
        set_pixels(img, newpixels);
        (hooks().allocate)(img);
        std::ptr::copy_nonoverlapping(colors.as_ptr() as *const u8, (*img).palette, 4 * (1usize << depth));
    }
}

// ---------------------------------------------------------------- C ABI (image.cpp forwards)

/// # Safety
/// Called once by Image::registerModule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_image_set_hooks(h: Hooks) {
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
    revc_image_set_pixels_dxt(img: *mut Image, kind: i32, src: *const u8) => set_pixels_dxt(img, kind, src);
    revc_image_has_alpha(img: *const Image) -> i32 => has_alpha(img) as i32;
    revc_image_unpalettize(img: *mut Image, force_alpha: i32) => unpalettize(img, force_alpha != 0);
    revc_image_convert_to32(img: *mut Image) => convert_to32(img);
    revc_image_palettize(img: *mut Image, depth: i32) => palettize(img, depth);
    revc_image_make_mask(img: *mut Image) => make_mask(img);
    revc_image_apply_mask(img: *mut Image, mask: *const Image) => apply_mask(img, mask);
    revc_image_remove_mask(img: *mut Image) => remove_mask(img);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwimage.txt (recorded from librw's C++, see the recorder), building
    //! the same images from the same seeds.
    use super::*;

    struct Gen(u32);
    impl Gen {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }
    }

    fn leak(n: usize) -> *mut u8 {
        Box::leak(vec![0u8; n.max(1)].into_boxed_slice()).as_mut_ptr()
    }
    unsafe extern "C" fn alloc(n: usize) -> *mut u8 {
        leak(n)
    }
    unsafe extern "C" fn free(img: *mut Image) {
        unsafe {
            if (*img).flags & 1 != 0 {
                (*img).pixels = std::ptr::null_mut();
            }
            if (*img).flags & 2 != 0 {
                (*img).palette = std::ptr::null_mut();
            }
            (*img).flags = 0;
        }
    }
    unsafe extern "C" fn allocate(img: *mut Image) {
        unsafe {
            let i = &mut *img;
            if i.pixels.is_null() {
                i.stride = i.width * i.bpp;
                i.pixels = leak((i.stride * i.height) as usize);
                i.flags |= 1;
            }
            if i.palette.is_null() {
                if i.depth == 4 || i.depth == 8 {
                    i.palette = leak((1usize << i.depth) * 4);
                }
                i.flags |= 2;
            }
        }
    }

    /// The recorder's build().
    fn build(p: &[u32]) -> Box<Image> {
        let (seed, depth, w, h, pad, ncolors, amode) = (p[0], p[1] as i32, p[2] as i32, p[3] as i32, p[4] as i32, p[5] as usize, p[6]);
        let mut g = Gen(seed | 1);
        let mut tab = [[0u8; 4]; 256];
        for t in tab.iter_mut().take(ncolors) {
            t[0] = g.next() as u8;
            t[1] = g.next() as u8;
            t[2] = g.next() as u8;
            t[3] = match amode {
                0 => 255,
                1 => g.next() as u8,
                _ => {
                    if g.next() & 1 != 0 {
                        255
                    } else {
                        0
                    }
                }
            };
        }
        let bpp = bpp_of(depth);
        let stride = w * bpp + pad;
        let pixels = leak((stride * h + 1) as usize);
        for y in 0..h {
            for x in 0..w {
                let k = (g.next() % ncolors as u32) as usize;
                let c = tab[k];
                let q = unsafe { pixels.offset((y * stride + x * bpp) as isize) };
                let bytes: Vec<u8> = match depth {
                    4 | 8 => vec![k as u8],
                    16 => {
                        let v = ((c[3] >= 128) as u32) << 15 | ((c[0] >> 3) as u32) << 10 | ((c[1] >> 3) as u32) << 5 | (c[2] >> 3) as u32;
                        vec![v as u8, (v >> 8) as u8]
                    }
                    24 => vec![c[0], c[1], c[2]],
                    _ => c.to_vec(),
                };
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), q, bytes.len()) };
            }
            for k in 0..pad {
                unsafe { *pixels.offset((y * stride + w * bpp + k) as isize) = 0xCD };
            }
        }
        let mut palette = std::ptr::null_mut();
        if depth <= 8 {
            let n = 1usize << depth;
            palette = leak(n * 4);
            for i in 0..n {
                for j in 0..4 {
                    let v = if i < ncolors { tab[i][j] } else { g.next() as u8 };
                    unsafe { *palette.add(i * 4 + j) = v };
                }
            }
        }
        Box::new(Image { flags: 3, width: w, height: h, depth, bpp, stride, pixels, palette })
    }

    fn hash(i: &Image) -> String {
        let mut h = 0xcbf29ce484222325u64;
        let mut hb = |b: u8| h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        for v in [i.depth, i.bpp, i.stride, i.flags] {
            for b in v.to_le_bytes() {
                hb(b);
            }
        }
        let mut maxi: i32 = -1;
        for y in 0..i.height {
            for x in 0..i.width * i.bpp {
                let b = unsafe { *i.pixels.offset((y * i.stride + x) as isize) };
                hb(b);
                maxi = maxi.max(b as i32);
            }
        }
        if i.depth <= 8 {
            for k in 0..((maxi + 1) * 4) as usize {
                hb(unsafe { *i.palette.add(k) });
            }
        }
        format!(" {h:016x}")
    }

    #[test]
    fn matches_librw_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwimage.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwimage.txt");
        unsafe { revc_image_set_hooks(Hooks { alloc, free, allocate }) };
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let w: Vec<&str> = input.split_whitespace().collect();
            let num = |s: &str| s.parse::<u32>().unwrap();
            let p: Vec<u32> = w[1..].iter().take(7).map(|s| if *s == "|" { 0 } else { num(s) }).collect();
            let got = unsafe {
                match w[0] {
                    "hasalpha" => format!(" {}", has_alpha(&*build(&p)) as i32),
                    "unpal" => {
                        let mut i = build(&p);
                        unpalettize(&mut *i, num(w[8]) != 0);
                        hash(&i)
                    }
                    "to32" => {
                        let mut i = build(&p);
                        convert_to32(&mut *i);
                        hash(&i)
                    }
                    "rmmask" => {
                        let mut i = build(&p);
                        remove_mask(&mut *i);
                        hash(&i)
                    }
                    "mkmask" => {
                        let mut i = build(&p);
                        make_mask(&mut *i);
                        hash(&i)
                    }
                    "apmask" => {
                        let mut i = build(&p);
                        let q: Vec<u32> = w[9..16].iter().map(|s| num(s)).collect();
                        let m = build(&q);
                        apply_mask(&mut *i, &*m);
                        hash(&i)
                    }
                    "pal" => {
                        let mut i = build(&p);
                        palettize(&mut *i, num(w[8]) as i32);
                        hash(&i)
                    }
                    "dxt" => {
                        let (t, seed, wd, ht) = (num(w[1]) as i32, num(w[2]), num(w[3]) as i32, num(w[4]) as i32);
                        let mut g = Gen(seed | 1);
                        let len = if t == 1 { wd * ht / 2 } else { wd * ht } as usize;
                        let src: Vec<u8> = (0..len).map(|_| g.next() as u8).collect();
                        let mut i = Image { flags: 0, width: wd, height: ht, depth: 32, bpp: 4, stride: 0, pixels: std::ptr::null_mut(), palette: std::ptr::null_mut() };
                        allocate(&mut i);
                        set_pixels_dxt(&mut i, t, src.as_ptr());
                        hash(&i)
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            n += 1;
            if got != want && bad.len() < 5 {
                bad.push(format!("{input}\n  want{want}\n  got {got}"));
            }
        }
        assert!(n >= 4000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

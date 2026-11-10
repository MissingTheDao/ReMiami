//! librw's image files, stage 2 step 5b (files): the image search path and
//! Image::getFilename, makePath with correctPathCase (case-insensitive path lookup on Unix), and
//! the image file formats: readPNG/writePNG (librw used the bundled lodepng; this uses the png
//! crate), readBMP/writeBMP and readTGA/writeTGA (librw's own code, byte for byte, quirks
//! included: BMP rows skip width*bpp % 4 bytes, 4-bit BMPs of odd width spill one nibble into the
//! next row). Images are made through librw's Image functions (rwimage hooks).
//!
//! Differences, all on input librw handled with undefined behaviour or a crash: missing or
//! malformed files give null instead of asserting or reading past buffers; a directory that
//! can't be listed ends the path lookup instead of crashing; PNG files written by writePNG decode
//! to the same pixels as lodepng's but aren't byte-identical (another encoder); writePNG of a
//! 16-bit image writes the converted pixels (librw encoded and then freed the buffer that the
//! conversion had already freed; the game only writes 32-bit images).
//! Checked against tests/golden/rwimagefile.txt and the files in tests/golden/imagefiles/,
//! recorded from the C++ (tests/golden/recorders/rwimagefile).

use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::sync::Mutex;

use crate::rwimage::{self, Image, hooks};

// ---------------------------------------------------------------- paths (base.cpp, image.cpp)

fn names_match(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

/// correctPathCase relative to `root` (librw: the current directory): each component replaced
/// by the first directory entry that matches it ignoring case; None (leave the name alone) when
/// one doesn't exist.
pub fn correct_path_case(root: &str, filename: &str) -> Option<String> {
    let absolute = filename.starts_with('/');
    let mut sofar = if absolute { "//".to_string() } else { root.to_string() };
    let rest = if absolute { &filename[1..] } else { filename };
    for comp in rest.split('/').filter(|c| !c.is_empty()) {
        let found = if comp == "." || comp == ".." {
            Some(comp.to_string())
        } else {
            let dir = std::fs::read_dir(&sofar).ok()?;
            dir.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| names_match(n, comp))
        };
        sofar.push('/');
        sofar.push_str(&found?);
    }
    Some(if absolute { sofar[2..].to_string() } else { sofar[(root.len() + 1).min(sofar.len())..].to_string() })
}

/// makePath: both slashes to '/', then the case corrected.
pub fn make_path(root: &str, name: &str) -> String {
    let s: String = name.chars().map(|c| if c == '\\' { '/' } else { c }).collect();
    correct_path_case(root, &s).unwrap_or(s)
}

static SEARCH_PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Image::setSearchPath: ';'-separated directory prefixes (each ends with its separator).
pub fn set_search_path(path: Option<&str>) {
    let mut v: Vec<String> = path.map(|p| p.split(';').map(str::to_string).collect()).unwrap_or_default();
    if v.last().is_some_and(|s| s.is_empty()) {
        v.pop();
    }
    *SEARCH_PATHS.lock().unwrap() = v;
}

fn exists(root: &str, s: &str) -> bool {
    let p = if s.starts_with('/') { Path::new(s).to_path_buf() } else { Path::new(root).join(s) };
    std::fs::File::open(p).is_ok()
}

/// Image::getFilename: the name, or the first search path + name, that opens.
pub fn get_filename(root: &str, name: &str) -> Option<String> {
    let paths = SEARCH_PATHS.lock().unwrap().clone();
    if paths.is_empty() {
        let s = make_path(root, name);
        if exists(root, &s) {
            println!("found {s}");
            return Some(s);
        }
        return None;
    }
    for p in paths {
        let s = make_path(root, &(p + name));
        if exists(root, &s) {
            println!("found {name}");
            return Some(s);
        }
    }
    None
}

// ---------------------------------------------------------------- byte reader (StreamMemory)

/// librw's StreamMemory as the BMP and TGA readers use it; past the end reads give zeros.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    eof: bool,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        if self.eof {
            return vec![0; n];
        }
        let end = (self.pos + n).min(self.data.len());
        let mut v = self.data[self.pos..end].to_vec();
        if v.len() < n {
            self.eof = true;
            v.resize(n, 0);
        }
        self.pos = end;
        v
    }
    fn u8(&mut self) -> u8 {
        self.bytes(1)[0]
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.bytes(2).try_into().unwrap())
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes(4).try_into().unwrap())
    }
    fn seek(&mut self, off: i64, absolute: bool) {
        if self.eof {
            return;
        }
        let p = if absolute { off } else { self.pos as i64 + off };
        if p < 0 || p as usize > self.data.len() {
            self.eof = true;
        } else {
            self.pos = p as usize;
        }
    }
}

unsafe fn new_image(w: i32, h: i32, depth: i32) -> *mut Image {
    unsafe {
        let img = (hooks().create)(w, h, depth);
        (hooks().allocate)(img);
        img
    }
}

// ---------------------------------------------------------------- BMP (bmp.cpp)

/// readBMP: uncompressed BMPs (and BI_BITFIELDS with the standard RGB masks).
///
/// # Safety
/// Image hooks set.
pub unsafe fn read_bmp(data: &[u8]) -> *mut Image {
    let null = std::ptr::null_mut();
    let mut f = Reader { data, pos: 0, eof: false };
    if f.u16() != 0x4D42 {
        return null;
    }
    let _size = f.u32();
    f.u16();
    f.u16();
    let offset = f.u32();
    let d = f.bytes(56);
    let le = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let header_size = le(0);
    let width = le(4) as i32;
    let height = le(8) as i32;
    let depth = i16::from_le_bytes([d[14], d[15]]) as i32;
    let mut compression = le(16);
    let (rmask, gmask, bmask, amask) = (le(40), le(44), le(48), le(52));
    f.seek(header_size as i64 - 56, false);
    if header_size <= 16 {
        compression = 0;
    }
    let mut noalpha = true;
    if compression == 3 {
        if rmask != 0xFF0000 || gmask != 0x00FF00 || bmask != 0x0000FF {
            return null;
        }
        compression = 0;
        if header_size > 52 && amask == 0xFF000000 {
            noalpha = false;
        }
    }
    if compression != 0 || width <= 0 || height <= 0 || !matches!(depth, 4 | 8 | 16 | 24 | 32) {
        return null;
    }
    unsafe {
        let img = new_image(width, height, depth);
        let i = &*img;
        if !i.palette.is_null() {
            for k in 0..(1usize << depth) {
                let c = i.palette.add(k * 4);
                *c.add(2) = f.u8();
                *c.add(1) = f.u8();
                *c = f.u8();
                *c.add(3) = f.u8();
                if noalpha {
                    *c.add(3) = 0xFF;
                }
            }
        }
        f.seek(offset as i64, true);
        let pad = (i.width * i.bpp % 4) as i64;
        let total = (i.stride * i.height) as isize;
        let put = |at: isize, v: u8| {
            if at >= 0 && at < total {
                *i.pixels.offset(at) = v;
            }
        };
        for y in 0..i.height {
            let line = ((i.height - 1 - y) * i.stride) as isize;
            let mut x = 0;
            while x < i.width {
                let px = line + (x * i.bpp) as isize;
                match depth {
                    4 => {
                        let b = f.u8();
                        put(line + x as isize, (b >> 4) & 0xF);
                        put(line + x as isize + 1, b & 0xF);
                        x += 1;
                    }
                    8 => put(px, f.u8()),
                    16 => {
                        put(px, f.u8());
                        put(px + 1, f.u8());
                    }
                    24 => {
                        put(px + 2, f.u8());
                        put(px + 1, f.u8());
                        put(px, f.u8());
                    }
                    _ => {
                        put(px + 2, f.u8());
                        put(px + 1, f.u8());
                        put(px, f.u8());
                        let a = f.u8();
                        put(px + 3, if noalpha { 0xFF } else { a });
                    }
                }
                x += 1;
            }
            f.seek(pad, false);
        }
        img
    }
}

/// writeBMP: no alpha (32 bits are written as 24); rows padded by (bytes written % 4) zeros, as
/// librw does.
///
/// # Safety
/// A valid image.
pub unsafe fn write_bmp(img: *const Image) -> Vec<u8> {
    unsafe {
        let i = &*img;
        let pallen = if i.depth > 8 { 0 } else if i.depth == 4 { 16 } else { 256 };
        let stride = (i.width * i.depth / 8 + 3) & !3;
        let depth = if i.depth == 32 { 24 } else { i.depth };
        let mut o = Vec::new();
        let u16le = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_le_bytes());
        let u32le = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
        u16le(&mut o, 0x4D42);
        u32le(&mut o, (0x36 + 4 * pallen + i.height * stride) as u32);
        u16le(&mut o, 0);
        u16le(&mut o, 0);
        u32le(&mut o, (0x36 + 4 * pallen) as u32);
        for v in [0x28u32, i.width as u32, i.height as u32] {
            u32le(&mut o, v);
        }
        u16le(&mut o, 1);
        u16le(&mut o, depth as u16);
        for v in [0u32, 0, 2835, 2835, 0, 0] {
            u32le(&mut o, v);
        }
        for k in 0..pallen as usize {
            let c = i.palette.add(k * 4);
            o.extend_from_slice(&[*c.add(2), *c.add(1), *c, 0xFF]);
        }
        for y in 0..i.height {
            let line = i.pixels.offset(((i.height - 1 - y) * i.stride) as isize);
            let mut p = line;
            let mut x = 0;
            while x < i.width {
                match i.depth {
                    4 => {
                        o.push((*p & 0xF) << 4 | (*p.add(1) & 0xF));
                        p = p.add(2);
                        x += 1;
                    }
                    8 => {
                        o.push(*p);
                        p = p.add(1);
                    }
                    16 => {
                        o.extend_from_slice(&[*p, *p.add(1)]);
                        p = p.add(2);
                    }
                    24 => {
                        o.extend_from_slice(&[*p.add(2), *p.add(1), *p]);
                        p = p.add(3);
                    }
                    32 => {
                        o.extend_from_slice(&[*p.add(2), *p.add(1), *p]);
                        p = p.add(4);
                    }
                    _ => {}
                }
                x += 1;
            }
            let n = p.offset_from(line) % 4;
            o.extend(std::iter::repeat_n(0u8, n as usize));
        }
        o
    }
}

// ---------------------------------------------------------------- TGA (tga.cpp)

/// readTGA: uncompressed colour-mapped (24/32-bit palettes) and true-colour (16/24/32) TGAs.
///
/// # Safety
/// Image hooks set.
pub unsafe fn read_tga(data: &[u8]) -> *mut Image {
    let null = std::ptr::null_mut();
    let mut f = Reader { data, pos: 0, eof: false };
    let id_len = f.u8() as i8;
    let color_map_type = f.u8() as i8;
    let image_type = f.u8() as i8;
    let color_map_origin = f.i16();
    let color_map_length = f.i16() as i32;
    let color_map_depth = f.u8() as i8 as i32;
    f.i16();
    f.i16();
    let width = f.i16() as i32;
    let height = f.i16() as i32;
    let hdepth = f.u8() as i32;
    let descriptor = f.u8();
    if image_type != 1 && image_type != 2 {
        return null;
    }
    f.seek(id_len as i64, false);
    let depth;
    if color_map_type != 0 {
        if color_map_origin != 0 || (color_map_depth != 24 && color_map_depth != 32) {
            return null;
        }
        depth = if color_map_length <= 16 { 4 } else { 8 };
        if color_map_length > if depth == 4 { 16 } else { 256 } {
            return null;
        }
    } else {
        depth = hdepth;
        if !matches!(depth, 16 | 24 | 32) {
            return null;
        }
    }
    if width <= 0 || height <= 0 {
        return null;
    }
    unsafe {
        let img = new_image(width, height, depth);
        let i = &*img;
        if color_map_type != 0 {
            let maxlen = if depth == 4 { 16 } else { 256 };
            let mut k = 0;
            while k < color_map_length {
                let c = i.palette.add(k as usize * 4);
                *c.add(2) = f.u8();
                *c.add(1) = f.u8();
                *c = f.u8();
                *c.add(3) = 0xFF;
                if color_map_depth == 32 {
                    *c.add(3) = f.u8();
                }
                k += 1;
            }
            while k < maxlen {
                let c = i.palette.add(k as usize * 4);
                *c = 0;
                *c.add(1) = 0;
                *c.add(2) = 0;
                *c.add(3) = 0xFF;
                k += 1;
            }
        }
        let top_down = descriptor & 0x20 != 0;
        for y in 0..i.height {
            let row = if top_down { y } else { i.height - 1 - y };
            let mut line = i.pixels.offset((row * i.stride) as isize);
            for _ in 0..i.width {
                match depth {
                    4 | 8 => *line = f.u8(),
                    16 => {
                        *line = f.u8();
                        *line.add(1) = f.u8();
                    }
                    24 => {
                        *line.add(2) = f.u8();
                        *line.add(1) = f.u8();
                        *line = f.u8();
                    }
                    _ => {
                        *line.add(2) = f.u8();
                        *line.add(1) = f.u8();
                        *line = f.u8();
                        *line.add(3) = f.u8();
                    }
                }
                line = line.add(i.bpp as usize);
            }
        }
        img
    }
}

/// writeTGA: top-down; 4-bit images are written as 8-bit colour-mapped.
///
/// # Safety
/// A valid image.
pub unsafe fn write_tga(img: *const Image) -> Vec<u8> {
    unsafe {
        let i = &*img;
        let has_pal = !i.palette.is_null();
        let map_len: i16 = if i.depth == 4 { 16 } else if i.depth == 8 { 256 } else { 0 };
        let mut o = vec![0u8, has_pal as u8, if has_pal { 1 } else { 2 }];
        o.extend_from_slice(&0i16.to_le_bytes());
        o.extend_from_slice(&map_len.to_le_bytes());
        o.push(if has_pal { 32 } else { 0 });
        o.extend_from_slice(&0i16.to_le_bytes());
        o.extend_from_slice(&0i16.to_le_bytes());
        o.extend_from_slice(&(i.width as i16).to_le_bytes());
        o.extend_from_slice(&(i.height as i16).to_le_bytes());
        o.push(if i.depth == 4 { 8 } else { i.depth as u8 });
        o.push(0x20 | if i.depth == 32 { 8 } else { 0 });
        if has_pal {
            for k in 0..map_len as usize {
                let c = i.palette.add(k * 4);
                o.extend_from_slice(&[*c.add(2), *c.add(1), *c, *c.add(3)]);
            }
        }
        for y in 0..i.height {
            let mut p = i.pixels.offset((y * i.stride) as isize);
            for _ in 0..i.width {
                match i.depth {
                    4 | 8 => o.push(*p),
                    16 => o.extend_from_slice(&[*p, *p.add(1)]),
                    24 => o.extend_from_slice(&[*p.add(2), *p.add(1), *p]),
                    32 => o.extend_from_slice(&[*p.add(2), *p.add(1), *p, *p.add(3)]),
                    _ => {}
                }
                p = p.add(i.bpp as usize);
            }
        }
        o
    }
}

// ---------------------------------------------------------------- PNG (png.cpp)

/// readPNG: 4 and 8-bit palettes (as 4 or 8-bit images), 8-bit RGB (24) and RGBA (32), 8-bit
/// grey (with or without alpha) as 32 bits; other bit depths give no image, as in librw.
///
/// # Safety
/// Image hooks set.
pub unsafe fn read_png(data: &[u8]) -> *mut Image {
    let null = std::ptr::null_mut();
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::IDENTITY);
    let Ok(mut r) = dec.read_info() else { return null };
    let (w, h, ct, bd) = {
        let info = r.info();
        (info.width as i32, info.height as i32, info.color_type, info.bit_depth)
    };
    let palette: Vec<[u8; 4]> = {
        let info = r.info();
        let trns = info.trns.as_deref().unwrap_or(&[]);
        info.palette
            .as_deref()
            .unwrap_or(&[])
            .chunks_exact(3)
            .enumerate()
            .map(|(k, c)| [c[0], c[1], c[2], trns.get(k).copied().unwrap_or(255)])
            .collect()
    };
    let Some(size) = r.output_buffer_size() else { return null };
    let mut raw = vec![0u8; size];
    let Ok(frame) = r.next_frame(&mut raw) else { return null };
    let line = frame.line_size;
    let wu = w as usize;
    unsafe {
        let set_palette = |img: *mut Image| {
            for (k, c) in palette.iter().enumerate().take(1 << (*img).depth) {
                std::ptr::copy_nonoverlapping(c.as_ptr(), (*img).palette.add(k * 4), 4);
            }
        };
        // lodepng's raw rows have no padding; rebuild that layout for the copies below
        let packed = |bytes_per_px: usize| -> Vec<u8> { (0..h as usize).flat_map(|y| raw[y * line..y * line + wu * bytes_per_px].to_vec()).collect() };
        match (ct, bd) {
            (png::ColorType::Indexed, png::BitDepth::Four) => {
                // the nibbles as one stream, as lodepng hands them over
                let mut stream = vec![0u8; (wu * h as usize).div_ceil(2)];
                for y in 0..h as usize {
                    for x in 0..wu {
                        let n = (raw[y * line + x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 0xF;
                        let k = y * wu + x;
                        stream[k / 2] |= if k % 2 == 0 { n << 4 } else { n };
                    }
                }
                let img = new_image(w, h, 4);
                set_palette(img);
                // expandPal4_BE(pixels, stride, raw, w/2, w, h)
                let (src_stride, i) = (wu / 2, &*img);
                for y in 0..h as usize {
                    for x in 0..wu / 2 {
                        let b = stream[y * src_stride + x];
                        *i.pixels.add(y * i.stride as usize + x * 2 + 1) = b & 0xF;
                        *i.pixels.add(y * i.stride as usize + x * 2) = b >> 4;
                    }
                }
                img
            }
            (png::ColorType::Indexed, png::BitDepth::Eight) => {
                let img = new_image(w, h, if palette.len() <= 16 { 4 } else { 8 });
                set_palette(img);
                let p = packed(1);
                std::ptr::copy_nonoverlapping(p.as_ptr(), (*img).pixels, p.len());
                img
            }
            (png::ColorType::Rgb, png::BitDepth::Eight) => {
                let img = new_image(w, h, 24);
                let p = packed(3);
                std::ptr::copy_nonoverlapping(p.as_ptr(), (*img).pixels, p.len());
                img
            }
            (png::ColorType::Rgba, png::BitDepth::Eight) => {
                let img = new_image(w, h, 32);
                let p = packed(4);
                std::ptr::copy_nonoverlapping(p.as_ptr(), (*img).pixels, p.len());
                img
            }
            (png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => {
                let Some(rgba) = decode_rgba(data) else { return null };
                let img = new_image(w, h, 32);
                std::ptr::copy_nonoverlapping(rgba.2.as_ptr(), (*img).pixels, rgba.2.len());
                img
            }
            _ => null,
        }
    }
}

/// A PNG as 8-bit RGBA (lodepng's default conversion: palettes and tRNS applied, 16 bits cut to
/// their high byte).
pub fn decode_rgba(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()?];
    let f = r.next_frame(&mut buf).ok()?;
    let px = &buf[..f.buffer_size()];
    let rgba = match f.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((f.width, f.height, rgba))
}

/// writePNG: palettes for 4 and 8 bits, RGB for 24, RGBA for 32 (16 bits are converted to 32
/// first, changing the image, as librw does). Like lodepng, it reads the pixels as one block of
/// width * height texels, ignoring the stride.
///
/// # Safety
/// A valid image.
pub unsafe fn write_png(img: *mut Image) -> Option<Vec<u8>> {
    unsafe {
        if (*img).depth == 16 {
            rwimage::convert_to32(img);
        }
        let i = &*img;
        let (w, h) = (i.width as u32, i.height as u32);
        let n = (w * h) as usize;
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            let data: Vec<u8> = match i.depth {
                4 | 8 => {
                    let entries = 1usize << i.depth;
                    let pal = std::slice::from_raw_parts(i.palette, entries * 4);
                    enc.set_color(png::ColorType::Indexed);
                    enc.set_palette(pal.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
                    enc.set_trns(pal.chunks_exact(4).map(|c| c[3]).collect::<Vec<u8>>());
                    let px = std::slice::from_raw_parts(i.pixels, n);
                    if i.depth == 8 {
                        enc.set_depth(png::BitDepth::Eight);
                        px.to_vec()
                    } else {
                        enc.set_depth(png::BitDepth::Four);
                        // compressPal4_BE, one packed row at a time
                        let row = (w as usize).div_ceil(2);
                        let mut v = vec![0u8; row * h as usize];
                        for y in 0..h as usize {
                            for x in 0..w as usize {
                                v[y * row + x / 2] |= (px[y * w as usize + x] & 0xF) << if x % 2 == 0 { 4 } else { 0 };
                            }
                        }
                        v
                    }
                }
                24 => {
                    enc.set_color(png::ColorType::Rgb);
                    enc.set_depth(png::BitDepth::Eight);
                    std::slice::from_raw_parts(i.pixels, n * 3).to_vec()
                }
                32 => {
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    std::slice::from_raw_parts(i.pixels, n * 4).to_vec()
                }
                _ => return None,
            };
            let mut wr = enc.write_header().ok()?;
            wr.write_image_data(&data).ok()?;
            wr.finish().ok()?;
        }
        Some(out)
    }
}

// ---------------------------------------------------------------- C ABI (image.cpp, png/bmp/tga.cpp)

unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    unsafe { CStr::from_ptr(p).to_str().unwrap_or("") }
}

fn read_file(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// # Safety
/// `path` a C string or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_image_set_search_path(path: *const c_char) {
    set_search_path(if path.is_null() { None } else { Some(unsafe { cstr(path) }) });
}

/// The found file name into `out` (`size` bytes); false when there is none.
///
/// # Safety
/// `name` a C string; `out` `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_image_get_filename(name: *const c_char, out: *mut c_char, size: usize) -> i32 {
    let Some(f) = get_filename(".", unsafe { cstr(name) }) else { return 0 };
    let c = CString::new(f).unwrap_or_default();
    let b = c.as_bytes_with_nul();
    if b.len() > size {
        return 0;
    }
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr() as *const c_char, out, b.len()) };
    1
}

/// makePath in place (the result is never longer).
///
/// # Safety
/// `filename` a writable C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_make_path(filename: *mut c_char) {
    unsafe {
        let s = make_path(".", cstr(filename));
        let len = CStr::from_ptr(filename).to_bytes().len();
        let b = s.as_bytes();
        if b.len() <= len {
            std::ptr::copy_nonoverlapping(b.as_ptr() as *const c_char, filename, b.len());
            *filename.add(b.len()) = 0;
        }
    }
}

macro_rules! reader {
    ($name:ident, $f:ident) => {
        /// # Safety
        /// `filename` a C string.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(filename: *const c_char) -> *mut Image {
            match read_file(unsafe { cstr(filename) }) {
                Some(d) => unsafe { $f(&d) },
                None => std::ptr::null_mut(),
            }
        }
    };
}
reader!(revc_read_png, read_png);
reader!(revc_read_bmp, read_bmp);
reader!(revc_read_tga, read_tga);

/// # Safety
/// A valid image; `filename` a C string. False when the file can't be written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_write_bmp(img: *const Image, filename: *const c_char) -> i32 {
    unsafe { std::fs::write(cstr(filename), write_bmp(img)).is_ok() as i32 }
}

/// # Safety
/// As revc_write_bmp.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_write_tga(img: *const Image, filename: *const c_char) -> i32 {
    unsafe { std::fs::write(cstr(filename), write_tga(img)).is_ok() as i32 }
}

/// # Safety
/// As revc_write_bmp (16-bit images become 32-bit).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_write_png(img: *mut Image, filename: *const c_char) -> i32 {
    unsafe {
        match write_png(img) {
            Some(d) => std::fs::write(cstr(filename), d).is_ok() as i32,
            None => 0,
        }
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwimagefile.txt over the corpus tests/golden/imagefiles/ (both
    //! recorded from librw's C++, see the recorder).
    use super::*;
    use crate::rwimage::testkit::{build, hash, test_hooks};
    use crate::rwimage::revc_image_set_hooks;

    fn fnv(parts: &[&[u8]]) -> String {
        let mut h = 0xcbf29ce484222325u64;
        for p in parts {
            for &b in *p {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        format!(" {h:016x}")
    }

    #[test]
    fn matches_librw_golden() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden");
        let text = std::fs::read_to_string(format!("{root}/rwimagefile.txt")).expect("tests/golden/rwimagefile.txt");
        unsafe { revc_image_set_hooks(test_hooks()) };
        let tmp = std::env::temp_dir().join(format!("revc-rwimagefile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let troot = tmp.to_str().unwrap().to_string();
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap_or((line, ""));
            let w: Vec<&str> = input.split_whitespace().collect();
            let num = |s: &str| s.parse::<u32>().unwrap();
            let got = unsafe {
                match w[0] {
                    "png" | "bmp" | "tga" => {
                        let d = std::fs::read(format!("{root}/imagefiles/{}", w[1])).unwrap();
                        let img = match w[0] {
                            "png" => read_png(&d),
                            "bmp" => read_bmp(&d),
                            _ => read_tga(&d),
                        };
                        if img.is_null() { " nil".to_string() } else { hash(&*img) }
                    }
                    "wbmp" | "wtga" | "wpng" => {
                        let p: Vec<u32> = w[1..8].iter().map(|s| num(s)).collect();
                        let mut img = build(&p);
                        match w[0] {
                            "wbmp" => fnv(&[&write_bmp(&*img)]),
                            "wtga" => fnv(&[&write_tga(&*img)]),
                            _ => {
                                let png = write_png(&mut *img).expect("writePNG");
                                let (pw, ph, rgba) = decode_rgba(&png).expect("decode");
                                fnv(&[&pw.to_le_bytes(), &ph.to_le_bytes(), &rgba])
                            }
                        }
                    }
                    "tree" => {
                        for f in &w[1..] {
                            let p = tmp.join(f);
                            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                            std::fs::write(p, "x").unwrap();
                        }
                        continue;
                    }
                    "path" => {
                        let name = if w[1] == "~" { "" } else { w[1] };
                        let r = make_path(&troot, name);
                        format!(" {}", if r.is_empty() { "~" } else { &r })
                    }
                    "search" => {
                        set_search_path(if w[1] == "-" { None } else { Some(w[1]) });
                        format!(" {}", get_filename(&troot, w[2]).unwrap_or("-".into()))
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            n += 1;
            if got != want && bad.len() < 8 {
                bad.push(format!("{input}\n  want{want}\n  got {got}"));
            }
        }
        set_search_path(None);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(n > 500, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

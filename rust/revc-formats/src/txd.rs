//! Texture dictionaries (`.txd`): PC D3D8 texture natives, read the way librw's
//! `d3d8::readNativeTexture` does. Pixel data is kept as stored (palettised, 16/32-bit or DXTn);
//! decoding to RGBA is left to the renderer / texture-mod code.

use crate::rw::{self, Chunk, ID_STRUCT, ID_TEXDICTIONARY, ID_TEXTURENATIVE};
use crate::{Error, Result};

pub const PLATFORM_D3D8: u32 = 8;
pub const PLATFORM_XBOX: u32 = 5;

/// Raster format bits (`rw::Raster` format field).
pub mod raster {
    pub const MASK: u32 = 0x0F00;
    pub const C1555: u32 = 0x0100;
    pub const C565: u32 = 0x0200;
    pub const C4444: u32 = 0x0300;
    pub const LUM8: u32 = 0x0400;
    pub const C8888: u32 = 0x0500;
    pub const C888: u32 = 0x0600;
    pub const C555: u32 = 0x0A00;
    pub const AUTOMIPMAP: u32 = 0x1000;
    pub const PAL8: u32 = 0x2000;
    pub const PAL4: u32 = 0x4000;
    pub const MIPMAP: u32 = 0x8000;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub name: String,
    pub mask: String,
    /// Filter mode in the low byte, U/V addressing in bits 8-11 / 12-15.
    pub filter_addressing: u32,
    pub raster_format: u32,
    pub has_alpha: bool,
    pub width: u16,
    pub height: u16,
    pub depth: u8,
    pub raster_type: u8,
    /// 0 = uncompressed, otherwise the DXT number (1, 3, 5) or a D3D FOURCC low byte.
    pub compression: u8,
    /// RGBA palette entries (16 or 256 of them; librw reads 32 entries for PAL4).
    pub palette: Vec<[u8; 4]>,
    /// Mip levels, largest first.
    pub levels: Vec<Vec<u8>>,
}

impl Texture {
    pub fn pixel_format(&self) -> u32 {
        self.raster_format & raster::MASK
    }

    /// Size of mip `level`.
    pub fn level_size(&self, level: usize) -> (u32, u32) {
        ((self.width as u32 >> level).max(1), (self.height as u32 >> level).max(1))
    }

    /// Mip `level` as RGBA8, top row first. D3D8 layouts: DXT1/3/5; 8/4-bit palettes (RGBA
    /// entries); 32-bit B,G,R,A; 16-bit 565, 1555, 4444 (little-endian); 8-bit luminance.
    pub fn rgba(&self, level: usize) -> Result<Vec<u8>> {
        let (w, h) = self.level_size(level);
        let data = self.levels.get(level).ok_or_else(|| Error::Invalid(format!("{}: no mip level {level}", self.name)))?;
        let n = (w * h) as usize;
        let short = || Error::Invalid(format!("{}: level {level} data too short", self.name));
        if self.compression != 0 {
            let kind = match self.compression {
                1..=5 => self.compression,
                c => return Err(Error::Invalid(format!("{}: compression {c}", self.name))),
            };
            return crate::dds::decode_dxt(if kind == 1 { 1 } else if kind <= 3 { 3 } else { 5 }, w, h, data).ok_or_else(short);
        }
        let fmt = self.pixel_format();
        let opaque = fmt == raster::C888 || fmt == raster::C565 || fmt == raster::C555;
        let mut out = Vec::with_capacity(n * 4);
        if self.raster_format & (raster::PAL8 | raster::PAL4) != 0 {
            let idx = |i: usize| -> Option<usize> {
                if self.raster_format & raster::PAL8 != 0 {
                    data.get(i).map(|&b| b as usize)
                } else {
                    data.get(i / 2).map(|&b| if i.is_multiple_of(2) { b & 0xF } else { b >> 4 } as usize)
                }
            };
            for i in 0..n {
                let p = self.palette.get(idx(i).ok_or_else(short)?).copied().unwrap_or([0, 0, 0, 255]);
                out.extend([p[0], p[1], p[2], if opaque { 255 } else { p[3] }]);
            }
            return Ok(out);
        }
        let expand = |v: u16, bits: u32| -> u8 { (v as u32 * 255 / ((1 << bits) - 1)) as u8 };
        match self.depth {
            32 => {
                for p in data.get(..n * 4).ok_or_else(short)?.as_chunks::<4>().0 {
                    out.extend([p[2], p[1], p[0], if opaque { 255 } else { p[3] }]);
                }
            }
            16 => {
                for p in data.get(..n * 2).ok_or_else(short)?.as_chunks::<2>().0 {
                    let v = u16::from_le_bytes([p[0], p[1]]);
                    out.extend(match fmt {
                        raster::C565 => [expand(v >> 11, 5), expand((v >> 5) & 63, 6), expand(v & 31, 5), 255],
                        raster::C4444 => [expand((v >> 8) & 15, 4), expand((v >> 4) & 15, 4), expand(v & 15, 4), expand(v >> 12, 4)],
                        raster::C555 => [expand((v >> 10) & 31, 5), expand((v >> 5) & 31, 5), expand(v & 31, 5), 255],
                        _ => [expand((v >> 10) & 31, 5), expand((v >> 5) & 31, 5), expand(v & 31, 5), if v & 0x8000 != 0 { 255 } else { 0 }],
                    });
                }
            }
            8 => {
                for &l in data.get(..n).ok_or_else(short)? {
                    out.extend([l, l, l, 255]);
                }
            }
            d => return Err(Error::Invalid(format!("{}: depth {d} format {fmt:#x}", self.name))),
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Txd {
    pub device_id: u16,
    pub textures: Vec<Texture>,
}

impl Txd {
    /// Case-insensitive, as RwTexDictionaryFindNamedTexture.
    pub fn find(&self, name: &str) -> Option<&Texture> {
        self.textures
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

pub fn parse(data: &[u8]) -> Result<Txd> {
    let root = rw::root(data)?;
    if root.id != ID_TEXDICTIONARY {
        return Err(Error::Invalid(format!(
            "not a texture dictionary (chunk {:#x})",
            root.id
        )));
    }
    let mut kids = root.children();
    let st = kids
        .next()
        .ok_or_else(|| Error::Invalid("texture dictionary has no struct".into()))??;
    if st.id != ID_STRUCT {
        return Err(Error::Invalid(
            "texture dictionary does not start with a struct".into(),
        ));
    }
    let mut r = st.reader();
    let count = r.u16("txd texture count")?;
    let device_id = r.u16("txd device id")?;
    let mut textures = Vec::with_capacity(count as usize);
    for c in kids {
        let c = c?;
        if c.id == ID_TEXTURENATIVE {
            textures.push(parse_native(&c)?);
        }
    }
    if textures.len() != count as usize {
        return Err(Error::Invalid(format!(
            "txd says {count} textures, found {}",
            textures.len()
        )));
    }
    Ok(Txd {
        device_id,
        textures,
    })
}

fn parse_native(c: &Chunk) -> Result<Texture> {
    parse_native_struct(&c.child(ID_STRUCT)?)
}

/// One texture native from the body of its chunk (what follows the 12-byte TEXTURENATIVE
/// header: the struct, then the extension, which is ignored as the game's reader does).
pub fn parse_native_body(body: &[u8]) -> Result<Texture> {
    for c in rw::Chunks::new(body) {
        let c = c?;
        if c.id == ID_STRUCT {
            return parse_native_struct(&c);
        }
    }
    Err(Error::Invalid("texture native without a struct".into()))
}

fn parse_native_struct(st: &Chunk) -> Result<Texture> {
    let mut r = st.reader();
    let platform = r.u32("texture platform")?;
    if platform == PLATFORM_XBOX {
        return parse_xbox(r);
    }
    if platform != PLATFORM_D3D8 {
        return Err(Error::Invalid(format!(
            "texture native platform {platform}, expected D3D8"
        )));
    }
    let filter_addressing = r.u32("texture filter")?;
    let name = r.fixed_str(32, "texture name")?;
    let mask = r.fixed_str(32, "texture mask")?;
    let raster_format = r.u32("raster format")?;
    let has_alpha = r.i32("raster alpha")? != 0;
    let width = r.u16("raster width")?;
    let height = r.u16("raster height")?;
    let depth = r.u8("raster depth")?;
    let num_levels = r.u8("raster levels")?;
    let raster_type = r.u8("raster type")?;
    let compression = r.u8("raster compression")?;
    let pal_len = if raster_format & raster::PAL4 != 0 {
        32
    } else if raster_format & raster::PAL8 != 0 {
        256
    } else {
        0
    };
    let palette = r
        .bytes(4 * pal_len, "texture palette")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect();
    let mut levels = Vec::with_capacity(num_levels as usize);
    for _ in 0..num_levels {
        let size = r.u32("level size")? as usize;
        levels.push(r.bytes(size, "level data")?.to_vec());
    }
    if width == 0 || height == 0 || levels.is_empty() {
        return Err(Error::Invalid(format!("texture {name}: empty raster")));
    }
    Ok(Texture {
        name,
        mask,
        filter_addressing,
        raster_format,
        has_alpha,
        width,
        height,
        depth,
        raster_type,
        compression,
        palette,
        levels,
    })
}

/// An Xbox texture native (the Xbox release's TXDs), read as librw's `xbox::readNativeTexture`
/// and turned into the D3D8 layout the rest of this module uses: DXT levels split, other formats
/// unswizzled per level and the palette's BGRA entries swapped to RGBA (as librw's
/// `xbox::rasterToImage` does), so `rgba` and the renderer treat both platforms alike.
fn parse_xbox(mut r: crate::bytes::Reader) -> Result<Texture> {
    let filter_addressing = r.u32("texture filter")?;
    let name = r.fixed_str(32, "texture name")?;
    let mask = r.fixed_str(32, "texture mask")?;
    let raster_format = r.u32("raster format")?;
    let has_alpha = r.u16("raster alpha")? != 0;
    let cube = r.u16("raster cube flag")?;
    let width = r.u16("raster width")?;
    let height = r.u16("raster height")?;
    let depth = r.u8("raster depth")?;
    let num_levels = r.u8("raster levels")?;
    let raster_type = r.u8("raster type")?;
    let d3d_format = r.u8("raster compression")?;
    let total = r.u32("raster data size")? as usize;
    if cube != 0 {
        return Err(Error::Invalid(format!("texture {name}: Xbox cube map")));
    }
    if width == 0 || height == 0 {
        return Err(Error::Invalid(format!("texture {name}: empty raster")));
    }
    let pal_len = if raster_format & raster::PAL4 != 0 {
        32
    } else if raster_format & raster::PAL8 != 0 {
        256
    } else {
        0
    };
    let palette = r
        .bytes(4 * pal_len, "texture palette")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    let data = r.bytes(total, "texture data")?;
    let mipmap = raster_format & raster::MIPMAP != 0;
    let full_chain = 32 - (width.max(height) as u32).leading_zeros();
    let (compression, count, level_bytes): (u8, u32, Box<dyn Fn(u32, u32) -> usize>) = match d3d_format {
        // D3DFMT_DXT1, DXT2/3, DXT4/5: blocks as on PC, levels one after the other
        0x0C | 0x0E | 0x0F => {
            let kind = match d3d_format {
                0x0C => 1,
                0x0E => 3,
                _ => 5,
            };
            let bytes = if kind == 1 { 8 } else { 16 };
            (kind, if mipmap { num_levels as u32 } else { 1 }, Box::new(move |w, h| w.div_ceil(4) as usize * h.div_ceil(4) as usize * bytes))
        }
        0 => {
            let bpp = match depth {
                8 | 16 | 32 => depth as usize / 8,
                d => return Err(Error::Invalid(format!("texture {name}: Xbox depth {d}"))),
            };
            // librw allocates the whole chain for a mipmapped raster and fills it from the data
            (0, if mipmap { full_chain } else { 1 }, Box::new(move |w, h| w as usize * h as usize * bpp))
        }
        f => return Err(Error::Invalid(format!("texture {name}: Xbox format {f:#x}"))),
    };
    let mut levels = Vec::with_capacity(count as usize);
    let mut off = 0;
    for i in 0..count {
        let (w, h) = ((width as u32 >> i).max(1), (height as u32 >> i).max(1));
        let n = level_bytes(w, h);
        let Some(src) = data.get(off..off + n) else {
            return Err(Error::Invalid(format!("texture {name}: Xbox data ends in level {i}")));
        };
        off += n;
        levels.push(if compression == 0 { unswizzle(src, w as usize, h as usize, n / (w * h) as usize) } else { src.to_vec() });
    }
    Ok(Texture {
        name,
        mask,
        filter_addressing,
        raster_format,
        has_alpha,
        width,
        height,
        depth,
        raster_type,
        compression,
        palette,
        levels,
    })
}

/// Xbox swizzled (Morton order) pixels to rows, as librw's `xbox::unswizzle`.
pub fn unswizzle(src: &[u8], w: usize, h: usize, bpp: usize) -> Vec<u8> {
    let (mut mask_u, mut mask_v) = (0usize, 0usize);
    let (mut i, mut j) = (1usize, 1usize);
    loop {
        let mut c = 0;
        if i < w {
            mask_u |= j;
            j <<= 1;
            c = j;
        }
        if i < h {
            mask_v |= j;
            j <<= 1;
            c = j;
        }
        i <<= 1;
        if c == 0 {
            break;
        }
    }
    let mut dst = vec![0u8; w * h * bpp];
    let mut v = 0usize;
    for y in 0..h {
        let mut u = 0usize;
        for x in 0..w {
            let s = (u | v) * bpp;
            dst[(y * w + x) * bpp..][..bpp].copy_from_slice(&src[s..s + bpp]);
            u = u.wrapping_sub(mask_u) & mask_u;
        }
        v = v.wrapping_sub(mask_v) & mask_v;
    }
    dst
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::rw::ID_EXTENSION;
    use crate::rw::build::*;

    fn name32(s: &str) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    }

    /// One 2x2 texture: C8888, or PAL8 with a 256-entry palette.
    pub fn native(name: &str, pal8: bool) -> Vec<u8> {
        let fmt = if pal8 {
            raster::PAL8 | raster::C8888
        } else {
            raster::C8888
        };
        let mut s = u32s(&[PLATFORM_D3D8, 0x1102]);
        s.extend_from_slice(&name32(name));
        s.extend_from_slice(&name32(""));
        s.extend(u32s(&[fmt, 1]));
        s.extend_from_slice(&2u16.to_le_bytes());
        s.extend_from_slice(&2u16.to_le_bytes());
        s.extend_from_slice(&[if pal8 { 8 } else { 32 }, 1, 4, 0]);
        let level = if pal8 {
            s.extend(std::iter::repeat_n(7u8, 1024));
            vec![0u8, 1, 2, 3]
        } else {
            vec![9u8; 16]
        };
        s.extend(u32s(&[level.len() as u32]));
        s.extend(level);
        chunk(
            ID_TEXTURENATIVE,
            &cat(&[&chunk(ID_STRUCT, &s), &chunk(ID_EXTENSION, &[])]),
        )
    }

    pub fn txd(natives: &[Vec<u8>]) -> Vec<u8> {
        let mut st = (natives.len() as u16).to_le_bytes().to_vec();
        st.extend_from_slice(&0u16.to_le_bytes());
        let mut body = chunk(ID_STRUCT, &st);
        for n in natives {
            body.extend_from_slice(n);
        }
        body.extend(chunk(ID_EXTENSION, &[]));
        chunk(ID_TEXDICTIONARY, &body)
    }

    #[test]
    fn parses_textures() {
        let t = parse(&txd(&[native("road", false), native("Sign", true)])).unwrap();
        assert_eq!(t.textures.len(), 2);
        let road = &t.textures[0];
        assert_eq!((road.width, road.height, road.depth), (2, 2, 32));
        assert_eq!(road.pixel_format(), raster::C8888);
        assert_eq!(road.levels, vec![vec![9u8; 16]]);
        let sign = t.find("sign").unwrap();
        assert_eq!(sign.palette.len(), 256);
        assert_eq!(sign.palette[0], [7, 7, 7, 7]);
        assert_eq!(sign.filter_addressing & 0xFF, 2);
    }

    #[test]
    fn decodes_pixels() {
        let t = parse(&txd(&[native("road", false), native("Sign", true)])).unwrap();
        // C8888 level is 16 bytes of 9: B=G=R=A=9.
        assert_eq!(t.textures[0].rgba(0).unwrap(), vec![9u8; 16]);
        // PAL8: indices 0..3 into a palette of [7,7,7,7].
        assert_eq!(t.textures[1].rgba(0).unwrap(), vec![7u8; 16]);
        assert!(t.textures[0].rgba(1).is_err());
    }

    #[test]
    fn rejects_count_mismatch_and_truncation() {
        let mut d = txd(&[native("a", false)]);
        d[24] = 2; // struct count
        assert!(parse(&d).is_err());
        let d = txd(&[native("a", false)]);
        assert!(parse(&d[..d.len() - 30]).is_err());
    }
}

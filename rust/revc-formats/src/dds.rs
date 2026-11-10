//! DDS images for texture mods: DXT1/3/5 blocks as they are (`blocks`, uploaded as BC textures),
//! or the top mip level decoded to RGBA8 (`decode`: DXT1/3/5 and uncompressed 24/32-bit RGB(A)
//! with channel masks).

use crate::bytes::Reader;
use crate::{Error, Result};

const DDPF_ALPHAPIXELS: u32 = 0x1;
const DDPF_FOURCC: u32 = 0x4;
const DDPF_RGB: u32 = 0x40;
const MAX_SIZE: u32 = 16384;

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Top row first, 4 bytes per pixel.
    pub rgba: Vec<u8>,
}

pub fn decode(data: &[u8]) -> Result<Image> {
    let mut r = Reader::new(data);
    if r.bytes(4, "dds magic")? != b"DDS " {
        return Err(Error::Invalid("not a DDS file".into()));
    }
    if r.u32("dds header size")? != 124 {
        return Err(Error::Invalid("bad DDS header size".into()));
    }
    r.skip(4)?; // flags
    let height = r.u32("dds height")?;
    let width = r.u32("dds width")?;
    r.skip(4 * 3 + 4 * 11)?; // pitch, depth, mip count, reserved
    if r.u32("dds pixel format size")? != 32 {
        return Err(Error::Invalid("bad DDS pixel format size".into()));
    }
    let pf_flags = r.u32("dds pixel format flags")?;
    let fourcc: [u8; 4] = r.bytes(4, "dds fourcc")?.try_into().unwrap();
    let bits = r.u32("dds bit count")?;
    let masks = [r.u32("dds r mask")?, r.u32("dds g mask")?, r.u32("dds b mask")?, r.u32("dds a mask")?];
    r.skip(20)?; // caps
    if !(1..=MAX_SIZE).contains(&width) || !(1..=MAX_SIZE).contains(&height) {
        return Err(Error::Invalid(format!("DDS size {width}x{height} out of range")));
    }
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    if pf_flags & DDPF_FOURCC != 0 {
        let block_bytes = match &fourcc {
            b"DXT1" => 8,
            b"DXT3" | b"DXT5" => 16,
            _ => return Err(Error::Invalid(format!("unsupported DDS pixel format {:?}", String::from_utf8_lossy(&fourcc)))),
        };
        let (bw, bh) = (width.div_ceil(4) as usize, height.div_ceil(4) as usize);
        let blocks = r.bytes(bw * bh * block_bytes, "dds blocks")?;
        let kind = fourcc[3] - b'0';
        rgba = decode_dxt(kind, width, height, blocks).expect("length checked above");
    } else if pf_flags & DDPF_RGB != 0 && (bits == 24 || bits == 32) {
        let bpp = bits as usize / 8;
        let px = r.bytes(width as usize * height as usize * bpp, "dds pixels")?;
        let has_alpha = pf_flags & DDPF_ALPHAPIXELS != 0 && masks[3] != 0;
        for (i, p) in px.chunks_exact(bpp).enumerate() {
            let mut v = [0u8; 4];
            v[..bpp].copy_from_slice(p);
            let v = u32::from_le_bytes(v);
            let o = i * 4;
            for c in 0..3 {
                rgba[o + c] = channel(v, masks[c]);
            }
            rgba[o + 3] = if has_alpha { channel(v, masks[3]) } else { 255 };
        }
    } else {
        return Err(Error::Invalid(format!("unsupported DDS pixel format (flags {pf_flags:#x}, {bits} bits)")));
    }
    Ok(Image { width, height, rgba })
}

const DDSD_MIPMAPCOUNT: u32 = 0x20000;

/// The compressed blocks of a DXT1/3/5 DDS, for uploading as a BC texture as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocks<'a> {
    pub width: u32,
    pub height: u32,
    /// 1, 3 or 5.
    pub kind: u8,
    /// Mip levels, largest first (as many as the file holds, at least one).
    pub levels: Vec<&'a [u8]>,
    /// DXT3/5, or DXT1 using its transparent texel somewhere.
    pub has_alpha: bool,
}

/// The DXT blocks of a DDS file; `Ok(None)` for a DDS that isn't DXT1/3/5 (decode it instead).
pub fn blocks(data: &[u8]) -> Result<Option<Blocks<'_>>> {
    let mut r = Reader::new(data);
    if r.bytes(4, "dds magic")? != b"DDS " {
        return Err(Error::Invalid("not a DDS file".into()));
    }
    if r.u32("dds header size")? != 124 {
        return Err(Error::Invalid("bad DDS header size".into()));
    }
    let flags = r.u32("dds flags")?;
    let height = r.u32("dds height")?;
    let width = r.u32("dds width")?;
    r.skip(4 * 2)?; // pitch, depth
    let mips = r.u32("dds mip count")?;
    r.skip(4 * 11)?;
    if r.u32("dds pixel format size")? != 32 {
        return Err(Error::Invalid("bad DDS pixel format size".into()));
    }
    let pf_flags = r.u32("dds pixel format flags")?;
    let fourcc: [u8; 4] = r.bytes(4, "dds fourcc")?.try_into().unwrap();
    r.skip(4 * 5 + 20)?; // bit count, masks, caps
    let kind = match (pf_flags & DDPF_FOURCC != 0, &fourcc) {
        (true, b"DXT1") => 1,
        (true, b"DXT3") => 3,
        (true, b"DXT5") => 5,
        _ => return Ok(None),
    };
    if !(1..=MAX_SIZE).contains(&width) || !(1..=MAX_SIZE).contains(&height) {
        return Err(Error::Invalid(format!("DDS size {width}x{height} out of range")));
    }
    let block_bytes = if kind == 1 { 8 } else { 16 };
    let full_chain = 32 - width.max(height).leading_zeros();
    let count = if flags & DDSD_MIPMAPCOUNT != 0 && mips > 1 { mips.min(full_chain) } else { 1 };
    let mut levels = Vec::with_capacity(count as usize);
    for i in 0..count {
        let (w, h) = ((width >> i).max(1), (height >> i).max(1));
        levels.push(r.bytes(w.div_ceil(4) as usize * h.div_ceil(4) as usize * block_bytes, "dds blocks")?);
    }
    let has_alpha = kind != 1 || levels[0].chunks_exact(8).any(dxt1_transparent);
    Ok(Some(Blocks { width, height, kind, levels, has_alpha }))
}

/// Whether a DXT1 block uses its transparent texel (3-colour mode, index 3).
fn dxt1_transparent(block: &[u8]) -> bool {
    let (c0, c1) = (u16::from_le_bytes([block[0], block[1]]), u16::from_le_bytes([block[2], block[3]]));
    let idx = u32::from_le_bytes(block[4..8].try_into().unwrap());
    c0 <= c1 && (0..16).any(|p| (idx >> (2 * p)) & 3 == 3)
}

/// DXT1 (`kind` 1), DXT3 (3) or DXT5 (5) blocks of a `width` x `height` image to RGBA8, top row
/// first. None if `data` is too short.
pub fn decode_dxt(kind: u8, width: u32, height: u32, data: &[u8]) -> Option<Vec<u8>> {
    let block_bytes = if kind == 1 { 8 } else { 16 };
    let (bw, bh) = (width.div_ceil(4) as usize, height.div_ceil(4) as usize);
    let blocks = data.get(..bw * bh * block_bytes)?;
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for (i, block) in blocks.chunks_exact(block_bytes).enumerate() {
        let texels = match kind {
            1 => colour_block(block, true),
            3 => {
                let mut t = colour_block(&block[8..], false);
                let a = u64::from_le_bytes(block[..8].try_into().unwrap());
                for (p, px) in t.iter_mut().enumerate() {
                    px[3] = ((a >> (4 * p)) & 0xF) as u8 * 17;
                }
                t
            }
            _ => {
                let mut t = colour_block(&block[8..], false);
                let alphas = dxt5_alphas(block[0], block[1]);
                let mut idx = [0u8; 8];
                idx[..6].copy_from_slice(&block[2..8]);
                let bits = u64::from_le_bytes(idx);
                for (p, px) in t.iter_mut().enumerate() {
                    px[3] = alphas[((bits >> (3 * p)) & 7) as usize];
                }
                t
            }
        };
        let (bx, by) = (i % bw, i / bw);
        for (p, px) in texels.iter().enumerate() {
            let (x, y) = (bx * 4 + p % 4, by * 4 + p / 4);
            if x < width as usize && y < height as usize {
                let o = (y * width as usize + x) * 4;
                rgba[o..o + 4].copy_from_slice(px);
            }
        }
    }
    Some(rgba)
}

/// Channel value under `mask`, scaled to 8 bits.
fn channel(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let max = mask >> mask.trailing_zeros();
    (((v & mask) >> mask.trailing_zeros()) as u64 * 255 / max as u64) as u8
}

fn rgb565(c: u16) -> [u32; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [((r << 3) | (r >> 2)) as u32, ((g << 2) | (g >> 4)) as u32, ((b << 3) | (b >> 2)) as u32]
}

/// The 16 texels of an 8-byte colour block. DXT1 allows the 3-colour + transparent mode when
/// c0 <= c1; DXT3/5 colour blocks always use 4 colours.
fn colour_block(b: &[u8], dxt1: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |w0: u32, w1: u32, d: u32| -> [u8; 4] {
        let c = |i: usize| ((w0 * p0[i] + w1 * p1[i]) / d) as u8;
        [c(0), c(1), c(2), 255]
    };
    let palette = if !dxt1 || c0 > c1 {
        [mix(1, 0, 1), mix(0, 1, 1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [mix(1, 0, 1), mix(0, 1, 1), mix(1, 1, 2), [0, 0, 0, 0]]
    };
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|p| palette[((idx >> (2 * p)) & 3) as usize])
}

fn dxt5_alphas(a0: u8, a1: u8) -> [u8; 8] {
    let (a, b) = (a0 as u32, a1 as u32);
    if a0 > a1 {
        std::array::from_fn(|i| match i {
            0 => a0,
            1 => a1,
            _ => (((8 - i as u32) * a + (i as u32 - 1) * b) / 7) as u8,
        })
    } else {
        std::array::from_fn(|i| match i {
            0 => a0,
            1 => a1,
            6 => 0,
            7 => 255,
            _ => (((6 - i as u32) * a + (i as u32 - 1) * b) / 5) as u8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u32, h: u32, pf_flags: u32, fourcc: &[u8; 4], bits: u32, masks: [u32; 4]) -> Vec<u8> {
        let mut d = b"DDS ".to_vec();
        for v in [124, 0x1007, h, w, 0, 0, 1] {
            d.extend_from_slice(&u32::to_le_bytes(v));
        }
        d.extend_from_slice(&[0; 44]);
        d.extend_from_slice(&32u32.to_le_bytes());
        d.extend_from_slice(&pf_flags.to_le_bytes());
        d.extend_from_slice(fourcc);
        d.extend_from_slice(&bits.to_le_bytes());
        for m in masks {
            d.extend_from_slice(&m.to_le_bytes());
        }
        d.extend_from_slice(&[0; 20]);
        assert_eq!(d.len(), 128);
        d
    }

    fn colour(c0: u16, c1: u16, idx: u32) -> Vec<u8> {
        [c0.to_le_bytes().as_slice(), &c1.to_le_bytes(), &idx.to_le_bytes()].concat()
    }

    fn px(img: &Image, x: usize, y: usize) -> [u8; 4] {
        let o = (y * img.width as usize + x) * 4;
        img.rgba[o..o + 4].try_into().unwrap()
    }

    #[test]
    fn blocks_single_level_and_mip_chain() {
        // 8x8 DXT1, one level: 4 blocks of opaque red.
        let mut d = header(8, 8, DDPF_FOURCC, b"DXT1", 0, [0; 4]);
        for _ in 0..4 {
            d.extend(colour(0xF800, 0x0000, 0));
        }
        let b = blocks(&d).unwrap().unwrap();
        assert_eq!((b.width, b.height, b.kind, b.levels.len(), b.has_alpha), (8, 8, 1, 1, false));
        assert_eq!(b.levels[0].len(), 32);

        // Same with a full chain (8x8, 4x4, 2x2, 1x1) and a transparent texel in the last block.
        let mut d = header(8, 8, DDPF_FOURCC, b"DXT1", 0, [0; 4]);
        d[8..12].copy_from_slice(&(0x1007u32 | DDSD_MIPMAPCOUNT).to_le_bytes());
        d[28..32].copy_from_slice(&4u32.to_le_bytes());
        for _ in 0..3 {
            d.extend(colour(0xF800, 0x0000, 0));
        }
        d.extend(colour(0x0000, 0xF800, 3));
        for _ in 0..3 {
            d.extend(colour(0xF800, 0x0000, 0));
        }
        let b = blocks(&d).unwrap().unwrap();
        assert_eq!((b.levels.len(), b.has_alpha), (4, true));
        assert!(b.levels.iter().skip(1).all(|l| l.len() == 8));

        // Truncated mip data is an error, an uncompressed DDS is not DXT.
        assert!(blocks(&d[..d.len() - 1]).is_err());
        let rgb = header(1, 1, DDPF_RGB, b"\0\0\0\0", 32, [0xFF0000, 0xFF00, 0xFF, 0]);
        assert_eq!(blocks(&rgb).unwrap(), None);
    }

    #[test]
    fn dxt1_four_colour() {
        let mut d = header(4, 4, DDPF_FOURCC, b"DXT1", 0, [0; 4]);
        d.extend(colour(0xF800, 0x001F, 1 << 10 | 2 << 12 | 3 << 14)); // pixels 5, 6, 7
        let img = decode(&d).unwrap();
        assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
        assert_eq!(px(&img, 1, 1), [0, 0, 255, 255]);
        assert_eq!(px(&img, 2, 1), [170, 0, 85, 255]);
        assert_eq!(px(&img, 3, 1), [85, 0, 170, 255]);
    }

    #[test]
    fn dxt1_punch_through() {
        let mut d = header(4, 4, DDPF_FOURCC, b"DXT1", 0, [0; 4]);
        d.extend(colour(0x001F, 0xF800, 3 | 2 << 2));
        let img = decode(&d).unwrap();
        assert_eq!(px(&img, 0, 0), [0, 0, 0, 0]);
        assert_eq!(px(&img, 1, 0), [127, 0, 127, 255]);
    }

    #[test]
    fn dxt3_and_dxt5_alpha() {
        let mut d = header(4, 4, DDPF_FOURCC, b"DXT3", 0, [0; 4]);
        d.extend_from_slice(&0xF0u64.to_le_bytes()); // pixel 0 alpha 0, pixel 1 alpha 15
        d.extend(colour(0x001F, 0xF800, 0)); // c0 < c1 still 4-colour: index 0 = c0
        let img = decode(&d).unwrap();
        assert_eq!(px(&img, 0, 0), [0, 0, 255, 0]);
        assert_eq!(px(&img, 1, 0)[3], 255);

        let mut d = header(4, 4, DDPF_FOURCC, b"DXT5", 0, [0; 4]);
        d.extend_from_slice(&[255, 0, 1 | 2 << 3, 0, 0, 0, 0, 0]); // pixel 0 -> idx 1, pixel 1 -> idx 2
        d.extend(colour(0, 0, 0));
        let img = decode(&d).unwrap();
        assert_eq!(px(&img, 0, 0)[3], 0);
        assert_eq!(px(&img, 1, 0)[3], (6 * 255 / 7) as u8);
        assert_eq!(px(&img, 2, 0)[3], 255);
        assert_eq!(dxt5_alphas(0, 100), [0, 100, 20, 40, 60, 80, 0, 255]);
    }

    #[test]
    fn uncompressed_bgra_and_bgr() {
        let mut d = header(2, 1, DDPF_RGB | DDPF_ALPHAPIXELS, &[0; 4], 32, [0xFF_0000, 0xFF00, 0xFF, 0xFF00_0000]);
        d.extend_from_slice(&[200, 150, 100, 255, 1, 2, 3, 128]);
        let img = decode(&d).unwrap();
        assert_eq!(img.rgba, vec![100, 150, 200, 255, 3, 2, 1, 128]);

        let mut d = header(1, 1, DDPF_RGB, &[0; 4], 24, [0xFF_0000, 0xFF00, 0xFF, 0]);
        d.extend_from_slice(&[10, 20, 30]);
        assert_eq!(decode(&d).unwrap().rgba, vec![30, 20, 10, 255]);

        // 16-bit-style narrow masks scale to full range.
        assert_eq!(channel(0x1F, 0x1F), 255);
        assert_eq!(channel(0, 0x1F), 0);
    }

    #[test]
    fn edges_and_errors() {
        let mut d = header(5, 3, DDPF_FOURCC, b"DXT1", 0, [0; 4]);
        d.extend(colour(0x07E0, 0x07E0, 0));
        d.extend(colour(0xF800, 0xF800, 0));
        let img = decode(&d).unwrap();
        assert_eq!(img.rgba.len(), 5 * 3 * 4);
        assert_eq!(px(&img, 3, 2), [0, 255, 0, 255]);
        assert_eq!(px(&img, 4, 0), [255, 0, 0, 255]);

        d.truncate(d.len() - 1);
        assert!(matches!(decode(&d), Err(Error::Truncated(..))));
        let d = header(4, 4, DDPF_FOURCC, b"ATI2", 0, [0; 4]);
        assert!(matches!(decode(&d), Err(Error::Invalid(_))));
        assert!(decode(b"PNG").is_err());
    }
}

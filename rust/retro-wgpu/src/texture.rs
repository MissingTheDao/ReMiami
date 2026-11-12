//! Texture formats of old games and their CPU-side conversion.
//!
//! Everything here is new code written for retro-wgpu from the public format definitions
//! (D3D/DXGI BC1-3, 16-bit packed formats, PS2 CLUT conventions).
//!
//! What the GPU stores:
//! - `Rgba8` and `Bgra8` as they are;
//! - BC1-3 (and BC7) compressed when the device samples BC formats and level 0 is whole 4x4
//!   blocks; otherwise BC1-3 are decoded here to RGBA8 (BC7 then needs a replacement path, see
//!   `replace`);
//! - the 16-bit packed formats and the paletted formats are **expanded to RGBA8 on upload**.
//!   wgpu has no portable 16-bit colour formats and no palette lookup sampler, and expanding on
//!   upload keeps filtering correct (a shader-side CLUT lookup can't filter bilinearly without
//!   four lookups). The palette and indices stay available to the caller (and to texture
//!   replacement hashing), so palette animation is done by re-uploading.

/// Pixel layout of texture data handed to the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexFormat {
    /// R, G, B, A bytes.
    Rgba8,
    /// B, G, R, A bytes (D3D's A8R8G8B8 in memory).
    Bgra8,
    /// 16-bit little-endian, R in bits 11-15, G 5-10, B 0-4 (D3D R5G6B5).
    Rgb565,
    /// A bit 15, R 10-14, G 5-9, B 0-4 (D3D A1R5G5B5).
    Argb1555,
    /// A bit 15, B 10-14, G 5-9, R 0-4 (PS2 PSMCT16 / GL ABGR1555).
    Abgr1555,
    /// R 11-15, G 6-10, B 1-5, A bit 0 (GL RGBA5551).
    Rgba5551,
    /// A 12-15, R 8-11, G 4-7, B 0-3 (D3D A4R4G4B4).
    Argb4444,
    /// R 12-15, G 8-11, B 4-7, A 0-3 (GL RGBA4444).
    Rgba4444,
    /// 4-bit palette indices, two per byte; `low_first`: the low nibble is the left texel
    /// (PS2 PSMT4), else the high nibble.
    Indexed4 { low_first: bool },
    /// 8-bit palette indices.
    Indexed8,
    /// DXT1 (with 1-bit alpha when colour0 <= colour1).
    Bc1,
    /// DXT2/3: explicit 4-bit alpha.
    Bc2,
    /// DXT4/5: interpolated alpha.
    Bc3,
    /// BPTC (high-res replacements); GPU only.
    Bc7,
}

impl TexFormat {
    pub fn is_block(self) -> bool {
        matches!(self, TexFormat::Bc1 | TexFormat::Bc2 | TexFormat::Bc3 | TexFormat::Bc7)
    }

    pub fn is_indexed(self) -> bool {
        matches!(self, TexFormat::Indexed4 { .. } | TexFormat::Indexed8)
    }

    /// Bytes of one mip level of `w` x `h` texels in this format.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        match self {
            TexFormat::Rgba8 | TexFormat::Bgra8 => w * h * 4,
            TexFormat::Rgb565 | TexFormat::Argb1555 | TexFormat::Abgr1555 | TexFormat::Rgba5551 | TexFormat::Argb4444 | TexFormat::Rgba4444 => w * h * 2,
            TexFormat::Indexed4 { .. } => (w * h).div_ceil(2),
            TexFormat::Indexed8 => w * h,
            TexFormat::Bc1 => w.div_ceil(4) * h.div_ceil(4) * 8,
            TexFormat::Bc2 | TexFormat::Bc3 | TexFormat::Bc7 => w.div_ceil(4) * h.div_ceil(4) * 16,
        }
    }

    /// The wgpu format this data is stored in, given whether the device samples BC formats.
    pub fn gpu_format(self, bc: bool, w: u32, h: u32) -> wgpu::TextureFormat {
        let blocks_ok = bc && w % 4 == 0 && h % 4 == 0;
        match self {
            TexFormat::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
            TexFormat::Bc1 if blocks_ok => wgpu::TextureFormat::Bc1RgbaUnorm,
            TexFormat::Bc2 if blocks_ok => wgpu::TextureFormat::Bc2RgbaUnorm,
            TexFormat::Bc3 if blocks_ok => wgpu::TextureFormat::Bc3RgbaUnorm,
            TexFormat::Bc7 if blocks_ok => wgpu::TextureFormat::Bc7RgbaUnorm,
            _ => wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

/// How 8-bit palettes are laid out in memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ClutLayout {
    /// Entry i is colour i.
    #[default]
    Linear,
    /// PS2 CSM1 storage of 256-entry palettes: bits 3 and 4 of the index are swapped.
    Ps2Csm1,
}

/// A colour lookup table for the indexed formats.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Palette {
    /// RGBA8 entries (16 or 256 used). PS2 games store alpha as 0..128; scale it to 0..255
    /// before handing the palette over if the game expects 0x80 = opaque.
    pub colors: Vec<[u8; 4]>,
    pub layout: ClutLayout,
}

impl Palette {
    pub fn lookup(&self, index: u8) -> [u8; 4] {
        let i = match self.layout {
            ClutLayout::Linear => index,
            ClutLayout::Ps2Csm1 => (index & 0xE7) | ((index & 0x08) << 1) | ((index & 0x10) >> 1),
        };
        self.colors.get(i as usize).copied().unwrap_or([0, 0, 0, 0])
    }
}

fn expand5(x: u16) -> u8 {
    ((x << 3) | (x >> 2)) as u8
}
fn expand6(x: u16) -> u8 {
    ((x << 2) | (x >> 4)) as u8
}
fn expand4(x: u16) -> u8 {
    (x * 17) as u8
}

/// One 16-bit texel to RGBA8.
pub fn unpack16(format: TexFormat, v: u16) -> [u8; 4] {
    match format {
        TexFormat::Rgb565 => [expand5(v >> 11), expand6((v >> 5) & 63), expand5(v & 31), 255],
        TexFormat::Argb1555 => [expand5((v >> 10) & 31), expand5((v >> 5) & 31), expand5(v & 31), if v >> 15 != 0 { 255 } else { 0 }],
        TexFormat::Abgr1555 => [expand5(v & 31), expand5((v >> 5) & 31), expand5((v >> 10) & 31), if v >> 15 != 0 { 255 } else { 0 }],
        TexFormat::Rgba5551 => [expand5(v >> 11), expand5((v >> 6) & 31), expand5((v >> 1) & 31), if v & 1 != 0 { 255 } else { 0 }],
        TexFormat::Argb4444 => [expand4((v >> 8) & 15), expand4((v >> 4) & 15), expand4(v & 15), expand4(v >> 12)],
        TexFormat::Rgba4444 => [expand4(v >> 12), expand4((v >> 8) & 15), expand4((v >> 4) & 15), expand4(v & 15)],
        _ => [0, 0, 0, 0],
    }
}

fn rgb565(c: u16) -> [u32; 3] {
    [expand5(c >> 11) as u32, expand6((c >> 5) & 63) as u32, expand5(c & 31) as u32]
}

/// The 16 RGBA texels of a BC1 colour block; `four_colour` forces the 4-colour mode (BC2/BC3).
fn bc1_block(b: &[u8], four_colour: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u32, b: u32, wa: u32, wb: u32, d: u32| ((a * wa + b * wb + d / 2) / d) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0] as u8, p0[1] as u8, p0[2] as u8, 255];
    pal[1] = [p1[0] as u8, p1[1] as u8, p1[2] as u8, 255];
    if c0 > c1 || four_colour {
        pal[2] = [mix(p0[0], p1[0], 2, 1, 3), mix(p0[1], p1[1], 2, 1, 3), mix(p0[2], p1[2], 2, 1, 3), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2, 3), mix(p0[1], p1[1], 1, 2, 3), mix(p0[2], p1[2], 1, 2, 3), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1, 2), mix(p0[1], p1[1], 1, 1, 2), mix(p0[2], p1[2], 1, 1, 2), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|i| pal[((bits >> (2 * i)) & 3) as usize])
}

/// Decodes one level of BC1/BC2/BC3 blocks to RGBA8 (`w` x `h`, top row first). None if the
/// data is short or the format isn't BC1-3.
pub fn decode_bc(format: TexFormat, w: u32, h: u32, data: &[u8]) -> Option<Vec<u8>> {
    let block = match format {
        TexFormat::Bc1 => 8,
        TexFormat::Bc2 | TexFormat::Bc3 => 16,
        _ => return None,
    };
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    if data.len() < bw * bh * block {
        return None;
    }
    let mut out = vec![0u8; (w * h * 4) as usize];
    for by in 0..bh {
        for bx in 0..bw {
            let b = &data[(by * bw + bx) * block..][..block];
            let mut texels = match format {
                TexFormat::Bc1 => bc1_block(b, false),
                _ => bc1_block(&b[8..], true),
            };
            match format {
                TexFormat::Bc2 => {
                    let a = u64::from_le_bytes(b[..8].try_into().unwrap());
                    for (i, t) in texels.iter_mut().enumerate() {
                        t[3] = expand4(((a >> (4 * i)) & 15) as u16);
                    }
                }
                TexFormat::Bc3 => {
                    let (a0, a1) = (b[0] as u32, b[1] as u32);
                    let mut pal = [0u8; 8];
                    pal[0] = a0 as u8;
                    pal[1] = a1 as u8;
                    if a0 > a1 {
                        for i in 1..7 {
                            pal[i + 1] = (((7 - i as u32) * a0 + i as u32 * a1 + 3) / 7) as u8;
                        }
                    } else {
                        for i in 1..5 {
                            pal[i + 1] = (((5 - i as u32) * a0 + i as u32 * a1 + 2) / 5) as u8;
                        }
                        pal[6] = 0;
                        pal[7] = 255;
                    }
                    let mut bits = 0u64;
                    for (k, &byte) in b[2..8].iter().enumerate() {
                        bits |= (byte as u64) << (8 * k);
                    }
                    for (i, t) in texels.iter_mut().enumerate() {
                        t[3] = pal[((bits >> (3 * i)) & 7) as usize];
                    }
                }
                _ => {}
            }
            for (i, t) in texels.iter().enumerate() {
                let (x, y) = (bx as u32 * 4 + i as u32 % 4, by as u32 * 4 + i as u32 / 4);
                if x < w && y < h {
                    out[((y * w + x) * 4) as usize..][..4].copy_from_slice(t);
                }
            }
        }
    }
    Some(out)
}

/// Converts one level of any non-block format to RGBA8. None if the data is short or needs a
/// palette that wasn't given.
pub fn to_rgba8(format: TexFormat, w: u32, h: u32, data: &[u8], palette: Option<&Palette>) -> Option<Vec<u8>> {
    let n = (w * h) as usize;
    if data.len() < format.level_size(w, h) {
        return None;
    }
    Some(match format {
        TexFormat::Rgba8 => data[..n * 4].to_vec(),
        TexFormat::Bgra8 => data[..n * 4].as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
        TexFormat::Indexed8 => {
            let p = palette?;
            data[..n].iter().flat_map(|&i| p.lookup(i)).collect()
        }
        TexFormat::Indexed4 { low_first } => {
            let p = palette?;
            (0..n).flat_map(|i| {
                let b = data[i / 2];
                let nib = if (i % 2 == 0) == low_first { b & 15 } else { b >> 4 };
                p.lookup(nib)
            })
            .collect()
        }
        f if f.is_block() => return decode_bc(f, w, h, data),
        f => data[..n * 2].as_chunks::<2>().0.iter().flat_map(|c| unpack16(f, u16::from_le_bytes(*c))).collect(),
    })
}

/// Number of levels of a full mip chain down to 1x1.
pub fn full_chain(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// The next mip level of an RGBA8 image: each texel is the rounded mean of the (up to) 2x2
/// texels it covers; odd edges repeat the last row/column.
pub fn downsample_rgba8(src: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let xs = [(2 * x).min(w - 1), (2 * x + 1).min(w - 1)];
            let ys = [(2 * y).min(h - 1), (2 * y + 1).min(h - 1)];
            for c in 0..4 {
                let sum: u32 = ys.iter().flat_map(|&sy| xs.iter().map(move |&sx| src[((sy * w + sx) * 4 + c) as usize] as u32)).sum();
                out[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// The whole mip chain of an RGBA8 image (level 0 included).
pub fn mip_chain(level0: &[u8], w: u32, h: u32) -> Vec<Vec<u8>> {
    let mut levels = vec![level0.to_vec()];
    let (mut cw, mut ch) = (w, h);
    while cw > 1 || ch > 1 {
        let (next, nw, nh) = downsample_rgba8(levels.last().unwrap(), cw, ch);
        levels.push(next);
        (cw, ch) = (nw, nh);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed16() {
        assert_eq!(unpack16(TexFormat::Rgb565, 0xF800), [255, 0, 0, 255]);
        assert_eq!(unpack16(TexFormat::Rgb565, 0x07E0), [0, 255, 0, 255]);
        assert_eq!(unpack16(TexFormat::Argb1555, 0x801F), [0, 0, 255, 255]);
        assert_eq!(unpack16(TexFormat::Abgr1555, 0x001F), [255, 0, 0, 0]);
        assert_eq!(unpack16(TexFormat::Rgba5551, 0xF801), [255, 0, 0, 255]);
        assert_eq!(unpack16(TexFormat::Argb4444, 0xF0F0), [0, 255, 0, 255]);
        assert_eq!(unpack16(TexFormat::Rgba4444, 0x0F0F), [0, 255, 0, 255]);
    }

    #[test]
    fn bc1_modes() {
        // c0 = red > c1 = blue: four colours; index 0,1,2,3 in the first row.
        let mut b = Vec::new();
        b.extend(0xF800u16.to_le_bytes());
        b.extend(0x001Fu16.to_le_bytes());
        b.extend(0xE4u32.to_le_bytes());
        let px = decode_bc(TexFormat::Bc1, 4, 4, &b).unwrap();
        assert_eq!(&px[0..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..8], &[0, 0, 255, 255]);
        assert_eq!(&px[8..12], &[170, 0, 85, 255]);
        assert_eq!(&px[12..16], &[85, 0, 170, 255]);
        // c0 <= c1: index 3 is transparent black, 2 the midpoint.
        let mut b = Vec::new();
        b.extend(0x001Fu16.to_le_bytes());
        b.extend(0xF800u16.to_le_bytes());
        b.extend(0xE4u32.to_le_bytes());
        let px = decode_bc(TexFormat::Bc1, 4, 4, &b).unwrap();
        assert_eq!(&px[8..12], &[128, 0, 128, 255]);
        assert_eq!(&px[12..16], &[0, 0, 0, 0]);
    }

    #[test]
    fn bc3_alpha() {
        let mut b = vec![255u8, 0];
        // indices 0, 1, 2 for the first three texels.
        b.extend([0b1000_1000u8, 0, 0, 0, 0, 0]);
        b.extend(0xFFFFu16.to_le_bytes());
        b.extend(0xFFFFu16.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let px = decode_bc(TexFormat::Bc3, 4, 4, &b).unwrap();
        assert_eq!([px[3], px[7], px[11]], [255, 0, 219]);
    }

    #[test]
    fn palettes() {
        let mut colors = vec![[0u8; 4]; 256];
        colors[8] = [1, 2, 3, 4];
        colors[16] = [5, 6, 7, 8];
        let lin = Palette { colors: colors.clone(), layout: ClutLayout::Linear };
        let csm = Palette { colors, layout: ClutLayout::Ps2Csm1 };
        assert_eq!(lin.lookup(8), [1, 2, 3, 4]);
        assert_eq!(csm.lookup(8), [5, 6, 7, 8]);
        assert_eq!(csm.lookup(16), [1, 2, 3, 4]);
        let p4 = Palette { colors: (0..16).map(|i| [i * 16, 0, 0, 255]).collect(), layout: ClutLayout::Linear };
        let low = to_rgba8(TexFormat::Indexed4 { low_first: true }, 2, 1, &[0x21], Some(&p4)).unwrap();
        assert_eq!(low, vec![16, 0, 0, 255, 32, 0, 0, 255]);
        let high = to_rgba8(TexFormat::Indexed4 { low_first: false }, 2, 1, &[0x21], Some(&p4)).unwrap();
        assert_eq!(high, vec![32, 0, 0, 255, 16, 0, 0, 255]);
    }

    #[test]
    fn mips() {
        let img: Vec<u8> = (0..3 * 2).flat_map(|i| [i as u8 * 10, 0, 0, 255]).collect();
        let chain = mip_chain(&img, 3, 2);
        assert_eq!(chain.len(), full_chain(3, 2) as usize);
        assert_eq!(chain.len(), 2);
        // (0 + 10 + 30 + 40) / 4 = 20
        assert_eq!(chain[1][0], 20);
    }
}

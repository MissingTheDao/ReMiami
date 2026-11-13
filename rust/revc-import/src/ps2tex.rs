//! PS2 native RenderWare texture dictionaries (Vice City's PS2 TXDs) decoded to RGBA.
//!
//! A PS2 texture is uploaded to the GS in one pixel format and sampled in another, both swizzled
//! in the GS's local memory. To read one, the upload is written into an emulated 4 MiB GS memory
//! and read back in the texture's own format (GS manual block and column tables). Palettised
//! textures (8 and 4 bit) carry their palette the same way; 8-bit palettes are stored with the
//! GS's CSM1 entry order. PS2 alpha runs 0..0x80 for 0..1.
//!
//! Layout per texture: Struct('PS2' platform), name, mask, then a Struct holding a 64-byte header
//! (width, height, depth, format, TEX0, TEX1, MIPTBP1/2, pixel and palette sizes, GPU size, sky mip
//! value) and a Struct with the pixels (and palette), each behind an 80-byte GIF upload header
//! when format & 0x20000. Ported from the research reader (ps2txd.py / gsmem.py).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Psm {
    Ct32,
    Ct16,
    T8,
    T4,
}

impl Psm {
    fn bits(self) -> usize {
        match self {
            Psm::Ct32 => 32,
            Psm::Ct16 => 16,
            Psm::T8 => 8,
            Psm::T4 => 4,
        }
    }
}

const BT32: [[u16; 8]; 4] = [
    [0, 1, 4, 5, 16, 17, 20, 21],
    [2, 3, 6, 7, 18, 19, 22, 23],
    [8, 9, 12, 13, 24, 25, 28, 29],
    [10, 11, 14, 15, 26, 27, 30, 31],
];
const CT32: [[u16; 8]; 8] = [
    [0, 1, 4, 5, 8, 9, 12, 13],
    [2, 3, 6, 7, 10, 11, 14, 15],
    [16, 17, 20, 21, 24, 25, 28, 29],
    [18, 19, 22, 23, 26, 27, 30, 31],
    [32, 33, 36, 37, 40, 41, 44, 45],
    [34, 35, 38, 39, 42, 43, 46, 47],
    [48, 49, 52, 53, 56, 57, 60, 61],
    [50, 51, 54, 55, 58, 59, 62, 63],
];
const BT16: [[u16; 4]; 8] = [
    [0, 2, 8, 10],
    [1, 3, 9, 11],
    [4, 6, 12, 14],
    [5, 7, 13, 15],
    [16, 18, 24, 26],
    [17, 19, 25, 27],
    [20, 22, 28, 30],
    [21, 23, 29, 31],
];
const CT16: [[u16; 16]; 8] = [
    [0, 2, 8, 10, 16, 18, 24, 26, 1, 3, 9, 11, 17, 19, 25, 27],
    [4, 6, 12, 14, 20, 22, 28, 30, 5, 7, 13, 15, 21, 23, 29, 31],
    [32, 34, 40, 42, 48, 50, 56, 58, 33, 35, 41, 43, 49, 51, 57, 59],
    [36, 38, 44, 46, 52, 54, 60, 62, 37, 39, 45, 47, 53, 55, 61, 63],
    [64, 66, 72, 74, 80, 82, 88, 90, 65, 67, 73, 75, 81, 83, 89, 91],
    [68, 70, 76, 78, 84, 86, 92, 94, 69, 71, 77, 79, 85, 87, 93, 95],
    [96, 98, 104, 106, 112, 114, 120, 122, 97, 99, 105, 107, 113, 115, 121, 123],
    [100, 102, 108, 110, 116, 118, 124, 126, 101, 103, 109, 111, 117, 119, 125, 127],
];
// The 8-bit column table: these 8 rows, then the same rows + 128.
const C8: [[u16; 16]; 8] = [
    [0, 4, 16, 20, 32, 36, 48, 52, 2, 6, 18, 22, 34, 38, 50, 54],
    [8, 12, 24, 28, 40, 44, 56, 60, 10, 14, 26, 30, 42, 46, 58, 62],
    [33, 37, 49, 53, 1, 5, 17, 21, 35, 39, 51, 55, 3, 7, 19, 23],
    [41, 45, 57, 61, 9, 13, 25, 29, 43, 47, 59, 63, 11, 15, 27, 31],
    [96, 100, 112, 116, 64, 68, 80, 84, 98, 102, 114, 118, 66, 70, 82, 86],
    [104, 108, 120, 124, 72, 76, 88, 92, 106, 110, 122, 126, 74, 78, 90, 94],
    [65, 69, 81, 85, 97, 101, 113, 117, 67, 71, 83, 87, 99, 103, 115, 119],
    [73, 77, 89, 93, 105, 109, 121, 125, 75, 79, 91, 95, 107, 111, 123, 127],
];
// The 4-bit column table: these 8 rows, then the same rows + 256.
const C4: [[u16; 32]; 8] = [
    [0, 8, 32, 40, 64, 72, 96, 104, 2, 10, 34, 42, 66, 74, 98, 106, 4, 12, 36, 44, 68, 76, 100, 108, 6, 14, 38, 46, 70, 78, 102, 110],
    [16, 24, 48, 56, 80, 88, 112, 120, 18, 26, 50, 58, 82, 90, 114, 122, 20, 28, 52, 60, 84, 92, 116, 124, 22, 30, 54, 62, 86, 94, 118, 126],
    [65, 73, 97, 105, 1, 9, 33, 41, 67, 75, 99, 107, 3, 11, 35, 43, 69, 77, 101, 109, 5, 13, 37, 45, 71, 79, 103, 111, 7, 15, 39, 47],
    [81, 89, 113, 121, 17, 25, 49, 57, 83, 91, 115, 123, 19, 27, 51, 59, 85, 93, 117, 125, 21, 29, 53, 61, 87, 95, 119, 127, 23, 31, 55, 63],
    [192, 200, 224, 232, 128, 136, 160, 168, 194, 202, 226, 234, 130, 138, 162, 170, 196, 204, 228, 236, 132, 140, 164, 172, 198, 206, 230, 238, 134, 142, 166, 174],
    [208, 216, 240, 248, 144, 152, 176, 184, 210, 218, 242, 250, 146, 154, 178, 186, 212, 220, 244, 252, 148, 156, 180, 188, 214, 222, 246, 254, 150, 158, 182, 190],
    [129, 137, 161, 169, 193, 201, 225, 233, 131, 139, 163, 171, 195, 203, 227, 235, 133, 141, 165, 173, 197, 205, 229, 237, 135, 143, 167, 175, 199, 207, 231, 239],
    [145, 153, 177, 185, 209, 217, 241, 249, 147, 155, 179, 187, 211, 219, 243, 251, 149, 157, 181, 189, 213, 221, 245, 253, 151, 159, 183, 191, 215, 223, 247, 255],
];

/// The GS address (in the format's units, from the start of memory) of pixel (x, y) of a buffer
/// at block `bp`, `bw` 64-pixel units wide.
fn address(fmt: Psm, x: usize, y: usize, bp: usize, bw: usize) -> usize {
    // (page w, page h, block w, block h, units per 256-byte block)
    let (pw, ph, bkw, bkh, upb) = match fmt {
        Psm::Ct32 => (64, 32, 8, 8, 64),
        Psm::Ct16 => (64, 64, 16, 8, 128),
        Psm::T8 => (128, 64, 16, 16, 256),
        Psm::T4 => (128, 128, 32, 16, 512),
    };
    let pages_per_row = (bw * 64 / pw).max(1);
    let page = (y / ph) * pages_per_row + x / pw;
    let (by, bx) = ((y % ph) / bkh, (x % pw) / bkw);
    let block = match fmt {
        Psm::Ct32 | Psm::T8 => BT32[by][bx],
        Psm::Ct16 | Psm::T4 => BT16[by][bx],
    } as usize;
    let (cy, cx) = (y % bkh, x % bkw);
    let column = match fmt {
        Psm::Ct32 => CT32[cy][cx],
        Psm::Ct16 => CT16[cy][cx],
        Psm::T8 => C8[cy % 8][cx] + if cy >= 8 { 128 } else { 0 },
        Psm::T4 => C4[cy % 8][cx] + if cy >= 8 { 256 } else { 0 },
    } as usize;
    (bp + page * 32 + block) * upb + column
}

/// The GS's 4 MiB local memory.
pub struct Gs {
    mem: Vec<u8>,
}

impl Default for Gs {
    fn default() -> Self {
        Gs { mem: vec![0; 4 * 1024 * 1024] }
    }
}

impl Gs {
    /// Writes a w x h upload in `fmt` (pixels in raster order; 4-bit: low nibble first).
    pub fn write(&mut self, fmt: Psm, data: &[u8], w: usize, h: usize, bp: usize, bw: usize) -> Result<(), String> {
        let bits = fmt.bits();
        if data.len() < w * h * bits / 8 {
            return Err(format!("upload of {w}x{h} needs {} bytes, has {}", w * h * bits / 8, data.len()));
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let a = address(fmt, x, y, bp, bw);
                if bits >= 8 {
                    let n = bits / 8;
                    let dst = self.mem.get_mut(a * n..a * n + n).ok_or("upload outside GS memory")?;
                    dst.copy_from_slice(&data[i * n..i * n + n]);
                } else {
                    let nib = if i & 1 == 0 { data[i / 2] & 15 } else { data[i / 2] >> 4 };
                    let b = self.mem.get_mut(a / 2).ok_or("upload outside GS memory")?;
                    *b = if a & 1 == 0 { (*b & 0xf0) | nib } else { (*b & 0x0f) | (nib << 4) };
                }
            }
        }
        Ok(())
    }

    /// Reads a w x h rectangle as `fmt`: one value per pixel (32/16-bit pixels little endian, or
    /// the palette index).
    pub fn read(&self, fmt: Psm, w: usize, h: usize, bp: usize, bw: usize) -> Result<Vec<u32>, String> {
        let bits = fmt.bits();
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let a = address(fmt, x, y, bp, bw);
                let v = if bits >= 8 {
                    let n = bits / 8;
                    let s = self.mem.get(a * n..a * n + n).ok_or("read outside GS memory")?;
                    s.iter().rev().fold(0u32, |acc, &b| (acc << 8) | b as u32)
                } else {
                    let b = *self.mem.get(a / 2).ok_or("read outside GS memory")?;
                    (if a & 1 == 0 { b & 15 } else { b >> 4 }) as u32
                };
                out.push(v);
            }
        }
        Ok(out)
    }
}

/// A decoded texture.
pub struct Ps2Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// width * height RGBA pixels, alpha 0..255.
    pub rgba: Vec<u8>,
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("truncated at {o}"))
}

fn u64_at(d: &[u8], o: usize) -> Result<u64, String> {
    Ok(u32_at(d, o)? as u64 | (u32_at(d, o + 4)? as u64) << 32)
}

/// A chunk header: (type, size).
fn chunk(d: &[u8], o: usize) -> Result<(u32, usize), String> {
    Ok((u32_at(d, o)?, u32_at(d, o + 4)? as usize))
}

/// An 80-byte GIF upload header: (width, height, data offset, data bytes).
fn upload(d: &[u8], o: usize) -> Result<(usize, usize, usize, usize), String> {
    let rrw = u32_at(d, o + 0x20)? as usize;
    let rrh = u32_at(d, o + 0x24)? as usize;
    let nq = (u32_at(d, o + 0x40)? & 0x7fff) as usize;
    Ok((rrw, rrh, o + 0x50, nq * 16))
}

/// The upload's pixel format, from its size and byte count.
fn guess_format(rrw: usize, rrh: usize, nbytes: usize) -> Option<Psm> {
    [Psm::Ct32, Psm::Ct16, Psm::T8, Psm::T4].into_iter().find(|f| rrw * rrh * f.bits() / 8 == nbytes)
}

fn expand5(c: u32) -> u8 {
    ((c << 3) | (c >> 2)) as u8
}

/// Decodes one texture: `r` is its 64-byte header, `data` the start of its pixel data.
fn decode(d: &[u8], r: usize, data: usize) -> Result<(u32, u32, Vec<u8>), String> {
    let w = u32_at(d, r)? as usize;
    let h = u32_at(d, r + 4)? as usize;
    let fmt = u32_at(d, r + 12)?;
    let tex0 = u64_at(d, r + 16)?;
    let psz = u32_at(d, r + 48)? as usize;
    let palsz = u32_at(d, r + 52)? as usize;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(format!("odd size {w}x{h}"));
    }
    let psm = match (tex0 >> 20) & 0x3f {
        0x00 => Psm::Ct32,
        0x02 | 0x0a => Psm::Ct16,
        0x13 => Psm::T8,
        0x14 => Psm::T4,
        other => return Err(format!("pixel format {other:#x} not handled")),
    };
    let tbw = ((tex0 >> 14) & 0x3f) as usize;
    let has_header = fmt & 0x20000 != 0;
    let pixels: Vec<u32> = if has_header {
        let (rrw, rrh, po, n) = upload(d, data)?;
        let up = guess_format(rrw, rrh, n).ok_or_else(|| format!("upload {rrw}x{rrh} of {n} bytes: no format fits"))?;
        let src = d.get(po..po + n).ok_or("upload data truncated")?;
        let mut gs = Gs::default();
        gs.write(up, src, rrw, rrh, 0, (rrw / 64).max(1))?;
        gs.read(psm, w, h, 0, tbw.max(1))?
    } else {
        let bits = psm.bits();
        let raw = d.get(data..data + w * h * bits / 8).ok_or("pixel data truncated")?;
        (0..w * h)
            .map(|i| match psm {
                Psm::T4 => (if i & 1 == 0 { raw[i / 2] & 15 } else { raw[i / 2] >> 4 }) as u32,
                Psm::T8 => raw[i] as u32,
                Psm::Ct16 => u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]) as u32,
                Psm::Ct32 => u32::from_le_bytes([raw[4 * i], raw[4 * i + 1], raw[4 * i + 2], raw[4 * i + 3]]),
            })
            .collect()
    };
    let mut rgba = Vec::with_capacity(w * h * 4);
    match psm {
        Psm::T4 | Psm::T8 => {
            let pd = data + psz;
            let pal_bytes = if has_header {
                let (_, _, ppo, pn) = upload(d, pd)?;
                d.get(ppo..ppo + pn).ok_or("palette truncated")?
            } else {
                d.get(pd..pd + palsz).ok_or("palette truncated")?
            };
            let mut pal: Vec<[u8; 4]> = pal_bytes.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
            if psm == Psm::T8 {
                // CSM1: entries 8-15 and 16-23 of each block of 32 are stored swapped
                let n = pal.len().min(256) / 32 * 32;
                for block in pal[..n].chunks_exact_mut(32) {
                    for k in 8..16 {
                        block.swap(k, k + 8);
                    }
                }
            }
            for &i in &pixels {
                let c = pal.get(i as usize).ok_or_else(|| format!("palette index {i} beyond {} entries", pal.len()))?;
                rgba.extend_from_slice(c);
            }
        }
        Psm::Ct32 => pixels.iter().for_each(|&v| rgba.extend_from_slice(&v.to_le_bytes())),
        Psm::Ct16 => pixels.iter().for_each(|&v| {
            rgba.extend_from_slice(&[expand5(v & 31), expand5((v >> 5) & 31), expand5((v >> 10) & 31), if v & 0x8000 != 0 { 0x80 } else { 0 }])
        }),
    }
    for px in rgba.chunks_exact_mut(4) {
        px[3] = (px[3] as u16 * 2).min(255) as u8; // PS2 alpha 0x80 = 1.0
    }
    Ok((w as u32, h as u32, rgba))
}

/// Every texture of a PS2 TXD (one Result per texture: a texture that can't be decoded doesn't
/// stop the rest).
pub fn read_ps2_txd(d: &[u8]) -> Vec<Result<Ps2Texture, String>> {
    let mut out = Vec::new();
    let Ok((_, s)) = chunk(d, 12) else { return out };
    let mut o = 12 + 12 + s; // past the dictionary's own struct
    while o + 12 < d.len() {
        let Ok((t, s)) = chunk(d, o) else { break };
        let end = o + 12 + s;
        if t != 0x15 {
            o = end;
            continue;
        }
        out.push(texture(d, o));
        o = end;
    }
    out
}

/// One texture native chunk at `o`.
fn texture(d: &[u8], o: usize) -> Result<Ps2Texture, String> {
    let mut p = o + 12;
    let (_, s2) = chunk(d, p)?; // Struct: platform 'PS2' and filter
    p += 12 + s2;
    let mut names = Vec::new();
    for _ in 0..2 {
        let (_, s3) = chunk(d, p)?;
        let raw = d.get(p + 12..p + 12 + s3).ok_or("name truncated")?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        names.push(raw[..end].iter().map(|&b| b as char).collect::<String>());
        p += 12 + s3;
    }
    p += 12; // into the Struct holding the header and the pixel data
    let (_, s5) = chunk(d, p)?;
    let r = p + 12;
    let q = r + s5;
    let name = names.swap_remove(0);
    let (width, height, rgba) = decode(d, r, q + 12).map_err(|e| format!("{name}: {e}"))?;
    Ok(Ps2Texture { name, width, height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gs_round_trip_in_each_format() {
        for fmt in [Psm::Ct32, Psm::Ct16, Psm::T8, Psm::T4] {
            let (w, h) = (128, 64);
            let bytes = w * h * fmt.bits() / 8;
            let data: Vec<u8> = (0..bytes).map(|i| (i * 7 + 3) as u8).collect();
            let mut gs = Gs::default();
            gs.write(fmt, &data, w, h, 0, 2).unwrap();
            let back = gs.read(fmt, w, h, 0, 2).unwrap();
            for (i, v) in back.iter().enumerate() {
                let want = match fmt {
                    Psm::Ct32 => u32::from_le_bytes([data[4 * i], data[4 * i + 1], data[4 * i + 2], data[4 * i + 3]]),
                    Psm::Ct16 => u16::from_le_bytes([data[2 * i], data[2 * i + 1]]) as u32,
                    Psm::T8 => data[i] as u32,
                    Psm::T4 => (if i & 1 == 0 { data[i / 2] & 15 } else { data[i / 2] >> 4 }) as u32,
                };
                assert_eq!(*v, want, "{fmt:?} pixel {i}");
            }
        }
    }

    #[test]
    fn swizzle_moves_pixels() {
        // a CT32 upload read back as T8 is not the identity: the layouts differ
        let data: Vec<u8> = (0..64 * 32 * 4).map(|i| i as u8).collect();
        let mut gs = Gs::default();
        gs.write(Psm::Ct32, &data, 64, 32, 0, 1).unwrap();
        let t8 = gs.read(Psm::T8, 128, 64, 0, 2).unwrap();
        assert!(t8.iter().enumerate().any(|(i, &v)| v != data[i] as u32));
    }

    #[test]
    fn raw_palettised_texture_decodes() {
        // one TXD with a 2x2 T8 texture without upload headers: palette entry 1 is opaque red
        let mut header = vec![0u8; 64];
        header[0..4].copy_from_slice(&2u32.to_le_bytes()); // width
        header[4..8].copy_from_slice(&2u32.to_le_bytes()); // height
        let tex0: u64 = 0x13 << 20; // PSMT8
        header[16..24].copy_from_slice(&tex0.to_le_bytes());
        header[48..52].copy_from_slice(&4u32.to_le_bytes()); // pixel bytes
        header[52..56].copy_from_slice(&1024u32.to_le_bytes()); // palette bytes
        let mut pixels = vec![1u8, 0, 0, 1];
        let mut pal = vec![0u8; 1024];
        pal[4..8].copy_from_slice(&[255, 0, 0, 0x80]);
        pixels.extend_from_slice(&pal);
        let chunk = |t: u32, body: &[u8]| [&t.to_le_bytes()[..], &(body.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), body].concat();
        let inner = [chunk(1, &header), chunk(1, &pixels)].concat();
        let native = [chunk(1, b"PS2\0"), chunk(2, b"dot\0"), chunk(2, b"\0"), chunk(1, &inner)].concat();
        let txd = [chunk(0x16, &[chunk(1, &[1, 0, 0, 0]), chunk(0x15, &native)].concat())].concat();
        let t = read_ps2_txd(&txd);
        assert_eq!(t.len(), 1);
        let t = t[0].as_ref().unwrap();
        assert_eq!((t.name.as_str(), t.width, t.height), ("dot", 2, 2));
        assert_eq!(&t.rgba[0..4], &[255, 0, 0, 255]); // index 1, alpha 0x80 -> 255
        assert_eq!(&t.rgba[4..8], &[0, 0, 0, 0]);
    }
}

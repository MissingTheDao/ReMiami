//! High-resolution texture replacement (stage 2), in the spirit of Dolphin/PCSX2 texture packs
//! but with its own key scheme and code.
//!
//! **Key.** Every texture upload (level 0 of a non-render-target texture) is hashed: a 64-bit
//! FNV-1a over a version tag, the format, the size and the level-0 bytes exactly as the game
//! handed them over; indexed textures add a second hash over the palette (entries and layout).
//! Names look like
//!
//! ```text
//! rt1_256x128_bc1_0123456789abcdef            (non-indexed)
//! rt1_64x64_i8_0123456789abcdef_fedcba9876543210   (indexed: data hash, palette hash)
//! rt1_64x64_i8_0123456789abcdef_any            (indexed, any palette)
//! ```
//!
//! **Packs.** A pack is a directory tree; files named `<key>.png` or `<key>.dds` anywhere in it
//! replace the texture with that key (first found wins, sorted by path). Replacements may have
//! any size and aspect: texture coordinates are normalised, and the game's sampler state
//! (addressing, filter, mip use) is kept, so the replacement samples exactly like the original
//! would at higher density. PNG replacements get a full mip chain (box filter); DDS files bring
//! their own mips (BC1/2/3, BC7 via a DX10 header, or 32-bit RGBA/BGRA). BC formats need a GPU
//! that samples them (else the replacement is skipped with a warning).
//!
//! **Filtering options.** `anisotropy` raises anisotropic filtering for replaced textures whose
//! sampler already filters linearly; `force_mips` turns on trilinear mip filtering for replaced
//! textures whose game sampler is linear without mips. Both default to off (original sampling).
//!
//! **Dump mode.** With `dump_dir`, each distinct original texture is written once as
//! `<key>.png` (RGBA8 after format conversion), ready to be edited and put in a pack.

use crate::gpu::Gpu;
use crate::renderer::{Renderer, TextureDesc, TextureId};
use crate::texture::{ClutLayout, Palette, TexFormat, mip_chain, to_rgba8};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Replacement settings.
#[derive(Clone, Debug, Default)]
pub struct ReplaceConfig {
    /// Texture pack directory (searched recursively).
    pub pack_dir: Option<PathBuf>,
    /// Write originals here as `<key>.png`.
    pub dump_dir: Option<PathBuf>,
    /// Anisotropy for replaced textures with linear sampling (1 = keep the game's).
    pub anisotropy: u16,
    /// Trilinear filtering for replaced textures sampled linearly without mips.
    pub force_mips: bool,
}

/// FNV-1a, 64 bit.
fn fnv(h: u64, data: &[u8]) -> u64 {
    data.iter().fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}
const FNV_INIT: u64 = 0xcbf2_9ce4_8422_2325;

fn format_tag(f: TexFormat) -> &'static str {
    match f {
        TexFormat::Rgba8 => "rgba8",
        TexFormat::Bgra8 => "bgra8",
        TexFormat::Rgb565 => "rgb565",
        TexFormat::Argb1555 => "argb1555",
        TexFormat::Abgr1555 => "abgr1555",
        TexFormat::Rgba5551 => "rgba5551",
        TexFormat::Argb4444 => "argb4444",
        TexFormat::Rgba4444 => "rgba4444",
        TexFormat::Indexed4 { low_first: true } => "i4l",
        TexFormat::Indexed4 { low_first: false } => "i4h",
        TexFormat::Indexed8 => "i8",
        TexFormat::Bc1 => "bc1",
        TexFormat::Bc2 => "bc2",
        TexFormat::Bc3 => "bc3",
        TexFormat::Bc7 => "bc7",
    }
}

/// The replacement key of a texture: (exact name, palette-independent name for indexed ones).
pub fn texture_key(width: u32, height: u32, format: TexFormat, level0: &[u8], palette: Option<&Palette>) -> (String, Option<String>) {
    let size = format.level_size(width, height).min(level0.len());
    let mut h = fnv(FNV_INIT, b"retro-wgpu texture v1");
    h = fnv(h, format_tag(format).as_bytes());
    h = fnv(h, &width.to_le_bytes());
    h = fnv(h, &height.to_le_bytes());
    h = fnv(h, &level0[..size]);
    let base = format!("rt1_{width}x{height}_{}_{h:016x}", format_tag(format));
    if !format.is_indexed() {
        return (base, None);
    }
    let used = match format {
        TexFormat::Indexed4 { .. } => 16,
        _ => 256,
    };
    let mut p = fnv(FNV_INIT, b"palette");
    if let Some(pal) = palette {
        p = fnv(p, &[matches!(pal.layout, ClutLayout::Ps2Csm1) as u8]);
        for c in pal.colors.iter().take(used) {
            p = fnv(p, c);
        }
    }
    (format!("{base}_{p:016x}"), Some(format!("{base}_any")))
}

/// A decoded replacement: wgpu format, size and mip levels.
#[derive(Clone)]
pub struct ReplacementImage {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Vec<u8>>,
}

/// Reads a PNG as RGBA8.
pub fn load_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(format!("{}: indexed PNG not expanded", path.display())),
    };
    Ok((info.width, info.height, rgba))
}

/// Writes RGBA8 as a PNG.
pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), width, height);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().and_then(|mut w| w.write_image_data(rgba)).map_err(|e| format!("{}: {e}", path.display()))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// Parses a DDS file: BC1/2/3 (DXT1-5 FourCC), BC7 (DX10 header, DXGI 98/99), or uncompressed
/// 32-bit RGBA/BGRA; all stored mip levels.
pub fn parse_dds(data: &[u8]) -> Result<ReplacementImage, String> {
    if data.len() < 128 || &data[..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    let height = u32_at(data, 12);
    let width = u32_at(data, 16);
    let mips = u32_at(data, 28).max(1);
    let pf_flags = u32_at(data, 80);
    let fourcc = &data[84..88];
    let bits = u32_at(data, 88);
    let rmask = u32_at(data, 92);
    let mut offset = 128;
    use wgpu::TextureFormat as F;
    let format = if pf_flags & 4 != 0 {
        match fourcc {
            b"DXT1" => F::Bc1RgbaUnorm,
            b"DXT2" | b"DXT3" => F::Bc2RgbaUnorm,
            b"DXT4" | b"DXT5" => F::Bc3RgbaUnorm,
            b"DX10" => {
                if data.len() < 148 {
                    return Err("short DX10 header".into());
                }
                offset = 148;
                match u32_at(data, 128) {
                    71 | 72 => F::Bc1RgbaUnorm,
                    74 | 75 => F::Bc2RgbaUnorm,
                    77 | 78 => F::Bc3RgbaUnorm,
                    98 | 99 => F::Bc7RgbaUnorm,
                    28 | 29 => F::Rgba8Unorm,
                    87 | 91 => F::Bgra8Unorm,
                    other => return Err(format!("unsupported DXGI format {other}")),
                }
            }
            other => return Err(format!("unsupported FourCC {:?}", String::from_utf8_lossy(other))),
        }
    } else if pf_flags & 0x40 != 0 && bits == 32 {
        if rmask == 0x00FF_0000 { F::Bgra8Unorm } else { F::Rgba8Unorm }
    } else {
        return Err("unsupported DDS pixel format".into());
    };
    let mut levels = Vec::new();
    for l in 0..mips {
        let (w, h) = ((width >> l).max(1), (height >> l).max(1));
        let size = match format.block_copy_size(None) {
            Some(b) if format.is_compressed() => (w.div_ceil(4) * h.div_ceil(4) * b) as usize,
            _ => (w * h * 4) as usize,
        };
        if offset + size > data.len() {
            break;
        }
        levels.push(data[offset..offset + size].to_vec());
        offset += size;
    }
    if levels.is_empty() {
        return Err("DDS without data".into());
    }
    Ok(ReplacementImage { format, width, height, levels })
}

/// Replacement state of a renderer.
pub struct Replacer {
    pub config: ReplaceConfig,
    /// Key -> file in the pack.
    index: HashMap<String, PathBuf>,
    /// Decoded replacements (None: failed to load).
    cache: HashMap<String, Option<ReplacementImage>>,
    dumped: std::collections::HashSet<String>,
    /// Replaced textures (for sampler upgrades).
    pub(crate) replaced: std::collections::HashSet<TextureId>,
}

fn scan(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            scan(&p, out);
        } else {
            out.push(p);
        }
    }
}

impl Replacer {
    pub fn new(config: ReplaceConfig) -> Replacer {
        let mut r = Replacer { config, index: HashMap::new(), cache: HashMap::new(), dumped: Default::default(), replaced: Default::default() };
        r.rescan();
        if let Some(d) = &r.config.dump_dir {
            let _ = std::fs::create_dir_all(d);
        }
        r
    }

    /// Re-reads the pack directory (e.g. after the user added files).
    pub fn rescan(&mut self) {
        self.index.clear();
        self.cache.clear();
        let Some(dir) = self.config.pack_dir.clone() else { return };
        let mut files = Vec::new();
        scan(&dir, &mut files);
        for f in files {
            let ext = f.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("png") | Some("dds")) {
                continue;
            }
            if let Some(stem) = f.file_stem().and_then(|s| s.to_str()) {
                self.index.entry(stem.to_string()).or_insert(f);
            }
        }
    }

    /// Number of replacement files found in the pack.
    pub fn pack_len(&self) -> usize {
        self.index.len()
    }

    fn load(&mut self, key: &str, bc: bool) -> Option<ReplacementImage> {
        if let Some(c) = self.cache.get(key) {
            return c.clone();
        }
        let path = self.index.get(key)?.clone();
        let img = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dds")) {
            std::fs::read(&path).map_err(|e| e.to_string()).and_then(|d| parse_dds(&d))
        } else {
            load_png(&path).map(|(w, h, px)| ReplacementImage { format: wgpu::TextureFormat::Rgba8Unorm, width: w, height: h, levels: mip_chain(&px, w, h) })
        };
        let img = match img {
            Ok(i) if i.format.is_compressed() && (!bc || i.width % 4 != 0 || i.height % 4 != 0) => {
                eprintln!("retro-wgpu: {}: block-compressed replacement needs BC support and a size in whole blocks; skipped", path.display());
                None
            }
            Ok(i) => Some(i),
            Err(e) => {
                eprintln!("retro-wgpu: replacement {}: {e}", path.display());
                None
            }
        };
        self.cache.insert(key.to_string(), img.clone());
        img
    }
}

impl Renderer {
    /// Turns texture replacement/dumping on (or off with None). Applies to uploads from now on.
    pub fn set_texture_replacement(&mut self, config: Option<ReplaceConfig>) {
        self.replacer = config.map(Replacer::new);
    }

    pub fn texture_replacer(&mut self) -> Option<&mut Replacer> {
        self.replacer.as_mut()
    }

    /// Whether a texture currently shows a replacement.
    pub fn is_replaced(&self, id: TextureId) -> bool {
        self.replacer.as_ref().is_some_and(|r| r.replaced.contains(&id))
    }
}

/// Called for level-0 uploads while a replacer is set: dumps the original, swaps in a
/// replacement. Returns true when the texture now shows a replacement.
pub(crate) fn on_upload(r: &mut Renderer, gpu: &Gpu, id: TextureId, desc: TextureDesc, data: &[u8], palette: Option<&Palette>) -> bool {
    let (key, any) = texture_key(desc.width, desc.height, desc.format, data, palette);
    let bc = gpu.bc_textures();
    let Some(rep) = r.replacer.as_mut() else { return false };
    if let Some(dir) = rep.config.dump_dir.clone()
        && rep.dumped.insert(key.clone())
    {
        let path = dir.join(format!("{key}.png"));
        if !path.exists()
            && let Some(px) = to_rgba8(desc.format, desc.width.max(1), desc.height.max(1), data, palette)
            && let Err(e) = save_png(&path, desc.width.max(1), desc.height.max(1), &px)
        {
            eprintln!("retro-wgpu: dump: {e}");
        }
    }
    let img = rep.load(&key, bc).or_else(|| any.as_deref().and_then(|a| rep.load(a, bc)));
    match img {
        Some(img) => {
            rep.replaced.insert(id);
            r.replace_texture_storage(gpu, id, img.format, img.width, img.height, &img.levels);
            true
        }
        None => {
            rep.replaced.remove(&id);
            false
        }
    }
}

/// The sampler a replaced texture is drawn with (see `ReplaceConfig`).
pub(crate) fn upgrade_sampler(cfg: &ReplaceConfig, mut s: crate::state::Sampler) -> crate::state::Sampler {
    use crate::state::Filter;
    if s.min == Filter::Linear {
        if cfg.force_mips && s.mip.is_none() {
            s.mip = Some(Filter::Linear);
        }
        if cfg.anisotropy > 1 && s.mip.is_some() {
            s.anisotropy = s.anisotropy.max(cfg.anisotropy);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable() {
        // These exact strings are the pack format: changing them breaks every pack.
        let (k, any) = texture_key(2, 2, TexFormat::Rgba8, &[1u8; 16], None);
        assert_eq!(k, "rt1_2x2_rgba8_88454b16acfc4a59");
        assert!(any.is_none());
        let pal = Palette { colors: vec![[1, 2, 3, 4]; 16], layout: ClutLayout::Linear };
        let (k, any) = texture_key(4, 2, TexFormat::Indexed4 { low_first: true }, &[0x10, 0x32, 0x54, 0x76], Some(&pal));
        assert!(k.starts_with("rt1_4x2_i4l_") && k.len() == "rt1_4x2_i4l_".len() + 33, "{k}");
        assert_eq!(any.unwrap(), format!("{}_any", &k[..k.len() - 17]));
        // Different palette, same data: same base, different palette hash.
        let pal2 = Palette { colors: vec![[9, 2, 3, 4]; 16], layout: ClutLayout::Linear };
        let (k2, _) = texture_key(4, 2, TexFormat::Indexed4 { low_first: true }, &[0x10, 0x32, 0x54, 0x76], Some(&pal2));
        assert_eq!(k[..k.len() - 16], k2[..k2.len() - 16]);
        assert_ne!(k, k2);
    }

    #[test]
    fn dds_headers() {
        let mut d = vec![0u8; 128];
        d[..4].copy_from_slice(b"DDS ");
        d[12..16].copy_from_slice(&8u32.to_le_bytes());
        d[16..20].copy_from_slice(&4u32.to_le_bytes());
        d[28..32].copy_from_slice(&2u32.to_le_bytes());
        d[80..84].copy_from_slice(&4u32.to_le_bytes());
        d[84..88].copy_from_slice(b"DXT1");
        d.extend(vec![7u8; 16 + 8]);
        let img = parse_dds(&d).unwrap();
        assert_eq!((img.format, img.width, img.height, img.levels.len()), (wgpu::TextureFormat::Bc1RgbaUnorm, 4, 8, 2));
        assert_eq!(img.levels[0].len(), 16);
        assert_eq!(img.levels[1].len(), 8);
    }
}

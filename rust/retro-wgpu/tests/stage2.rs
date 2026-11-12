//! Stage 2: texture replacement packs, dumping, DDS/BC7, filtering upgrades.

mod common;
use common::*;
use retro_wgpu::replace::{load_png, save_png};
use retro_wgpu::*;
use std::path::PathBuf;

macro_rules! per_backend {
    ($name:ident) => {
        mod $name {
            #[test]
            fn vulkan() {
                super::$name(retro_wgpu::Backend::Vulkan);
            }
            #[test]
            fn gl() {
                super::$name(retro_wgpu::Backend::Gl);
            }
            #[cfg(windows)]
            #[test]
            fn dx12() {
                super::$name(retro_wgpu::Backend::Dx12);
            }
        }
    };
}

fn scratch(name: &str, backend: Backend) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("retro-wgpu-{name}-{backend:?}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn draw_full(c: &mut Ctx, tex: TextureId, sampler: Sampler, uv_scale: f32) -> Image {
    let l = c.layout(PCU);
    let mut w = VertexWriter::new();
    for (x, y, u, v) in [(-1.0, -1.0, 0.0, 1.0), (1.0, -1.0, 1.0, 1.0), (1.0, 1.0, 1.0, 0.0), (-1.0, 1.0, 0.0, 0.0)] {
        w.f32s(&[x, y, 0.5]).u8x4([255; 4]).f32s(&[u * uv_scale, v * uv_scale]);
    }
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &w.bytes, Indices::None, &RenderState::default().with_texture(tex, sampler), &DrawParams::default());
    c.frame()
}

const ORIGINAL: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];

fn dump_and_replace(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 64), SceneSettings::default()) else { return };
    let dump = scratch("dump", backend);
    c.r.set_texture_replacement(Some(ReplaceConfig { dump_dir: Some(dump.clone()), ..Default::default() }));
    let t = c.r.upload_texture(&c.gpu, 2, 2, TexFormat::Rgba8, &[&ORIGINAL], None, false).unwrap();
    let (key, _) = texture_key(2, 2, TexFormat::Rgba8, &ORIGINAL, None);
    let (w, h, px) = load_png(&dump.join(format!("{key}.png"))).expect("dumped original");
    assert_eq!((w, h, px.as_slice()), (2, 2, &ORIGINAL[..]));
    assert!(!c.r.is_replaced(t));

    // A 32x32 replacement: left half yellow, right half magenta, in a sub-folder of the pack.
    let pack = scratch("pack", backend);
    std::fs::create_dir_all(pack.join("sub")).unwrap();
    let rep: Vec<u8> = (0..32 * 32).flat_map(|i| if i % 32 < 16 { [255, 255, 0, 255] } else { [255, 0, 255, 255] }).collect();
    save_png(&pack.join("sub").join(format!("{key}.png")), 32, 32, &rep).unwrap();
    c.r.set_texture_replacement(Some(ReplaceConfig { pack_dir: Some(pack), ..Default::default() }));
    assert_eq!(c.r.texture_replacer().unwrap().pack_len(), 1);
    let t = c.r.upload_texture(&c.gpu, 2, 2, TexFormat::Rgba8, &[&ORIGINAL], None, false).unwrap();
    assert!(c.r.is_replaced(t));
    assert_eq!(c.r.texture_size(t), Some((32, 32)));
    let img = draw_full(&mut c, t, Sampler::POINT_CLAMP, 1.0);
    expect_px(&img, 10, 10, [255, 255, 0, 255], 0, "replacement left");
    expect_px(&img, 50, 50, [255, 0, 255, 255], 0, "replacement right");
    golden(&img, "replaced_png", backend);
    // New contents without a replacement: the original storage comes back.
    let other = [9u8; 16];
    c.r.write_texture(&c.gpu, t, 0, &other, None).unwrap();
    assert!(!c.r.is_replaced(t));
    assert_eq!(c.r.texture_size(t), Some((2, 2)));
    let img = draw_full(&mut c, t, Sampler::POINT_CLAMP, 1.0);
    expect_px(&img, 10, 10, [9, 9, 9, 255], 0, "restored original");
}
per_backend!(dump_and_replace);

/// A palette-independent replacement (`_any`) applies to every palette of the same indices.
fn palette_any(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 64), SceneSettings::default()) else { return };
    let idx = [0x10u8, 0x32];
    let pal = |base: u8| Palette { colors: (0..16).map(|i| [base, i * 16, 0, 255]).collect(), layout: ClutLayout::Linear };
    let fmt = TexFormat::Indexed4 { low_first: true };
    let (_, any) = texture_key(2, 2, fmt, &idx, Some(&pal(0)));
    let pack = scratch("pal", backend);
    save_png(&pack.join(format!("{}.png", any.unwrap())), 4, 4, &[0, 200, 200, 255].repeat(16)).unwrap();
    c.r.set_texture_replacement(Some(ReplaceConfig { pack_dir: Some(pack), ..Default::default() }));
    for base in [0, 100] {
        let t = c.r.upload_texture(&c.gpu, 2, 2, fmt, &[&idx], Some(&pal(base)), false).unwrap();
        assert!(c.r.is_replaced(t), "palette {base}");
        let img = draw_full(&mut c, t, Sampler::POINT_CLAMP, 1.0);
        expect_px(&img, 32, 32, [0, 200, 200, 255], 0, "any-palette replacement");
    }
}
per_backend!(palette_any);

fn dds_file(fourcc: &[u8; 4], dxgi: Option<u32>, w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
    let mut d = vec![0u8; 128];
    d[..4].copy_from_slice(b"DDS ");
    d[4..8].copy_from_slice(&124u32.to_le_bytes());
    d[12..16].copy_from_slice(&h.to_le_bytes());
    d[16..20].copy_from_slice(&w.to_le_bytes());
    d[28..32].copy_from_slice(&1u32.to_le_bytes());
    d[76..80].copy_from_slice(&32u32.to_le_bytes());
    d[80..84].copy_from_slice(&4u32.to_le_bytes());
    d[84..88].copy_from_slice(fourcc);
    if let Some(f) = dxgi {
        let mut x = vec![0u8; 20];
        x[..4].copy_from_slice(&f.to_le_bytes());
        x[4..8].copy_from_slice(&3u32.to_le_bytes());
        d.extend(x);
    }
    d.extend_from_slice(payload);
    d
}

/// BC7 mode 6 block of one colour (7-bit endpoints with p-bit 1).
fn bc7_solid(r7: u8, g7: u8, b7: u8) -> [u8; 16] {
    let mut bits = 0u128;
    let mut at = 0;
    let mut put = |v: u128, n: u32| {
        bits |= v << at;
        at += n;
    };
    put(1 << 6, 7);
    for c in [r7, r7, g7, g7, b7, b7, 127, 127] {
        put(c as u128, 7);
    }
    put(1, 1);
    put(1, 1);
    bits.to_le_bytes()
}

fn dds_replacements(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 64), SceneSettings::default()) else { return };
    let bc = c.gpu.bc_textures();
    let pack = scratch("dds", backend);
    let orig_a = [1u8; 16];
    let orig_b = [2u8; 16];
    let (ka, _) = texture_key(2, 2, TexFormat::Rgba8, &orig_a, None);
    let (kb, _) = texture_key(2, 2, TexFormat::Rgba8, &orig_b, None);
    // BC1 4x4: solid green (c0 = c1 = 0x07E0, index 0).
    let mut bc1 = Vec::new();
    bc1.extend(0x07E0u16.to_le_bytes());
    bc1.extend(0x07E0u16.to_le_bytes());
    bc1.extend(0u32.to_le_bytes());
    std::fs::write(pack.join(format!("{ka}.dds")), dds_file(b"DXT1", None, 4, 4, &bc1)).unwrap();
    std::fs::write(pack.join(format!("{kb}.DDS")), dds_file(b"DX10", Some(98), 4, 4, &bc7_solid(127, 0, 64))).unwrap();
    c.r.set_texture_replacement(Some(ReplaceConfig { pack_dir: Some(pack), ..Default::default() }));
    let ta = c.r.upload_texture(&c.gpu, 2, 2, TexFormat::Rgba8, &[&orig_a], None, false).unwrap();
    let tb = c.r.upload_texture(&c.gpu, 2, 2, TexFormat::Rgba8, &[&orig_b], None, false).unwrap();
    if !bc {
        eprintln!("{backend:?}: no BC support, DDS replacements must be skipped");
        assert!(!c.r.is_replaced(ta) && !c.r.is_replaced(tb));
        return;
    }
    assert!(c.r.is_replaced(ta) && c.r.is_replaced(tb));
    let img = draw_full(&mut c, ta, Sampler::POINT_CLAMP, 1.0);
    expect_px(&img, 32, 32, [0, 255, 0, 255], 0, "BC1 replacement");
    let img = draw_full(&mut c, tb, Sampler::POINT_CLAMP, 1.0);
    expect_px(&img, 32, 32, [255, 1, 129, 255], 1, "BC7 replacement");
}
per_backend!(dds_replacements);

/// A dense replacement drawn small: with the game's linear no-mip sampler it aliases (pure
/// black/white texels); `force_mips` + anisotropy filter it to grey.
fn filtering_upgrade(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 64), SceneSettings::default()) else { return };
    let orig = [3u8; 16];
    let (key, _) = texture_key(2, 2, TexFormat::Rgba8, &orig, None);
    let pack = scratch("filter", backend);
    let checker: Vec<u8> = (0..64 * 64).flat_map(|i| if (i % 64 + i / 64) % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] }).collect();
    save_png(&pack.join(format!("{key}.png")), 64, 64, &checker).unwrap();
    for force in [false, true] {
        c.r.set_texture_replacement(Some(ReplaceConfig { pack_dir: Some(pack.clone()), force_mips: force, anisotropy: 8, ..Default::default() }));
        let t = c.r.upload_texture(&c.gpu, 2, 2, TexFormat::Rgba8, &[&orig], None, false).unwrap();
        // 3 texels per pixel: samples land on texel centres of level 0.
        let img = draw_full(&mut c, t, Sampler::BILINEAR_WRAP, 3.0);
        let p = img.pixel(32, 32)[0];
        if force {
            assert!(p.abs_diff(128) <= 3, "{backend:?} force_mips: {p}");
        } else {
            assert!(p < 10 || p > 245, "{backend:?} game sampling kept: {p}");
        }
    }
}
per_backend!(filtering_upgrade);

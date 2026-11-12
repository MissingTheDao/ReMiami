//! Stage 1 (core): headless offscreen tests on every backend this machine has. Each test
//! checks pixels analytically and against a golden image the test itself wrote on first run
//! (tests/golden; RETRO_WGPU_BLESS=1 rewrites them).

mod common;
use common::*;
use retro_wgpu::texture::{TexFormat, decode_bc, to_rgba8};
use retro_wgpu::*;

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

const S: (u32, u32) = (64, 64);

fn new(backend: Backend) -> Option<Ctx> {
    ctx(backend, S, SceneSettings::default())
}

fn clear(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    c.r.clear(Some([0.25, 0.5, 0.75, 1.0]), Some(1.0), Some(0));
    let img = c.frame();
    for (x, y) in [(0, 0), (63, 63), (31, 17)] {
        expect_px(&img, x, y, [64, 128, 191, 255], 1, "clear");
    }
    golden(&img, "clear", backend);
}
per_backend!(clear);

fn vertex_colour(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PC);
    let st = RenderState::default();
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 0.0, 1.0, 0.5, [255, 0, 0, 255]), Indices::None, &st, &p);
    // Right half: per-vertex colours, indexed triangle list.
    let mut w = VertexWriter::new();
    for (x, y, col) in [(0.0, -1.0, [0, 0, 255, 255]), (1.0, -1.0, [0, 0, 255, 255]), (1.0, 1.0, [0, 255, 0, 255]), (0.0, 1.0, [0, 255, 0, 255])] {
        w.f32s(&[x, y, 0.5]).u8x4(col);
    }
    c.draw(l, Topology::TriangleList, &w.bytes, Indices::U16(&[0, 1, 2, 0, 2, 3]), &st, &p);
    let img = c.frame();
    expect_px(&img, 10, 10, [255, 0, 0, 255], 0, "flat red");
    // Gradient green (top) to blue (bottom): row 32 is halfway.
    expect_px(&img, 48, 1, [0, 251, 4, 255], 3, "top");
    expect_px(&img, 48, 62, [0, 4, 251, 255], 3, "bottom");
    expect_px(&img, 48, 32, [0, 124, 131, 255], 4, "middle");
    golden(&img, "vertex_colour", backend);
}
per_backend!(vertex_colour);

/// One quadrant per format, point sampled: BC1 (GPU or CPU decoded), RGB565, PS2 CSM1 8-bit
/// palette, ARGB4444. Every texel must match the CPU conversion.
fn formats(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PCU);
    // 8x8 BC1: four blocks, different endpoints and modes.
    let mut bc1 = Vec::new();
    for (c0, c1, bits) in [(0xF800u16, 0x001Fu16, 0xE4E4_E4E4u32), (0x07E0, 0xFFFF, 0x1B1B_1B1B), (0x001F, 0xF800, 0xE4E4_E4E4), (0x0000, 0xFFE0, 0x5555_AAAA)] {
        bc1.extend(c0.to_le_bytes());
        bc1.extend(c1.to_le_bytes());
        bc1.extend(bits.to_le_bytes());
    }
    let t_bc1 = c.r.upload_texture(&c.gpu, 8, 8, TexFormat::Bc1, &[&bc1], None, false).unwrap();
    let rgb565: Vec<u8> = (0..64u16).flat_map(|i| (i.wrapping_mul(0x9E37) ^ 0x1234).to_le_bytes()).collect();
    let t_565 = c.r.upload_texture(&c.gpu, 8, 8, TexFormat::Rgb565, &[&rgb565], None, false).unwrap();
    let pal = Palette { colors: (0..256u32).map(|i| [(i * 7) as u8, (i * 13) as u8, (255 - i) as u8, 255]).collect(), layout: ClutLayout::Ps2Csm1 };
    let idx: Vec<u8> = (0..64u8).map(|i| i.wrapping_mul(37)).collect();
    let t_pal = c.r.upload_texture(&c.gpu, 8, 8, TexFormat::Indexed8, &[&idx], Some(&pal), false).unwrap();
    let argb: Vec<u8> = (0..64u16).flat_map(|i| (0xF000 | i.wrapping_mul(0x0731)).to_le_bytes()).collect();
    let t_4444 = c.r.upload_texture(&c.gpu, 8, 8, TexFormat::Argb4444, &[&argb], None, false).unwrap();
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    let quads = [(t_bc1, -1.0, 0.0), (t_565, 0.0, 0.0), (t_pal, -1.0, -1.0), (t_4444, 0.0, -1.0)];
    for (t, x, y) in quads {
        let st = RenderState::default().with_texture(t, Sampler::POINT_CLAMP);
        c.draw(l, Topology::TriangleFan, &quad_pcu(x, y, x + 1.0, y + 1.0, 0.5, [255; 4]), Indices::None, &st, &p);
    }
    let img = c.frame();
    let want = [
        decode_bc(TexFormat::Bc1, 8, 8, &bc1).unwrap(),
        to_rgba8(TexFormat::Rgb565, 8, 8, &rgb565, None).unwrap(),
        to_rgba8(TexFormat::Indexed8, 8, 8, &idx, Some(&pal)).unwrap(),
        to_rgba8(TexFormat::Argb4444, 8, 8, &argb, None).unwrap(),
    ];
    for (q, (ox, oy)) in [(0, 0), (32, 0), (0, 32), (32, 32)].into_iter().enumerate() {
        for ty in 0..8u32 {
            for tx in 0..8u32 {
                let i = ((ty * 8 + tx) * 4) as usize;
                let w = &want[q][i..i + 4];
                // Only colour is compared (the scene keeps alpha; BC1 punch-through is black).
                let tol = if q == 0 { 3 } else { 0 };
                expect_px(&img, ox + tx * 4 + 2, oy + ty * 4 + 2, [w[0], w[1], w[2], w[3]], tol, &format!("{backend:?} format {q} texel {tx},{ty}"));
            }
        }
    }
    golden(&img, "formats", backend);
}
per_backend!(formats);

fn blending(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PC);
    let p = DrawParams::default();
    c.r.clear(Some([0.2, 0.4, 0.6, 1.0]), Some(1.0), Some(0));
    let eq = |src, dst, op| BlendEq { src, dst, op };
    use BlendFactor as F;
    use BlendOperation as O;
    let cases: [(Blend, [f32; 4]); 6] = [
        (Blend::ALPHA, [0.0; 4]),
        (Blend::ADD, [0.0; 4]),
        (Blend::same(eq(F::One, F::One, O::ReverseSubtract)), [0.0; 4]),
        (Blend::same(eq(F::One, F::One, O::Min)), [0.0; 4]),
        (Blend::same(eq(F::Constant, F::OneMinusConstant, O::Add)), [0.25, 0.25, 0.25, 0.25]),
        (Blend::same(eq(F::Dst, F::Zero, O::Add)), [0.0; 4]),
    ];
    for (i, (b, k)) in cases.iter().enumerate() {
        let st = RenderState { blend: Some(*b), blend_constant: *k, depth: DepthState::OFF, ..Default::default() };
        let x = -1.0 + i as f32 / 3.0;
        c.draw(l, Topology::TriangleFan, &quad_pc(x, -1.0, x + 1.0 / 3.0, 1.0, 0.5, [255, 128, 0, 128]), Indices::None, &st, &p);
    }
    let img = c.frame();
    let dst = [0.2f32, 0.4, 0.6];
    let src = [1.0f32, 128.0 / 255.0, 0.0];
    let a = 128.0 / 255.0;
    let to8 = |v: [f32; 3]| [0, 1, 2].map(|i| (v[i].clamp(0.0, 1.0) * 255.0).round() as u8);
    let wants = [
        to8([0, 1, 2].map(|i| src[i] * a + dst[i] * (1.0 - a))),
        to8([0, 1, 2].map(|i| src[i] + dst[i])),
        to8([0, 1, 2].map(|i| dst[i] - src[i])),
        to8([0, 1, 2].map(|i| src[i].min(dst[i]))),
        to8([0, 1, 2].map(|i| src[i] * 0.25 + dst[i] * 0.75)),
        to8([0, 1, 2].map(|i| src[i] * dst[i])),
    ];
    for (i, w) in wants.iter().enumerate() {
        expect_px(&img, (i as u32 * 64 / 6) + 5, 32, [w[0], w[1], w[2], 255], 2, &format!("{backend:?} blend case {i}"));
    }
    golden(&img, "blending", backend);
}
per_backend!(blending);

fn depth_stencil(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PC);
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    // Depth: near green first, far red over everything: red only where green isn't.
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, 0.0, 0.0, 1.0, 0.2, [0, 255, 0, 255]), Indices::None, &RenderState::default(), &p);
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, 0.0, 1.0, 1.0, 0.8, [255, 0, 0, 255]), Indices::None, &RenderState::default(), &p);
    // Stencil: write 1 in the bottom-left quarter without colour or depth...
    let write = StencilFace { compare: CompareFunction::Always, fail: StencilOperation::Keep, depth_fail: StencilOperation::Keep, pass: StencilOperation::Replace };
    let mark = RenderState { color_mask: ColorMask::NONE, depth: DepthState::OFF, stencil: StencilState::both(write, 0xFF, 0xFF), stencil_reference: 1, ..Default::default() };
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 0.0, 0.0, 0.5, [0; 4]), Indices::None, &mark, &p);
    // ...then a bottom-wide blue quad only where stencil == 1, and yellow where != 1.
    let test = |cmp| RenderState { depth: DepthState::OFF, stencil: StencilState::both(StencilFace { compare: cmp, ..StencilFace::KEEP }, 0xFF, 0), stencil_reference: 1, ..Default::default() };
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 1.0, 0.0, 0.5, [0, 0, 255, 255]), Indices::None, &test(CompareFunction::Equal), &p);
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 1.0, 0.0, 0.5, [255, 255, 0, 255]), Indices::None, &test(CompareFunction::NotEqual), &p);
    // Polygon offset: a coplanar quad pulled forward wins a LessEqual... with Less it needs the bias.
    let less = RenderState { depth: DepthState { compare: CompareFunction::Less, write: true }, ..Default::default() };
    let biased = RenderState { depth_bias: DepthBias { constant: -100, slope: 0.0, clamp: 0.0 }, ..less };
    c.draw(l, Topology::TriangleFan, &quad_pc(0.5, 0.5, 1.0, 1.0, 0.8, [255, 0, 255, 255]), Indices::None, &biased, &p);
    let img = c.frame();
    expect_px(&img, 8, 8, [0, 255, 0, 255], 0, "near quad kept");
    expect_px(&img, 40, 24, [255, 0, 0, 255], 0, "far quad elsewhere");
    expect_px(&img, 56, 8, [255, 0, 255, 255], 0, "depth bias");
    expect_px(&img, 8, 48, [0, 0, 255, 255], 0, "stencil equal");
    expect_px(&img, 48, 48, [255, 255, 0, 255], 0, "stencil not equal");
    golden(&img, "depth_stencil", backend);
}
per_backend!(depth_stencil);

fn alpha_test(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PC);
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    // Four columns with alpha 50, 100, 101, 200; alpha test "greater than 100".
    for (i, a) in [50u8, 100, 101, 200].into_iter().enumerate() {
        let st = RenderState { alpha_test: AlphaTest { compare: CompareFunction::Greater, reference: 100.0 / 255.0 }, ..Default::default() };
        let x = -1.0 + i as f32 * 0.5;
        c.draw(l, Topology::TriangleFan, &quad_pc(x, -1.0, x + 0.5, 1.0, 0.5, [255, 255, 255, a]), Indices::None, &st, &p);
    }
    let img = c.frame();
    for (i, drawn) in [false, false, true, true].into_iter().enumerate() {
        expect_px(&img, i as u32 * 16 + 8, 32, if drawn { [255; 4] } else { [0, 0, 0, 255] }, 0, &format!("{backend:?} alpha column {i}"));
    }
    golden(&img, "alpha_test", backend);
}
per_backend!(alpha_test);

fn fog(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PC);
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    let blue = [0.0, 0.0, 1.0, 1.0];
    let modes = [FogMode::Linear { start: 0.0, end: 1.0 }, FogMode::Exp { density: 2.0 }, FogMode::Exp2 { density: 2.0 }, FogMode::Off];
    for (i, m) in modes.into_iter().enumerate() {
        let st = RenderState { fog: Fog { mode: m, color: blue }, ..Default::default() };
        let x = -1.0 + i as f32 * 0.5;
        c.draw(l, Topology::TriangleFan, &quad_pc(x, -1.0, x + 0.5, 1.0, 0.5, [255; 4]), Indices::None, &st, &p);
    }
    let img = c.frame();
    // View-space depth 0.5 (identity camera).
    for (i, f) in [0.5f32, (-1.0f32).exp(), (-1.0f32).exp(), 1.0].into_iter().enumerate() {
        let v = (f * 255.0).round() as u8;
        expect_px(&img, i as u32 * 16 + 8, 32, [v, v, 255, 255], 1, &format!("{backend:?} fog mode {i}"));
    }
    golden(&img, "fog", backend);
}
per_backend!(fog);

fn lit(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(&[(Semantic::Position, AttrFormat::F32x3), (Semantic::Normal, AttrFormat::F32x3), (Semantic::Color, AttrFormat::Unorm8x4)]);
    let quad = |x0: f32, x1: f32, col: [u8; 4]| {
        let mut w = VertexWriter::new();
        for (x, y) in [(x0, -1.0), (x1, -1.0), (x1, 1.0), (x0, 1.0)] {
            w.f32s(&[x, y, 0.5, 0.0, 0.0, -1.0]).u8x4(col);
        }
        w.bytes
    };
    let mut lighting = Lighting { enabled: true, ambient: [0.2, 0.1, 0.0], count: 2, ..Lighting::OFF };
    lighting.lights[0] = DirLight { direction: [0.0, 0.0, 1.0], color: [0.5, 0.5, 0.5] };
    // Second light from behind the quad: contributes nothing.
    lighting.lights[1] = DirLight { direction: [0.0, 0.0, -1.0], color: [1.0, 1.0, 1.0] };
    let st = RenderState::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &quad(-1.0, 0.0, [255, 255, 255, 255]), Indices::None, &st, &DrawParams { lighting, ..Default::default() });
    // Material colour instead of the vertex colour, plus emissive.
    let material = Material { diffuse: [1.0, 0.0, 0.0, 1.0], ambient: [1.0; 4], emissive: [0.0, 0.0, 0.25, 0.0], diffuse_from_vertex: false, ambient_from_vertex: false, emissive_from_vertex: false };
    c.draw(l, Topology::TriangleFan, &quad(0.0, 1.0, [0, 255, 0, 255]), Indices::None, &st, &DrawParams { lighting, material, ..Default::default() });
    let img = c.frame();
    expect_px(&img, 16, 32, [179, 153, 128, 255], 1, "vertex colour lit");
    expect_px(&img, 48, 32, [179, 26, 64, 255], 1, "material lit");
    golden(&img, "lit", backend);
}
per_backend!(lit);

fn skinning(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(&[(Semantic::Position, AttrFormat::F32x3), (Semantic::Color, AttrFormat::Unorm8x4), (Semantic::BlendWeights, AttrFormat::F32x2), (Semantic::BlendIndices, AttrFormat::U8x4)]);
    let mut w = VertexWriter::new();
    // A quad left of centre on bone 1 (moved +1 in x), a thin strip half on bone 0, half on 1.
    for (x, y) in [(-1.0, -0.5), (-0.5, -0.5), (-0.5, 0.5), (-1.0, 0.5)] {
        w.f32s(&[x, y, 0.5]).u8x4([255, 255, 255, 255]).f32s(&[0.0, 1.0]).u8x4([0, 1, 0, 0]);
    }
    for (x, y) in [(-1.0, -1.0), (-0.75, -1.0), (-0.75, -0.75), (-1.0, -0.75)] {
        w.f32s(&[x, y, 0.5]).u8x4([0, 255, 0, 255]).f32s(&[0.5, 0.5]).u8x4([0, 1, 0, 0]);
    }
    let mut moved = IDENTITY;
    moved[12] = 1.0;
    let bones = [IDENTITY, moved];
    let st = RenderState::default();
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.r.draw(&c.gpu, &Draw { layout: l, topology: Topology::TriangleList, vertices: Vertices::Slice(&w.bytes), indices: Indices::U16(&[0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]), range: None, base_vertex: 0, state: &st, params: &p, bones: Some(&bones) });
    let img = c.frame();
    expect_px(&img, 40, 32, [255, 255, 255, 255], 0, "moved by bone 1");
    expect_px(&img, 8, 32, [0, 0, 0, 255], 0, "old place empty");
    expect_px(&img, 20, 60, [0, 255, 0, 255], 0, "half-weighted strip at +0.5");
    golden(&img, "skinning", backend);
}
per_backend!(skinning);

fn render_to_texture(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PCU);
    let lc = c.layout(PC);
    let p = DrawParams::default();
    for samples in [1, 4] {
        let rt = c.r.create_texture(&c.gpu, TextureDesc { samples, ..TextureDesc::render_target(16, 16) });
        c.r.set_target(Target::Texture(rt));
        c.r.clear(Some([1.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
        c.draw(lc, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 0.0, 1.0, 0.5, [0, 255, 0, 255]), Indices::None, &RenderState::default(), &p);
        c.r.set_target(Target::Scene);
        c.r.clear(Some([0.0, 0.0, 1.0, 1.0]), Some(1.0), Some(0));
        c.draw(l, Topology::TriangleFan, &quad_pcu(-1.0, 0.0, 0.0, 1.0, 0.5, [255; 4]), Indices::None, &RenderState::default().with_texture(rt, Sampler::POINT_CLAMP), &p);
        // Frame copy: the scene so far (top-left quarter) into a texture, drawn bottom-right.
        let copy = c.r.create_texture(&c.gpu, TextureDesc::render_target(32, 32));
        c.r.copy_to_texture(Target::Scene, Some([0, 0, 32, 32]), copy, [0, 0]);
        c.draw(l, Topology::TriangleFan, &quad_pcu(0.0, -1.0, 1.0, 0.0, 0.5, [255; 4]), Indices::None, &RenderState::default().with_texture(copy, Sampler::POINT_CLAMP), &p);
        let img = c.frame();
        let what = format!("{backend:?} rtt {samples}x");
        expect_px(&img, 4, 16, [0, 255, 0, 255], 0, &what);
        expect_px(&img, 28, 16, [255, 0, 0, 255], 0, &what);
        expect_px(&img, 48, 16, [0, 0, 255, 255], 0, &what);
        expect_px(&img, 36, 48, [0, 255, 0, 255], 0, &(what.clone() + " copy"));
        expect_px(&img, 60, 48, [255, 0, 0, 255], 0, &(what.clone() + " copy"));
        golden(&img, &format!("render_to_texture_{samples}x"), backend);
        c.r.destroy_texture(rt);
        c.r.destroy_texture(copy);
    }
}
per_backend!(render_to_texture);

fn msaa_resolve(backend: Backend) {
    let Some(_) = new(backend) else { return };
    let soft = |msaa: u32, supersample: u32, post: Option<&str>| {
        let mut c = ctx(backend, S, SceneSettings { msaa, supersample, internal_size: None }).unwrap();
        match post {
            Some("fxaa") => c.r.set_post_chain(vec![Box::new(Fxaa::new(&c.gpu))]),
            Some("smaa") => c.r.set_post_chain(vec![Box::new(Smaa::new(&c.gpu))]),
            _ => {}
        }
        let l = c.layout(PC);
        let mut w = VertexWriter::new();
        for (x, y) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            w.f32s(&[x, y * 0.97, 0.5]).u8x4([255; 4]);
        }
        c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
        c.draw(l, Topology::TriangleList, &w.bytes, Indices::None, &RenderState::default(), &DrawParams::default());
        let img = c.frame();
        assert_eq!(c.r.scene_settings().msaa, msaa.min(c.r.scene_settings().msaa));
        let n = img.rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 10 && p[0] < 245).count();
        (n, img)
    };
    let (hard, _) = soft(1, 1, None);
    assert_eq!(hard, 0, "{backend:?}: no AA must leave the edge hard");
    for (name, msaa, ss, post) in [("msaa4", 4, 1, None), ("ssaa2", 1, 2, None), ("fxaa", 1, 1, Some("fxaa")), ("smaa", 1, 1, Some("smaa"))] {
        let (n, img) = soft(msaa, ss, post);
        eprintln!("{backend:?} {name}: {n} soft edge pixels");
        assert!(n > 20, "{backend:?} {name}: only {n} soft edge pixels");
        golden(&img, &format!("aa_{name}"), backend);
    }
}
per_backend!(msaa_resolve);

/// Combiners: stage 0 texture x diffuse, stage 1 adds the stage constant; second quad: blend
/// texture alpha between texture and constant, with inverted/alpha-replicated arguments.
fn combiners(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PCU);
    let tex = c.r.upload_texture(&c.gpu, 1, 1, TexFormat::Rgba8, &[&[200, 100, 50, 64]], None, false).unwrap();
    let p = DrawParams::default();
    let mut st = RenderState::default().with_texture(tex, Sampler::POINT_CLAMP);
    st.stages[1] = Stage { color_op: Op::Add, color_args: [Arg::CURRENT, Arg::CONSTANT], alpha_op: Op::Arg1, alpha_args: [Arg::CURRENT, Arg::CURRENT], constant: [0.1, 0.2, 0.3, 1.0], ..Stage::OFF };
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &quad_pcu(-1.0, -1.0, 0.0, 1.0, 0.5, [128, 255, 255, 255]), Indices::None, &st, &p);
    let mut st2 = RenderState::default();
    st2.stages[0] = Stage {
        texture: Some(tex),
        sampler: Sampler::POINT_CLAMP,
        color_op: Op::BlendTextureAlpha,
        color_args: [Arg::TEXTURE, Arg::CONSTANT],
        alpha_op: Op::Arg1,
        alpha_args: [Arg { source: Source::Texture, invert: true, alpha: true }, Arg::CURRENT],
        constant: [0.0, 0.0, 1.0, 1.0],
        ..Stage::OFF
    };
    c.draw(l, Topology::TriangleFan, &quad_pcu(0.0, -1.0, 1.0, 1.0, 0.5, [255; 4]), Indices::None, &st2, &p);
    let img = c.frame();
    let t = [200.0f32 / 255.0, 100.0 / 255.0, 50.0 / 255.0];
    let d = [128.0f32 / 255.0, 1.0, 1.0];
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    expect_px(&img, 16, 32, [to8(t[0] * d[0] + 0.1), to8(t[1] + 0.2), to8(t[2] + 0.3), 255], 1, "modulate + add constant");
    let a = 64.0 / 255.0;
    expect_px(&img, 48, 32, [to8(t[0] * a), to8(t[1] * a), to8(t[2] * a + (1.0 - a)), 255], 1, "blend texture alpha");
    golden(&img, "combiners", backend);
}
per_backend!(combiners);

/// Screen-space vertices (D3D XYZRHW) with BGRA colours, pixel-exact rectangles.
fn pretransformed(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(&[(Semantic::PositionT, AttrFormat::F32x4), (Semantic::Color, AttrFormat::Unorm8x4Bgra)]);
    c.r.set_view(&View::screen(64.0, 64.0));
    let mut w = VertexWriter::new();
    for (x, y) in [(8.0, 8.0), (24.0, 8.0), (24.0, 40.0), (8.0, 40.0)] {
        // BGRA bytes for orange (255, 128, 0).
        w.f32s(&[x, y, 0.5, 1.0]).u8x4([0, 128, 255, 255]);
    }
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &w.bytes, Indices::None, &RenderState::overlay(), &DrawParams::default());
    let img = c.frame();
    expect_px(&img, 8, 8, [255, 128, 0, 255], 0, "inside top-left");
    expect_px(&img, 23, 39, [255, 128, 0, 255], 0, "inside bottom-right");
    expect_px(&img, 7, 8, [0, 0, 0, 255], 0, "left of the rect");
    expect_px(&img, 24, 8, [0, 0, 0, 255], 0, "right of the rect");
    expect_px(&img, 8, 40, [0, 0, 0, 255], 0, "below the rect");
    golden(&img, "pretransformed", backend);
}
per_backend!(pretransformed);

/// Low internal resolution: 16x16 game image, integer-scaled nearest into 64x48 (3x, centred),
/// then sharp-bilinear and an output gamma.
fn output_stage(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 48), SceneSettings { internal_size: Some((16, 16)), ..Default::default() }) else { return };
    let l = c.layout(PC);
    let draw = |c: &mut Ctx| {
        c.r.clear(Some([0.25, 0.25, 0.25, 1.0]), Some(1.0), Some(0));
        c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, 0.0, 0.0, 1.0, 0.5, [255, 255, 255, 255]), Indices::None, &RenderState::default(), &DrawParams::default());
    };
    c.r.output = OutputSettings { mode: ScaleMode::Integer, filter: ScaleFilter::Nearest, background: [1.0, 0.0, 1.0, 1.0], ..Default::default() };
    draw(&mut c);
    let img = c.frame();
    assert_eq!(c.r.final_image().map(|t| (t.width(), t.height())), Some((16, 16)));
    // 3x: 48x48 at x 8..56.
    expect_px(&img, 7, 10, [255, 0, 255, 255], 0, "border");
    expect_px(&img, 8, 0, [255, 255, 255, 255], 0, "white texel row 0");
    expect_px(&img, 31, 23, [255, 255, 255, 255], 0, "white quarter edge");
    expect_px(&img, 32, 23, [64, 64, 64, 255], 0, "grey right of it");
    expect_px(&img, 56, 10, [255, 0, 255, 255], 0, "right border");
    golden(&img, "output_integer", backend);
    c.r.output = OutputSettings { mode: ScaleMode::Fit, filter: ScaleFilter::SharpBilinear, gamma: 2.0, display_aspect: Some(4.0 / 3.0), ..Default::default() };
    draw(&mut c);
    let img = c.frame();
    // 4:3 fills 64x48; grey 0.25 through gamma 2 = 0.5.
    expect_px(&img, 60, 40, [128, 128, 128, 255], 1, "gamma");
    expect_px(&img, 2, 2, [255, 255, 255, 255], 0, "white corner");
    golden(&img, "output_sharp_gamma", backend);
}
per_backend!(output_stage);

/// Retained buffers, strips, mipmaps (auto-generated, trilinear), anisotropy, wrap/mirror.
fn retained_and_mips(backend: Backend) {
    let Some(mut c) = new(backend) else { return };
    let l = c.layout(PCU);
    let mut w = VertexWriter::new();
    for (x, y, u, v) in [(-1.0, -1.0, 0.0, 4.0), (1.0, -1.0, 4.0, 4.0), (-1.0, 1.0, 0.0, 0.0), (1.0, 1.0, 4.0, 0.0)] {
        w.f32s(&[x, y, 0.5]).u8x4([255; 4]).f32s(&[u, v]);
    }
    let vb = c.r.create_vertex_buffer(&c.gpu, &w.bytes);
    let ib = c.r.create_index_buffer16(&c.gpu, &[0, 1, 2, 3]);
    // 64x64 checker of 1-texel squares: its mips average to grey.
    let checker: Vec<u8> = (0..64 * 64).flat_map(|i| if (i % 64 + i / 64) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
    let tex = c.r.upload_texture(&c.gpu, 64, 64, TexFormat::Rgba8, &[&checker], None, true).unwrap();
    let mut s = Sampler::TRILINEAR_WRAP;
    s.address_u = Address::Mirror;
    s.anisotropy = 4;
    let st = RenderState::default().with_texture(tex, s);
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.r.draw(&c.gpu, &Draw { layout: l, topology: Topology::TriangleStrip, vertices: Vertices::Buffer(vb, 0), indices: Indices::Buffer(ib), range: None, base_vertex: 0, state: &st, params: &DrawParams::default(), bones: None });
    let img = c.frame();
    // 256 texels over 64 pixels: mip level 2 = uniform grey.
    expect_px(&img, 32, 32, [128, 128, 128, 255], 3, "mipmapped grey");
    golden(&img, "retained_mips", backend);
}
per_backend!(retained_and_mips);

#[test]
fn backends_listed() {
    let all = Backend::available();
    assert!(all.contains(&Backend::Vulkan) && all.contains(&Backend::Gl));
    assert_eq!(Backend::parse("D3D12"), Some(Backend::Dx12));
    assert_eq!(Backend::Dx12.compiled_in(), cfg!(windows));
}

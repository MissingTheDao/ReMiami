//! Front-end programs (`Renderer::create_program` / `draw_program`): a front end's own WGSL
//! drawn through the renderer's recording, fixed-function state, targets and output.

mod common;
use common::*;
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

/// Colour = vertex colour * texture stage 0 * the block's tint, plus the block's offset; the
/// block's `discard_below` drops fragments with less red (an engine-specific alpha rule).
const WGSL: &str = r#"
struct ViewU { view: mat4x4<f32>, proj: mat4x4<f32>, screen: vec4<f32> };
@group(0) @binding(0) var<uniform> view: ViewU;
struct P { tint: vec4<f32>, offset: vec4<f32>, discard_below: vec4<f32>, pad: vec4<f32> };
@group(1) @binding(0) var<uniform> p: P;
@group(2) @binding(0) var t0: texture_2d<f32>;
@group(2) @binding(1) var s0: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) color: vec4<f32>, @location(1) uv: vec2<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> V {
    var o: V;
    o.pos = vec4<f32>(pos, 1.0);
    o.color = color;
    o.uv = uv;
    return o;
}

@fragment
fn fs(i: V) -> @location(0) vec4<f32> {
    let c = i.color * textureSample(t0, s0, i.uv) * p.tint + p.offset;
    if (c.r < p.discard_below.x) {
        discard;
    }
    return c;
}
"#;

fn block(tint: [f32; 4], offset: [f32; 4], discard_below: f32) -> Vec<u8> {
    [tint, offset, [discard_below, 0.0, 0.0, 0.0], [0.0; 4]].iter().flatten().flat_map(|f| f.to_le_bytes()).collect()
}

fn programs(backend: Backend) {
    let Some(mut c) = ctx(backend, S, SceneSettings::default()) else { return };
    let desc = ProgramDesc {
        label: "test program",
        wgsl: WGSL,
        vertex_entry: "vs",
        fragment_entry: "fs",
        layout: VertexLayout::packed(PCU),
        block_size: 64,
    };
    let prog = c.r.create_program(&c.gpu, &desc).expect("program");
    // A bad program is an error, not a crash.
    assert!(c.r.create_program(&c.gpu, &ProgramDesc { wgsl: "fn broken(", ..desc.clone() }).is_err());
    assert!(c.r.create_program(&c.gpu, &ProgramDesc { block_size: 20, ..desc.clone() }).is_err());

    // 2x1 texture: left texel white, right texel half grey.
    let tex = c.r.upload_texture(&c.gpu, 2, 1, TexFormat::Rgba8, &[&[255, 255, 255, 255, 128, 128, 128, 255]], None, false).unwrap();
    let st = RenderState::default().with_texture(tex, Sampler::POINT_CLAMP);
    let blend = RenderState { blend: Some(Blend::same(BlendEq::ADD)), ..st };
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    let quad = quad_pcu(-1.0, -1.0, 1.0, 0.0, 0.5, [255, 255, 255, 255]);
    let params = DrawParams::default();
    let draw = |layout| Draw {
        layout,
        topology: Topology::TriangleFan,
        vertices: Vertices::Slice(&quad),
        indices: Indices::None,
        range: None,
        base_vertex: 0,
        state: &st,
        params: &params,
        bones: None,
    };
    // Bottom half: tint (1, 0.5, 0.25), no offset.
    c.r.draw_program(&c.gpu, &draw(LayoutId(u32::MAX)), prog, &block([1.0, 0.5, 0.25, 1.0], [0.0; 4], -1.0));
    // Top half: green offset added on top of black with additive blending (fixed-function state
    // still applies); the right half is discarded by the block's rule (texel red 0.5 < 0.6).
    let top = quad_pcu(-1.0, 0.0, 1.0, 1.0, 0.5, [255, 255, 255, 255]);
    c.r.draw_program(&c.gpu, &Draw { vertices: Vertices::Slice(&top), state: &blend, ..draw(LayoutId(u32::MAX)) }, prog, &block([1.0, 0.0, 0.0, 1.0], [0.0, 0.5, 0.0, 0.0], 0.6));
    // A fixed-function draw after program draws still works.
    let l = c.layout(PC);
    c.draw(l, Topology::TriangleFan, &quad_pc(-0.1, -0.1, 0.1, 0.1, 0.4, [0, 0, 255, 255]), Indices::None, &RenderState::default(), &DrawParams::default());
    let img = c.frame();
    // Image row 0 is the top (clip y = +1).
    expect_px(&img, 8, 56, [255, 128, 64, 255], 2, "bottom left: white texel tinted");
    expect_px(&img, 56, 56, [128, 64, 32, 255], 2, "bottom right: grey texel tinted");
    expect_px(&img, 8, 8, [255, 128, 0, 255], 2, "top left: red + green offset");
    expect_px(&img, 56, 8, [0, 0, 0, 255], 0, "top right: discarded");
    expect_px(&img, 32, 32, [0, 0, 255, 255], 0, "fixed-function draw");
}
per_backend!(programs);

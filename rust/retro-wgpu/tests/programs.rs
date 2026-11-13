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
        stages: 1,
        instance_block_size: 0,
    };
    let prog = c.r.create_program(&c.gpu, &desc).expect("program");
    // A bad program is an error, not a crash.
    assert!(c.r.create_program(&c.gpu, &ProgramDesc { wgsl: "fn broken(", ..desc.clone() }).is_err());
    assert!(c.r.create_program(&c.gpu, &ProgramDesc { block_size: 20, ..desc.clone() }).is_err());
    assert!(c.r.create_program(&c.gpu, &ProgramDesc { stages: 0, ..desc.clone() }).is_err());

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
    // (The block in two parts, joined without a copy.)
    let b = block([1.0, 0.5, 0.25, 1.0], [0.0; 4], -1.0);
    c.r.draw_program_parts(&c.gpu, &draw(LayoutId(u32::MAX)), prog, &[&b[..20], &b[20..]]);
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

/// Instance blocks: a shared block (tint) plus one block per draw (offset and colour), read as
/// `inst[instance_index]` in the vertex and, through a flat varying, the fragment stage.
const INST_WGSL: &str = r#"
struct P { tint: vec4<f32> };
@group(1) @binding(0) var<uniform> p: P;
struct I { offset: vec4<f32>, color: vec4<f32> };
//INST_DECL

struct V { @builtin(position) pos: vec4<f32>, @location(0) @interpolate(flat) ii: u32 };

@vertex
fn vs(@location(0) pos: vec3<f32>, @builtin(instance_index) ii: u32) -> V {
    var o: V;
    o.pos = vec4<f32>(pos + inst[ii].offset.xyz, 1.0);
    o.ii = ii;
    return o;
}

@fragment
fn fs(i: V) -> @location(0) vec4<f32> {
    return inst[i.ii].color * p.tint;
}
"#;

fn instance_blocks_on(backend: Backend, storage: bool) {
    let gpu = match Gpu::headless_with(backend, &GpuOptions { instance_storage: storage, ..Default::default() }) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED on {backend:?}: {e}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm, SceneSettings::default());
    if storage && !r.instance_storage() {
        eprintln!("{backend:?}: no vertex-stage storage buffers, storage mode not tested");
        return;
    }
    assert_eq!(r.instance_storage(), storage);
    let wgsl = INST_WGSL.replace("//INST_DECL", &r.instance_block_decl("inst", "I"));
    let desc = ProgramDesc { label: "inst", wgsl: &wgsl, vertex_entry: "vs", fragment_entry: "fs", layout: VertexLayout::packed(&[(Semantic::Position, AttrFormat::F32x3)]), block_size: 16, stages: 1, instance_block_size: 32 };
    let prog = r.create_program(&gpu, &desc).expect("program");
    // A small quad around the origin, as a retained buffer drawn four times.
    let mut w = VertexWriter::new();
    for (x, y) in [(-0.2f32, -0.2f32), (0.2, -0.2), (0.2, 0.2), (-0.2, 0.2)] {
        w.f32s(&[x, y, 0.5]);
    }
    let vb = r.create_vertex_buffer(&gpu, &w.bytes);
    let ib = r.create_index_buffer16(&gpu, &[0, 1, 2, 0, 2, 3]);
    let st = RenderState::default();
    let params = DrawParams::default();
    let d = Draw { layout: LayoutId(u32::MAX), topology: Topology::TriangleList, vertices: Vertices::Buffer(vb, 0), indices: Indices::Buffer(ib), range: None, base_vertex: 0, state: &st, params: &params, bones: None };
    let f = |v: &[f32]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    let white = f(&[1.0, 1.0, 1.0, 1.0]);
    let half = f(&[0.5, 0.5, 0.5, 1.0]);
    // Four instances: corners, each its own colour; the last two with another shared block.
    let places = [(-0.5f32, 0.5f32, [1.0f32, 0.0, 0.0, 1.0]), (0.5, 0.5, [0.0, 1.0, 0.0, 1.0]), (-0.5, -0.5, [0.0, 0.0, 1.0, 1.0]), (0.5, -0.5, [1.0, 1.0, 0.0, 1.0])];
    for (n, (x, y, c)) in places.iter().enumerate() {
        let mut inst = f(&[*x, *y, 0.0, 0.0]);
        inst.extend(f(c));
        r.draw_program_instance(&gpu, &d, prog, &[if n < 2 { &white } else { &half }], &inst);
    }
    let out = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    // Two frames: the pools are reused.
    for _ in 0..2 {
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        r.encode(&gpu, &mut enc, &out.create_view(&Default::default()), (64, 64));
        let rb = readback::readback_start(&gpu, &mut enc, &out);
        gpu.queue.submit([enc.finish()]);
        let img = readback::readback_finish(&gpu, rb).unwrap();
        let what = if storage { "storage" } else { "uniform" };
        expect_px(&img, 16, 16, [255, 0, 0, 255], 1, what);
        expect_px(&img, 48, 16, [0, 255, 0, 255], 1, what);
        expect_px(&img, 16, 48, [0, 0, 128, 255], 1, what);
        expect_px(&img, 48, 48, [128, 128, 0, 255], 1, what);
        expect_px(&img, 32, 32, [0, 0, 0, 255], 0, what);
        r.end_frame();
        r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
        for (n, (x, y, c)) in places.iter().enumerate() {
            let mut inst = f(&[*x, *y, 0.0, 0.0]);
            inst.extend(f(c));
            r.draw_program_instance(&gpu, &d, prog, &[if n < 2 { &white } else { &half }], &inst);
        }
    }
}

fn instance_blocks(backend: Backend) {
    instance_blocks_on(backend, false);
    instance_blocks_on(backend, true);
}
per_backend!(instance_blocks);

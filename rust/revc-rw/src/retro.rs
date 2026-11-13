//! A second back end: draws the front end's frames with retro-wgpu, the engine-neutral core
//! shared with other ports. RenderWare specifics stay here: the RW shading (lighting model,
//! MatFX env maps, re3's Neo and CPostFX shaders, alpha window, fog) runs as retro-wgpu
//! programs generated from the same WGSL as `frame.rs`, so both back ends shade identically;
//! retro-wgpu owns resources, recording, targets, MSAA, the post chain and the output.
//!
//! The overlay split (render scale, supersampling or post AA with a HUD marker) maps onto
//! retro-wgpu's output-resolution overlay. One difference with a render scale below 100 %:
//! WgpuBackend filters the scene straight to window size and runs post AA there, retro-wgpu runs
//! post AA at the scaled size and then scales (render scale is not a player option). Without
//! post AA both filter the scene to the window in one bilinear step. FXAA is this renderer's own
//! (`RwFxaa`, frame.rs's shader as a retro-wgpu post pass); SMAA is the same port in both.

use crate::frame::{Frame, MESH_NEO_GLOSS, MESH_NEO_RIM, MESH_NEO_VEHICLE, MESH_NEO_WORLD, ModelState, TexData};
use crate::front::{BONE_FLOATS, HashMap, Kind, MeshDraw, Op, PipeKey, SamplerKey, Upload};
use retro_wgpu as rw;
use std::collections::HashSet;

/// RW `U` uniforms (72 floats) then `M` (ModelState): the per-draw block of the mesh programs.
const U_BYTES: usize = 72 * 4;
const BLOCK: u32 = (U_BYTES + std::mem::size_of::<ModelState>()) as u32;
const _: () = assert!(BLOCK.is_multiple_of(16));

/// `frame::SHADER` on retro-wgpu's binding contract: the U and M blocks become the program's
/// per-draw block, textures move to the stage group, vertex inputs to retro-wgpu's locations.
pub(crate) fn mesh_wgsl() -> String {
    let mut s = crate::frame::SHADER.to_string();
    let mut sub = |from: &str, to: &str| {
        assert!(s.contains(from), "retro shader port: `{from}` not found in frame::SHADER");
        s = s.replace(from, to);
    };
    sub("@group(0) @binding(0) var<uniform> u: U;", "struct B { u: U, m: M };\n@group(1) @binding(0) var<uniform> blk: B;");
    sub("@group(1) @binding(0) var tex: texture_2d<f32>;", "@group(2) @binding(0) var tex: texture_2d<f32>;");
    sub("@group(1) @binding(1) var samp: sampler;", "@group(2) @binding(1) var samp: sampler;");
    sub("//MODELS_DECL", "");
    sub("@group(3) @binding(0) var env_tex: texture_2d<f32>;", "@group(2) @binding(2) var env_tex: texture_2d<f32>;");
    sub("@group(3) @binding(1) var env_samp: sampler;", "@group(2) @binding(3) var env_samp: sampler;");
    sub("models[mi]", "blk.m");
    // Vertex inputs: retro-wgpu puts colour at 2 and UV sets at 3.., blend data at 7 and 8.
    let lines: Vec<String> = s
        .lines()
        .map(|l| {
            let mut l = l.to_string();
            if l.starts_with("fn vs_2d(") || l.starts_with("fn vs_3d(") {
                l = l.replace("@location(2) uv", "@location(3) uv").replace("@location(1) color", "@location(2) color");
            }
            if l.starts_with("fn vs_") {
                l = l.replace("@location(5) weights", "@location(7) weights").replace("@location(6) indices", "@location(8) indices");
            }
            l
        })
        .collect();
    let s = lines.join("\n");
    // `u.` (the RW uniforms) is now inside the block.
    let mut out = String::with_capacity(s.len() + 1024);
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'u' && b.get(i + 1) == Some(&b'.') && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.')) {
            out.push_str("blk.u.");
            i += 2;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// `frame::UV2_SHADER` (screen droplets) on retro-wgpu's bindings; its block is U alone.
pub(crate) fn uv2_wgsl() -> String {
    let mut s = crate::frame::UV2_SHADER.to_string();
    for (from, to) in [
        ("@group(0) @binding(0) var<uniform> u: U;", "@group(1) @binding(0) var<uniform> u: U;"),
        ("@group(1) @binding(0) var tex: texture_2d<f32>;", "@group(2) @binding(0) var tex: texture_2d<f32>;"),
        ("@group(1) @binding(1) var samp: sampler;", "@group(2) @binding(1) var samp: sampler;"),
        ("@group(2) @binding(0) var tex1: texture_2d<f32>;", "@group(2) @binding(2) var tex1: texture_2d<f32>;"),
        ("@group(2) @binding(1) var samp1: sampler;", "@group(2) @binding(3) var samp1: sampler;"),
        ("fn vs(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>, @location(3) uv2: vec2<f32>)", "fn vs(@location(0) pos: vec4<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>, @location(4) uv2: vec2<f32>)"),
    ] {
        assert!(s.contains(from), "retro shader port: `{from}` not found in frame::UV2_SHADER");
        s = s.replace(from, to);
    }
    s
}

fn layout(kind: Kind) -> rw::VertexLayout {
    use rw::AttrFormat as F;
    use rw::Semantic as S;
    let at = |semantic, format, offset| rw::Attribute { semantic, format, offset };
    let mesh = [at(S::Position, F::F32x3, 0), at(S::Normal, F::F32x3, 12), at(S::Color, F::Unorm8x4, 24), at(S::TexCoord(0), F::F32x2, 28), at(S::TexCoord(1), F::F32x2, 36)];
    match kind {
        Kind::Im2D => rw::VertexLayout { stride: 28, attributes: vec![at(S::PositionT, F::F32x4, 0), at(S::Color, F::Unorm8x4, 16), at(S::TexCoord(0), F::F32x2, 20)] },
        Kind::Im2DUV2 => rw::VertexLayout { stride: 36, attributes: vec![at(S::PositionT, F::F32x4, 0), at(S::Color, F::Unorm8x4, 16), at(S::TexCoord(0), F::F32x2, 20), at(S::TexCoord(1), F::F32x2, 28)] },
        Kind::Im3D => rw::VertexLayout { stride: 24, attributes: vec![at(S::Position, F::F32x3, 0), at(S::Color, F::Unorm8x4, 12), at(S::TexCoord(0), F::F32x2, 16)] },
        Kind::Mesh | Kind::MeshEnv => rw::VertexLayout { stride: 44, attributes: mesh.to_vec() },
        Kind::Skin => {
            let mut a = mesh.to_vec();
            a.push(at(S::BlendWeights, F::F32x4, 44));
            a.push(at(S::BlendIndices, F::U8x4, 60));
            rw::VertexLayout { stride: 64, attributes: a }
        }
    }
}

/// Vertex and fragment entry points of a draw (as `WgpuBackend::ensure_pipeline` picks them).
fn entries(kind: Kind, shader: u32) -> (&'static str, &'static str) {
    match (kind, shader) {
        (Kind::Im2D, 1) => ("vs_2d", "fs_colourfilter"),
        (Kind::Im2D, 2) => ("vs_2d", "fs_contrast"),
        (Kind::Im2D, _) => ("vs_2d", "fs_simple"),
        (Kind::Im3D, _) => ("vs_3d", "fs_simple"),
        (Kind::Im2DUV2, _) => ("vs", "fs"),
        (Kind::MeshEnv, MESH_NEO_VEHICLE) => ("vs_neo_vehicle", "fs_neo_vehicle"),
        (Kind::MeshEnv, MESH_NEO_WORLD) => ("vs_neo_world", "fs_neo_world"),
        (Kind::MeshEnv, _) => ("vs_env", "fs_env"),
        (Kind::Mesh, MESH_NEO_GLOSS) => ("vs_neo_gloss", "fs_neo_gloss"),
        (Kind::Mesh, MESH_NEO_RIM) => ("vs_neo_rim", "fs_simple"),
        (Kind::Mesh, _) => ("vs_mesh", "fs_simple"),
        (Kind::Skin, MESH_NEO_RIM) => ("vs_neo_rim_skin", "fs_simple"),
        (Kind::Skin, _) => ("vs_skin", "fs_simple"),
    }
}

fn blend_factor(f: u32) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as B;
    match f {
        1 => B::Zero,
        2 => B::One,
        3 => B::Src,
        4 => B::OneMinusSrc,
        5 => B::SrcAlpha,
        6 => B::OneMinusSrcAlpha,
        7 => B::DstAlpha,
        8 => B::OneMinusDstAlpha,
        9 => B::Dst,
        10 => B::OneMinusDst,
        11 => B::SrcAlphaSaturated,
        _ => B::One,
    }
}

/// RW texture filter and addressing as a retro-wgpu sampler (gl3's filter tables, like
/// `WgpuBackend::ensure_bind_group`).
fn sampler(k: SamplerKey) -> rw::Sampler {
    use rw::Filter::{Linear as L, Nearest as N};
    let (min, mag, mip) = match k.filter {
        1 => (N, N, None),
        3 => (N, N, Some(N)),
        4 => (L, L, Some(N)),
        5 => (N, N, Some(L)),
        6 => (L, L, Some(L)),
        0 => (N, L, Some(L)),
        _ => (L, L, None),
    };
    let address = |a: u32| match a {
        2 => rw::Address::Mirror,
        3 | 4 => rw::Address::Clamp,
        _ => rw::Address::Wrap,
    };
    let aniso = if mip.is_some() && min == L && k.anisotropy > 1 && crate::frame::anisotropy_enabled() { k.anisotropy } else { 1 };
    rw::Sampler { min, mag, mip, address_u: address(k.address_u), address_v: address(k.address_v), anisotropy: aniso }
}

fn render_state(pipe: &PipeKey) -> rw::RenderState {
    let blend = pipe.blend.map(|(s, d)| rw::Blend::same(rw::BlendEq { src: blend_factor(s), dst: blend_factor(d), op: wgpu::BlendOperation::Add }));
    rw::RenderState {
        blend,
        // The frame has no alpha channel (as in WgpuBackend): alpha stays 1.
        color_mask: rw::ColorMask { rgb: true, alpha: false },
        depth: rw::DepthState { compare: if pipe.z_test { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }, write: pipe.z_write },
        cull: match pipe.cull {
            2 => Some(wgpu::Face::Back),
            3 => Some(wgpu::Face::Front),
            _ => None,
        },
        ..rw::RenderState::default()
    }
}

/// WgpuBackend's FXAA (frame.rs `POST_SHADER`) as a retro-wgpu post pass, so both back ends
/// antialias alike.
pub struct RwFxaa {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl RwFxaa {
    pub fn new(gpu: &rw::Gpu) -> RwFxaa {
        let d = &gpu.device;
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rw fxaa"),
            entries: &[tex(0), wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }],
        });
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("rw fxaa"), source: wgpu::ShaderSource::Wgsl(crate::frame::POST_SHADER.into()) });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("rw fxaa"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rw fxaa"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fxaa"), targets: &[Some(rw::TARGET_FORMAT.into())], compilation_options: Default::default() }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor { label: Some("rw fxaa"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        RwFxaa { pipeline, layout, sampler }
    }
}

impl rw::PostPass for RwFxaa {
    fn name(&self) -> &str {
        "fxaa (revc-rw)"
    }

    fn run(&mut self, gpu: &rw::Gpu, enc: &mut wgpu::CommandEncoder, input: &wgpu::Texture, output: &wgpu::TextureView) {
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rw fxaa"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&input.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rw fxaa"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: output, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

struct RetroTex {
    id: rw::TextureId,
    width: u32,
    height: u32,
    data: TexData,
    levels: usize,
    autogen: bool,
    /// Recreated as a render target (drawn into by a camera).
    target: bool,
}

pub struct RetroBackend {
    pub r: rw::Renderer,
    programs: HashMap<(Kind, u32), rw::ProgramId>,
    textures: HashMap<u32, RetroTex>,
    geometries: HashMap<u32, (rw::BufferId, rw::BufferId)>,
    scene: Option<rw::SceneSettings>,
    post: Option<(bool, bool)>,
    warned: HashSet<&'static str>,
    mesh_src: String,
    uv2_src: String,
    block: Vec<u8>,
}

impl RetroBackend {
    pub fn new(gpu: &rw::Gpu, output: wgpu::TextureFormat) -> RetroBackend {
        RetroBackend {
            r: rw::Renderer::new(gpu, output, rw::SceneSettings::default()),
            programs: HashMap::default(),
            textures: HashMap::default(),
            geometries: HashMap::default(),
            scene: None,
            post: None,
            warned: HashSet::new(),
            mesh_src: mesh_wgsl(),
            uv2_src: uv2_wgsl(),
            block: Vec::with_capacity(BLOCK as usize),
        }
    }

    fn warn(&mut self, what: &'static str) {
        if self.warned.insert(what) {
            eprintln!("revc-rw retro: {what}");
        }
    }

    fn program(&mut self, gpu: &rw::Gpu, kind: Kind, shader: u32) -> Option<rw::ProgramId> {
        let shader = match kind {
            Kind::Im2D => shader.min(3),
            Kind::Im3D | Kind::Im2DUV2 => 0,
            _ => shader,
        };
        if let Some(p) = self.programs.get(&(kind, shader)) {
            return Some(*p);
        }
        let (vs, fs) = entries(kind, shader);
        let uv2 = kind == Kind::Im2DUV2;
        let desc = rw::ProgramDesc {
            label: "revc rw",
            wgsl: if uv2 { &self.uv2_src } else { &self.mesh_src },
            vertex_entry: vs,
            fragment_entry: fs,
            layout: layout(kind),
            block_size: if uv2 { U_BYTES as u32 } else { BLOCK },
        };
        match self.r.create_program(gpu, &desc) {
            Ok(p) => {
                self.programs.insert((kind, shader), p);
                Some(p)
            }
            Err(e) => {
                eprintln!("revc-rw retro: program {vs}/{fs}: {e}");
                None
            }
        }
    }

    /// The frame's settings for a `window`-size output: as WgpuBackend sizes its scene, the
    /// render scale only applies to frames with a HUD marker.
    fn apply_settings(&mut self, gpu: &rw::Gpu, s: crate::front::FrameSettings, window: (u32, u32), overlay: bool) {
        // With a render scale the scene (supersampling included) is the internal image, filtered
        // to the window in one step by the output stage, as WgpuBackend does.
        let scene = if overlay && s.render_scale != 100 {
            rw::SceneSettings { internal_size: Some(s.scene_size(window, true)), supersample: 1, msaa: s.samples }
        } else {
            rw::SceneSettings { internal_size: None, supersample: s.scale, msaa: s.samples }
        };
        if self.scene != Some(scene) {
            self.scene = Some(scene);
            self.r.set_scene(gpu, scene);
        }
        if self.post != Some((s.smaa, s.fxaa)) {
            self.post = Some((s.smaa, s.fxaa));
            let mut post: Vec<Box<dyn rw::PostPass>> = Vec::new();
            if s.smaa {
                post.push(Box::new(rw::Smaa::new(gpu)));
            } else if s.fxaa {
                post.push(Box::new(RwFxaa::new(gpu)));
            }
            self.r.set_post_chain(post);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_texture(&mut self, gpu: &rw::Gpu, id: u32, width: u32, height: u32, data: TexData, levels: &[Vec<u8>], autogen: bool) {
        let fmt = match data {
            TexData::Rgba8 => rw::TexFormat::Rgba8,
            TexData::Dxt1 => rw::TexFormat::Bc1,
            TexData::Dxt3 => rw::TexFormat::Bc2,
            TexData::Dxt5 => rw::TexFormat::Bc3,
        };
        let autogen = autogen && data == TexData::Rgba8;
        let same = self.textures.get(&id).is_some_and(|t| t.width == width && t.height == height && t.data == data && t.levels == levels.len() && t.autogen == autogen);
        if !same {
            let desc = rw::TextureDesc { width, height, format: fmt, mip_levels: if autogen { 0 } else { levels.len().max(1) as u32 }, render_target: false, samples: 1 };
            let tid = self.r.create_texture(gpu, desc);
            if let Some(old) = self.textures.insert(id, RetroTex { id: tid, width, height, data, levels: levels.len(), autogen, target: false }) {
                self.r.destroy_texture(old.id);
            }
        }
        let tid = self.textures[&id].id;
        for (l, px) in levels.iter().enumerate() {
            if let Err(e) = self.r.write_texture(gpu, tid, l as u32, px, None) {
                eprintln!("revc-rw retro: texture {id} level {l}: {e}");
            }
            if autogen || self.textures[&id].target {
                break;
            }
        }
    }

    /// Texture `id` as a render target (created as one on first use as a target).
    fn target(&mut self, gpu: &rw::Gpu, id: u32) -> Option<rw::TextureId> {
        let t = self.textures.get(&id)?;
        if t.target {
            return Some(t.id);
        }
        if t.data != TexData::Rgba8 {
            return None;
        }
        let (w, h) = (t.width, t.height);
        let tid = self.r.create_texture(gpu, rw::TextureDesc::render_target(w, h));
        let t = self.textures.get_mut(&id).unwrap();
        let old = std::mem::replace(&mut t.id, tid);
        t.target = true;
        t.levels = 1;
        t.autogen = false;
        self.r.destroy_texture(old);
        Some(tid)
    }

    fn tex(&self, id: u32) -> Option<rw::TextureId> {
        self.textures.get(&id).map(|t| t.id)
    }

    /// Records `frame` into retro-wgpu and encodes it into `output` (`size` pixels).
    pub fn encode(&mut self, gpu: &rw::Gpu, enc: &mut wgpu::CommandEncoder, output: &wgpu::TextureView, size: (u32, u32), frame: &Frame) {
        let split = frame.has_overlay() && frame.settings.overlay_split();
        self.apply_settings(gpu, frame.settings, size, frame.has_overlay());
        let scaled = frame.settings.scene_size(size, frame.has_overlay()) != size;
        for up in &frame.uploads {
            match up {
                Upload::Texture { id, width, height, data, levels, autogen } => self.upload_texture(gpu, *id, *width, *height, *data, levels, *autogen),
                Upload::Geometry { id, vertices, indices } => {
                    let vb = self.r.create_vertex_buffer(gpu, vertices);
                    let ib = self.r.create_index_buffer16(gpu, indices);
                    if let Some((ov, oi)) = self.geometries.insert(*id, (vb, ib)) {
                        self.r.destroy_buffer(ov);
                        self.r.destroy_buffer(oi);
                    }
                }
            }
        }
        // The scene's first pass starts black with cleared depth (as in WgpuBackend).
        self.r.set_target(rw::Target::Scene);
        self.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
        let mut cur = 0u32;
        let mut in_overlay = false;
        let screen = rw::View::screen(size.0 as f32, size.1 as f32);
        self.r.set_view(&screen);
        for op in &frame.ops {
            match *op {
                Op::Overlay => {
                    if split && !in_overlay {
                        self.r.begin_overlay();
                        in_overlay = true;
                    }
                }
                Op::Target { texture } => {
                    cur = texture;
                    let t = if texture == 0 {
                        Some(rw::Target::Scene)
                    } else {
                        self.target(gpu, texture).map(rw::Target::Texture)
                    };
                    match t {
                        Some(t) => self.r.set_target(t),
                        None => self.warn("draw target that can't be drawn into (compressed or unknown) skipped"),
                    }
                }
                Op::CopyFrame { texture, x, y } => {
                    if let Some(t) = self.tex(texture) {
                        if scaled && !in_overlay {
                            // The scaled scene filtered to window pixels (WgpuBackend's "down" pass).
                            self.r.copy_to_texture_scaled(rw::Target::Scene, t, [x, y], [size.0, size.1]);
                        } else {
                            self.r.copy_to_texture(rw::Target::Scene, None, t, [x, y]);
                        }
                    }
                }
                Op::Clear { color, depth } => {
                    if cur != 0 && !self.textures.get(&cur).is_some_and(|t| t.target) {
                        continue;
                    }
                    self.r.clear(color.map(|c| [c[0], c[1], c[2], 1.0]), depth.then_some(1.0), None);
                }
                Op::Draw { pipe, texture, sampler: smp, uniform, first, count, mesh } => {
                    if cur != 0 && !self.textures.get(&cur).is_some_and(|t| t.target) {
                        continue;
                    }
                    let Some(prog) = self.program(gpu, pipe.kind, pipe.shader) else { continue };
                    let mut state = render_state(&pipe);
                    state.stages[0].texture = if texture != 0 { self.tex(texture) } else { None };
                    state.stages[0].sampler = sampler(smp);
                    if let Some(MeshDraw { env: Some((t, s)), .. }) = mesh {
                        state.stages[1].texture = if t != 0 { self.tex(t) } else { None };
                        state.stages[1].sampler = sampler(s);
                    }
                    // The per-draw block: RW uniforms, then the model (meshes).
                    self.block.clear();
                    for f in &frame.uniforms[uniform as usize] {
                        self.block.extend_from_slice(&f.to_le_bytes());
                    }
                    if let Some(MeshDraw { model, .. }) = mesh
                        && pipe.kind != Kind::Im2DUV2
                        && let Some(m) = frame.models.get(model as usize)
                    {
                        // SAFETY: ModelState is repr(C) f32 data.
                        let bytes = unsafe { std::slice::from_raw_parts((m as *const ModelState).cast::<u8>(), std::mem::size_of::<ModelState>()) };
                        self.block.extend_from_slice(bytes);
                    }
                    let topology = if pipe.lines { rw::Topology::LineList } else { rw::Topology::TriangleList };
                    let params = rw::DrawParams::default();
                    let block = std::mem::take(&mut self.block);
                    match (pipe.kind, mesh) {
                        (Kind::Mesh | Kind::MeshEnv | Kind::Skin, Some(MeshDraw { geometry, bones, .. })) => {
                            let Some(&(vb, ib)) = self.geometries.get(&geometry) else {
                                self.block = block;
                                continue;
                            };
                            let bone_mats: Option<Vec<rw::Mat4>> = (pipe.kind == Kind::Skin).then(|| {
                                let b = &frame.bones[bones as usize * BONE_FLOATS..][..BONE_FLOATS];
                                b.as_chunks::<16>().0.to_vec()
                            });
                            let d = rw::Draw {
                                layout: rw::LayoutId(0),
                                topology,
                                vertices: rw::Vertices::Buffer(vb, 0),
                                indices: rw::Indices::Buffer(ib),
                                range: Some((first, count)),
                                base_vertex: 0,
                                state: &state,
                                params: &params,
                                bones: bone_mats.as_deref(),
                            };
                            self.r.draw_program(gpu, &d, prog, &block);
                        }
                        (Kind::Im2D, _) => {
                            let v = &frame.verts[first as usize..(first + count) as usize];
                            // SAFETY: Vertex2D is repr(C) plain data.
                            let bytes: &[u8] = unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) };
                            let d = rw::Draw { layout: rw::LayoutId(0), topology, vertices: rw::Vertices::Slice(bytes), indices: rw::Indices::None, range: None, base_vertex: 0, state: &state, params: &params, bones: None };
                            self.r.draw_program(gpu, &d, prog, &block);
                        }
                        (Kind::Im3D, _) => {
                            let v = &frame.verts3[first as usize..(first + count) as usize];
                            // SAFETY: Vertex3D is repr(C) plain data.
                            let bytes: &[u8] = unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) };
                            let d = rw::Draw { layout: rw::LayoutId(0), topology, vertices: rw::Vertices::Slice(bytes), indices: rw::Indices::None, range: None, base_vertex: 0, state: &state, params: &params, bones: None };
                            self.r.draw_program(gpu, &d, prog, &block);
                        }
                        (Kind::Im2DUV2, _) => {
                            let v = &frame.verts_uv2[first as usize..(first + count) as usize];
                            // SAFETY: Vertex2DUV2 is repr(C) plain data.
                            let bytes: &[u8] = unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) };
                            let d = rw::Draw { layout: rw::LayoutId(0), topology, vertices: rw::Vertices::Slice(bytes), indices: rw::Indices::None, range: None, base_vertex: 0, state: &state, params: &params, bones: None };
                            self.r.draw_program(gpu, &d, prog, &block[..U_BYTES]);
                        }
                        _ => {}
                    }
                    self.block = block;
                }
            }
        }
        // Xbox-style output curve (WgpuBackend's fs_xbox blit).
        let xbox = crate::frame::OUTPUT_TRANSFORM.load(std::sync::atomic::Ordering::Relaxed) == 1;
        self.r.output.gamma = if xbox { 1.1 } else { 1.0 };
        self.r.encode(gpu, enc, output, size);
    }

    /// After the frame's commands were submitted: frees what the frame destroyed.
    pub fn end_frame(&mut self, frame: &Frame) {
        self.r.end_frame();
        for id in &frame.dead_textures {
            if let Some(t) = self.textures.remove(id) {
                self.r.destroy_texture(t.id);
            }
        }
        for id in &frame.dead_geometries {
            if let Some((v, i)) = self.geometries.remove(id) {
                self.r.destroy_buffer(v);
                self.r.destroy_buffer(i);
            }
        }
    }

    /// The image before the output stage (window size without scaling), for comparisons.
    pub fn final_image(&self) -> Option<&wgpu::Texture> {
        self.r.final_image()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_ports_parse() {
        for (name, src) in [("mesh", super::mesh_wgsl()), ("uv2", super::uv2_wgsl())] {
            if let Err(e) = naga::front::wgsl::parse_str(&src) {
                panic!("{name}: {}", e.emit_to_string(&src));
            }
        }
    }
}

/// RetroBackend with its view of the device, presenting through `DrawFrames`.
pub struct RetroPresent {
    pub gpu: rw::Gpu,
    pub back: RetroBackend,
}

impl RetroPresent {
    /// For output views of `output` format on `gpu`'s device.
    pub fn new(gpu: &crate::Gpu, output: wgpu::TextureFormat) -> RetroPresent {
        let rgpu = gpu.retro();
        let back = RetroBackend::new(&rgpu, output);
        RetroPresent { gpu: rgpu, back }
    }
}

impl crate::frame::DrawFrames for RetroPresent {
    fn draw(&mut self, _gpu: &crate::Gpu, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: (u32, u32), frame: &mut Frame) {
        self.back.encode(&self.gpu, enc, target, size, frame);
    }

    fn shown_image(&self) -> Option<&wgpu::Texture> {
        self.back.final_image()
    }

    fn finish_frame(&mut self, frame: &Frame) {
        self.back.end_frame(frame);
    }
}

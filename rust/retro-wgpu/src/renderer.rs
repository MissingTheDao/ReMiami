//! The per-frame command recorder and its encoder.
//!
//! Draws, clears, target switches and copies are recorded during the frame (cheap: no wgpu
//! calls), then `encode` uploads the frame's vertex/index/uniform data in a few buffer writes,
//! creates missing pipelines/bind groups and replays the commands into render passes, skipping
//! redundant state changes. After the scene: SSAA downsample, the post chain, the output stage.
//!
//! New code for retro-wgpu. The record-then-encode structure, the redundant-bind tracking and
//! the frame-copy/feedback handling follow ideas from revc-rw (the user's own code), rewritten
//! for the generic state model.

use crate::gpu::Gpu;
use crate::layout::{Semantic, VertexLayout};
use crate::post::{Blitter, OutputSettings, PostPass, ScaleFilter, TARGET_FORMAT, output_rect, render_texture};
use crate::shader::{self, BONES_BLOCK, DRAW_BLOCK, DrawBlock, MAX_BONES, VIEW_BLOCK};
use crate::state::*;
use crate::texture::{Palette, TexFormat, full_chain, mip_chain, to_rgba8};
use std::collections::HashMap;

/// Depth/stencil format of every target.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(pub u32);

/// A front end's own shader for draws the fixed-function model doesn't cover (an engine's
/// lighting model, custom effects). The renderer still owns the recording, targets, MSAA, post
/// chain and output, and the fixed-function state of `RenderState` (blend, depth, stencil, cull,
/// bias, colour mask) still applies; lighting, fog, alpha test and the stage combiners are the
/// program's job.
///
/// The WGSL module must use the renderer's bind groups:
/// - `@group(0) @binding(0)`: the view block (`struct { view: mat4x4<f32>, proj: mat4x4<f32>,
///   screen: vec4<f32> }`, uniform), as set with `set_view`;
/// - `@group(1) @binding(0)`: the program's own per-draw block, `block_size` bytes (uniform),
///   from the bytes given with each draw;
/// - `@group(2)`: the four texture stages, texture i at binding 2i, its sampler at 2i + 1
///   (`texture_2d<f32>`, filtering `sampler`);
/// - `@group(3) @binding(0)`: bone matrices for skinned layouts (`array<mat4x4<f32>, 64>`).
///
/// Vertex attributes come at `VertexLayout::location(semantic)`; the fragment entry point writes
/// one colour (`@location(0) vec4<f32>`).
#[derive(Clone, Debug)]
pub struct ProgramDesc<'a> {
    pub label: &'a str,
    pub wgsl: &'a str,
    pub vertex_entry: &'a str,
    pub fragment_entry: &'a str,
    pub layout: VertexLayout,
    /// Size of the per-draw block in bytes (a multiple of 16).
    pub block_size: u32,
}

struct Program {
    module: wgpu::ShaderModule,
    vertex_entry: String,
    fragment_entry: String,
    layout: LayoutId,
    block_size: u32,
}

/// Where draws go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// The game's frame (internal resolution x supersampling, with the configured MSAA).
    Scene,
    /// A texture created with `render_target: true`.
    Texture(TextureId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topology {
    PointList,
    LineList,
    LineStrip,
    TriangleList,
    TriangleStrip,
    /// Expanded to a triangle list on the CPU.
    TriangleFan,
}

/// Vertex data of a draw.
#[derive(Clone, Copy, Debug)]
pub enum Vertices<'a> {
    /// Copied into the frame's vertex stream.
    Slice(&'a [u8]),
    /// A retained buffer (`create_vertex_buffer`), from this byte offset.
    Buffer(BufferId, u64),
}

#[derive(Clone, Copy, Debug)]
pub enum Indices<'a> {
    None,
    U16(&'a [u16]),
    U32(&'a [u32]),
    /// A retained index buffer.
    Buffer(BufferId),
}

/// One draw call.
#[derive(Clone, Copy, Debug)]
pub struct Draw<'a> {
    pub layout: LayoutId,
    pub topology: Topology,
    pub vertices: Vertices<'a>,
    pub indices: Indices<'a>,
    /// (first, count) of the indices (indexed) or vertices; None = all of them.
    pub range: Option<(u32, u32)>,
    /// Added to every index.
    pub base_vertex: i32,
    pub state: &'a RenderState,
    pub params: &'a DrawParams,
    /// Bone matrices (skinned layouts); at most MAX_BONES, missing ones are identity.
    pub bones: Option<&'a [Mat4]>,
}

/// Description of a texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    /// Mip levels; 0 = the full chain, generated from level 0 on upload.
    pub mip_levels: u32,
    /// Can be drawn into (`Target::Texture`); stored as RGBA8 with its own depth/stencil.
    pub render_target: bool,
    /// MSAA samples of a render target (resolved into the texture at the end of every pass).
    pub samples: u32,
}

impl TextureDesc {
    pub fn new(width: u32, height: u32, format: TexFormat) -> TextureDesc {
        TextureDesc { width, height, format, mip_levels: 1, render_target: false, samples: 1 }
    }

    pub fn render_target(width: u32, height: u32) -> TextureDesc {
        TextureDesc { width, height, format: TexFormat::Rgba8, mip_levels: 1, render_target: true, samples: 1 }
    }
}

struct Tex {
    desc: TextureDesc,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    levels: u32,
    auto_mips: bool,
    /// Level-0 attachment view, MSAA colour, depth/stencil.
    rt: Option<RtParts>,
    /// Replaced by a high-resolution texture (later uploads of the original are ignored).
    pub(crate) replaced: bool,
}

struct RtParts {
    attach: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    samples: u32,
}

struct Buf {
    buffer: wgpu::Buffer,
    size: u64,
    /// CPU copy of index buffers (fans are expanded from it), and their format.
    shadow: Option<(Vec<u32>, wgpu::IndexFormat)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipeKey {
    layout: LayoutId,
    program: Option<ProgramId>,
    fixed: FixedKey,
    topology: wgpu::PrimitiveTopology,
    strip_index: Option<wgpu::IndexFormat>,
    samples: u32,
}

type BindKey = [(Option<TextureId>, Sampler); STAGES];

#[derive(Clone, Copy, Debug, PartialEq)]
enum VbRef {
    Stream(u64),
    Buffer(BufferId, u64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum IbRef {
    Stream(u64, wgpu::IndexFormat),
    Buffer(BufferId, wgpu::IndexFormat),
}

#[derive(Clone, Copy, Debug)]
struct DrawOp {
    pipe: PipeKey,
    binds: BindKey,
    view: u32,
    /// Draw block: the fixed-function block, or (with `prog`) one of a program's blocks.
    block: u32,
    /// Block size of the program pool the block is in.
    prog: Option<u32>,
    bones: Option<u32>,
    vb: VbRef,
    ib: Option<IbRef>,
    first: u32,
    count: u32,
    base_vertex: i32,
    stencil_ref: u8,
    blend_constant: [f32; 4],
    viewport: Option<[f32; 6]>,
    scissor: Option<[u32; 4]>,
}

#[derive(Clone, Copy, Debug)]
enum Cmd {
    Target(Target),
    Clear { color: Option<[f64; 4]>, depth: Option<f32>, stencil: Option<u32> },
    Draw(DrawOp),
    Copy { src: Target, rect: Option<[u32; 4]>, dst: TextureId, at: [u32; 2] },
}

/// A uniform buffer of fixed-size blocks bound at dynamic offsets.
struct Blocks {
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
    stride: u64,
    capacity: usize,
    size: u64,
}

impl Blocks {
    fn new(gpu: &Gpu, layout: &wgpu::BindGroupLayout, size: usize, capacity: usize) -> Blocks {
        let stride = (size as u64).next_multiple_of(gpu.device.limits().min_uniform_buffer_offset_alignment as u64);
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("blocks"), size: stride * capacity as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blocks"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(size as u64) }) }],
        });
        Blocks { buffer, bind, stride, capacity, size: size as u64 }
    }

    fn write(&mut self, gpu: &Gpu, layout: &wgpu::BindGroupLayout, blocks: &[&[u32]]) {
        if blocks.len() > self.capacity {
            *self = Blocks::new(gpu, layout, self.size as usize, blocks.len().next_power_of_two());
        }
        if blocks.is_empty() {
            return;
        }
        let mut bytes = vec![0u8; blocks.len() * self.stride as usize];
        for (i, b) in blocks.iter().enumerate() {
            for (j, w) in b.iter().enumerate() {
                bytes[i * self.stride as usize + j * 4..][..4].copy_from_slice(&w.to_le_bytes());
            }
        }
        gpu.queue.write_buffer(&self.buffer, 0, &bytes);
    }

    fn offset(&self, i: u32) -> u32 {
        (i as u64 * self.stride) as u32
    }
}

/// A growable GPU copy of a per-frame byte stream.
struct Stream {
    buffer: wgpu::Buffer,
    usage: wgpu::BufferUsages,
}

impl Stream {
    fn new(gpu: &Gpu, usage: wgpu::BufferUsages, size: u64) -> Stream {
        Stream { buffer: gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("stream"), size, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }), usage }
    }

    fn write(&mut self, gpu: &Gpu, data: &[u8]) {
        if data.len() as u64 > self.buffer.size() {
            *self = Stream::new(gpu, self.usage, (data.len() as u64).next_power_of_two());
        }
        if !data.is_empty() {
            gpu.queue.write_buffer(&self.buffer, 0, data);
        }
    }
}

struct SceneTarget {
    color: wgpu::Texture,
    view: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    size: (u32, u32),
    samples: u32,
}

/// Renderer settings that change the scene's targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSettings {
    /// The game's resolution; None = the output's size.
    pub internal_size: Option<(u32, u32)>,
    /// Supersampling factor per axis (1 = off).
    pub supersample: u32,
    /// MSAA samples (clamped to what the device does).
    pub msaa: u32,
}

impl Default for SceneSettings {
    fn default() -> SceneSettings {
        SceneSettings { internal_size: None, supersample: 1, msaa: 1 }
    }
}

/// Hook for the output stage (built-in scaler or e.g. a slang shader chain).
pub trait OutputStage {
    /// Draws `image` (the post-processed game image at internal resolution) into `target`
    /// (`size` pixels, `format`).
    #[allow(clippy::too_many_arguments)]
    fn run(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, image: &wgpu::Texture, target: &wgpu::TextureView, format: wgpu::TextureFormat, size: (u32, u32), frame: u64);
}

pub struct Renderer {
    output_format: wgpu::TextureFormat,
    settings: SceneSettings,
    samples: u32,
    pub output: OutputSettings,
    post: Vec<Box<dyn PostPass>>,
    custom_output: Option<Box<dyn OutputStage>>,
    blitter: Blitter,
    view_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    bones_layout: wgpu::BindGroupLayout,
    pl_plain: wgpu::PipelineLayout,
    pl_skin: wgpu::PipelineLayout,
    layouts: Vec<(VertexLayout, wgpu::ShaderModule)>,
    programs: Vec<Program>,
    /// Program draw blocks of the frame and their GPU buffers, by block size.
    prog_blocks: HashMap<u32, Vec<Vec<u32>>>,
    prog_ub: HashMap<u32, Blocks>,
    pipelines: HashMap<PipeKey, wgpu::RenderPipeline>,
    samplers: HashMap<Sampler, wgpu::Sampler>,
    bind_groups: HashMap<BindKey, wgpu::BindGroup>,
    textures: HashMap<TextureId, Tex>,
    next_texture: u32,
    white: wgpu::TextureView,
    buffers: HashMap<BufferId, Buf>,
    next_buffer: u32,
    dead_textures: Vec<TextureId>,
    dead_buffers: Vec<BufferId>,
    retired: Vec<wgpu::Buffer>,
    pub(crate) replacer: Option<crate::replace::Replacer>,
    // The frame being recorded.
    cmds: Vec<Cmd>,
    vdata: Vec<u8>,
    idata: Vec<u8>,
    draw_blocks: Vec<DrawBlock>,
    view_blocks: Vec<[u32; VIEW_BLOCK / 4]>,
    bone_blocks: Vec<Vec<u32>>,
    target: Target,
    viewport: Option<[f32; 6]>,
    scissor: Option<[u32; 4]>,
    scratch: Vec<u32>,
    // Per-frame GPU buffers.
    vstream: Stream,
    istream: Stream,
    view_ub: Blocks,
    draw_ub: Blocks,
    bones_ub: Blocks,
    scene: Option<SceneTarget>,
    /// Internal-resolution images: downsampled scene, post ping-pong.
    work: [Option<wgpu::Texture>; 2],
    prescale: Option<wgpu::Texture>,
    final_image: Option<wgpu::Texture>,
    frame: u64,
    warned: std::collections::HashSet<&'static str>,
}

fn uniform_entry(visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding: 0, visibility, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None }, count: None }
}

fn as_bytes16(v: &[u16]) -> Vec<u8> {
    let mut b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    b.resize(b.len().next_multiple_of(4), 0);
    b
}

fn as_bytes32(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Indices of a triangle fan as a triangle list.
fn fan_to_list(ix: &[u32], out: &mut Vec<u32>) {
    out.clear();
    for i in 1..ix.len().saturating_sub(1) {
        out.extend([ix[0], ix[i], ix[i + 1]]);
    }
}

impl Renderer {
    /// A renderer whose output stage draws into textures/surfaces of `output_format`.
    pub fn new(gpu: &Gpu, output_format: wgpu::TextureFormat, settings: SceneSettings) -> Renderer {
        let d = &gpu.device;
        let view_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("view"), entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)] });
        let draw_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("draw"), entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)] });
        let bones_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("bones"), entries: &[uniform_entry(wgpu::ShaderStages::VERTEX)] });
        let mut entries = Vec::new();
        for s in 0..STAGES as u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2 * s,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry { binding: 2 * s + 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None });
        }
        let tex_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("stages"), entries: &entries });
        let pl_plain = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("draw"), bind_group_layouts: &[Some(&view_layout), Some(&draw_layout), Some(&tex_layout)], immediate_size: 0 });
        let pl_skin = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("skin"), bind_group_layouts: &[Some(&view_layout), Some(&draw_layout), Some(&tex_layout), Some(&bones_layout)], immediate_size: 0 });
        let white_tex = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("white"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &white_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &[255; 4],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let samples = gpu.supported_samples(TARGET_FORMAT, DEPTH_FORMAT, settings.msaa);
        Renderer {
            output_format,
            settings,
            samples,
            output: OutputSettings::default(),
            post: Vec::new(),
            custom_output: None,
            blitter: Blitter::new(gpu),
            view_ub: Blocks::new(gpu, &view_layout, VIEW_BLOCK, 16),
            draw_ub: Blocks::new(gpu, &draw_layout, DRAW_BLOCK, 256),
            bones_ub: Blocks::new(gpu, &bones_layout, BONES_BLOCK, 8),
            view_layout,
            draw_layout,
            tex_layout,
            bones_layout,
            pl_plain,
            pl_skin,
            layouts: Vec::new(),
            programs: Vec::new(),
            prog_blocks: HashMap::new(),
            prog_ub: HashMap::new(),
            pipelines: HashMap::new(),
            samplers: HashMap::new(),
            bind_groups: HashMap::new(),
            textures: HashMap::new(),
            next_texture: 1,
            white: white_tex.create_view(&Default::default()),
            buffers: HashMap::new(),
            next_buffer: 1,
            dead_textures: Vec::new(),
            dead_buffers: Vec::new(),
            retired: Vec::new(),
            replacer: None,
            cmds: Vec::new(),
            vdata: Vec::new(),
            idata: Vec::new(),
            draw_blocks: Vec::new(),
            view_blocks: Vec::new(),
            bone_blocks: Vec::new(),
            target: Target::Scene,
            viewport: None,
            scissor: None,
            scratch: Vec::new(),
            vstream: Stream::new(gpu, wgpu::BufferUsages::VERTEX, 1 << 16),
            istream: Stream::new(gpu, wgpu::BufferUsages::INDEX, 1 << 14),
            scene: None,
            work: [None, None],
            prescale: None,
            final_image: None,
            frame: 0,
            warned: Default::default(),
        }
    }

    fn warn(&mut self, what: &'static str) {
        if self.warned.insert(what) {
            eprintln!("retro-wgpu: {what}");
        }
    }

    // ---- Settings -------------------------------------------------------------------------

    /// Changes internal resolution, supersampling or MSAA (from the next frame on).
    pub fn set_scene(&mut self, gpu: &Gpu, settings: SceneSettings) {
        self.settings = SceneSettings { supersample: settings.supersample.clamp(1, 4), ..settings };
        self.samples = gpu.supported_samples(TARGET_FORMAT, DEPTH_FORMAT, settings.msaa);
    }

    /// The settings in effect (`msaa` as clamped to the device).
    pub fn scene_settings(&self) -> SceneSettings {
        SceneSettings { msaa: self.samples, ..self.settings }
    }

    /// The post chain run on the game image at internal resolution, in order.
    pub fn set_post_chain(&mut self, passes: Vec<Box<dyn PostPass>>) {
        self.post = passes;
    }

    pub fn post_chain(&self) -> &[Box<dyn PostPass>] {
        &self.post
    }

    /// Replaces the built-in scaler (e.g. with a slang shader preset); None restores it.
    pub fn set_output_stage(&mut self, stage: Option<Box<dyn OutputStage>>) {
        self.custom_output = stage;
    }

    /// The internal size for an output of `output` pixels.
    pub fn internal_size(&self, output: (u32, u32)) -> (u32, u32) {
        self.settings.internal_size.unwrap_or(output)
    }

    // ---- Resources ------------------------------------------------------------------------

    /// Registers a vertex layout (its shader is generated and compiled here).
    pub fn create_layout(&mut self, gpu: &Gpu, layout: VertexLayout) -> Result<LayoutId, String> {
        layout.validate()?;
        if let Some(i) = self.layouts.iter().position(|(l, _)| *l == layout) {
            return Ok(LayoutId(i as u32));
        }
        let src = shader::source(&layout);
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("retro draw"), source: wgpu::ShaderSource::Wgsl(src.into()) });
        self.layouts.push((layout, module));
        Ok(LayoutId(self.layouts.len() as u32 - 1))
    }

    /// Registers a front end's shader program (see `ProgramDesc`).
    pub fn create_program(&mut self, gpu: &Gpu, desc: &ProgramDesc) -> Result<ProgramId, String> {
        if desc.block_size == 0 || desc.block_size % 16 != 0 {
            return Err(format!("program {}: block size {} is not a positive multiple of 16", desc.label, desc.block_size));
        }
        if desc.block_size > gpu.device.limits().max_uniform_buffer_binding_size as u32 {
            return Err(format!("program {}: block size {} over the device's uniform limit", desc.label, desc.block_size));
        }
        let layout = self.create_layout(gpu, desc.layout.clone())?;
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(desc.label), source: wgpu::ShaderSource::Wgsl(desc.wgsl.into()) });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("program {}: {e}", desc.label));
        }
        self.programs.push(Program { module, vertex_entry: desc.vertex_entry.into(), fragment_entry: desc.fragment_entry.into(), layout, block_size: desc.block_size });
        Ok(ProgramId(self.programs.len() as u32 - 1))
    }

    /// The generated WGSL of a layout (for debugging).
    pub fn layout_source(&self, id: LayoutId) -> Option<String> {
        self.layouts.get(id.0 as usize).map(|(l, _)| shader::source(l))
    }

    pub fn create_vertex_buffer(&mut self, gpu: &Gpu, data: &[u8]) -> BufferId {
        let mut b = data.to_vec();
        b.resize(b.len().next_multiple_of(4).max(4), 0);
        use wgpu::util::DeviceExt;
        let buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("vertices"), contents: &b, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
        self.add_buffer(Buf { buffer, size: data.len() as u64, shadow: None })
    }

    pub fn create_index_buffer16(&mut self, gpu: &Gpu, data: &[u16]) -> BufferId {
        self.index_buffer(gpu, as_bytes16(data), data.iter().map(|&i| i as u32).collect(), wgpu::IndexFormat::Uint16)
    }

    pub fn create_index_buffer32(&mut self, gpu: &Gpu, data: &[u32]) -> BufferId {
        self.index_buffer(gpu, as_bytes32(data), data.to_vec(), wgpu::IndexFormat::Uint32)
    }

    fn index_buffer(&mut self, gpu: &Gpu, bytes: Vec<u8>, shadow: Vec<u32>, f: wgpu::IndexFormat) -> BufferId {
        let mut b = bytes;
        b.resize(b.len().max(4), 0);
        use wgpu::util::DeviceExt;
        let buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("indices"), contents: &b, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST });
        self.add_buffer(Buf { buffer, size: b.len() as u64, shadow: Some((shadow, f)) })
    }

    fn add_buffer(&mut self, b: Buf) -> BufferId {
        let id = BufferId(self.next_buffer);
        self.next_buffer += 1;
        self.buffers.insert(id, b);
        id
    }

    /// Overwrites part of a retained vertex buffer (bytes, offset a multiple of 4).
    pub fn update_vertex_buffer(&mut self, gpu: &Gpu, id: BufferId, offset: u64, data: &[u8]) {
        if let Some(b) = self.buffers.get(&id)
            && offset + data.len() as u64 <= b.buffer.size()
        {
            let mut d = data.to_vec();
            d.resize(d.len().next_multiple_of(4), 0);
            gpu.queue.write_buffer(&b.buffer, offset, &d);
        }
    }

    /// Frees a buffer after the current frame.
    pub fn destroy_buffer(&mut self, id: BufferId) {
        self.dead_buffers.push(id);
    }

    /// Creates a texture. Data comes with `write_texture` (or `upload_texture` does both).
    pub fn create_texture(&mut self, gpu: &Gpu, desc: TextureDesc) -> TextureId {
        let id = TextureId(self.next_texture);
        self.next_texture += 1;
        let tex = self.make_texture(gpu, desc, None);
        self.textures.insert(id, tex);
        id
    }

    fn make_texture(&mut self, gpu: &Gpu, desc: TextureDesc, forced: Option<(wgpu::TextureFormat, u32, u32, u32)>) -> Tex {
        let (w, h) = (desc.width.max(1), desc.height.max(1));
        let (format, w, h, levels, auto) = match forced {
            Some((f, w, h, l)) => (f, w, h, l, false),
            None => {
                let format = if desc.render_target { TARGET_FORMAT } else { desc.format.gpu_format(gpu.bc_textures(), w, h) };
                let auto = desc.mip_levels == 0 && !format.is_compressed();
                let levels = if desc.mip_levels == 0 { if auto { full_chain(w, h) } else { 1 } } else { desc.mip_levels.min(full_chain(w, h)) };
                (format, w, h, levels, auto)
            }
        };
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        if !format.is_compressed() {
            usage |= wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT;
        }
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let rt = desc.render_target.then(|| {
            let samples = gpu.supported_samples(TARGET_FORMAT, DEPTH_FORMAT, desc.samples);
            let attach = texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 0, mip_level_count: Some(1), ..Default::default() });
            let msaa = (samples > 1).then(|| attachment(gpu, "rt msaa", (w, h), TARGET_FORMAT, samples));
            RtParts { attach, msaa, depth: attachment(gpu, "rt depth", (w, h), DEPTH_FORMAT, samples), samples }
        });
        Tex { desc, texture, view, format, levels, auto_mips: auto, rt, replaced: false }
    }

    /// Writes mip `level` of a texture from data in its format (`palette` for indexed formats).
    /// Level 0 of a texture with automatic mips also rebuilds the chain.
    pub fn write_texture(&mut self, gpu: &Gpu, id: TextureId, level: u32, data: &[u8], palette: Option<&Palette>) -> Result<(), String> {
        let Some(t) = self.textures.get(&id) else { return Err(format!("no texture {id:?}")) };
        let desc = t.desc;
        if level == 0 && !desc.render_target && self.replacer.is_some() {
            let replaced = crate::replace::on_upload(self, gpu, id, desc, data, palette);
            if replaced {
                return Ok(());
            }
        }
        if self.textures[&id].replaced {
            if level != 0 || self.replacer.as_ref().is_some_and(|r| r.replaced.contains(&id)) {
                return Ok(());
            }
            // The texture got new contents without a replacement: back to the original storage.
            let t = self.make_texture(gpu, desc, None);
            self.textures.insert(id, t);
            self.bind_groups.retain(|k, _| !k.iter().any(|(t, _)| *t == Some(id)));
        }
        let t = &self.textures[&id];
        if level >= t.levels {
            return Ok(());
        }
        let (w, h) = ((t.desc.width.max(1) >> level).max(1), (t.desc.height.max(1) >> level).max(1));
        if t.format.is_compressed() {
            let need = desc.format.level_size(w, h);
            if data.len() < need {
                return Err(format!("level {level}: {} bytes, need {need}", data.len()));
            }
            write_raw(gpu, &t.texture, t.format, level, (0, 0, w, h), &data[..need]);
            return Ok(());
        }
        let rgba = if t.format == wgpu::TextureFormat::Bgra8Unorm {
            let n = (w * h * 4) as usize;
            if data.len() < n {
                return Err("short data".into());
            }
            data[..n].to_vec()
        } else {
            to_rgba8(desc.format, w, h, data, palette).ok_or_else(|| format!("level {level}: data too short or palette missing"))?
        };
        if level == 0 && t.auto_mips {
            for (l, px) in mip_chain(&rgba, w, h).iter().enumerate().take(t.levels as usize) {
                let (lw, lh) = ((w >> l).max(1), (h >> l).max(1));
                write_raw(gpu, &t.texture, t.format, l as u32, (0, 0, lw, lh), px);
            }
        } else {
            write_raw(gpu, &t.texture, t.format, level, (0, 0, w, h), &rgba);
        }
        Ok(())
    }

    /// Updates a rectangle of a level (non-block formats; the rectangle's data in the texture's
    /// format, `w * h` texels). Automatic mips are not rebuilt.
    #[allow(clippy::too_many_arguments)]
    pub fn write_texture_region(&mut self, gpu: &Gpu, id: TextureId, level: u32, x: u32, y: u32, w: u32, h: u32, data: &[u8], palette: Option<&Palette>) -> Result<(), String> {
        let Some(t) = self.textures.get(&id) else { return Err(format!("no texture {id:?}")) };
        if t.replaced {
            return Ok(());
        }
        if t.format.is_compressed() || t.desc.format.is_block() {
            return Err("region updates of block-compressed textures aren't supported".into());
        }
        let rgba = if t.format == wgpu::TextureFormat::Bgra8Unorm { data[..(w * h * 4) as usize].to_vec() } else { to_rgba8(t.desc.format, w, h, data, palette).ok_or("short data or no palette")? };
        write_raw(gpu, &t.texture, t.format, level, (x, y, w, h), &rgba);
        Ok(())
    }

    /// Creates a texture and writes its levels (`levels.len()` levels, or the full chain from
    /// level 0 with `auto_mips`).
    pub fn upload_texture(&mut self, gpu: &Gpu, width: u32, height: u32, format: TexFormat, levels: &[&[u8]], palette: Option<&Palette>, auto_mips: bool) -> Result<TextureId, String> {
        let desc = TextureDesc { width, height, format, mip_levels: if auto_mips { 0 } else { levels.len().max(1) as u32 }, render_target: false, samples: 1 };
        let id = self.create_texture(gpu, desc);
        for (l, data) in levels.iter().enumerate() {
            self.write_texture(gpu, id, l as u32, data, palette)?;
            if auto_mips {
                break;
            }
        }
        Ok(id)
    }

    /// Frees a texture after the current frame.
    pub fn destroy_texture(&mut self, id: TextureId) {
        self.dead_textures.push(id);
    }

    /// The wgpu texture behind an id (e.g. to read a render target back).
    pub fn texture(&self, id: TextureId) -> Option<&wgpu::Texture> {
        self.textures.get(&id).map(|t| &t.texture)
    }

    pub fn texture_desc(&self, id: TextureId) -> Option<TextureDesc> {
        self.textures.get(&id).map(|t| t.desc)
    }

    /// Size of the texture as stored (a replacement may be larger than the original).
    pub fn texture_size(&self, id: TextureId) -> Option<(u32, u32)> {
        self.textures.get(&id).map(|t| (t.texture.width(), t.texture.height()))
    }

    pub(crate) fn replace_texture_storage(&mut self, gpu: &Gpu, id: TextureId, format: wgpu::TextureFormat, w: u32, h: u32, levels: &[Vec<u8>]) {
        let Some(old) = self.textures.get(&id) else { return };
        let desc = old.desc;
        let mut t = self.make_texture(gpu, desc, Some((format, w, h, levels.len().max(1) as u32)));
        for (l, data) in levels.iter().enumerate() {
            write_raw(gpu, &t.texture, format, l as u32, (0, 0, (w >> l).max(1), (h >> l).max(1)), data);
        }
        t.replaced = true;
        self.textures.insert(id, t);
        self.bind_groups.retain(|k, _| !k.iter().any(|(t, _)| *t == Some(id)));
    }

    // ---- Recording ------------------------------------------------------------------------

    /// Following clears and draws go to `target`.
    pub fn set_target(&mut self, target: Target) {
        if target != self.target {
            self.cmds.push(Cmd::Target(target));
            self.target = target;
            self.viewport = None;
            self.scissor = None;
        }
    }

    /// Clears the whole current target: colour (0..1), depth, stencil, each if given.
    pub fn clear(&mut self, color: Option<[f64; 4]>, depth: Option<f32>, stencil: Option<u32>) {
        self.cmds.push(Cmd::Clear { color, depth, stencil });
    }

    /// Viewport for following draws (x, y, w, h, min depth, max depth); None = whole target.
    pub fn set_viewport(&mut self, viewport: Option<[f32; 6]>) {
        self.viewport = viewport;
    }

    /// Scissor rectangle (x, y, w, h); None = off.
    pub fn set_scissor(&mut self, scissor: Option<[u32; 4]>) {
        self.scissor = scissor;
    }

    /// The camera for following draws.
    pub fn set_view(&mut self, view: &View) {
        let b = shader::view_block(view);
        if self.view_blocks.last() != Some(&b) {
            self.view_blocks.push(b);
        }
    }

    /// Copies (a rectangle of) `src` into `dst` at `at` when the frame is drawn, e.g. the frame
    /// so far into a texture for feedback effects. `rect` in source pixels (x, y, w, h); None =
    /// all of it. Same-format copies are exact; otherwise the copy is filtered.
    pub fn copy_to_texture(&mut self, src: Target, rect: Option<[u32; 4]>, dst: TextureId, at: [u32; 2]) {
        self.cmds.push(Cmd::Copy { src, rect, dst, at });
    }

    /// Records a draw.
    pub fn draw(&mut self, gpu: &Gpu, d: &Draw) {
        self.record_draw(gpu, d, None);
    }

    /// Records a draw with a program (its layout replaces `d.layout`; `d.params` is unused) and
    /// the program's per-draw block (`block`, at most its block size; the rest is zero).
    pub fn draw_program(&mut self, gpu: &Gpu, d: &Draw, program: ProgramId, block: &[u8]) {
        let Some(p) = self.programs.get(program.0 as usize) else {
            self.warn("draw with an unknown program skipped");
            return;
        };
        let (layout, size) = (p.layout, p.block_size);
        let mut words = vec![0u32; size as usize / 4];
        for (w, c) in words.iter_mut().zip(block.chunks(4)) {
            let mut b = [0u8; 4];
            b[..c.len()].copy_from_slice(c);
            *w = u32::from_le_bytes(b);
        }
        self.record_draw(gpu, &Draw { layout, ..*d }, Some((program, size, words)));
    }

    fn record_draw(&mut self, gpu: &Gpu, d: &Draw, program: Option<(ProgramId, u32, Vec<u32>)>) {
        let _ = gpu;
        let Some((layout, _)) = self.layouts.get(d.layout.0 as usize) else {
            self.warn("draw with an unknown layout skipped");
            return;
        };
        let stride = layout.stride as u64;
        let skinned = layout.skinned();
        if layout.has(Semantic::PositionT) && d.params.lighting.enabled {
            // Pre-transformed vertices are never lit (as in D3D); the shader ignores lighting.
        }
        // Vertices.
        let vb = match d.vertices {
            Vertices::Slice(bytes) => {
                let at = self.vdata.len().next_multiple_of(stride.max(4) as usize);
                self.vdata.resize(at, 0);
                self.vdata.extend_from_slice(bytes);
                let pad = self.vdata.len().next_multiple_of(4);
                self.vdata.resize(pad, 0);
                VbRef::Stream(at as u64)
            }
            Vertices::Buffer(id, off) => {
                if !self.buffers.contains_key(&id) {
                    return;
                }
                VbRef::Buffer(id, off)
            }
        };
        let vertex_count = match d.vertices {
            Vertices::Slice(b) => (b.len() as u64 / stride.max(1)) as u32,
            Vertices::Buffer(id, off) => (self.buffers[&id].size.saturating_sub(off) / stride.max(1)) as u32,
        };
        // Indices (fans become lists).
        let fan = d.topology == Topology::TriangleFan;
        let mut ib = None;
        let (mut first, mut count);
        let indexed_count = match d.indices {
            Indices::None => None,
            Indices::U16(v) => Some(v.len() as u32),
            Indices::U32(v) => Some(v.len() as u32),
            Indices::Buffer(id) => self.buffers.get(&id).and_then(|b| b.shadow.as_ref()).map(|s| s.0.len() as u32),
        };
        let total = indexed_count.unwrap_or(vertex_count);
        (first, count) = d.range.unwrap_or((0, total));
        if first >= total {
            return;
        }
        count = count.min(total - first);
        if fan {
            self.scratch.clear();
            let src: Vec<u32> = match d.indices {
                Indices::None => (first..first + count).collect(),
                Indices::U16(v) => v[first as usize..(first + count) as usize].iter().map(|&i| i as u32).collect(),
                Indices::U32(v) => v[first as usize..(first + count) as usize].to_vec(),
                Indices::Buffer(id) => match self.buffers.get(&id).and_then(|b| b.shadow.as_ref()) {
                    Some(s) => s.0[first as usize..(first + count) as usize].to_vec(),
                    None => return,
                },
            };
            let mut list = std::mem::take(&mut self.scratch);
            fan_to_list(&src, &mut list);
            let at = self.idata.len();
            self.idata.extend(as_bytes32(&list));
            ib = Some(IbRef::Stream(at as u64, wgpu::IndexFormat::Uint32));
            first = 0;
            count = list.len() as u32;
            self.scratch = list;
        } else {
            match d.indices {
                Indices::None => {}
                Indices::U16(v) => {
                    let at = self.idata.len();
                    self.idata.extend(as_bytes16(v));
                    ib = Some(IbRef::Stream(at as u64, wgpu::IndexFormat::Uint16));
                }
                Indices::U32(v) => {
                    let at = self.idata.len();
                    self.idata.extend(as_bytes32(v));
                    ib = Some(IbRef::Stream(at as u64, wgpu::IndexFormat::Uint32));
                }
                Indices::Buffer(id) => {
                    let Some((_, f)) = self.buffers.get(&id).and_then(|b| b.shadow.as_ref()) else { return };
                    ib = Some(IbRef::Buffer(id, *f));
                }
            }
        }
        if count == 0 {
            return;
        }
        let topology = match d.topology {
            Topology::PointList => wgpu::PrimitiveTopology::PointList,
            Topology::LineList => wgpu::PrimitiveTopology::LineList,
            Topology::LineStrip => wgpu::PrimitiveTopology::LineStrip,
            Topology::TriangleList | Topology::TriangleFan => wgpu::PrimitiveTopology::TriangleList,
            Topology::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        };
        let strip_index = match (topology, ib) {
            (wgpu::PrimitiveTopology::LineStrip | wgpu::PrimitiveTopology::TriangleStrip, Some(IbRef::Stream(_, f) | IbRef::Buffer(_, f))) => Some(f),
            _ => None,
        };
        let samples = match self.target {
            Target::Scene => self.samples,
            Target::Texture(t) => match self.textures.get(&t).and_then(|t| t.rt.as_ref()) {
                Some(rt) => rt.samples,
                None => {
                    self.warn("draw into a texture that isn't a render target skipped");
                    return;
                }
            },
        };
        let mut fixed = d.state.fixed_key();
        if !matches!(topology, wgpu::PrimitiveTopology::TriangleList | wgpu::PrimitiveTopology::TriangleStrip) {
            fixed.bias = (0, 0, 0);
        }
        let pipe = PipeKey { layout: d.layout, program: program.as_ref().map(|p| p.0), fixed, topology, strip_index, samples };
        // Textures: the current target can't be sampled while drawn into (white instead).
        let binds: BindKey = std::array::from_fn(|s| {
            let st = &d.state.stages[s];
            let tex = st.texture.filter(|t| self.textures.contains_key(t) && Target::Texture(*t) != self.target);
            let sampler = match (tex, &self.replacer) {
                (Some(t), Some(r)) if r.replaced.contains(&t) => crate::replace::upgrade_sampler(&r.config, st.sampler),
                (Some(_), _) => st.sampler,
                (None, _) => Sampler::POINT_CLAMP,
            };
            (tex, sampler)
        });
        let (block, prog) = match program {
            Some((_, size, words)) => {
                let pool = self.prog_blocks.entry(size).or_default();
                if pool.last() != Some(&words) {
                    pool.push(words);
                }
                (pool.len() as u32 - 1, Some(size))
            }
            None => {
                let block = shader::draw_block(d.state, d.params);
                if self.draw_blocks.last() != Some(&block) {
                    self.draw_blocks.push(block);
                }
                (self.draw_blocks.len() as u32 - 1, None)
            }
        };
        if self.view_blocks.is_empty() {
            self.view_blocks.push(shader::view_block(&View::screen(640.0, 480.0)));
        }
        let bones = skinned.then(|| {
            let mut b = vec![0u32; MAX_BONES * 16];
            for i in 0..MAX_BONES {
                let m = d.bones.and_then(|bs| bs.get(i)).copied().unwrap_or(IDENTITY);
                for (j, x) in m.iter().enumerate() {
                    b[i * 16 + j] = x.to_bits();
                }
            }
            if self.bone_blocks.last() != Some(&b) {
                self.bone_blocks.push(b);
            }
            self.bone_blocks.len() as u32 - 1
        });
        self.cmds.push(Cmd::Draw(DrawOp {
            pipe,
            binds,
            view: self.view_blocks.len() as u32 - 1,
            block,
            prog,
            bones,
            vb,
            ib,
            first,
            count,
            base_vertex: d.base_vertex,
            stencil_ref: d.state.stencil_reference,
            blend_constant: d.state.blend_constant,
            viewport: self.viewport,
            scissor: self.scissor,
        }));
    }

    // ---- Encoding -------------------------------------------------------------------------

    fn ensure_pipeline(&mut self, gpu: &Gpu, key: PipeKey) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let (layout, core) = &self.layouts[key.layout.0 as usize];
        let (module, vs, fs) = match key.program.and_then(|p| self.programs.get(p.0 as usize)) {
            Some(p) => (&p.module, p.vertex_entry.as_str(), p.fragment_entry.as_str()),
            None => (core, "vs", "fs"),
        };
        let attrs: Vec<wgpu::VertexAttribute> = layout.attributes.iter().map(|a| wgpu::VertexAttribute { format: a.format.wgpu(), offset: a.offset as u64, shader_location: VertexLayout::location(a.semantic) }).collect();
        let f = key.fixed;
        let stencil_face = |s: StencilFace| wgpu::StencilFaceState { compare: s.compare, fail_op: s.fail, depth_fail_op: s.depth_fail, pass_op: s.pass };
        let mut mask = wgpu::ColorWrites::empty();
        if f.mask.rgb {
            mask |= wgpu::ColorWrites::COLOR;
        }
        if f.mask.alpha {
            mask |= wgpu::ColorWrites::ALPHA;
        }
        let p = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("retro draw"),
            layout: Some(if layout.skinned() { &self.pl_skin } else { &self.pl_plain }),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(vs),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: layout.stride as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fs),
                targets: &[Some(wgpu::ColorTargetState { format: TARGET_FORMAT, blend: f.blend.map(|b| b.wgpu()), write_mask: mask })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: key.topology, strip_index_format: key.strip_index, front_face: f.front_face, cull_mode: f.cull, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(f.depth.write),
                depth_compare: Some(f.depth.compare),
                stencil: wgpu::StencilState { front: stencil_face(f.stencil.front), back: stencil_face(f.stencil.back), read_mask: f.stencil.read_mask as u32, write_mask: f.stencil.write_mask as u32 },
                bias: wgpu::DepthBiasState { constant: f.bias.0, slope_scale: f32::from_bits(f.bias.1), clamp: f32::from_bits(f.bias.2) },
            }),
            multisample: wgpu::MultisampleState { count: key.samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, p);
    }

    fn ensure_bind_group(&mut self, gpu: &Gpu, key: BindKey) {
        if self.bind_groups.contains_key(&key) {
            return;
        }
        for (_, s) in &key {
            self.samplers.entry(*s).or_insert_with(|| gpu.device.create_sampler(&wgpu_sampler(s)));
        }
        let mut entries = Vec::new();
        for (i, (t, s)) in key.iter().enumerate() {
            let view = t.and_then(|t| self.textures.get(&t)).map_or(&self.white, |t| &t.view);
            entries.push(wgpu::BindGroupEntry { binding: 2 * i as u32, resource: wgpu::BindingResource::TextureView(view) });
            entries.push(wgpu::BindGroupEntry { binding: 2 * i as u32 + 1, resource: wgpu::BindingResource::Sampler(&self.samplers[s]) });
        }
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("stages"), layout: &self.tex_layout, entries: &entries });
        self.bind_groups.insert(key, bg);
    }

    fn scene_size(&self, output: (u32, u32)) -> (u32, u32) {
        let (w, h) = self.internal_size(output);
        (w.max(1) * self.settings.supersample, h.max(1) * self.settings.supersample)
    }

    /// Replays the recorded frame and draws the result into `output` (`size` pixels, the format
    /// given to `new`). The recording stays until `end_frame`.
    pub fn encode(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, output: &wgpu::TextureView, size: (u32, u32)) {
        let scene_size = self.scene_size(size);
        if self.scene.as_ref().is_none_or(|s| s.size != scene_size || s.samples != self.samples) {
            let color = render_texture(gpu, "scene", scene_size, TARGET_FORMAT);
            let view = color.create_view(&Default::default());
            let msaa = (self.samples > 1).then(|| attachment(gpu, "scene msaa", scene_size, TARGET_FORMAT, self.samples));
            let depth = attachment(gpu, "scene depth", scene_size, DEPTH_FORMAT, self.samples);
            self.scene = Some(SceneTarget { color, view, msaa, depth, size: scene_size, samples: self.samples });
        }
        // Upload the frame's data.
        self.vstream.write(gpu, &self.vdata);
        self.istream.write(gpu, &self.idata);
        let vb: Vec<&[u32]> = self.view_blocks.iter().map(|b| b.as_slice()).collect();
        self.view_ub.write(gpu, &self.view_layout, &vb);
        let db: Vec<&[u32]> = self.draw_blocks.iter().map(|b| b.as_slice()).collect();
        self.draw_ub.write(gpu, &self.draw_layout, &db);
        let bb: Vec<&[u32]> = self.bone_blocks.iter().map(|b| b.as_slice()).collect();
        self.bones_ub.write(gpu, &self.bones_layout, &bb);
        for (size, blocks) in &self.prog_blocks {
            let ub = self.prog_ub.entry(*size).or_insert_with(|| Blocks::new(gpu, &self.draw_layout, *size as usize, 64));
            let pb: Vec<&[u32]> = blocks.iter().map(|b| b.as_slice()).collect();
            ub.write(gpu, &self.draw_layout, &pb);
        }
        let cmds = std::mem::take(&mut self.cmds);
        for c in &cmds {
            if let Cmd::Draw(d) = c {
                self.ensure_pipeline(gpu, d.pipe);
                self.ensure_bind_group(gpu, d.binds);
            }
        }

        let scene = self.scene.as_ref().unwrap();
        let mut pass: Option<wgpu::RenderPass<'static>> = None;
        let mut target = Target::Scene;
        let mut target_size = scene_size;
        let mut ok = true;
        #[derive(Default)]
        struct Bound {
            pipe: Option<PipeKey>,
            view: Option<u32>,
            block: Option<(Option<u32>, u32)>,
            binds: Option<BindKey>,
            bones: Option<u32>,
            vb: Option<VbRef>,
            ib: Option<IbRef>,
            stencil: Option<u8>,
            blend: Option<[f32; 4]>,
            viewport: Option<Option<[f32; 6]>>,
            scissor: Option<Option<[u32; 4]>>,
        }
        let mut bound = Bound::default();
        let attachments = |t: Target| -> Option<(&wgpu::TextureView, Option<&wgpu::TextureView>, &wgpu::TextureView, (u32, u32))> {
            match t {
                Target::Scene => Some(match &scene.msaa {
                    Some(m) => (m, Some(&scene.view), &scene.depth, scene.size),
                    None => (&scene.view, None, &scene.depth, scene.size),
                }),
                Target::Texture(id) => {
                    let tex = self.textures.get(&id)?;
                    let rt = tex.rt.as_ref()?;
                    let size = (tex.texture.width(), tex.texture.height());
                    Some(match &rt.msaa {
                        Some(m) => (m, Some(&rt.attach), &rt.depth, size),
                        None => (&rt.attach, None, &rt.depth, size),
                    })
                }
            }
        };
        let begin = |enc: &mut wgpu::CommandEncoder, t: Target, color: Option<[f64; 4]>, depth: Option<f32>, stencil: Option<u32>| -> Option<wgpu::RenderPass<'static>> {
            let (view, resolve, ds, _) = attachments(t)?;
            Some(
                enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("retro frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: resolve,
                        depth_slice: None,
                        ops: wgpu::Operations { load: color.map_or(wgpu::LoadOp::Load, |c| wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] })), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: ds,
                        depth_ops: Some(wgpu::Operations { load: depth.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear), store: wgpu::StoreOp::Store }),
                        stencil_ops: Some(wgpu::Operations { load: stencil.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear), store: wgpu::StoreOp::Store }),
                    }),
                    ..Default::default()
                })
                .forget_lifetime(),
            )
        };
        for c in &cmds {
            match *c {
                Cmd::Target(t) => {
                    drop(pass.take());
                    target = t;
                    ok = attachments(t).is_some();
                    target_size = attachments(t).map_or((1, 1), |a| a.3);
                }
                Cmd::Clear { color, depth, stencil } => {
                    if !ok {
                        continue;
                    }
                    drop(pass.take());
                    pass = begin(enc, target, color, depth, stencil);
                    bound = Bound::default();
                }
                Cmd::Copy { src, rect, dst, at } => {
                    drop(pass.take());
                    let (Some(s), Some(d)) = (attachments(src).map(|a| (a.1.is_some(), a.3)), self.textures.get(&dst)) else { continue };
                    let src_tex = match src {
                        Target::Scene => &scene.color,
                        Target::Texture(id) => &self.textures[&id].texture,
                    };
                    let _ = s.0;
                    let r = rect.unwrap_or([0, 0, s.1.0, s.1.1]);
                    let (sx, sy) = (r[0].min(s.1.0), r[1].min(s.1.1));
                    let (dw, dh) = (d.texture.width(), d.texture.height());
                    let w = r[2].min(s.1.0 - sx).min(dw.saturating_sub(at[0]));
                    let h = r[3].min(s.1.1 - sy).min(dh.saturating_sub(at[1]));
                    if w == 0 || h == 0 {
                        continue;
                    }
                    if d.format == TARGET_FORMAT {
                        enc.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo { texture: src_tex, mip_level: 0, origin: wgpu::Origin3d { x: sx, y: sy, z: 0 }, aspect: wgpu::TextureAspect::All },
                            wgpu::TexelCopyTextureInfo { texture: &d.texture, mip_level: 0, origin: wgpu::Origin3d { x: at[0], y: at[1], z: 0 }, aspect: wgpu::TextureAspect::All },
                            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        );
                    } else if !d.format.is_compressed() {
                        let dview = d.texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 0, mip_level_count: Some(1), ..Default::default() });
                        let (fw, fh) = (s.1.0 as f32, s.1.1 as f32);
                        let uv = [sx as f32 / fw, sy as f32 / fh, w as f32 / fw, h as f32 / fh];
                        self.blitter.blit(gpu, enc, src_tex, false, &dview, d.format, Some([at[0] as f32, at[1] as f32, w as f32, h as f32]), uv, None, None);
                    }
                }
                Cmd::Draw(d) => {
                    if !ok {
                        continue;
                    }
                    if pass.is_none() {
                        pass = begin(enc, target, None, None, None);
                        bound = Bound::default();
                    }
                    let Some(p) = pass.as_mut() else { continue };
                    if bound.pipe != Some(d.pipe) {
                        p.set_pipeline(&self.pipelines[&d.pipe]);
                        bound.pipe = Some(d.pipe);
                    }
                    if bound.view != Some(d.view) {
                        p.set_bind_group(0, &self.view_ub.bind, &[self.view_ub.offset(d.view)]);
                        bound.view = Some(d.view);
                    }
                    if bound.block != Some((d.prog, d.block)) {
                        let ub = match d.prog {
                            Some(size) => &self.prog_ub[&size],
                            None => &self.draw_ub,
                        };
                        p.set_bind_group(1, &ub.bind, &[ub.offset(d.block)]);
                        bound.block = Some((d.prog, d.block));
                    }
                    if bound.binds != Some(d.binds) {
                        p.set_bind_group(2, &self.bind_groups[&d.binds], &[]);
                        bound.binds = Some(d.binds);
                    }
                    if let Some(b) = d.bones
                        && bound.bones != Some(b)
                    {
                        p.set_bind_group(3, &self.bones_ub.bind, &[self.bones_ub.offset(b)]);
                        bound.bones = Some(b);
                    }
                    if bound.stencil != Some(d.stencil_ref) {
                        p.set_stencil_reference(d.stencil_ref as u32);
                        bound.stencil = Some(d.stencil_ref);
                    }
                    if bound.blend != Some(d.blend_constant) {
                        let c = d.blend_constant;
                        p.set_blend_constant(wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 });
                        bound.blend = Some(d.blend_constant);
                    }
                    if bound.viewport != Some(d.viewport) {
                        let (tw, th) = (target_size.0 as f32, target_size.1 as f32);
                        let v = d.viewport.unwrap_or([0.0, 0.0, tw, th, 0.0, 1.0]);
                        let x = v[0].clamp(0.0, tw);
                        let y = v[1].clamp(0.0, th);
                        let w = v[2].min(tw - x).max(0.0);
                        let h = v[3].min(th - y).max(0.0);
                        if w <= 0.0 || h <= 0.0 {
                            continue;
                        }
                        p.set_viewport(x, y, w, h, v[4].clamp(0.0, 1.0), v[5].clamp(0.0, 1.0));
                        bound.viewport = Some(d.viewport);
                    }
                    if bound.scissor != Some(d.scissor) {
                        let s = d.scissor.unwrap_or([0, 0, target_size.0, target_size.1]);
                        let x = s[0].min(target_size.0);
                        let y = s[1].min(target_size.1);
                        p.set_scissor_rect(x, y, s[2].min(target_size.0 - x), s[3].min(target_size.1 - y));
                        bound.scissor = Some(d.scissor);
                    }
                    if bound.vb != Some(d.vb) {
                        match d.vb {
                            VbRef::Stream(off) => p.set_vertex_buffer(0, self.vstream.buffer.slice(off..)),
                            VbRef::Buffer(id, off) => p.set_vertex_buffer(0, self.buffers[&id].buffer.slice(off..)),
                        }
                        bound.vb = Some(d.vb);
                    }
                    match d.ib {
                        Some(ib) => {
                            if bound.ib != Some(ib) {
                                match ib {
                                    IbRef::Stream(off, f) => p.set_index_buffer(self.istream.buffer.slice(off..), f),
                                    IbRef::Buffer(id, f) => p.set_index_buffer(self.buffers[&id].buffer.slice(..), f),
                                }
                                bound.ib = Some(ib);
                            }
                            p.draw_indexed(d.first..d.first + d.count, d.base_vertex, 0..1);
                        }
                        None => p.draw(d.first..d.first + d.count, 0..1),
                    }
                }
            }
        }
        drop(pass.take());
        self.cmds = cmds;

        // After the scene: downsample to the internal size, post chain, output.
        let internal = self.internal_size(size);
        let internal = (internal.0.max(1), internal.1.max(1));
        for w in self.work.iter_mut() {
            if w.as_ref().is_none_or(|t| (t.width(), t.height()) != internal) {
                *w = Some(render_texture(gpu, "post", internal, TARGET_FORMAT));
            }
        }
        let scene_color = self.scene.as_ref().unwrap().color.clone();
        let mut image = scene_color;
        let mut slot = 0;
        if scene_size != internal {
            let dst = self.work[slot].clone().unwrap();
            self.blitter.blit(gpu, enc, &image, true, &dst.create_view(&Default::default()), TARGET_FORMAT, None, [0.0, 0.0, 1.0, 1.0], None, None);
            image = dst;
            slot ^= 1;
        }
        let mut post = std::mem::take(&mut self.post);
        for p in post.iter_mut() {
            let dst = self.work[slot].clone().unwrap();
            p.run(gpu, enc, &image, &dst.create_view(&Default::default()));
            image = dst;
            slot ^= 1;
        }
        self.post = post;
        self.final_image = Some(image.clone());
        if let Some(mut stage) = self.custom_output.take() {
            stage.run(gpu, enc, &image, output, self.output_format, size, self.frame);
            self.custom_output = Some(stage);
        } else {
            self.run_scaler(gpu, enc, &image, output, size);
        }
        self.frame += 1;
    }

    fn run_scaler(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, image: &wgpu::Texture, output: &wgpu::TextureView, size: (u32, u32)) {
        let s = self.output;
        let src = (image.width(), image.height());
        let r = output_rect(src, size, &s);
        // Clip the rectangle to the output, adjusting the sampled part of the image.
        let (x0, y0) = (r[0].max(0.0), r[1].max(0.0));
        let (x1, y1) = ((r[0] + r[2]).min(size.0 as f32), (r[1] + r[3]).min(size.1 as f32));
        let uv = [(x0 - r[0]) / r[2], (y0 - r[1]) / r[3], (x1 - x0) / r[2], (y1 - y0) / r[3]];
        let bg = wgpu::Color { r: s.background[0], g: s.background[1], b: s.background[2], a: s.background[3] };
        // A gamma of 1 is a plain copy (pow(x, 1) isn't exact on every GPU).
        let gamma = (s.gamma != 1.0).then_some(s.gamma);
        let vp = Some([x0, y0, x1 - x0, y1 - y0]);
        let mut src_tex = image.clone();
        let mut linear = s.filter != ScaleFilter::Nearest;
        if s.filter == ScaleFilter::SharpBilinear {
            let k = ((r[2] / src.0 as f32).min(r[3] / src.1 as f32)).floor().max(1.0) as u32;
            if k > 1 {
                let pre = (src.0 * k, src.1 * k);
                if self.prescale.as_ref().is_none_or(|t| (t.width(), t.height()) != pre) {
                    self.prescale = Some(render_texture(gpu, "prescale", pre, TARGET_FORMAT));
                }
                let t = self.prescale.clone().unwrap();
                self.blitter.blit(gpu, enc, image, false, &t.create_view(&Default::default()), TARGET_FORMAT, None, [0.0, 0.0, 1.0, 1.0], None, None);
                src_tex = t;
            }
            linear = true;
        }
        self.blitter.blit(gpu, enc, &src_tex, linear, output, self.output_format, vp, uv, Some(bg), gamma);
    }

    /// The last encoded frame after the post chain, before the output stage (internal
    /// resolution, TARGET_FORMAT): what screenshots and texture dumps should read.
    pub fn final_image(&self) -> Option<&wgpu::Texture> {
        self.final_image.as_ref()
    }

    /// The scene's colour texture (after MSAA resolve, before downsampling/post).
    pub fn scene_texture(&self) -> Option<&wgpu::Texture> {
        self.scene.as_ref().map(|s| &s.color)
    }

    /// Frames encoded so far.
    pub fn frame_count(&self) -> u64 {
        self.frame
    }

    /// Drops the recorded frame (keeping allocations) and frees what was destroyed during it.
    pub fn end_frame(&mut self) {
        self.cmds.clear();
        self.vdata.clear();
        self.idata.clear();
        self.draw_blocks.clear();
        self.view_blocks.clear();
        self.bone_blocks.clear();
        self.prog_blocks.values_mut().for_each(Vec::clear);
        self.retired.clear();
        self.target = Target::Scene;
        self.viewport = None;
        self.scissor = None;
        for id in std::mem::take(&mut self.dead_textures) {
            self.textures.remove(&id);
            self.bind_groups.retain(|k, _| !k.iter().any(|(t, _)| *t == Some(id)));
        }
        for id in std::mem::take(&mut self.dead_buffers) {
            if let Some(b) = self.buffers.remove(&id) {
                self.retired.push(b.buffer);
            }
        }
    }
}

fn attachment(gpu: &Gpu, label: &str, size: (u32, u32), format: wgpu::TextureFormat, samples: u32) -> wgpu::TextureView {
    gpu.device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

/// Writes a rectangle of a level: RGBA/BGRA8 (4 bytes per texel) or whole BC blocks.
pub(crate) fn write_raw(gpu: &Gpu, texture: &wgpu::Texture, format: wgpu::TextureFormat, level: u32, rect: (u32, u32, u32, u32), data: &[u8]) {
    let (x, y, w, h) = rect;
    let (bw, bh, bytes) = match format.block_copy_size(None) {
        Some(b) if format.is_compressed() => (w.div_ceil(4), h.div_ceil(4), b),
        _ => (w, h, 4),
    };
    let size = (bw * bh * bytes) as usize;
    if data.len() < size {
        return;
    }
    let (ew, eh) = if format.is_compressed() { (bw * 4, bh * 4) } else { (w, h) };
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture, mip_level: level, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
        &data[..size],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bw * bytes), rows_per_image: Some(bh) },
        wgpu::Extent3d { width: ew, height: eh, depth_or_array_layers: 1 },
    );
}

/// Builds vertex data byte by byte (keeps vertex structs free of unsafe casts).
#[derive(Clone, Debug, Default)]
pub struct VertexWriter {
    pub bytes: Vec<u8>,
}

impl VertexWriter {
    pub fn new() -> VertexWriter {
        VertexWriter::default()
    }
    pub fn f32s(&mut self, v: &[f32]) -> &mut Self {
        self.bytes.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        self
    }
    pub fn u8x4(&mut self, v: [u8; 4]) -> &mut Self {
        self.bytes.extend(v);
        self
    }
    pub fn u16s(&mut self, v: &[u16]) -> &mut Self {
        self.bytes.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        self
    }
}

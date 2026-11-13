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
use crate::layout::VertexLayout;
use crate::post::{Blitter, OutputSettings, PostPass, ScaleFilter, TARGET_FORMAT, output_rect, render_texture};
use crate::shader::{self, BONES_BLOCK, DRAW_BLOCK, MAX_BONES, VIEW_BLOCK};
use crate::state::*;
use crate::texture::{Palette, TexFormat, full_chain, mip_chain, to_rgba8};
use crate::fxhash::HashMap;
use crate::slab::{Slab, SlabKey};

/// Depth/stencil format of every target.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(pub u32);

impl SlabKey for TextureId {
    #[inline]
    fn index(&self) -> usize {
        self.0 as usize
    }
}

impl SlabKey for BufferId {
    #[inline]
    fn index(&self) -> usize {
        self.0 as usize
    }
}
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
    /// Texture stages the program samples (1..=4; bindings 0..2 * stages in group 2). Fewer
    /// stages make texture changes cheaper.
    pub stages: u32,
    /// Size in bytes (a multiple of 16) of a second per-draw block, the *instance block*, for
    /// data that changes with every draw (a model's matrix and lights) while the first block
    /// stays the same; 0 = none. It is declared with `Renderer::instance_block_decl` and read as
    /// `name[instance_index]`: where the device has storage buffers in vertex shaders, all of a
    /// frame's instance blocks sit in one buffer bound once and the draw's instance index picks
    /// one; otherwise it is a uniform block at a per-draw offset and the index is always 0.
    /// Draw with `draw_program_instance`.
    pub instance_block_size: u32,
}

struct Program {
    module: wgpu::ShaderModule,
    vertex_entry: String,
    fragment_entry: String,
    layout: LayoutId,
    block_size: u32,
    stages: u32,
    instance_block_size: u32,
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

/// Number of stages bound (a program's `stages`, 4 for fixed-function draws) and the stages
/// (unused ones are (None, POINT_CLAMP)).
type BindKey = (u8, [(Option<TextureId>, Sampler); STAGES]);

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

/// A recorded draw: indices into the renderer's tables and the frame's pools (resolved when
/// the draw is recorded, so replaying compares and binds by index).
#[derive(Clone, Copy, Debug)]
struct DrawOp {
    pipe: u32,
    bind: u32,
    view: u32,
    /// Draw block: the fixed-function block, or (with `prog` != 0) one of a program's blocks.
    block: u32,
    /// Block size of the program pool the block is in; 0 = fixed-function.
    prog: u32,
    /// Instance block: block size (0 = none) and index.
    inst_size: u32,
    inst: u32,
    /// Bone block; u32::MAX = none.
    bones: u32,
    vb: VbRef,
    ib: Option<IbRef>,
    first: u32,
    count: u32,
    base_vertex: i32,
}

#[derive(Clone, Copy, Debug)]
enum Cmd {
    /// End of the scene: what follows for `Target::Scene` is drawn at output resolution on top
    /// of the post-processed, scaled image.
    Overlay,
    Target(Target),
    Clear { color: Option<[f64; 4]>, depth: Option<f32>, stencil: Option<u32> },
    Draw(DrawOp),
    Copy { src: Target, rect: Option<[u32; 4]>, dst: TextureId, at: [u32; 2], scale_to: Option<[u32; 2]> },
    // Dynamic state for the following draws (recorded when it changes).
    Viewport(Option<[f32; 6]>),
    Scissor(Option<[u32; 4]>),
    StencilRef(u8),
    BlendConstant([f32; 4]),
}

/// Dynamic state already recorded for the draws that follow (None = not yet).
#[derive(Default)]
struct Sent {
    viewport: Option<Option<[f32; 6]>>,
    scissor: Option<Option<[u32; 4]>>,
    stencil: Option<u8>,
    blend: Option<[f32; 4]>,
}

/// What a draw needs besides its blocks (the public draw calls reduce to this).
struct Rec<'a> {
    layout: LayoutId,
    program: Option<ProgramId>,
    nstages: usize,
    topology: Topology,
    vertices: Vertices<'a>,
    indices: Indices<'a>,
    range: Option<(u32, u32)>,
    base_vertex: i32,
    fixed: crate::state::FixedKey,
    stencil_ref: u8,
    blend_constant: [f32; 4],
    stages: [(Option<TextureId>, Sampler); STAGES],
    bones: Option<&'a [Mat4]>,
    block: u32,
    prog: u32,
    inst: Option<(u32, u32)>,
}

/// A draw with a program, without the fixed-function state a program ignores.
#[derive(Clone, Copy, Debug)]
pub struct ProgramDraw<'a> {
    pub program: ProgramId,
    pub topology: Topology,
    pub vertices: Vertices<'a>,
    pub indices: Indices<'a>,
    /// (first, count) of the indices (indexed) or vertices; None = all of them.
    pub range: Option<(u32, u32)>,
    pub base_vertex: i32,
    pub pipeline: &'a PipelineState,
    pub stencil_reference: u8,
    pub blend_constant: [f32; 4],
    /// The program's texture stages, in order (missing ones are white).
    pub textures: &'a [(Option<TextureId>, Sampler)],
    pub bones: Option<&'a [Mat4]>,
    /// The per-draw block in consecutive parts.
    pub block: &'a [&'a [u8]],
    /// The instance block, for programs that have one.
    pub instance: Option<&'a [u8]>,
}

/// One frame's program blocks of one size, laid out at the GPU stride, ready to upload.
#[derive(Default)]
struct ProgPool {
    bytes: Vec<u8>,
    count: u32,
    /// Offset of the last block (for deduplicating consecutive identical blocks).
    last: Option<usize>,
}

impl ProgPool {
    /// Adds a block of `size` bytes given in consecutive `parts` (the rest is zero) at `stride`;
    /// returns its index. Consecutive identical blocks share one (one model's meshes).
    fn push(&mut self, parts: &[&[u8]], size: usize, stride: usize) -> u32 {
        let same = self.last.is_some_and(|at| {
            let last = &self.bytes[at..at + size];
            let mut o = 0;
            for part in parts {
                let n = part.len().min(size - o);
                if last[o..o + n] != part[..n] {
                    return false;
                }
                o += n;
            }
            last[o..].iter().all(|&b| b == 0)
        });
        if !same {
            let at = self.bytes.len();
            self.bytes.resize(at + stride, 0);
            let mut o = at;
            for part in parts {
                let n = part.len().min(at + size - o);
                self.bytes[o..o + n].copy_from_slice(&part[..n]);
                o += n;
            }
            self.last = Some(at);
            self.count += 1;
        }
        self.count - 1
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.count = 0;
        self.last = None;
    }
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

    /// Uploads `count` blocks already laid out at this buffer's stride.
    fn write_bytes(&mut self, gpu: &Gpu, layout: &wgpu::BindGroupLayout, bytes: &[u8], count: usize) {
        if count > self.capacity {
            *self = Blocks::new(gpu, layout, self.size as usize, count.next_power_of_two());
        }
        if !bytes.is_empty() {
            gpu.queue.write_buffer(&self.buffer, 0, bytes);
        }
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

/// Where `Target::Scene` draws go during a replay: the scene, or the overlay image.
struct SceneAtt<'a> {
    color: &'a wgpu::Texture,
    view: &'a wgpu::TextureView,
    msaa: Option<&'a wgpu::TextureView>,
    depth: &'a wgpu::TextureView,
    size: (u32, u32),
}

/// Output-size image the overlay is drawn on (with its depth/stencil).
struct OverlayTarget {
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    size: (u32, u32),
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
pub trait OutputStage: Send {
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
    /// In a RefCell: replaying the commands (`&self`) blits filtered frame copies.
    blitter: std::cell::RefCell<Blitter>,
    view_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    /// Stage bind group layouts for 1..=4 stages, and pipeline layouts (plain, skinned) for each.
    tex_layouts: Vec<wgpu::BindGroupLayout>,
    stage_pls: Vec<[wgpu::PipelineLayout; 2]>,
    bones_layout: wgpu::BindGroupLayout,
    layouts: Vec<(VertexLayout, wgpu::ShaderModule)>,
    programs: Vec<Program>,
    /// Program draw blocks of the frame and their GPU buffers, by block size.
    prog_blocks: HashMap<u32, ProgPool>,
    /// Uniform buffer offset alignment (block strides are multiples of it).
    ub_align: u64,
    /// Instance blocks live in a storage buffer indexed by instance (else uniform blocks).
    inst_storage: bool,
    inst_layout: wgpu::BindGroupLayout,
    /// Pipeline layouts of programs with an instance block: per stage count, plain and skinned.
    inst_pls: Vec<[wgpu::PipelineLayout; 2]>,
    /// The frame's instance blocks by block size, their GPU buffers, and this frame's group-1
    /// bind groups by (block size, instance block size).
    inst_blocks: HashMap<u32, ProgPool>,
    inst_bufs: HashMap<u32, wgpu::Buffer>,
    inst_binds: HashMap<(u32, u32), wgpu::BindGroup>,
    prog_ub: HashMap<u32, Blocks>,
    /// Pipelines and stage bind groups by index (what draws record), found by key.
    pipelines: Vec<wgpu::RenderPipeline>,
    pipe_index: HashMap<PipeKey, u32>,
    last_pipe: Option<(PipeKey, u32)>,
    binds: Vec<Option<wgpu::BindGroup>>,
    bind_index: HashMap<BindKey, u32>,
    last_bind: Option<(BindKey, u32)>,
    /// One-stage bind groups by texture id: (sampler, index) of the last one used with it, so
    /// the common texture change is an array lookup instead of a hash.
    bind1: Vec<Option<(Sampler, u32)>>,
    free_binds: Vec<u32>,
    /// Bind groups of changed textures: recorded draws may still use them until the frame ends.
    retired_binds: Vec<u32>,
    samplers: HashMap<Sampler, wgpu::Sampler>,
    textures: Slab<TextureId, Tex>,
    next_texture: u32,
    white: wgpu::TextureView,
    buffers: Slab<BufferId, Buf>,
    next_buffer: u32,
    dead_textures: Vec<TextureId>,
    dead_buffers: Vec<BufferId>,
    retired: Vec<wgpu::Buffer>,
    pub(crate) replacer: Option<crate::replace::Replacer>,
    // The frame being recorded.
    cmds: Vec<Cmd>,
    vdata: Vec<u8>,
    idata: Vec<u8>,
    draw_pool: ProgPool,
    view_pool: ProgPool,
    bone_pool: ProgPool,
    sent: Sent,
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
    /// The frame being recorded is past `begin_overlay`.
    overlay: bool,
    overlay_target: Option<OverlayTarget>,
    frame: u64,
    warned: std::collections::HashSet<&'static str>,
}

fn uniform_entry(visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding: 0, visibility, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None }, count: None }
}

#[cfg(not(target_endian = "little"))]
compile_error!("retro-wgpu uploads uniform words as they are in memory: little-endian targets only");

/// Uniform words as bytes.
fn words(w: &[u32]) -> &[u8] {
    // SAFETY: u32 is plain data; any alignment suits u8.
    unsafe { std::slice::from_raw_parts(w.as_ptr().cast::<u8>(), std::mem::size_of_val(w)) }
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
        let mut tex_layouts = Vec::new();
        let mut entries = Vec::new();
        for s in 0..STAGES as u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2 * s,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry { binding: 2 * s + 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None });
            tex_layouts.push(d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("stages"), entries: &entries }));
        }
        let inst_storage = d.limits().max_storage_buffers_per_shader_stage >= 1 && gpu.adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE);
        let inst_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw + instance"),
            entries: &[
                uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: if inst_storage {
                        wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }
                    } else {
                        wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None }
                    },
                    count: None,
                },
            ],
        });
        let inst_pls: Vec<[wgpu::PipelineLayout; 2]> = tex_layouts
            .iter()
            .map(|tl| {
                [
                    d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("draw inst"), bind_group_layouts: &[Some(&view_layout), Some(&inst_layout), Some(tl)], immediate_size: 0 }),
                    d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("skin inst"), bind_group_layouts: &[Some(&view_layout), Some(&inst_layout), Some(tl), Some(&bones_layout)], immediate_size: 0 }),
                ]
            })
            .collect();
        let stage_pls: Vec<[wgpu::PipelineLayout; 2]> = tex_layouts
            .iter()
            .map(|tl| {
                [
                    d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("draw"), bind_group_layouts: &[Some(&view_layout), Some(&draw_layout), Some(tl)], immediate_size: 0 }),
                    d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("skin"), bind_group_layouts: &[Some(&view_layout), Some(&draw_layout), Some(tl), Some(&bones_layout)], immediate_size: 0 }),
                ]
            })
            .collect();
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
            blitter: std::cell::RefCell::new(Blitter::new(gpu)),
            view_ub: Blocks::new(gpu, &view_layout, VIEW_BLOCK, 16),
            draw_ub: Blocks::new(gpu, &draw_layout, DRAW_BLOCK, 256),
            bones_ub: Blocks::new(gpu, &bones_layout, BONES_BLOCK, 8),
            view_layout,
            draw_layout,
            tex_layouts,
            stage_pls,
            bones_layout,
            layouts: Vec::new(),
            programs: Vec::new(),
            prog_blocks: HashMap::default(),
            ub_align: gpu.device.limits().min_uniform_buffer_offset_alignment as u64,
            inst_storage,
            inst_layout,
            inst_pls,
            inst_blocks: HashMap::default(),
            inst_bufs: HashMap::default(),
            inst_binds: HashMap::default(),
            prog_ub: HashMap::default(),
            pipelines: Vec::new(),
            pipe_index: HashMap::default(),
            last_pipe: None,
            binds: Vec::new(),
            bind_index: HashMap::default(),
            last_bind: None,
            bind1: Vec::new(),
            free_binds: Vec::new(),
            retired_binds: Vec::new(),
            samplers: HashMap::default(),
            textures: Slab::new(),
            next_texture: 1,
            white: white_tex.create_view(&Default::default()),
            buffers: Slab::new(),
            next_buffer: 1,
            dead_textures: Vec::new(),
            dead_buffers: Vec::new(),
            retired: Vec::new(),
            replacer: None,
            cmds: Vec::new(),
            vdata: Vec::new(),
            idata: Vec::new(),
            draw_pool: ProgPool::default(),
            view_pool: ProgPool::default(),
            bone_pool: ProgPool::default(),
            sent: Sent::default(),
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
            overlay: false,
            overlay_target: None,
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
        if desc.block_size == 0 || !desc.block_size.is_multiple_of(16) {
            return Err(format!("program {}: block size {} is not a positive multiple of 16", desc.label, desc.block_size));
        }
        if desc.block_size > gpu.device.limits().max_uniform_buffer_binding_size as u32 {
            return Err(format!("program {}: block size {} over the device's uniform limit", desc.label, desc.block_size));
        }
        if !desc.instance_block_size.is_multiple_of(16) || desc.instance_block_size > gpu.device.limits().max_uniform_buffer_binding_size as u32 {
            return Err(format!("program {}: instance block size {} is not a multiple of 16 or too large", desc.label, desc.instance_block_size));
        }
        if !(1..=STAGES as u32).contains(&desc.stages) {
            return Err(format!("program {}: {} stages (1..={STAGES})", desc.label, desc.stages));
        }
        let layout = self.create_layout(gpu, desc.layout.clone())?;
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(desc.label), source: wgpu::ShaderSource::Wgsl(desc.wgsl.into()) });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("program {}: {e}", desc.label));
        }
        self.programs.push(Program { module, vertex_entry: desc.vertex_entry.into(), fragment_entry: desc.fragment_entry.into(), layout, block_size: desc.block_size, stages: desc.stages, instance_block_size: desc.instance_block_size });
        Ok(ProgramId(self.programs.len() as u32 - 1))
    }

    /// Whether instance blocks are indexed in a storage buffer (else bound per draw).
    pub fn instance_storage(&self) -> bool {
        self.inst_storage
    }

    /// The WGSL declaration of a program's instance block: `name[i]` with `i` the vertex
    /// shader's `@builtin(instance_index)` is the draw's block of struct type `ty`, in either mode
    /// (pass the index to the fragment stage as a flat varying to read it there).
    pub fn instance_block_decl(&self, name: &str, ty: &str) -> String {
        if self.inst_storage {
            format!("@group(1) @binding(1) var<storage, read> {name}: array<{ty}>;")
        } else {
            format!("@group(1) @binding(1) var<uniform> {name}: array<{ty}, 1>;")
        }
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
            self.drop_binds_of(id);
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
        self.drop_binds_of(id);
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

    /// Ends the scene of this frame: following `Target::Scene` clears, draws and copies go to an
    /// output-resolution image on top of the scene after its downsampling, post chain and output
    /// scaling (a HUD drawn sharp at window resolution whatever the internal resolution,
    /// supersampling or post AA). Its depth starts cleared; draws are single-sampled. The output
    /// curve (gamma) is applied after the overlay. Once per frame.
    pub fn begin_overlay(&mut self) {
        if !self.overlay {
            self.cmds.push(Cmd::Overlay);
            self.overlay = true;
            // The overlay is replayed on its own: its dynamic state is recorded afresh.
            self.sent = Sent::default();
            if self.target == Target::Scene {
                self.viewport = None;
                self.scissor = None;
            }
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
        self.view_pool.push(&[words(&b)], VIEW_BLOCK, self.view_ub.stride as usize);
    }

    /// Copies (a rectangle of) `src` into `dst` at `at` when the frame is drawn, e.g. the frame
    /// so far into a texture for feedback effects. `rect` in source pixels (x, y, w, h); None =
    /// all of it. Same-format copies are exact; otherwise the copy is filtered.
    pub fn copy_to_texture(&mut self, src: Target, rect: Option<[u32; 4]>, dst: TextureId, at: [u32; 2]) {
        self.cmds.push(Cmd::Copy { src, rect, dst, at, scale_to: None });
    }

    /// Copies all of `src`, filtered (bilinear) to `size` pixels, into `dst` at `at` when the
    /// frame is drawn: e.g. a supersampled or scaled frame into a window-size texture.
    pub fn copy_to_texture_scaled(&mut self, src: Target, dst: TextureId, at: [u32; 2], size: [u32; 2]) {
        self.cmds.push(Cmd::Copy { src, rect: None, dst, at, scale_to: Some(size) });
    }

    /// Records a draw.
    pub fn draw(&mut self, gpu: &Gpu, d: &Draw) {
        let block = shader::draw_block(d.state, d.params);
        let block = self.draw_pool.push(&[words(&block)], DRAW_BLOCK, self.draw_ub.stride as usize);
        let stages = std::array::from_fn(|i| (d.state.stages[i].texture, d.state.stages[i].sampler));
        self.record(
            gpu,
            &Rec {
                layout: d.layout,
                program: None,
                nstages: STAGES,
                topology: d.topology,
                vertices: d.vertices,
                indices: d.indices,
                range: d.range,
                base_vertex: d.base_vertex,
                fixed: d.state.fixed_key(),
                stencil_ref: d.state.stencil_reference,
                blend_constant: d.state.blend_constant,
                stages,
                bones: d.bones,
                block,
                prog: 0,
                inst: None,
            },
        );
    }

    /// Records a draw with a program (its layout replaces `d.layout`; `d.params` is unused) and
    /// the program's per-draw block (`block`, at most its block size; the rest is zero).
    pub fn draw_program(&mut self, gpu: &Gpu, d: &Draw, program: ProgramId, block: &[u8]) {
        self.draw_program_parts(gpu, d, program, &[block]);
    }

    /// As `draw_program`, with the block given in consecutive parts (no copy to join them).
    pub fn draw_program_parts(&mut self, gpu: &Gpu, d: &Draw, program: ProgramId, parts: &[&[u8]]) {
        self.draw_program_state(gpu, d, program, parts, None);
    }

    /// Records a draw with a program that has an instance block: the per-draw block in `parts`
    /// (as `draw_program_parts`) and the draw's instance block.
    pub fn draw_program_instance(&mut self, gpu: &Gpu, d: &Draw, program: ProgramId, parts: &[&[u8]], instance: &[u8]) {
        self.draw_program_state(gpu, d, program, parts, Some(instance));
    }

    fn draw_program_state(&mut self, gpu: &Gpu, d: &Draw, program: ProgramId, parts: &[&[u8]], instance: Option<&[u8]>) {
        let textures: [(Option<TextureId>, Sampler); STAGES] = std::array::from_fn(|i| (d.state.stages[i].texture, d.state.stages[i].sampler));
        self.draw_with_program(
            gpu,
            &ProgramDraw {
                program,
                topology: d.topology,
                vertices: d.vertices,
                indices: d.indices,
                range: d.range,
                base_vertex: d.base_vertex,
                pipeline: &d.state.pipeline(),
                stencil_reference: d.state.stencil_reference,
                blend_constant: d.state.blend_constant,
                textures: &textures,
                bones: d.bones,
                block: parts,
                instance,
            },
        );
    }

    /// Records a program draw (the lean call: no fixed-function state is built or hashed).
    pub fn draw_with_program(&mut self, gpu: &Gpu, d: &ProgramDraw) {
        let Some(p) = self.programs.get(d.program.0 as usize) else {
            self.warn("draw with an unknown program skipped");
            return;
        };
        let (layout, size, isize, nstages) = (p.layout, p.block_size, p.instance_block_size, p.stages as usize);
        if (isize != 0) != d.instance.is_some() {
            self.warn("program draw skipped: an instance block must be given exactly for programs that have one");
            return;
        }
        let stride = (size as u64).next_multiple_of(self.ub_align) as usize;
        let block = self.prog_blocks.entry(size).or_default().push(d.block, size as usize, stride);
        let inst = d.instance.map(|data| {
            let istride = if self.inst_storage { isize as usize } else { (isize as u64).next_multiple_of(self.ub_align) as usize };
            (isize, self.inst_blocks.entry(isize).or_default().push(&[data], isize as usize, istride))
        });
        let stages = std::array::from_fn(|i| d.textures.get(i).copied().unwrap_or((None, Sampler::POINT_CLAMP)));
        self.record(
            gpu,
            &Rec {
                layout,
                program: Some(d.program),
                nstages,
                topology: d.topology,
                vertices: d.vertices,
                indices: d.indices,
                range: d.range,
                base_vertex: d.base_vertex,
                fixed: d.pipeline.fixed_key(),
                stencil_ref: d.stencil_reference,
                blend_constant: d.blend_constant,
                stages,
                bones: d.bones,
                block,
                prog: size,
                inst,
            },
        );
    }

    fn record(&mut self, gpu: &Gpu, d: &Rec) {
        let Some((layout, _)) = self.layouts.get(d.layout.0 as usize) else {
            self.warn("draw with an unknown layout skipped");
            return;
        };
        let stride = layout.stride as u64;
        let skinned = layout.skinned();
        // Vertices.
        let (vb, vertex_count) = match d.vertices {
            Vertices::Slice(bytes) => {
                let at = self.vdata.len().next_multiple_of(stride.max(4) as usize);
                self.vdata.resize(at, 0);
                self.vdata.extend_from_slice(bytes);
                let pad = self.vdata.len().next_multiple_of(4);
                self.vdata.resize(pad, 0);
                (VbRef::Stream(at as u64), (bytes.len() as u64 / stride.max(1)) as u32)
            }
            Vertices::Buffer(id, off) => {
                let Some(b) = self.buffers.get(&id) else { return };
                (VbRef::Buffer(id, off), (b.size.saturating_sub(off) / stride.max(1)) as u32)
            }
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
            Target::Scene if self.overlay => 1,
            Target::Scene => self.samples,
            Target::Texture(t) => match self.textures.get(&t).and_then(|t| t.rt.as_ref()) {
                Some(rt) => rt.samples,
                None => {
                    self.warn("draw into a texture that isn't a render target skipped");
                    return;
                }
            },
        };
        let mut fixed = d.fixed;
        if !matches!(topology, wgpu::PrimitiveTopology::TriangleList | wgpu::PrimitiveTopology::TriangleStrip) {
            fixed.bias = (0, 0, 0);
        }
        // Pipeline: most draws use the previous one.
        let key = PipeKey { layout: d.layout, program: d.program, fixed, topology, strip_index, samples };
        let pipe = match self.last_pipe {
            Some((k, i)) if k == key => i,
            _ => {
                let i = match self.pipe_index.get(&key) {
                    Some(i) => *i,
                    None => {
                        let p = self.make_pipeline(gpu, key);
                        self.pipelines.push(p);
                        let i = self.pipelines.len() as u32 - 1;
                        self.pipe_index.insert(key, i);
                        i
                    }
                };
                self.last_pipe = Some((key, i));
                i
            }
        };
        // Textures: the current target can't be sampled while drawn into (white instead).
        let nstages = d.nstages;
        let binds: BindKey = (nstages as u8, std::array::from_fn(|s| {
            if s >= nstages {
                return (None, Sampler::POINT_CLAMP);
            }
            let (texture, smp) = d.stages[s];
            let tex = texture.filter(|t| self.textures.contains_key(t) && Target::Texture(*t) != self.target);
            let sampler = match (tex, &self.replacer) {
                (Some(t), Some(r)) if r.replaced.contains(&t) => crate::replace::upgrade_sampler(&r.config, smp),
                (Some(_), _) => smp,
                (None, _) => Sampler::POINT_CLAMP,
            };
            (tex, sampler)
        }));
        let bind = match self.last_bind {
            Some((k, i)) if k == binds => i,
            _ => {
                let one = match (nstages, binds.1[0]) {
                    (1, (Some(t), smp)) => Some((t.0 as usize, smp)),
                    _ => None,
                };
                let cached = one.and_then(|(t, smp)| self.bind1.get(t).copied().flatten().filter(|c| c.0 == smp).map(|c| c.1));
                let i = match cached.or_else(|| self.bind_index.get(&binds).copied()) {
                    Some(i) => i,
                    None => {
                        let bg = self.make_bind_group(gpu, binds);
                        let i = match self.free_binds.pop() {
                            Some(i) => {
                                self.binds[i as usize] = Some(bg);
                                i
                            }
                            None => {
                                self.binds.push(Some(bg));
                                self.binds.len() as u32 - 1
                            }
                        };
                        self.bind_index.insert(binds, i);
                        i
                    }
                };
                if let Some((t, smp)) = one {
                    if t >= self.bind1.len() {
                        self.bind1.resize(t + 1, None);
                    }
                    self.bind1[t] = Some((smp, i));
                }
                self.last_bind = Some((binds, i));
                i
            }
        };
        if self.view_pool.count == 0 {
            let b = shader::view_block(&View::screen(640.0, 480.0));
            self.view_pool.push(&[words(&b)], VIEW_BLOCK, self.view_ub.stride as usize);
        }
        let bones = if skinned {
            let mut m = [IDENTITY; MAX_BONES];
            if let Some(bs) = d.bones {
                let n = bs.len().min(MAX_BONES);
                m[..n].copy_from_slice(&bs[..n]);
            }
            // SAFETY: f32 matrices as bytes (plain data).
            let bytes = unsafe { std::slice::from_raw_parts(m.as_ptr().cast::<u8>(), BONES_BLOCK) };
            self.bone_pool.push(&[bytes], BONES_BLOCK, self.bones_ub.stride as usize)
        } else {
            u32::MAX
        };
        // Dynamic state, when it changed.
        if self.sent.viewport != Some(self.viewport) {
            self.cmds.push(Cmd::Viewport(self.viewport));
            self.sent.viewport = Some(self.viewport);
        }
        if self.sent.scissor != Some(self.scissor) {
            self.cmds.push(Cmd::Scissor(self.scissor));
            self.sent.scissor = Some(self.scissor);
        }
        if self.sent.stencil != Some(d.stencil_ref) {
            self.cmds.push(Cmd::StencilRef(d.stencil_ref));
            self.sent.stencil = Some(d.stencil_ref);
        }
        if self.sent.blend != Some(d.blend_constant) {
            self.cmds.push(Cmd::BlendConstant(d.blend_constant));
            self.sent.blend = Some(d.blend_constant);
        }
        self.cmds.push(Cmd::Draw(DrawOp {
            pipe,
            bind,
            view: self.view_pool.count - 1,
            block: d.block,
            prog: d.prog,
            inst_size: d.inst.map_or(0, |i| i.0),
            inst: d.inst.map_or(0, |i| i.1),
            bones,
            vb,
            ib,
            first,
            count,
            base_vertex: d.base_vertex,
        }));
    }

    /// Forgets the bind groups that use texture `id` (it changed or is gone). Draws already
    /// recorded keep theirs until the frame ends.
    fn drop_binds_of(&mut self, id: TextureId) {
        let retired = &mut self.retired_binds;
        self.bind_index.retain(|k, i| {
            let uses = k.1.iter().any(|(t, _)| *t == Some(id));
            if uses {
                retired.push(*i);
            }
            !uses
        });
        self.last_bind = None;
        if let Some(c) = self.bind1.get_mut(id.0 as usize) {
            *c = None;
        }
    }

    // ---- Encoding -------------------------------------------------------------------------

    fn make_pipeline(&self, gpu: &Gpu, key: PipeKey) -> wgpu::RenderPipeline {
        let (layout, core) = &self.layouts[key.layout.0 as usize];
        let nstages = key.program.and_then(|p| self.programs.get(p.0 as usize)).map_or(STAGES, |p| p.stages as usize);
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
        gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("retro draw"),
            layout: Some(if key.program.and_then(|p| self.programs.get(p.0 as usize)).is_some_and(|p| p.instance_block_size != 0) {
                &self.inst_pls[nstages - 1][layout.skinned() as usize]
            } else {
                &self.stage_pls[nstages - 1][layout.skinned() as usize]
            }),
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
        })
    }

    fn make_bind_group(&mut self, gpu: &Gpu, key: BindKey) -> wgpu::BindGroup {
        let n = key.0 as usize;
        for (_, s) in &key.1[..n] {
            self.samplers.entry(*s).or_insert_with(|| gpu.device.create_sampler(&wgpu_sampler(s)));
        }
        let mut entries = Vec::new();
        for (i, (t, s)) in key.1[..n].iter().enumerate() {
            let view = t.and_then(|t| self.textures.get(&t)).map_or(&self.white, |t| &t.view);
            entries.push(wgpu::BindGroupEntry { binding: 2 * i as u32, resource: wgpu::BindingResource::TextureView(view) });
            entries.push(wgpu::BindGroupEntry { binding: 2 * i as u32 + 1, resource: wgpu::BindingResource::Sampler(&self.samplers[s]) });
        }
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("stages"), layout: &self.tex_layouts[n - 1], entries: &entries })
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
        self.view_ub.write_bytes(gpu, &self.view_layout, &self.view_pool.bytes, self.view_pool.count as usize);
        self.draw_ub.write_bytes(gpu, &self.draw_layout, &self.draw_pool.bytes, self.draw_pool.count as usize);
        self.bones_ub.write_bytes(gpu, &self.bones_layout, &self.bone_pool.bytes, self.bone_pool.count as usize);
        for (size, pool) in &self.prog_blocks {
            let ub = self.prog_ub.entry(*size).or_insert_with(|| Blocks::new(gpu, &self.draw_layout, *size as usize, 64));
            debug_assert_eq!(ub.stride, (*size as u64).next_multiple_of(self.ub_align));
            ub.write_bytes(gpu, &self.draw_layout, &pool.bytes, pool.count as usize);
        }
        let cmds = std::mem::take(&mut self.cmds);
        // Instance blocks: one buffer per block size, and this frame's group-1 bind groups.
        for (isize, pool) in &self.inst_blocks {
            let need = (pool.bytes.len() as u64).max(*isize as u64);
            if self.inst_bufs.get(isize).is_none_or(|b| b.size() < need) {
                let usage = if self.inst_storage { wgpu::BufferUsages::STORAGE } else { wgpu::BufferUsages::UNIFORM } | wgpu::BufferUsages::COPY_DST;
                self.inst_bufs.insert(*isize, gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("instance blocks"), size: need.next_power_of_two().max(1 << 16), usage, mapped_at_creation: false }));
            }
            if !pool.bytes.is_empty() {
                gpu.queue.write_buffer(&self.inst_bufs[isize], 0, &pool.bytes);
            }
        }
        self.inst_binds.clear();
        for c in &cmds {
            if let Cmd::Draw(DrawOp { prog: size, inst_size: isize, .. }) = c
                && *isize != 0
                && !self.inst_binds.contains_key(&(*size, *isize))
            {
                let inst = &self.inst_bufs[isize];
                let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("draw + instance"),
                    layout: &self.inst_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.prog_ub[size].buffer, offset: 0, size: wgpu::BufferSize::new(*size as u64) }) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: inst, offset: 0, size: if self.inst_storage { None } else { wgpu::BufferSize::new(*isize as u64) } }) },
                    ],
                });
                self.inst_binds.insert((*size, *isize), bg);
            }
        }
        // The scene's commands, then (after a `begin_overlay`) the overlay's.
        let split = cmds.iter().position(|c| matches!(c, Cmd::Overlay));
        let (scene_cmds, overlay_cmds) = match split {
            Some(i) => (&cmds[..i], Some(&cmds[i + 1..])),
            None => (&cmds[..], None),
        };
        {
            let sc = self.scene.as_ref().unwrap();
            self.replay(gpu, enc, scene_cmds, SceneAtt { color: &sc.color, view: &sc.view, msaa: sc.msaa.as_ref(), depth: &sc.depth, size: sc.size });
        }

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
            self.blitter.borrow_mut().blit(gpu, enc, &image, true, &dst.create_view(&Default::default()), TARGET_FORMAT, None, [0.0, 0.0, 1.0, 1.0], None, None);
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
        match overlay_cmds {
            None => {
                self.final_image = Some(image.clone());
                if let Some(mut stage) = self.custom_output.take() {
                    stage.run(gpu, enc, &image, output, self.output_format, size, self.frame);
                    self.custom_output = Some(stage);
                } else {
                    self.run_scaler(gpu, enc, &image, output, self.output_format, size, true);
                }
            }
            Some(ov) => {
                // The game image scaled to the output into an output-size image, the overlay
                // drawn on it, then the output curve on the way out.
                let size = (size.0.max(1), size.1.max(1));
                if self.overlay_target.as_ref().is_none_or(|o| o.size != size) {
                    let color = render_texture(gpu, "overlay", size, TARGET_FORMAT);
                    let view = color.create_view(&Default::default());
                    let depth = attachment(gpu, "overlay depth", size, DEPTH_FORMAT, 1);
                    self.overlay_target = Some(OverlayTarget { color, view, depth, size });
                }
                let o = self.overlay_target.as_ref().unwrap();
                let (oc, ovw, od) = (o.color.clone(), o.view.clone(), o.depth.clone());
                if let Some(mut stage) = self.custom_output.take() {
                    stage.run(gpu, enc, &image, &ovw, TARGET_FORMAT, size, self.frame);
                    self.custom_output = Some(stage);
                } else {
                    self.run_scaler(gpu, enc, &image, &ovw, TARGET_FORMAT, size, false);
                }
                // The overlay's depth/stencil starts cleared.
                drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("overlay depth clear"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &od,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Store }),
                    }),
                    ..Default::default()
                }));
                self.replay(gpu, enc, ov, SceneAtt { color: &oc, view: &ovw, msaa: None, depth: &od, size });
                let gamma = (self.output.gamma != 1.0).then_some(self.output.gamma);
                self.blitter.borrow_mut().blit(gpu, enc, &oc, false, output, self.output_format, None, [0.0, 0.0, 1.0, 1.0], None, gamma);
                self.final_image = Some(oc);
            }
        }
        self.cmds = cmds;
        self.frame += 1;
    }

    /// Replays recorded commands, with `Target::Scene` drawing into `scene`.
    fn replay(&self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, cmds: &[Cmd], scene: SceneAtt) {
        let mut pass: Option<wgpu::RenderPass<'static>> = None;
        let mut target = Target::Scene;
        let mut target_size = scene.size;
        let mut ok = true;
        // What the current pass has bound (u32::MAX = nothing), and which dynamic state it has.
        struct Bound {
            pipe: u32,
            view: u32,
            block: Option<(u32, u32, u32, u32)>,
            bind: u32,
            bones: u32,
            vb: Option<VbRef>,
            ib: Option<IbRef>,
            viewport: bool,
            viewport_empty: bool,
            scissor: bool,
            stencil: bool,
            blend: bool,
        }
        // Dynamic state as recorded so far; a new pass starts with wgpu's defaults, which are
        // "whole target", no scissor, reference 0 and a zero constant.
        let mut cur_viewport: Option<[f32; 6]> = None;
        let mut cur_scissor: Option<[u32; 4]> = None;
        let mut cur_stencil = 0u8;
        let mut cur_blend = [0f32; 4];
        macro_rules! fresh {
            () => {
                Bound { pipe: u32::MAX, view: u32::MAX, block: None, bind: u32::MAX, bones: u32::MAX, vb: None, ib: None, viewport: cur_viewport.is_none(), viewport_empty: false, scissor: cur_scissor.is_none(), stencil: cur_stencil == 0, blend: cur_blend == [0.0; 4] }
            };
        }
        let mut bound = fresh!();

        let attachments = |t: Target| -> Option<(&wgpu::TextureView, Option<&wgpu::TextureView>, &wgpu::TextureView, (u32, u32))> {
            match t {
                Target::Scene => Some(match scene.msaa {
                    Some(m) => (m, Some(scene.view), scene.depth, scene.size),
                    None => (scene.view, None, scene.depth, scene.size),
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
        for c in cmds {
            match *c {
                Cmd::Overlay => {}
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
                    bound = fresh!();
                }
                Cmd::Copy { src, rect: _, dst, at, scale_to: Some(sz) } => {
                    drop(pass.take());
                    let Some(d) = self.textures.get(&dst) else { continue };
                    if d.format.is_compressed() || attachments(src).is_none() {
                        continue;
                    }
                    let src_tex = match src {
                        Target::Scene => scene.color,
                        Target::Texture(id) => &self.textures[&id].texture,
                    };
                    // Clip the destination rectangle to the texture, sampling the matching part.
                    let (dw, dh) = (d.texture.width(), d.texture.height());
                    let w = sz[0].min(dw.saturating_sub(at[0]));
                    let h = sz[1].min(dh.saturating_sub(at[1]));
                    if w == 0 || h == 0 || sz[0] == 0 || sz[1] == 0 {
                        continue;
                    }
                    let uv = [0.0, 0.0, w as f32 / sz[0] as f32, h as f32 / sz[1] as f32];
                    let dview = d.texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 0, mip_level_count: Some(1), ..Default::default() });
                    self.blitter.borrow_mut().blit(gpu, enc, src_tex, true, &dview, d.format, Some([at[0] as f32, at[1] as f32, w as f32, h as f32]), uv, None, None);
                }
                Cmd::Copy { src, rect, dst, at, scale_to: None } => {
                    drop(pass.take());
                    let (Some(s), Some(d)) = (attachments(src).map(|a| (a.1.is_some(), a.3)), self.textures.get(&dst)) else { continue };
                    let src_tex = match src {
                        Target::Scene => scene.color,
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
                        self.blitter.borrow_mut().blit(gpu, enc, src_tex, false, &dview, d.format, Some([at[0] as f32, at[1] as f32, w as f32, h as f32]), uv, None, None);
                    }
                }
                Cmd::Viewport(v) => {
                    cur_viewport = v;
                    bound.viewport = false;
                }
                Cmd::Scissor(v) => {
                    cur_scissor = v;
                    bound.scissor = false;
                }
                Cmd::StencilRef(v) => {
                    cur_stencil = v;
                    bound.stencil = false;
                }
                Cmd::BlendConstant(v) => {
                    cur_blend = v;
                    bound.blend = false;
                }
                Cmd::Draw(d) => {
                    if !ok {
                        continue;
                    }
                    if pass.is_none() {
                        pass = begin(enc, target, None, None, None);
                        bound = fresh!();
                    }
                    let Some(p) = pass.as_mut() else { continue };
                    if !bound.viewport {
                        let (tw, th) = (target_size.0 as f32, target_size.1 as f32);
                        let v = cur_viewport.unwrap_or([0.0, 0.0, tw, th, 0.0, 1.0]);
                        let x = v[0].clamp(0.0, tw);
                        let y = v[1].clamp(0.0, th);
                        let w = v[2].min(tw - x).max(0.0);
                        let h = v[3].min(th - y).max(0.0);
                        bound.viewport_empty = w <= 0.0 || h <= 0.0;
                        if !bound.viewport_empty {
                            p.set_viewport(x, y, w, h, v[4].clamp(0.0, 1.0), v[5].clamp(0.0, 1.0));
                        }
                        bound.viewport = true;
                    }
                    if bound.viewport_empty {
                        continue;
                    }
                    if !bound.scissor {
                        let sc = cur_scissor.unwrap_or([0, 0, target_size.0, target_size.1]);
                        let x = sc[0].min(target_size.0);
                        let y = sc[1].min(target_size.1);
                        p.set_scissor_rect(x, y, sc[2].min(target_size.0 - x), sc[3].min(target_size.1 - y));
                        bound.scissor = true;
                    }
                    if !bound.stencil {
                        p.set_stencil_reference(cur_stencil as u32);
                        bound.stencil = true;
                    }
                    if !bound.blend {
                        let c = cur_blend;
                        p.set_blend_constant(wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 });
                        bound.blend = true;
                    }
                    if bound.pipe != d.pipe {
                        p.set_pipeline(&self.pipelines[d.pipe as usize]);
                        bound.pipe = d.pipe;
                    }
                    if bound.view != d.view {
                        p.set_bind_group(0, &self.view_ub.bind, &[self.view_ub.offset(d.view)]);
                        bound.view = d.view;
                    }
                    // With storage, the instance block is picked by the instance index: no rebind.
                    let inst_key = if self.inst_storage { 0 } else { d.inst };
                    let block_key = (d.prog, d.block, d.inst_size, inst_key);
                    if bound.block != Some(block_key) {
                        if d.inst_size != 0 {
                            let off = self.prog_ub[&d.prog].offset(d.block);
                            let bg = &self.inst_binds[&(d.prog, d.inst_size)];
                            if self.inst_storage {
                                p.set_bind_group(1, bg, &[off]);
                            } else {
                                p.set_bind_group(1, bg, &[off, (d.inst as u64 * (d.inst_size as u64).next_multiple_of(self.ub_align)) as u32]);
                            }
                        } else {
                            let ub = if d.prog != 0 { &self.prog_ub[&d.prog] } else { &self.draw_ub };
                            p.set_bind_group(1, &ub.bind, &[ub.offset(d.block)]);
                        }
                        bound.block = Some(block_key);
                    }
                    let instances = if d.inst_size != 0 && self.inst_storage { d.inst..d.inst + 1 } else { 0..1 };
                    if bound.bind != d.bind {
                        if let Some(bg) = &self.binds[d.bind as usize] {
                            p.set_bind_group(2, bg, &[]);
                        }
                        bound.bind = d.bind;
                    }
                    if d.bones != u32::MAX && bound.bones != d.bones {
                        p.set_bind_group(3, &self.bones_ub.bind, &[self.bones_ub.offset(d.bones)]);
                        bound.bones = d.bones;
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
                            p.draw_indexed(d.first..d.first + d.count, d.base_vertex, instances);
                        }
                        None => p.draw(d.first..d.first + d.count, instances),
                    }
                }
            }
        }
        drop(pass.take());
    }

    /// Scales `image` into `output` (`format`) per the output settings; `curve`: with the gamma.
    #[allow(clippy::too_many_arguments)]
    fn run_scaler(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, image: &wgpu::Texture, output: &wgpu::TextureView, format: wgpu::TextureFormat, size: (u32, u32), curve: bool) {
        let s = self.output;
        let src = (image.width(), image.height());
        let r = output_rect(src, size, &s);
        // Clip the rectangle to the output, adjusting the sampled part of the image.
        let (x0, y0) = (r[0].max(0.0), r[1].max(0.0));
        let (x1, y1) = ((r[0] + r[2]).min(size.0 as f32), (r[1] + r[3]).min(size.1 as f32));
        let uv = [(x0 - r[0]) / r[2], (y0 - r[1]) / r[3], (x1 - x0) / r[2], (y1 - y0) / r[3]];
        let bg = wgpu::Color { r: s.background[0], g: s.background[1], b: s.background[2], a: s.background[3] };
        // A gamma of 1 is a plain copy (pow(x, 1) isn't exact on every GPU).
        let gamma = (curve && s.gamma != 1.0).then_some(s.gamma);
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
                self.blitter.borrow_mut().blit(gpu, enc, image, false, &t.create_view(&Default::default()), TARGET_FORMAT, None, [0.0, 0.0, 1.0, 1.0], None, None);
                src_tex = t;
            }
            linear = true;
        }
        self.blitter.borrow_mut().blit(gpu, enc, &src_tex, linear, output, format, vp, uv, Some(bg), gamma);
    }

    /// The last encoded frame after the post chain, before the output stage (internal
    /// resolution, TARGET_FORMAT): what screenshots and texture dumps should read. In a frame
    /// with an overlay: the output-size image with the overlay, before the output curve.
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
        self.draw_pool.clear();
        self.view_pool.clear();
        self.bone_pool.clear();
        self.sent = Sent::default();
        for i in std::mem::take(&mut self.retired_binds) {
            self.binds[i as usize] = None;
            self.free_binds.push(i);
        }
        self.prog_blocks.values_mut().for_each(ProgPool::clear);
        self.inst_blocks.values_mut().for_each(ProgPool::clear);
        self.retired.clear();
        self.target = Target::Scene;
        self.overlay = false;
        self.viewport = None;
        self.scissor = None;
        for id in std::mem::take(&mut self.dead_textures) {
            self.textures.remove(&id);
            self.drop_binds_of(id);
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

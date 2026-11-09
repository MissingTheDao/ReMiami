//! Immediate-mode drawing for librw's WGPU platform (renderer port stage 1): the C++ device
//! forwards camera clears, texture uploads and Im2D primitives here; they are recorded per frame
//! and encoded into render passes when the frame is shown.
//!
//! Semantics follow librw's GL3 device so both renderers draw the same picture:
//! - Im2D vertices are screen pixels; `xform` maps them to clip space and the position is
//!   multiplied by w (camera z) for perspective-correct interpolation (gl3 im2d.vert).
//! - fog factor `clamp((w - fogEnd) / (fogStart - fogEnd), fogDisable, 1)`, colour mixed towards
//!   the fog colour; alpha test as a [low, high) window (gl3 header.frag / flushCache).
//! - blending is on when vertex alpha or texture alpha is (decided by the caller), with the RW
//!   blend factors for both colour and alpha (glBlendFunc).
//! - a z test that is off while z writes are on becomes "always" (gl3 setDepthTest).
//! - samplers follow the texture stage's filter and addressing; non-mip filters sample level 0.
//!
//! Rasters are RGBA8, top row first, so texture coordinates are used as they are.

use crate::Gpu;
use crate::prim::{expand_primitive, generate_mips, is_line_primitive};
use std::collections::HashMap;

/// librw `wgpu::Im2DVertex` (same layout as gl3's).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex2D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub u: f32,
    pub v: f32,
}
const _: () = assert!(std::mem::size_of::<Vertex2D>() == 28);

/// Device state for one draw, filled by the C++ device (`RevcDrawState` in wgpudevice.cpp).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawState {
    /// Texture id from `upload_texture`; 0 = untextured (white).
    pub texture: u32,
    /// rw::Texture::FilterMode (1 nearest .. 6 linear-mip-linear).
    pub filter: u32,
    /// rw::Texture::Addressing (1 wrap, 2 mirror, 3 clamp, 4 border).
    pub address_u: u32,
    pub address_v: u32,
    /// Blending on (vertex alpha or texture alpha), with rw::BlendFunction factors.
    pub blend: u32,
    pub src_blend: u32,
    pub dst_blend: u32,
    pub z_test: u32,
    pub z_write: u32,
    /// rw::CullMode (1 none, 2 back, 3 front).
    pub cull: u32,
    /// rw::AlphaTestFunc as the shader sees it (0 always, 1 >= ref, 2 < ref) and ref in 0..1.
    pub alpha_func: u32,
    pub alpha_ref: f32,
    pub fog: u32,
    pub fog_start: f32,
    pub fog_end: f32,
    /// RGBA bytes, red in the low byte (RWRGBAINT).
    pub fog_color: u32,
    /// Screen pixels to clip space: x * xform[0] + xform[2], y * xform[1] + xform[3].
    pub xform: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipeKey {
    lines: bool,
    blend: Option<(u32, u32)>,
    z_test: bool,
    z_write: bool,
    cull: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SamplerKey {
    filter: u32,
    address_u: u32,
    address_v: u32,
}

type Uniforms = [f32; 16];

#[derive(Clone, Copy, Debug)]
enum Op {
    Clear { color: Option<[f64; 4]>, depth: bool },
    Draw { pipe: PipeKey, texture: u32, sampler: SamplerKey, uniform: u32, first: u32, count: u32 },
}

struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    levels: u32,
}

const SHADER: &str = r#"
struct U { xform: vec4<f32>, alpha: vec4<f32>, fog: vec4<f32>, fog_color: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fog: f32,
};

@vertex
fn vs_2d(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    let w = pos.w;
    let xy = pos.xy * u.xform.xy + u.xform.zw;
    o.pos = vec4<f32>(vec3<f32>(xy, pos.z) * w, w);
    o.fog = clamp((w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    o.color = color;
    o.uv = uv;
    return o;
}

@fragment
fn fs_simple(i: Out) -> @location(0) vec4<f32> {
    var c = i.color * textureSample(tex, samp, i.uv);
    c = vec4<f32>(mix(u.fog_color.rgb, c.rgb, i.fog), c.a);
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}
"#;

pub struct Renderer {
    format: wgpu::TextureFormat,
    shader: wgpu::ShaderModule,
    texture_layout: wgpu::BindGroupLayout,
    uniform_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<PipeKey, wgpu::RenderPipeline>,
    samplers: HashMap<SamplerKey, wgpu::Sampler>,
    textures: HashMap<u32, Tex>,
    next_texture: u32,
    white: Tex,
    bind_groups: HashMap<(u32, SamplerKey), wgpu::BindGroup>,
    uniform_stride: u64,
    uniform_buf: wgpu::Buffer,
    uniform_capacity: usize,
    uniform_bind: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    depth: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
    ops: Vec<Op>,
    verts: Vec<Vertex2D>,
    uniforms: Vec<Uniforms>,
    scratch_in: Vec<u32>,
    scratch_out: Vec<u32>,
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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

fn address_mode(a: u32) -> wgpu::AddressMode {
    match a {
        2 => wgpu::AddressMode::MirrorRepeat,
        // Border colour addressing isn't core wgpu; the game doesn't rely on the border colour.
        3 | 4 => wgpu::AddressMode::ClampToEdge,
        _ => wgpu::AddressMode::Repeat,
    }
}

fn new_texture(gpu: &Gpu, width: u32, height: u32, levels: u32) -> Tex {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raster"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Tex { texture, view, width, height, levels }
}

fn write_levels(gpu: &Gpu, t: &Tex, levels: &[&[u8]]) {
    for (level, data) in levels.iter().enumerate() {
        let (w, h) = ((t.width >> level).max(1), (t.height >> level).max(1));
        if data.len() < (w * h * 4) as usize {
            continue;
        }
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &data[..(w * h * 4) as usize],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
}

impl Renderer {
    /// For render targets of `format` (the window's swapchain format).
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Renderer {
        let d = &gpu.device;
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("im2d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let uniform_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let texture_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("im2d"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let uniform_stride = (d.limits().min_uniform_buffer_offset_alignment as u64).max(std::mem::size_of::<Uniforms>() as u64);
        let (uniform_buf, uniform_bind) = Self::make_uniforms(gpu, &uniform_layout, uniform_stride, 256);
        let white = new_texture(gpu, 1, 1, 1);
        write_levels(gpu, &white, &[&[255, 255, 255, 255]]);
        let vertex_buf = Self::make_vertices(gpu, 4096);
        Renderer {
            format,
            shader,
            texture_layout,
            uniform_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            samplers: HashMap::new(),
            textures: HashMap::new(),
            next_texture: 1,
            white,
            bind_groups: HashMap::new(),
            uniform_stride,
            uniform_buf,
            uniform_capacity: 256,
            uniform_bind,
            vertex_buf,
            depth: None,
            ops: Vec::new(),
            verts: Vec::new(),
            uniforms: Vec::new(),
            scratch_in: Vec::new(),
            scratch_out: Vec::new(),
        }
    }

    fn make_uniforms(gpu: &Gpu, layout: &wgpu::BindGroupLayout, stride: u64, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw uniforms"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw uniforms"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                }),
            }],
        });
        (buf, bind)
    }

    fn make_vertices(gpu: &Gpu, count: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("im2d vertices"),
            size: (count * std::mem::size_of::<Vertex2D>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Creates or replaces a texture from RGBA8 levels (top row first, each half the previous
    /// size). `autogen` builds the mip chain from level 0 (rasters with AUTOMIPMAP). Returns the
    /// texture's id (`id` itself when it already existed).
    pub fn upload_texture(&mut self, gpu: &Gpu, id: u32, width: u32, height: u32, levels: &[&[u8]], autogen: bool) -> u32 {
        let (width, height) = (width.max(1), height.max(1));
        let generated: Vec<Vec<u8>>;
        let generated_refs: Vec<&[u8]>;
        let levels: &[&[u8]] = if autogen && !levels.is_empty() && levels[0].len() >= (width * height * 4) as usize {
            generated = generate_mips(width, height, &levels[0][..(width * height * 4) as usize]);
            generated_refs = generated.iter().map(|l| l.as_slice()).collect();
            &generated_refs
        } else {
            levels
        };
        let count = levels.len().max(1) as u32;
        let id = if id != 0 && self.textures.contains_key(&id) {
            id
        } else {
            let id = self.next_texture;
            self.next_texture += 1;
            id
        };
        let reuse = self.textures.get(&id).is_some_and(|t| t.width == width && t.height == height && t.levels == count);
        if !reuse {
            self.textures.insert(id, new_texture(gpu, width, height, count));
            self.bind_groups.retain(|k, _| k.0 != id);
        }
        write_levels(gpu, &self.textures[&id], levels);
        id
    }

    pub fn destroy_texture(&mut self, id: u32) {
        self.textures.remove(&id);
        self.bind_groups.retain(|k, _| k.0 != id);
    }

    /// Camera clear: colour (when given) and/or depth.
    pub fn clear(&mut self, color: Option<[f64; 4]>, depth: bool) {
        self.ops.push(Op::Clear { color, depth });
    }

    /// An Im2D primitive (rw::PrimitiveType `prim`), optionally indexed.
    pub fn draw_2d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex2D], indices: Option<&[u16]>) {
        self.scratch_in.clear();
        match indices {
            Some(ix) => self.scratch_in.extend(ix.iter().map(|&i| i as u32)),
            None => self.scratch_in.extend(0..verts.len() as u32),
        }
        self.scratch_out.clear();
        expand_primitive(prim, &self.scratch_in, &mut self.scratch_out);
        let first = self.verts.len() as u32;
        self.verts.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts.len() as u32 - first;
        if count == 0 {
            return;
        }

        let alpha = match state.alpha_func {
            1 => [state.alpha_ref, 1000.0],
            2 => [-1000.0, state.alpha_ref],
            _ => [-1000.0, 1000.0],
        };
        let c = state.fog_color;
        let u: Uniforms = [
            state.xform[0],
            state.xform[1],
            state.xform[2],
            state.xform[3],
            alpha[0],
            alpha[1],
            0.0,
            0.0,
            state.fog_start,
            state.fog_end,
            1.0 / (state.fog_start - state.fog_end),
            if state.fog != 0 { 0.0 } else { 1.0 },
            (c & 0xFF) as f32 / 255.0,
            (c >> 8 & 0xFF) as f32 / 255.0,
            (c >> 16 & 0xFF) as f32 / 255.0,
            (c >> 24 & 0xFF) as f32 / 255.0,
        ];
        // GL3 computes the range unguarded; keep the uniform finite either way.
        let mut u = u;
        if !u[10].is_finite() {
            u[10] = 0.0;
        }
        if self.uniforms.last() != Some(&u) {
            self.uniforms.push(u);
        }
        let uniform = self.uniforms.len() as u32 - 1;

        let pipe = PipeKey {
            lines: is_line_primitive(prim),
            blend: (state.blend != 0).then_some((state.src_blend, state.dst_blend)),
            z_test: state.z_test != 0,
            z_write: state.z_write != 0,
            cull: state.cull,
        };
        let texture = if self.textures.contains_key(&state.texture) { state.texture } else { 0 };
        let sampler = SamplerKey { filter: state.filter, address_u: state.address_u, address_v: state.address_v };
        if let Some(Op::Draw { pipe: p, texture: t, sampler: s, uniform: un, first: f, count: n }) = self.ops.last_mut()
            && *p == pipe
            && *t == texture
            && *s == sampler
            && *un == uniform
            && *f + *n == first
        {
            *n += count;
            return;
        }
        self.ops.push(Op::Draw { pipe, texture, sampler, uniform, first, count });
    }

    fn ensure_pipeline(&mut self, gpu: &Gpu, key: PipeKey) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let blend = key.blend.map(|(s, d)| {
            let c = wgpu::BlendComponent { src_factor: blend_factor(s), dst_factor: blend_factor(d), operation: wgpu::BlendOperation::Add };
            wgpu::BlendState { color: c, alpha: c }
        });
        let p = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("im2d"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_2d"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex2D>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Unorm8x4, 2 => Float32x2],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_simple"),
                targets: &[Some(wgpu::ColorTargetState { format: self.format, blend, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: if key.lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: match key.cull {
                    2 => Some(wgpu::Face::Back),
                    3 => Some(wgpu::Face::Front),
                    _ => None,
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(key.z_write),
                depth_compare: Some(if key.z_test { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, p);
    }

    fn ensure_bind_group(&mut self, gpu: &Gpu, texture: u32, key: SamplerKey) {
        if self.bind_groups.contains_key(&(texture, key)) {
            return;
        }
        let sampler = self.samplers.entry(key).or_insert_with(|| {
            use wgpu::FilterMode::{Linear as L, Nearest as N};
            use wgpu::MipmapFilterMode as M;
            // gl3 filterConvMap_NoMIP / _MIP: (min, mag, mip filter, uses mips)
            let (min, mag, mip, mips) = match key.filter {
                1 => (N, N, M::Nearest, false),
                3 => (N, N, M::Nearest, true),
                4 => (L, L, M::Nearest, true),
                5 => (N, N, M::Linear, true),
                6 => (L, L, M::Linear, true),
                _ => (L, L, M::Nearest, false),
            };
            gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("raster"),
                address_mode_u: address_mode(key.address_u),
                address_mode_v: address_mode(key.address_v),
                mag_filter: mag,
                min_filter: min,
                mipmap_filter: mip,
                lod_max_clamp: if mips { 32.0 } else { 0.0 },
                ..Default::default()
            })
        });
        let view = self.textures.get(&texture).map_or(&self.white.view, |t| &t.view);
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        self.bind_groups.insert((texture, key), bind);
    }

    /// Encodes the recorded frame into `target` (`size` pixels, the renderer's format). The
    /// recording stays until `end_frame`, so a frame can be encoded into more than one target.
    pub fn encode(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: (u32, u32)) {
        let ops = std::mem::take(&mut self.ops);
        let verts = std::mem::take(&mut self.verts);
        let uniforms = std::mem::take(&mut self.uniforms);

        if self.depth.as_ref().is_none_or(|d| d.2 != size) {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            self.depth = Some((t, v, size));
        }
        if uniforms.len() > self.uniform_capacity {
            let cap = uniforms.len().next_power_of_two();
            (self.uniform_buf, self.uniform_bind) = Self::make_uniforms(gpu, &self.uniform_layout, self.uniform_stride, cap);
            self.uniform_capacity = cap;
        }
        if !uniforms.is_empty() {
            let mut bytes = vec![0u8; uniforms.len() * self.uniform_stride as usize];
            for (i, u) in uniforms.iter().enumerate() {
                let at = i * self.uniform_stride as usize;
                for (j, f) in u.iter().enumerate() {
                    bytes[at + j * 4..at + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
                }
            }
            gpu.queue.write_buffer(&self.uniform_buf, 0, &bytes);
        }
        let vsize = std::mem::size_of::<Vertex2D>();
        if verts.len() * vsize > self.vertex_buf.size() as usize {
            self.vertex_buf = Self::make_vertices(gpu, verts.len().next_power_of_two());
        }
        if !verts.is_empty() {
            // SAFETY: Vertex2D is repr(C) plain data without padding (28 bytes, asserted above).
            let bytes = unsafe { std::slice::from_raw_parts(verts.as_ptr().cast::<u8>(), verts.len() * vsize) };
            gpu.queue.write_buffer(&self.vertex_buf, 0, bytes);
        }
        for op in &ops {
            if let Op::Draw { pipe, texture, sampler, .. } = *op {
                self.ensure_pipeline(gpu, pipe);
                self.ensure_bind_group(gpu, texture, sampler);
            }
        }

        let depth_view = &self.depth.as_ref().unwrap().1;
        let begin = |enc: &mut wgpu::CommandEncoder, first: bool, color: Option<[f64; 4]>, depth: bool| {
            let color_load = match color {
                Some(c) => wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] }),
                None if first => wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                None => wgpu::LoadOp::Load,
            };
            let depth_load = if depth || first { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load };
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: color_load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth_view,
                        depth_ops: Some(wgpu::Operations { load: depth_load, store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                })
                .forget_lifetime();
            if !verts.is_empty() {
                pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            }
            pass
        };

        let mut pass: Option<wgpu::RenderPass<'static>> = None;
        for op in &ops {
            match *op {
                Op::Clear { color, depth } => {
                    let first = pass.is_none();
                    drop(pass.take());
                    pass = Some(begin(enc, first, color, depth));
                }
                Op::Draw { pipe, texture, sampler, uniform, first, count } => {
                    let p = pass.get_or_insert_with(|| begin(enc, true, None, false));
                    p.set_pipeline(&self.pipelines[&pipe]);
                    p.set_bind_group(0, &self.uniform_bind, &[(uniform as u64 * self.uniform_stride) as u32]);
                    p.set_bind_group(1, &self.bind_groups[&(texture, sampler)], &[]);
                    p.draw(first..first + count, 0..1);
                }
            }
        }
        if pass.is_none() {
            pass = Some(begin(enc, true, None, false));
        }
        drop(pass);

        self.ops = ops;
        self.verts = verts;
        self.uniforms = uniforms;
    }

    /// Drops the recorded frame (keeping the allocations) to start recording the next one.
    pub fn end_frame(&mut self) {
        self.ops.clear();
        self.verts.clear();
        self.uniforms.clear();
    }
}

/// A texture copy on its way to the CPU (`readback_finish` after the commands are submitted).
pub struct Readback {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    row: u32,
    bgra: bool,
}

/// Copies `texture` (RGBA8 or BGRA8, `size` pixels, needs COPY_SRC) into a buffer.
pub fn readback_start(gpu: &Gpu, enc: &mut wgpu::CommandEncoder, texture: &wgpu::Texture, size: (u32, u32)) -> Readback {
    let row = (size.0 * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * size.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size.1) } },
        wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
    );
    let bgra = matches!(texture.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    Readback { buffer, width: size.0, height: size.1, row, bgra }
}

/// Waits for the copy and returns tightly packed RGBA8, top row first.
pub fn readback_finish(gpu: &Gpu, r: Readback) -> Result<Vec<u8>, String> {
    let slice = r.buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("poll: {e}"))?;
    let data = slice.get_mapped_range().map_err(|e| format!("readback: {e:?}"))?;
    let mut out = Vec::with_capacity((r.width * r.height * 4) as usize);
    for y in 0..r.height as usize {
        let line = &data[y * r.row as usize..][..r.width as usize * 4];
        if r.bgra {
            out.extend(line.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]));
        } else {
            out.extend_from_slice(line);
        }
    }
    Ok(out)
}

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

/// librw `wgpu::Im3DVertex` (same layout as gl3's).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub u: f32,
    pub v: f32,
}
const _: () = assert!(std::mem::size_of::<Vertex3D>() == 24);

/// A model vertex as the device instances geometry (gl3 defaultInstanceCB attributes; missing
/// ones are filled with GL's defaults: normal 0, colour (0, 0, 0, 1), texcoords 0).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}
const _: () = assert!(std::mem::size_of::<MeshVertex>() == 36);

/// A skinned model vertex: up to four bones per vertex (gl3 skinInstanceCB: RW skin weights and
/// bone indices as stored in the geometry's skin).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkinVertex {
    pub base: MeshVertex,
    pub weights: [f32; 4],
    pub indices: [u8; 4],
}
const _: () = assert!(std::mem::size_of::<SkinVertex>() == 56);

/// Bone matrices per skinned draw (gl3 u_boneMatrices).
pub const MAX_BONES: usize = 64;
const BONE_FLOATS: usize = MAX_BONES * 16;

/// Per-draw model data for the lit pipeline (gl3 Object uniforms + material).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelState {
    /// Column-major world matrix.
    pub world: [f32; 16],
    /// Material colour (white unless the geometry modulates), 0..1.
    pub mat_color: [f32; 4],
    /// Ambient, specular, diffuse, extra.
    pub surf_props: [f32; 4],
    /// Sum of ambient lights, 0..1.
    pub ambient: [f32; 4],
    /// 8 lights x (params: type 0 end / 1 directional / 2 point / 3 spot, radius, minusCosAngle,
    /// hardSpot; position; direction; colour).
    pub lights: [[f32; 4]; 32],
}
const MODEL_FLOATS: usize = std::mem::size_of::<ModelState>() / 4;

/// Column-major 4x4 matrix (RW RawMatrix / GL layout: right, up, at, pos columns).
pub type Mat4 = [f32; 16];

/// a * b, column-major.
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

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
enum Kind {
    Im2D,
    Im3D,
    Mesh,
    Skin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipeKey {
    kind: Kind,
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

/// xform, alpha window, fog, fog colour, then the Im3D model-view-projection matrix.
type Uniforms = [f32; 32];

#[derive(Clone, Copy, Debug)]
enum Op {
    Clear { color: Option<[f64; 4]>, depth: bool },
    /// Immediate geometry (first/count vertices) or, with `mesh` = (geometry, model uniform), an
    /// index range of a stored geometry.
    /// For skinned meshes the third value is the bone-matrix block.
    Draw { pipe: PipeKey, texture: u32, sampler: SamplerKey, uniform: u32, first: u32, count: u32, mesh: Option<(u32, u32, u32)> },
}

struct Geom {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    levels: u32,
}

/// Pixel data of a texture upload: RGBA8, or DXT1/3/5 blocks as they are in the TXD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexData {
    Rgba8,
    Dxt1,
    Dxt3,
    Dxt5,
}

impl TexData {
    /// librw WGPU raster `dxt` field: 0, 1, 3 or 5.
    pub fn from_dxt(dxt: u32) -> TexData {
        match dxt {
            1 => TexData::Dxt1,
            3 => TexData::Dxt3,
            5 => TexData::Dxt5,
            _ => TexData::Rgba8,
        }
    }

    fn kind(self) -> u8 {
        match self {
            TexData::Rgba8 => 0,
            TexData::Dxt1 => 1,
            TexData::Dxt3 => 3,
            TexData::Dxt5 => 5,
        }
    }

    fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            TexData::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            TexData::Dxt1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            TexData::Dxt3 => wgpu::TextureFormat::Bc2RgbaUnorm,
            TexData::Dxt5 => wgpu::TextureFormat::Bc3RgbaUnorm,
        }
    }
}

const SHADER: &str = r#"
struct U { xform: vec4<f32>, alpha: vec4<f32>, fog: vec4<f32>, fog_color: vec4<f32>, mvp: mat4x4<f32> };
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

// gl3 im3d.vert: projection * view * world, fog from clip w (camera-space depth).
@vertex
fn vs_3d(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = u.mvp * vec4<f32>(pos, 1.0);
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    o.color = color;
    o.uv = uv;
    return o;
}

// gl3 default.vert: prelit colour + ambient * surfAmbient + dynamic lights * surfDiffuse,
// clamped, times the material colour. Normals are transformed by the world matrix unnormalised.
struct M { world: mat4x4<f32>, mat_color: vec4<f32>, surf: vec4<f32>, ambient: vec4<f32>, lights: array<vec4<f32>, 32> };
@group(2) @binding(0) var<uniform> m: M;

fn dynamic_light(v: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let params = m.lights[i * 4];
        let pos = m.lights[i * 4 + 1].xyz;
        let dir = m.lights[i * 4 + 2].xyz;
        let col = m.lights[i * 4 + 3].rgb;
        if (params.x == 0.0) {
            break;
        }
        if (params.x == 1.0) {
            color += max(0.0, dot(n, -dir)) * col;
        } else if (params.x == 2.0) {
            let d = v - pos;
            let dist = length(d);
            let atten = max(0.0, 1.0 - dist / params.y);
            color += max(0.0, dot(n, -normalize(d))) * col * atten;
        } else if (params.x == 3.0) {
            let d = v - pos;
            let dist = length(d);
            let atten = max(0.0, 1.0 - dist / params.y);
            let dn = d / dist;
            var l = max(0.0, dot(n, -dn));
            let pcos = dot(dn, dir);
            let ccos = -params.z;
            let falloff = (pcos - ccos) / (1.0 - ccos);
            if (falloff < 0.0) {
                l = 0.0;
            }
            l *= max(falloff, params.w);
            // gl3 returns here (dropping the other lights); D3D adds spots up like the rest.
            color += l * col * atten;
        }
    }
    return color;
}

@vertex
fn vs_mesh(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> Out {
    return lit(pos, normal, color, uv);
}

// gl3 skin.vert: position and normal blended over up to four bones, then lit like vs_mesh
// (fog from clip w like the default pipeline; gl3's skin shader uses clip z there).
struct Bones { m: array<mat4x4<f32>, 64> };
@group(3) @binding(0) var<uniform> bones: Bones;

@vertex
fn vs_skin(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>, @location(4) weights: vec4<f32>, @location(5) indices: vec4<u32>) -> Out {
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let b = bones.m[min(indices[i], 63u)];
        p += (b * vec4<f32>(pos, 1.0)).xyz * weights[i];
        n += (b * vec4<f32>(normal, 0.0)).xyz * weights[i];
    }
    return lit(p, n, color, uv);
}

fn lit(pos: vec3<f32>, normal: vec3<f32>, color: vec4<f32>, uv: vec2<f32>) -> Out {
    var o: Out;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    let n = (m.world * vec4<f32>(normal, 0.0)).xyz;
    var c = color;
    c = vec4<f32>(c.rgb + m.ambient.rgb * m.surf.x, c.a);
    c = vec4<f32>(c.rgb + dynamic_light(v.xyz, n) * m.surf.z, c.a);
    o.color = clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * m.mat_color;
    o.uv = uv;
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
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
    mesh_layout: wgpu::PipelineLayout,
    model_layout: wgpu::BindGroupLayout,
    model_stride: u64,
    model_buf: wgpu::Buffer,
    model_capacity: usize,
    model_bind: wgpu::BindGroup,
    models: Vec<ModelState>,
    skin_layout: wgpu::PipelineLayout,
    bones_layout: wgpu::BindGroupLayout,
    bones_stride: u64,
    bones_buf: wgpu::Buffer,
    bones_capacity: usize,
    bones_bind: wgpu::BindGroup,
    /// BONE_FLOATS per skinned draw.
    bones: Vec<f32>,
    geometries: HashMap<u32, Geom>,
    next_geometry: u32,
    /// Textures and geometries destroyed while a recorded frame may still use them.
    dead_textures: Vec<u32>,
    dead_geometries: Vec<u32>,
    retired: Vec<Geom>,
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
    vertex3_buf: wgpu::Buffer,
    depth: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
    ops: Vec<Op>,
    verts: Vec<Vertex2D>,
    verts3: Vec<Vertex3D>,
    uniforms: Vec<Uniforms>,
    scratch_in: Vec<u32>,
    scratch_out: Vec<u32>,
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ATTRS_2D: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Unorm8x4, 2 => Float32x2];
const ATTRS_3D: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2];
const ATTRS_MESH: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32x2];
const ATTRS_SKIN: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32x2, 4 => Float32x4, 5 => Uint8x4];

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

fn new_texture(gpu: &Gpu, format: wgpu::TextureFormat, width: u32, height: u32, levels: u32) -> Tex {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raster"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Tex { texture, view, format, width, height, levels }
}

fn write_levels(gpu: &Gpu, t: &Tex, levels: &[&[u8]]) {
    for (level, data) in levels.iter().enumerate() {
        let (w, h) = ((t.width >> level).max(1), (t.height >> level).max(1));
        // Block formats are copied in whole 4x4 blocks (the level's physical size).
        let (bw, bh, bytes) = match t.format.block_copy_size(None) {
            Some(b) if t.format.is_compressed() => (w.div_ceil(4), h.div_ceil(4), b),
            _ => (w, h, 4),
        };
        let size = (bw * bh * bytes) as usize;
        if data.len() < size {
            continue;
        }
        let (dim_w, dim_h) = if t.format.is_compressed() { (bw * 4, bh * 4) } else { (w, h) };
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &data[..size],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bw * bytes), rows_per_image: Some(bh) },
            wgpu::Extent3d { width: dim_w, height: dim_h, depth_or_array_layers: 1 },
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
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("im2d"), bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)], immediate_size: 0 });
        let model_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let mesh_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let align = d.limits().min_uniform_buffer_offset_alignment as u64;
        let model_stride = (std::mem::size_of::<ModelState>() as u64).next_multiple_of(align);
        let (model_buf, model_bind) = Self::make_dyn_uniforms(gpu, &model_layout, model_stride, 64, std::mem::size_of::<ModelState>() as u64);
        let bones_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bone matrices"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let skin_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&model_layout), Some(&bones_layout)],
            immediate_size: 0,
        });
        let bones_stride = ((BONE_FLOATS * 4) as u64).next_multiple_of(align);
        let (bones_buf, bones_bind) = Self::make_dyn_uniforms(gpu, &bones_layout, bones_stride, 16, (BONE_FLOATS * 4) as u64);
        let uniform_stride = (d.limits().min_uniform_buffer_offset_alignment as u64).max(std::mem::size_of::<Uniforms>() as u64);
        let (uniform_buf, uniform_bind) = Self::make_uniforms(gpu, &uniform_layout, uniform_stride, 256);
        let white = new_texture(gpu, wgpu::TextureFormat::Rgba8Unorm, 1, 1, 1);
        write_levels(gpu, &white, &[&[255, 255, 255, 255]]);
        let vertex_buf = Self::make_vertices(gpu, 4096 * std::mem::size_of::<Vertex2D>());
        let vertex3_buf = Self::make_vertices(gpu, 4096 * std::mem::size_of::<Vertex3D>());
        Renderer {
            format,
            shader,
            texture_layout,
            uniform_layout,
            pipeline_layout,
            mesh_layout,
            model_layout,
            model_stride,
            model_buf,
            model_capacity: 64,
            model_bind,
            models: Vec::new(),
            skin_layout,
            bones_layout,
            bones_stride,
            bones_buf,
            bones_capacity: 16,
            bones_bind,
            bones: Vec::new(),
            geometries: HashMap::new(),
            next_geometry: 1,
            dead_textures: Vec::new(),
            dead_geometries: Vec::new(),
            retired: Vec::new(),
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
            vertex3_buf,
            depth: None,
            ops: Vec::new(),
            verts: Vec::new(),
            verts3: Vec::new(),
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
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64) }),
            }],
        });
        (buf, bind)
    }

    /// A uniform buffer of `capacity` blocks `stride` apart, bound with a dynamic offset and
    /// `binding` bytes visible per block.
    fn make_dyn_uniforms(gpu: &Gpu, layout: &wgpu::BindGroupLayout, stride: u64, capacity: usize, binding: u64) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("per-draw uniforms"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("per-draw uniforms"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: wgpu::BufferSize::new(binding) }) }],
        });
        (buf, bind)
    }

    /// Creates or replaces a geometry (vertices + triangle-list indices); returns its id.
    pub fn upload_geometry(&mut self, gpu: &Gpu, id: u32, vertices: &[MeshVertex], indices: &[u16]) -> u32 {
        // SAFETY: MeshVertex is repr(C) plain data without padding (36 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(gpu, id, vbytes, indices)
    }

    /// A skinned geometry (drawn with bone matrices by `draw_mesh`).
    pub fn upload_skinned_geometry(&mut self, gpu: &Gpu, id: u32, vertices: &[SkinVertex], indices: &[u16]) -> u32 {
        // SAFETY: SkinVertex is repr(C) plain data without padding (56 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(gpu, id, vbytes, indices)
    }

    fn upload_raw(&mut self, gpu: &Gpu, id: u32, vbytes: &[u8], indices: &[u16]) -> u32 {
        let id = if id != 0 && self.geometries.contains_key(&id) {
            id
        } else {
            let id = self.next_geometry;
            self.next_geometry += 1;
            id
        };
        let mut ibytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        // Buffer copies come in multiples of 4 bytes.
        ibytes.resize(ibytes.len().next_multiple_of(4).max(4), 0);
        let mut vb = vbytes.to_vec();
        vb.resize(vb.len().next_multiple_of(4).max(4), 0);
        use wgpu::util::DeviceExt;
        let geom = Geom {
            vertices: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("geometry vertices"), contents: &vb, usage: wgpu::BufferUsages::VERTEX }),
            indices: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("geometry indices"), contents: &ibytes, usage: wgpu::BufferUsages::INDEX }),
        };
        // A replaced geometry may still be drawn by the frame being recorded: keep the old
        // buffers alive until the frame is done.
        if let Some(old) = self.geometries.insert(id, geom) {
            self.retired.push(old);
        }
        id
    }

    pub fn destroy_geometry(&mut self, id: u32) {
        self.dead_geometries.push(id);
    }

    fn make_vertices(gpu: &Gpu, bytes: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("immediate vertices"),
            size: bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Creates or replaces a texture from mip levels (top row first, each half the previous
    /// size), RGBA8 or DXT blocks. DXT stays compressed when the device samples BC formats and
    /// the size is whole blocks, else it is decoded here. `autogen` builds the mip chain from
    /// level 0 (RGBA8 rasters with AUTOMIPMAP). Returns the texture's id (`id` itself when it
    /// already existed).
    // One argument per field of the device's upload call (revc_rw_texture_upload).
    #[allow(clippy::too_many_arguments)]
    pub fn upload_texture(&mut self, gpu: &Gpu, id: u32, width: u32, height: u32, data: TexData, levels: &[&[u8]], autogen: bool) -> u32 {
        let (width, height) = (width.max(1), height.max(1));
        let bc = gpu.device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) && width % 4 == 0 && height % 4 == 0;
        let decoded: Vec<Vec<u8>>;
        let decoded_refs: Vec<&[u8]>;
        let (data, levels): (TexData, &[&[u8]]) = if data != TexData::Rgba8 && !bc {
            decoded = levels.iter().enumerate().map_while(|(i, l)| revc_formats::dds::decode_dxt(data.kind(), (width >> i).max(1), (height >> i).max(1), l)).collect();
            decoded_refs = decoded.iter().map(|l| l.as_slice()).collect();
            (TexData::Rgba8, &decoded_refs)
        } else {
            (data, levels)
        };
        let autogen = autogen && data == TexData::Rgba8;
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
        let format = data.wgpu();
        let reuse = self.textures.get(&id).is_some_and(|t| t.format == format && t.width == width && t.height == height && t.levels == count);
        if !reuse {
            self.textures.insert(id, new_texture(gpu, format, width, height, count));
            self.bind_groups.retain(|k, _| k.0 != id);
        }
        write_levels(gpu, &self.textures[&id], levels);
        id
    }

    pub fn destroy_texture(&mut self, id: u32) {
        self.dead_textures.push(id);
    }

    /// Camera clear: colour (when given) and/or depth.
    pub fn clear(&mut self, color: Option<[f64; 4]>, depth: bool) {
        self.ops.push(Op::Clear { color, depth });
    }

    fn expand(&mut self, prim: u32, num_verts: usize, indices: Option<&[u16]>) {
        self.scratch_in.clear();
        match indices {
            Some(ix) => self.scratch_in.extend(ix.iter().map(|&i| i as u32)),
            None => self.scratch_in.extend(0..num_verts as u32),
        }
        self.scratch_out.clear();
        expand_primitive(prim, &self.scratch_in, &mut self.scratch_out);
    }

    /// An Im2D primitive (rw::PrimitiveType `prim`), optionally indexed.
    pub fn draw_2d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex2D], indices: Option<&[u16]>) {
        self.expand(prim, verts.len(), indices);
        let first = self.verts.len() as u32;
        self.verts.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts.len() as u32 - first;
        self.record(state, prim, Kind::Im2D, first, count, &[0.0; 16], None);
    }

    /// An Im3D primitive: `mvp` = projection * view * world (column-major).
    pub fn draw_3d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex3D], indices: Option<&[u16]>, mvp: &Mat4) {
        self.expand(prim, verts.len(), indices);
        let first = self.verts3.len() as u32;
        self.verts3.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts3.len() as u32 - first;
        self.record(state, prim, Kind::Im3D, first, count, mvp, None);
    }

    /// Triangles `first..first + count` of the index list of a stored geometry, lit with
    /// `model`; `view_proj` = projection * view. A skinned geometry gets its `bones` (at most
    /// MAX_BONES; missing ones are identity).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_mesh(&mut self, state: &DrawState, geometry: u32, first: u32, count: u32, view_proj: &Mat4, model: &ModelState, bones: Option<&[Mat4]>) {
        if !self.geometries.contains_key(&geometry) {
            return;
        }
        if self.models.last() != Some(model) {
            self.models.push(*model);
        }
        let mi = self.models.len() as u32 - 1;
        let (kind, bi) = match bones {
            Some(b) => {
                let mut block = [0.0f32; BONE_FLOATS];
                for i in 0..MAX_BONES {
                    let m = b.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                    block[i * 16..][..16].copy_from_slice(&m);
                }
                let n = self.bones.len();
                if n < BONE_FLOATS || self.bones[n - BONE_FLOATS..] != block {
                    self.bones.extend_from_slice(&block);
                }
                (Kind::Skin, (self.bones.len() / BONE_FLOATS - 1) as u32)
            }
            None => (Kind::Mesh, u32::MAX),
        };
        self.record(state, 3, kind, first, count, view_proj, Some((geometry, mi, bi)));
    }

    #[allow(clippy::too_many_arguments)]
    fn record(&mut self, state: &DrawState, prim: u32, kind: Kind, first: u32, count: u32, mvp: &Mat4, mesh: Option<(u32, u32, u32)>) {
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
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ];
        // GL3 computes the range unguarded; keep the uniform finite either way.
        let mut u = u;
        if !u[10].is_finite() {
            u[10] = 0.0;
        }
        u[16..].copy_from_slice(mvp);
        if self.uniforms.last() != Some(&u) {
            self.uniforms.push(u);
        }
        let uniform = self.uniforms.len() as u32 - 1;

        let pipe = PipeKey {
            kind,
            lines: is_line_primitive(prim),
            blend: (state.blend != 0).then_some((state.src_blend, state.dst_blend)),
            z_test: state.z_test != 0,
            z_write: state.z_write != 0,
            cull: state.cull,
        };
        let texture = if self.textures.contains_key(&state.texture) { state.texture } else { 0 };
        let sampler = SamplerKey { filter: state.filter, address_u: state.address_u, address_v: state.address_v };
        if let Some(Op::Draw { pipe: p, texture: t, sampler: s, uniform: un, first: f, count: n, mesh: me }) = self.ops.last_mut()
            && *me == mesh
            && *p == pipe
            && *t == texture
            && *s == sampler
            && *un == uniform
            && *f + *n == first
        {
            *n += count;
            return;
        }
        self.ops.push(Op::Draw { pipe, texture, sampler, uniform, first, count, mesh });
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
            label: Some(match key.kind {
                Kind::Im2D => "im2d",
                Kind::Im3D => "im3d",
                Kind::Mesh => "mesh",
                Kind::Skin => "skin",
            }),
            layout: Some(match key.kind {
                Kind::Mesh => &self.mesh_layout,
                Kind::Skin => &self.skin_layout,
                _ => &self.pipeline_layout,
            }),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some(match key.kind {
                    Kind::Im2D => "vs_2d",
                    Kind::Im3D => "vs_3d",
                    Kind::Mesh => "vs_mesh",
                    Kind::Skin => "vs_skin",
                }),
                buffers: &[Some(match key.kind {
                    Kind::Im2D => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex2D>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_2D },
                    Kind::Im3D => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex3D>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_3D },
                    Kind::Mesh => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<MeshVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_MESH },
                    Kind::Skin => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<SkinVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_SKIN },
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
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
        });
        self.bind_groups.insert((texture, key), bind);
    }

    /// Encodes the recorded frame into `target` (`size` pixels, the renderer's format). The
    /// recording stays until `end_frame`, so a frame can be encoded into more than one target.
    pub fn encode(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: (u32, u32)) {
        let ops = std::mem::take(&mut self.ops);
        let verts = std::mem::take(&mut self.verts);
        let verts3 = std::mem::take(&mut self.verts3);
        let uniforms = std::mem::take(&mut self.uniforms);
        let models = std::mem::take(&mut self.models);
        if models.len() > self.model_capacity {
            let cap = models.len().next_power_of_two();
            (self.model_buf, self.model_bind) = Self::make_dyn_uniforms(gpu, &self.model_layout, self.model_stride, cap, std::mem::size_of::<ModelState>() as u64);
            self.model_capacity = cap;
        }
        if !models.is_empty() {
            let mut bytes = vec![0u8; models.len() * self.model_stride as usize];
            for (i, mdl) in models.iter().enumerate() {
                // SAFETY: ModelState is repr(C) f32 data.
                let src = unsafe { std::slice::from_raw_parts((mdl as *const ModelState).cast::<u8>(), MODEL_FLOATS * 4) };
                bytes[i * self.model_stride as usize..][..src.len()].copy_from_slice(src);
            }
            gpu.queue.write_buffer(&self.model_buf, 0, &bytes);
        }
        let bones = std::mem::take(&mut self.bones);
        let blocks = bones.len() / BONE_FLOATS;
        if blocks > self.bones_capacity {
            let cap = blocks.next_power_of_two();
            (self.bones_buf, self.bones_bind) = Self::make_dyn_uniforms(gpu, &self.bones_layout, self.bones_stride, cap, (BONE_FLOATS * 4) as u64);
            self.bones_capacity = cap;
        }
        if blocks > 0 {
            let mut bytes = vec![0u8; blocks * self.bones_stride as usize];
            for (i, block) in bones.as_chunks::<BONE_FLOATS>().0.iter().enumerate() {
                let at = i * self.bones_stride as usize;
                for (j, f) in block.iter().enumerate() {
                    bytes[at + j * 4..at + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
                }
            }
            gpu.queue.write_buffer(&self.bones_buf, 0, &bytes);
        }

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
        // SAFETY: Vertex2D/Vertex3D are repr(C) plain data without padding (sizes asserted above).
        let bytes2 = unsafe { std::slice::from_raw_parts(verts.as_ptr().cast::<u8>(), std::mem::size_of_val(verts.as_slice())) };
        let bytes3 = unsafe { std::slice::from_raw_parts(verts3.as_ptr().cast::<u8>(), std::mem::size_of_val(verts3.as_slice())) };
        if bytes2.len() > self.vertex_buf.size() as usize {
            self.vertex_buf = Self::make_vertices(gpu, bytes2.len().next_power_of_two());
        }
        if bytes3.len() > self.vertex3_buf.size() as usize {
            self.vertex3_buf = Self::make_vertices(gpu, bytes3.len().next_power_of_two());
        }
        if !bytes2.is_empty() {
            gpu.queue.write_buffer(&self.vertex_buf, 0, bytes2);
        }
        if !bytes3.is_empty() {
            gpu.queue.write_buffer(&self.vertex3_buf, 0, bytes3);
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
            enc.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            .forget_lifetime()
        };

        let mut pass: Option<wgpu::RenderPass<'static>> = None;
        // Which immediate vertex buffer the pass has bound (None after starting a pass or after a
        // geometry's buffers).
        let mut bound: Option<Kind> = None;
        let mut started = false;
        for op in &ops {
            match *op {
                Op::Clear { color, depth } => {
                    drop(pass.take());
                    pass = Some(begin(enc, !started, color, depth));
                    started = true;
                    bound = None;
                }
                Op::Draw { pipe, texture, sampler, uniform, first, count, mesh } => {
                    if pass.is_none() {
                        pass = Some(begin(enc, !started, None, false));
                        started = true;
                        bound = None;
                    }
                    let p = pass.as_mut().unwrap();
                    p.set_pipeline(&self.pipelines[&pipe]);
                    p.set_bind_group(0, &self.uniform_bind, &[(uniform as u64 * self.uniform_stride) as u32]);
                    p.set_bind_group(1, &self.bind_groups[&(texture, sampler)], &[]);
                    match mesh {
                        Some((geometry, model, bones)) => {
                            let Some(g) = self.geometries.get(&geometry) else { continue };
                            p.set_bind_group(2, &self.model_bind, &[(model as u64 * self.model_stride) as u32]);
                            if pipe.kind == Kind::Skin {
                                p.set_bind_group(3, &self.bones_bind, &[(bones as u64 * self.bones_stride) as u32]);
                            }
                            p.set_vertex_buffer(0, g.vertices.slice(..));
                            p.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint16);
                            bound = None;
                            p.draw_indexed(first..first + count, 0, 0..1);
                        }
                        None => {
                            if bound != Some(pipe.kind) {
                                p.set_vertex_buffer(0, if pipe.kind == Kind::Im3D { self.vertex3_buf.slice(..) } else { self.vertex_buf.slice(..) });
                                bound = Some(pipe.kind);
                            }
                            p.draw(first..first + count, 0..1);
                        }
                    }
                }
            }
        }
        if pass.is_none() {
            pass = Some(begin(enc, true, None, false));
        }
        drop(pass);

        self.ops = ops;
        self.verts = verts;
        self.verts3 = verts3;
        self.uniforms = uniforms;
        self.models = models;
        self.bones = bones;
    }

    /// Drops the recorded frame (keeping the allocations) to start recording the next one.
    pub fn end_frame(&mut self) {
        self.ops.clear();
        self.verts.clear();
        self.verts3.clear();
        self.uniforms.clear();
        self.models.clear();
        self.bones.clear();
        self.retired.clear();
        for id in std::mem::take(&mut self.dead_textures) {
            self.textures.remove(&id);
            self.bind_groups.retain(|k, _| k.0 != id);
        }
        for id in std::mem::take(&mut self.dead_geometries) {
            self.geometries.remove(&id);
        }
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

//! revc-rw: the Rust replacement for librw (plan task 12), on wgpu with a user-selectable Vulkan
//! or OpenGL backend.
//!
//! So far: a headless device on either backend and an offscreen renderer for whole RenderWare
//! models (clump + texture dictionary parsed by revc-formats): parts placed by their frame
//! hierarchy, per-material textures with mipmaps and material colours, a fixed directional
//! light and RW-style alpha test. Used to compare the backends before anything is wired into the
//! game.
//!
//! In the game (librw's WGPU platform): `present` shows frames in the SDL window and `frame`
//! records and draws what the device sends (camera clears, textures, Im2D).

use revc_formats::{dff, txd};
use wgpu::util::DeviceExt;

pub mod frame;
pub mod present;
mod prim;

/// The graphics API to render with (the game's settings will offer both).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Vulkan,
    Gl,
}

impl Backend {
    fn wgpu(self) -> wgpu::Backends {
        match self {
            Backend::Vulkan => wgpu::Backends::VULKAN,
            Backend::Gl => wgpu::Backends::GL,
        }
    }
}

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}

impl Gpu {
    /// A device without a window (for tests and offscreen work).
    pub fn headless(backend: Backend) -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: backend.wgpu(), ..wgpu::InstanceDescriptor::new_without_display_handle() });
        Gpu::create(&instance, backend, None)
    }

    /// Adapter and device on `instance`, able to present to `surface` if given.
    pub fn create(instance: &wgpu::Instance, backend: Backend, surface: Option<&wgpu::Surface<'_>>) -> Result<Gpu, String> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no {backend:?} adapter: {e}"))?;
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("revc-rw"),
            // What GL (GLES 3 level) can do too, so both backends run the same code.
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| format!("{backend:?} device: {e}"))?;
        Ok(Gpu { adapter, device, queue, info })
    }
}

/// RGBA8 texture with its mip chain (largest first).
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Vec<u8>>,
}

impl Image {
    /// All mip levels of a TXD texture.
    pub fn from_texture(t: &txd::Texture) -> Result<Image, String> {
        let levels = (0..t.levels.len()).map(|l| t.rgba(l).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
        Ok(Image { width: t.width as u32, height: t.height as u32, levels })
    }
}

/// Triangles of one material.
pub struct Batch {
    pub indices: Vec<u32>,
    /// Index into `Model::images`; None = untextured (white).
    pub image: Option<usize>,
    pub color: [u8; 4],
}

/// A whole model in its own space: vertices are position, normal, uv.
pub struct Model {
    pub vertices: Vec<[f32; 8]>,
    pub batches: Vec<Batch>,
    pub images: Vec<Image>,
}

impl Model {
    /// The default parts of a clump (see dff::Clump::is_default_part), each placed by its frame,
    /// with textures from `dict` (looked up by name, case-insensitive, as the game does).
    pub fn from_clump(clump: &dff::Clump, dict: Option<&txd::Txd>) -> Result<Model, String> {
        let mut m = Model { vertices: Vec::new(), batches: Vec::new(), images: Vec::new() };
        let mut image_of: Vec<(String, usize)> = Vec::new();
        for a in clump.atomics.iter().filter(|a| clump.is_default_part(a.frame as usize)) {
            let g = &clump.geometries[a.geometry as usize];
            let Some(mt) = g.morph_targets.first() else { continue };
            if mt.vertices.is_empty() {
                continue;
            }
            let world = clump.world_transform(a.frame as usize);
            let normals = if mt.normals.len() == mt.vertices.len() { mt.normals.clone() } else { smooth_normals(&mt.vertices, &g.triangles) };
            let base = m.vertices.len() as u32;
            for (i, p) in mt.vertices.iter().enumerate() {
                let p = dff::transform_point(&world, *p);
                let n = normalize(dff::transform_dir(&world, normals[i]));
                let uv = g.tex_coords.first().map_or([0.0, 0.0], |set| set[i]);
                m.vertices.push([p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]]);
            }
            for (mi, mat) in g.materials.iter().enumerate() {
                let indices: Vec<u32> = g.triangles.iter().filter(|t| t.material as usize == mi).flat_map(|t| t.v.map(|v| base + v as u32)).collect();
                if indices.is_empty() {
                    continue;
                }
                let image = match (&mat.texture, dict) {
                    (Some(t), Some(d)) => match image_of.iter().find(|(n, _)| n.eq_ignore_ascii_case(&t.name)) {
                        Some((_, i)) => Some(*i),
                        None => match d.find(&t.name) {
                            Some(tex) => {
                                m.images.push(Image::from_texture(tex)?);
                                image_of.push((t.name.clone(), m.images.len() - 1));
                                Some(m.images.len() - 1)
                            }
                            None => None, // the game leaves the material untextured too
                        },
                    },
                    _ => None,
                };
                m.batches.push(Batch { indices, image, color: mat.color });
            }
        }
        Ok(m)
    }

    /// Axis-aligned bounds (min, max) of the vertices that are drawn.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for b in &self.batches {
            for &i in &b.indices {
                let v = self.vertices[i as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        (lo, hi)
    }
}

/// Per-vertex normals from the faces around each vertex (for geometry stored without normals).
fn smooth_normals(pos: &[[f32; 3]], tris: &[dff::Triangle]) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let [a, b, c] = t.v.map(|i| pos[i as usize]);
        let f = cross(sub(b, a), sub(c, a));
        for v in t.v {
            for k in 0..3 {
                n[v as usize][k] += f[k];
            }
        }
    }
    n.into_iter().map(normalize).collect()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 0.0, 1.0] }
}

/// Column-major 4x4 matrices (WGSL's layout).
pub type Mat4 = [[f32; 4]; 4];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [[0.0; 4]; 4];
    for c in 0..4 {
        for row in 0..4 {
            r[c][row] = (0..4).map(|k| a[k][row] * b[c][k]).sum();
        }
    }
    r
}

/// Right-handed look-at with Z up (the game's world convention).
pub fn look_at(eye: [f32; 3], target: [f32; 3]) -> Mat4 {
    let f = normalize(sub(target, eye));
    let s = normalize(cross(f, [0.0, 0.0, 1.0]));
    let u = cross(s, f);
    [[s[0], u[0], -f[0], 0.0], [s[1], u[1], -f[1], 0.0], [s[2], u[2], -f[2], 0.0], [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]]
}

/// Perspective projection to wgpu's 0..1 depth range.
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    [[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]]
}

const SHADER: &str = r#"
struct Frame { view_proj: mat4x4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> material: vec4<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = frame.view_proj * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.uv = uv;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv) * material;
    // RenderWare's default alpha test for opaque passes.
    if (c.a < 0.5) {
        discard;
    }
    let lambert = max(dot(normalize(i.normal), normalize(frame.light.xyz)), 0.0);
    return vec4<f32>(c.rgb * (0.3 + 0.7 * lambert), 1.0);
}
"#;

fn upload(gpu: &Gpu, img: &Image) -> wgpu::Texture {
    let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model texture"),
        size: wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
        mip_level_count: img.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, data) in img.levels.iter().enumerate() {
        let (w, h) = ((img.width >> level).max(1), (img.height >> level).max(1));
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
    t
}

/// Renders `model` from a camera that frames it, into a `size` x `size` RGBA8 image (top row
/// first). `size` must be a multiple of 64 (row alignment of the readback).
pub fn render_offscreen(gpu: &Gpu, model: &Model, size: u32) -> Vec<u8> {
    assert!(size.is_multiple_of(64), "size must be a multiple of 64");
    let d = &gpu.device;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 };
    let target = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    // Camera: in front-left and above, looking at the centre of the model.
    let (lo, hi) = model.bounds();
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let radius = dot(sub(hi, lo), sub(hi, lo)).sqrt() / 2.0;
    let eye = [centre[0] + radius * 1.6, centre[1] - radius * 1.9, centre[2] + radius * 1.1];
    let view_proj = mul(&perspective(0.8, 1.0, radius * 0.1, radius * 10.0), &look_at(eye, centre));
    let mut frame: Vec<f32> = view_proj.iter().flatten().copied().collect();
    frame.extend([0.4, -0.6, 0.8, 0.0]);
    let bytes = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
    let frame_buf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("frame"), contents: &bytes(&frame), usage: wgpu::BufferUsages::UNIFORM });

    let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("model"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("model"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), targets: &[Some(format.into())], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState::default(), // no culling: RW models aren't consistently wound
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let frame_bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() }],
    });

    // RW's default: wrap addressing, trilinear filtering.
    let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let white = upload(gpu, &Image { width: 1, height: 1, levels: vec![vec![255; 4]] });
    let textures: Vec<wgpu::Texture> = model.images.iter().map(|i| upload(gpu, i)).collect();
    let vb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertices"),
        contents: &bytes(&model.vertices.iter().flatten().copied().collect::<Vec<f32>>()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let draws: Vec<(wgpu::Buffer, wgpu::BindGroup, u32)> = model
        .batches
        .iter()
        .map(|b| {
            let ib = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: &b.indices.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(),
                usage: wgpu::BufferUsages::INDEX,
            });
            let colour: Vec<f32> = b.color.iter().map(|&c| c as f32 / 255.0).collect();
            let mb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("material"), contents: &bytes(&colour), usage: wgpu::BufferUsages::UNIFORM });
            let view = b.image.map_or(&white, |i| &textures[i]).create_view(&Default::default());
            let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("material"),
                layout: &pipeline.get_bind_group_layout(1),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: mb.as_entire_binding() },
                ],
            });
            (ib, bind, b.indices.len() as u32)
        })
        .collect();

    let readback = d.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (size * size * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = d.create_command_encoder(&Default::default());
    {
        let view = target.create_view(&Default::default());
        let dview = depth.create_view(&Default::default());
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("model"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.12, b: 0.16, a: 1.0 }), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &dview,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &frame_bind, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        for (ib, bind, n) in &draws {
            pass.set_bind_group(1, bind, &[]);
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
        }
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: Some(size) } },
        extent,
    );
    gpu.queue.submit([enc.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
    d.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    slice.get_mapped_range().expect("mapped readback").to_vec()
}

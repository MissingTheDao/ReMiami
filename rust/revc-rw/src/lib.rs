//! revc-rw: the Rust replacement for librw (plan task 12), on wgpu with a user-selectable Vulkan
//! or OpenGL backend.
//!
//! First step: a headless device on either backend and an offscreen renderer for RenderWare
//! geometry parsed by revc-formats (lit with a fixed directional light), so both backends can be
//! compared before anything is wired into the game.

use revc_formats::dff;
use wgpu::util::DeviceExt;

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
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}

impl Gpu {
    /// A device without a window (for tests and offscreen work).
    pub fn headless(backend: Backend) -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: backend.wgpu(), ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
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
        Ok(Gpu { device, queue, info })
    }
}

/// Triangle mesh in model space: position and normal per vertex.
pub struct Mesh {
    pub vertices: Vec<[f32; 6]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// From the first morph target of a DFF geometry; vertex normals from the file, or each
    /// triangle's face normal when the geometry has none.
    pub fn from_geometry(g: &dff::Geometry) -> Mesh {
        let mt = &g.morph_targets[0];
        let pos = &mt.vertices;
        if mt.normals.len() == pos.len() {
            let vertices = pos.iter().zip(&mt.normals).map(|(p, n)| [p[0], p[1], p[2], n[0], n[1], n[2]]).collect();
            let indices = g.triangles.iter().flat_map(|t| t.v.map(u32::from)).collect();
            return Mesh { vertices, indices };
        }
        let mut vertices = Vec::with_capacity(g.triangles.len() * 3);
        for t in &g.triangles {
            let [a, b, c] = t.v.map(|i| pos[i as usize]);
            let n = normalize(cross(sub(b, a), sub(c, a)));
            for p in [a, b, c] {
                vertices.push([p[0], p[1], p[2], n[0], n[1], n[2]]);
            }
        }
        let indices = (0..vertices.len() as u32).collect();
        Mesh { vertices, indices }
    }

    /// Axis-aligned bounds (min, max).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &self.vertices {
            for i in 0..3 {
                lo[i] = lo[i].min(v[i]);
                hi[i] = hi[i].max(v[i]);
            }
        }
        (lo, hi)
    }
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
struct Uniforms { view_proj: mat4x4<f32>, light: vec4<f32>, color: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) normal: vec3<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> Out {
    var o: Out;
    o.pos = u.view_proj * vec4<f32>(pos, 1.0);
    o.normal = normal;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    let lambert = max(dot(normalize(i.normal), normalize(u.light.xyz)), 0.0);
    return vec4<f32>(u.color.rgb * (0.25 + 0.75 * lambert), 1.0);
}
"#;

/// Renders `meshes` from a camera that frames all of them, into a `size` x `size` RGBA8 image
/// (top row first). `size` must be a multiple of 64 (row alignment of the readback).
pub fn render_offscreen(gpu: &Gpu, meshes: &[Mesh], size: u32) -> Vec<u8> {
    assert!(size.is_multiple_of(64), "size must be a multiple of 64");
    let d = &gpu.device;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    // Camera: in front-left and above, looking at the centre of everything.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for m in meshes {
        let (l, h) = m.bounds();
        for i in 0..3 {
            lo[i] = lo[i].min(l[i]);
            hi[i] = hi[i].max(h[i]);
        }
    }
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let radius = dot(sub(hi, lo), sub(hi, lo)).sqrt() / 2.0;
    let eye = [centre[0] + radius * 1.6, centre[1] - radius * 1.9, centre[2] + radius * 1.1];
    let view_proj = mul(&perspective(0.8, 1.0, radius * 0.1, radius * 10.0), &look_at(eye, centre));

    let mut uniforms: Vec<f32> = view_proj.iter().flatten().copied().collect();
    uniforms.extend([0.4, -0.6, 0.8, 0.0, 0.85, 0.2, 0.15, 1.0]); // light direction, body colour
    let ubuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: &uniforms.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("mesh"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mesh"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
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
    let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("uniforms"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
    });
    let buffers: Vec<(wgpu::Buffer, wgpu::Buffer, u32)> = meshes
        .iter()
        .filter(|m| !m.indices.is_empty())
        .map(|m| {
            let vb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: &m.vertices.iter().flatten().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>(),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: &m.indices.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(),
                usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib, m.indices.len() as u32)
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
            label: Some("mesh"),
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
        pass.set_bind_group(0, &bind, &[]);
        for (vb, ib, n) in &buffers {
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
        }
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: Some(size) },
        },
        wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
    d.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    
    slice.get_mapped_range().expect("mapped readback").to_vec()
}

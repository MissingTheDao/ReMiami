//! Shared helpers: a headless renderer per backend, frame readback, golden images.
#![allow(dead_code)]

use retro_wgpu::*;
use std::path::PathBuf;

pub struct Ctx {
    pub backend: Backend,
    pub gpu: Gpu,
    pub r: Renderer,
    pub size: (u32, u32),
    pub out: wgpu::Texture,
}

/// None (test skipped, with a note) when the backend isn't usable on this machine.
pub fn ctx(backend: Backend, size: (u32, u32), settings: SceneSettings) -> Option<Ctx> {
    let gpu = match Gpu::headless(backend) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED on {backend:?}: {e}");
            return None;
        }
    };
    let r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm, settings);
    let out = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test output"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    eprintln!("{backend:?}: {} ({:?})", gpu.info.name, gpu.info.driver);
    Some(Ctx { backend, gpu, r, size, out })
}

impl Ctx {
    /// Encodes the recorded frame into the output texture and reads it back.
    pub fn frame(&mut self) -> Image {
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        let view = self.out.create_view(&Default::default());
        self.r.encode(&self.gpu, &mut enc, &view, self.size);
        self.r.end_frame();
        let rb = retro_wgpu::readback::readback_start(&self.gpu, &mut enc, &self.out);
        self.gpu.queue.submit([enc.finish()]);
        retro_wgpu::readback::readback_finish(&self.gpu, rb).expect("readback")
    }

    pub fn layout(&mut self, attrs: &[(Semantic, AttrFormat)]) -> LayoutId {
        self.r.create_layout(&self.gpu, VertexLayout::packed(attrs)).expect("layout")
    }

    pub fn draw(&mut self, layout: LayoutId, topology: Topology, verts: &[u8], indices: Indices, state: &RenderState, params: &DrawParams) {
        self.r.draw(&self.gpu, &Draw { layout, topology, vertices: Vertices::Slice(verts), indices, range: None, base_vertex: 0, state, params, bones: None });
    }
}

pub fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).take(3).all(|(x, y)| x.abs_diff(y) <= tol)
}

#[track_caller]
pub fn expect_px(img: &Image, x: u32, y: u32, want: [u8; 4], tol: u8, what: &str) {
    let got = img.pixel(x, y);
    assert!(near(got, want, tol), "{what}: pixel ({x},{y}) got {got:?}, want {want:?} (±{tol})");
}

/// Quad from (x0, y0) to (x1, y1) in clip space at depth z, as a triangle fan, one colour.
pub fn quad_pc(x0: f32, y0: f32, x1: f32, y1: f32, z: f32, c: [u8; 4]) -> Vec<u8> {
    let mut w = VertexWriter::new();
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        w.f32s(&[x, y, z]).u8x4(c);
    }
    w.bytes
}

/// Same with texture coordinates (0,0 at (x0, y1) = top left).
pub fn quad_pcu(x0: f32, y0: f32, x1: f32, y1: f32, z: f32, c: [u8; 4]) -> Vec<u8> {
    let mut w = VertexWriter::new();
    for (x, y, u, v) in [(x0, y0, 0.0, 1.0), (x1, y0, 1.0, 1.0), (x1, y1, 1.0, 0.0), (x0, y1, 0.0, 0.0)] {
        w.f32s(&[x, y, z]).u8x4(c).f32s(&[u, v]);
    }
    w.bytes
}

pub const PC: &[(Semantic, AttrFormat)] = &[(Semantic::Position, AttrFormat::F32x3), (Semantic::Color, AttrFormat::Unorm8x4)];
pub const PCU: &[(Semantic, AttrFormat)] = &[(Semantic::Position, AttrFormat::F32x3), (Semantic::Color, AttrFormat::Unorm8x4), (Semantic::TexCoord(0), AttrFormat::F32x2)];

/// Compares `img` with tests/golden/<name>-<backend>.png: at most 0.5 % of the pixels may
/// differ by more than 2 per channel (rasterisation and BC rounding differ between GPUs). A
/// missing golden, or RETRO_WGPU_BLESS=1, writes it instead.
#[track_caller]
pub fn golden(img: &Image, name: &str, backend: Backend) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let path = dir.join(format!("{name}-{}.png", format!("{backend:?}").to_lowercase()));
    let bless = std::env::var_os("RETRO_WGPU_BLESS").is_some();
    if bless || !path.exists() {
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::File::create(&path).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut opaque = img.rgba.clone();
        opaque.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
        e.write_header().unwrap().write_image_data(&opaque).unwrap();
        eprintln!("golden written: {}", path.display());
        return;
    }
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (img.width, img.height), "{name}: golden size");
    let bad = buf.as_chunks::<4>().0.iter().zip(img.rgba.as_chunks::<4>().0).filter(|(a, b)| !near(**a, **b, 2)).count();
    let limit = (img.width * img.height) as usize / 200;
    assert!(bad <= limit, "{name} on {backend:?}: {bad} pixels differ from {} (limit {limit})", path.display());
}

/// Runs `f` on every backend this machine can create headless (Vulkan, GL; DX12 on Windows).
pub fn each_backend(f: impl Fn(Backend)) {
    for b in Backend::available() {
        f(b);
    }
}

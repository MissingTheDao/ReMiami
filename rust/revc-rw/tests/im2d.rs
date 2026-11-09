//! Im2D through revc_rw::frame::Renderer on both backends, offscreen: clear, untextured and
//! textured quads (texture orientation, nearest filtering), RW alpha blending and alpha test,
//! and a camera clear in the middle of a frame.

use revc_rw::frame::{DrawState, Renderer, TexData, Vertex2D, readback_finish, readback_start};
use revc_rw::{Backend, Gpu};

const SIZE: u32 = 64;

fn quad(x0: f32, y0: f32, x1: f32, y1: f32, rgba: [u8; 4]) -> [Vertex2D; 4] {
    let v = |x, y, u, v| Vertex2D { x, y, z: 0.0, w: 1.0, r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3], u, v };
    [v(x0, y0, 0.0, 0.0), v(x1, y0, 1.0, 0.0), v(x1, y1, 1.0, 1.0), v(x0, y1, 0.0, 1.0)]
}

fn base() -> DrawState {
    DrawState {
        filter: 1,
        address_u: 3,
        address_v: 3,
        src_blend: 5,
        dst_blend: 6,
        cull: 1,
        alpha_func: 1,
        alpha_ref: 10.0 / 255.0,
        xform: [2.0 / SIZE as f32, -2.0 / SIZE as f32, -1.0, 1.0],
        ..Default::default()
    }
}

fn render(backend: Backend) -> Vec<u8> {
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    // 2x2: red, green / blue, white (top row first).
    let texels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let tex = r.upload_texture(&gpu, 0, 2, 2, TexData::Rgba8, &[&texels], false);

    r.clear(Some([0.0, 0.0, 1.0, 1.0]), true);
    let fan = 5;
    r.draw_2d(&base(), fan, &quad(0.0, 0.0, 32.0, 32.0, [255, 0, 0, 255]), None);
    let textured = DrawState { texture: tex, ..base() };
    r.draw_2d(&textured, fan, &quad(32.0, 0.0, 64.0, 32.0, [255, 255, 255, 255]), None);
    // Half-transparent white over the blue clear, as an indexed triangle list.
    let blended = DrawState { blend: 1, ..base() };
    r.draw_2d(&blended, 3, &quad(0.0, 32.0, 32.0, 64.0, [255, 255, 255, 128]), Some(&[0, 1, 2, 0, 2, 3]));
    // Alpha 5 is below the reference of 10: nothing drawn.
    r.draw_2d(&blended, fan, &quad(32.0, 32.0, 64.0, 64.0, [255, 255, 0, 5]), None);
    // Depth-only clear mid-frame keeps the colour.
    r.clear(None, true);
    r.draw_2d(&base(), fan, &quad(60.0, 60.0, 64.0, 64.0, [0, 255, 0, 255]), None);

    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &view, (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    readback_finish(&gpu, rb).expect("readback")
}

fn px(img: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [img[i], img[i + 1], img[i + 2], img[i + 3]]
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

fn check(backend: Backend) {
    let img = render(backend);
    let expect = [
        ((8, 8), [255, 0, 0, 255]),       // untextured red
        ((40, 8), [255, 0, 0, 255]),      // texture top-left
        ((56, 8), [0, 255, 0, 255]),      // texture top-right
        ((40, 24), [0, 0, 255, 255]),     // texture bottom-left
        ((56, 24), [255, 255, 255, 255]), // texture bottom-right
        ((8, 48), [128, 128, 255, 255]), // 50 % white over blue (the frame keeps no alpha, like D3D's X8R8G8B8)
        ((40, 48), [0, 0, 255, 255]),     // alpha-tested away
        ((62, 62), [0, 255, 0, 255]),     // drawn after the depth-only clear
    ];
    for ((x, y), want) in expect {
        let got = px(&img, x, y);
        assert!(near(got, want), "{backend:?} pixel ({x},{y}): got {got:?}, want {want:?}");
    }
}

#[test]
fn im2d_vulkan() {
    check(Backend::Vulkan);
}

#[test]
fn im2d_gl() {
    check(Backend::Gl);
}

/// DXT textures: kept as BC on the GPU (8x4, whole blocks) or decoded on the CPU (2x2) — both
/// must sample exactly like revc-formats' decoder with nearest filtering.
fn dxt_case(backend: Backend, width: u32, height: u32, kind: u8, data: TexData, blocks: &[u8]) {
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let tex = r.upload_texture(&gpu, 0, width, height, data, &[blocks], false);
    // Blended over black without alpha test: the frame shows colour * alpha, which checks the
    // decoded alpha too (the frame itself keeps no alpha).
    let state = DrawState { texture: tex, alpha_func: 0, blend: 1, ..base() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    r.draw_2d(&state, 5, &quad(0.0, 0.0, 64.0, 64.0, [255, 255, 255, 255]), None);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    let want = revc_formats::dds::decode_dxt(kind, width, height, blocks).expect("decode");
    for ty in 0..height {
        for tx in 0..width {
            let (x, y) = ((tx * 2 + 1) * SIZE / (width * 2), (ty * 2 + 1) * SIZE / (height * 2));
            let i = ((ty * width + tx) * 4) as usize;
            let w = [want[i], want[i + 1], want[i + 2], want[i + 3]];
            let got = px(&img, x, y);
            // Hardware BC decoders round the interpolated colours their own way (a level or two).
            let w = [0, 1, 2].map(|c| (w[c] as u32 * w[3] as u32 / 255) as u8);
            assert!(got[..3].iter().zip(&w).all(|(a, b)| a.abs_diff(*b) <= 3), "{backend:?} {data:?} {width}x{height} texel ({tx},{ty}): got {got:?}, want rgb*a {w:?}");
        }
    }
}

fn dxt(backend: Backend) {
    // DXT1 block: red / blue endpoints (four-colour mode), every index once per row.
    let red: u16 = 0xF800;
    let blue: u16 = 0x001F;
    let mut b1 = Vec::new();
    b1.extend(red.to_le_bytes());
    b1.extend(blue.to_le_bytes());
    b1.extend(0xE4E4_E4E4u32.to_le_bytes());
    // Second block: punch-through mode (c0 <= c1), index 3 transparent black.
    b1.extend(blue.to_le_bytes());
    b1.extend(red.to_le_bytes());
    b1.extend(0x1B1B_1B1Bu32.to_le_bytes());
    dxt_case(backend, 8, 4, 1, TexData::Dxt1, &b1);
    // DXT3: explicit 4-bit alpha ramp, then the first colour block.
    let mut b3: Vec<u8> = (0..8u8).map(|i| (i * 2) | ((i * 2 + 1) << 4)).collect();
    b3.extend_from_slice(&b1[..8]);
    dxt_case(backend, 4, 4, 3, TexData::Dxt3, &b3);
    // 2x2 isn't whole blocks: decoded on the CPU.
    dxt_case(backend, 2, 2, 1, TexData::Dxt1, &b1[..8]);
}

#[test]
fn dxt_vulkan() {
    dxt(Backend::Vulkan);
}

#[test]
fn dxt_gl() {
    dxt(Backend::Gl);
}

/// Im3D: camera-space quads through the device's projection (wgpu depth range): the nearer quad
/// wins whatever the draw order, and the far one only shows where nothing is in front.
fn im3d(backend: Backend) {
    use revc_rw::frame::{Mat4, Vertex3D};
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let (n, f) = (1.0f32, 100.0f32);
    // As wgpudevice.cpp beginUpdate: view window 1x1, depth mapped to 0..1.
    let proj: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
    let quad3 = |x0: f32, y0: f32, x1: f32, y1: f32, z: f32, c: [u8; 4]| {
        let v = |x, y| Vertex3D { x, y, z, r: c[0], g: c[1], b: c[2], a: c[3], u: 0.0, v: 0.0 };
        [v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)]
    };
    let state = DrawState { z_test: 1, z_write: 1, alpha_func: 0, ..base() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    // At z, the view spans -z..z: red covers the middle, green (nearer) its left half.
    r.draw_3d(&state, 5, &quad3(-10.0, -10.0, 10.0, 10.0, 20.0, [255, 0, 0, 255]), None, &proj);
    r.draw_3d(&state, 5, &quad3(-5.0, -5.0, 0.0, 5.0, 10.0, [0, 255, 0, 255]), None, &proj);
    // Blue far behind, drawn last, covering the whole view.
    r.draw_3d(&state, 3, &quad3(-90.0, -90.0, 90.0, 90.0, 90.0, [0, 0, 255, 255]), Some(&[0, 1, 2, 0, 2, 3]), &proj);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    for ((x, y), want) in [((24, 32), [0, 255, 0, 255]), ((40, 32), [255, 0, 0, 255]), ((4, 4), [0, 0, 255, 255]), ((32, 20), [255, 0, 0, 255]), ((32, 10), [0, 0, 255, 255])] {
        assert_eq!(px(&img, x, y), want, "{backend:?} im3d pixel ({x},{y})");
    }
}

#[test]
fn im3d_vulkan() {
    im3d(Backend::Vulkan);
}

#[test]
fn im3d_gl() {
    im3d(Backend::Gl);
}

/// Lit geometry (gl3 default pipeline maths): prelit black + ambient * surfAmbient + directional
/// * surfDiffuse, clamped, times the material colour; two meshes as index ranges of one geometry.
fn mesh(backend: Backend) {
    use revc_rw::frame::{Mat4, MeshExtras, MeshVertex, ModelState};
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let (n, f) = (1.0f32, 100.0f32);
    let proj: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
    // Two quads facing the camera (normal -z) at z = 10, left and right of the centre.
    let v = |x: f32, y: f32| MeshVertex { pos: [x, y, 0.0], normal: [0.0, 0.0, -1.0], color: [0, 0, 0, 255], uv: [0.0, 0.0] };
    let verts = [v(-5.0, -5.0), v(0.0, -5.0), v(0.0, 5.0), v(-5.0, 5.0), v(0.0, -5.0), v(5.0, -5.0), v(5.0, 5.0), v(0.0, 5.0)];
    let indices = [0u16, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let geo = r.upload_geometry(&gpu, 0, &verts, &indices);
    let mut lights = [[0.0f32; 4]; 32];
    lights[0] = [1.0, 0.0, 0.0, 0.0]; // directional
    lights[2] = [0.0, 0.0, 1.0, 0.0]; // shining along +z, at the camera-facing quads
    lights[3] = [0.5, 0.0, 0.0, 1.0];
    let mut world = [0.0f32; 16];
    world[0] = 1.0;
    world[5] = 1.0;
    world[10] = 1.0;
    world[14] = 10.0;
    world[15] = 1.0;
    let model = ModelState { ambient: [0.2, 0.2, 0.2, 1.0], lights, ..ModelState::new(world) };
    let state = DrawState { z_test: 1, z_write: 1, alpha_func: 0, ..base() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    r.draw_mesh(&state, geo, 0, 6, &proj, &model, &MeshExtras::default());
    r.draw_mesh(&state, geo, 6, 6, &proj, &ModelState { mat_color: [0.5, 0.5, 0.5, 1.0], ..model }, &MeshExtras::default());
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    // 0.2 + 0.5 = 0.7 red, 0.2 green/blue; right quad halved.
    for ((x, y), want) in [((20, 32), [179, 51, 51, 255]), ((44, 32), [89, 26, 26, 255]), ((2, 2), [0, 0, 0, 255])] {
        assert!(near(px(&img, x, y), want), "{backend:?} mesh pixel ({x},{y}): got {:?}, want {want:?}", px(&img, x, y));
    }
}

#[test]
fn mesh_vulkan() {
    mesh(Backend::Vulkan);
}

#[test]
fn mesh_gl() {
    mesh(Backend::Gl);
}

/// Skinning: a quad bound fully to bone 1 is moved by that bone's matrix (gl3 skin.vert), and
/// half-and-half weights blend the two bones.
fn skin(backend: Backend) {
    use revc_rw::frame::{Mat4, MeshExtras, MeshVertex, ModelState, SkinVertex};
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let (n, f) = (1.0f32, 100.0f32);
    let proj: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
    let v = |x: f32, y: f32, w: [f32; 4]| SkinVertex { base: MeshVertex { pos: [x, y, 0.0], normal: [0.0, 0.0, -1.0], color: [255, 255, 255, 255], uv: [0.0, 0.0] }, weights: w, indices: [0, 1, 0, 0] };
    let one = [0.0, 1.0, 0.0, 0.0];
    let half = [0.5, 0.5, 0.0, 0.0];
    // Left quad on bone 1 (moved +5 in x => ends up right of centre); a thin strip halfway.
    let verts = [v(-5.0, -5.0, one), v(-1.0, -5.0, one), v(-1.0, 5.0, one), v(-5.0, 5.0, one), v(-5.0, -5.0, half), v(-4.0, -5.0, half), v(-4.0, 5.0, half), v(-5.0, 5.0, half)];
    let indices = [0u16, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let geo = r.upload_skinned_geometry(&gpu, 0, &verts, &indices);
    let ident: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut moved = ident;
    moved[12] = 6.0;
    let mut world = ident;
    world[14] = 10.0;
    let model = ModelState::new(world);
    let state = DrawState { z_test: 1, z_write: 0, alpha_func: 0, ..base() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    r.draw_mesh(&state, geo, 0, 6, &proj, &model, &MeshExtras { bones: Some(&[ident, moved]), env: None });
    r.draw_mesh(&state, geo, 6, 6, &proj, &ModelState { mat_color: [0.0, 1.0, 0.0, 1.0], ..model }, &MeshExtras { bones: Some(&[ident, moved]), env: None });
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    // Bone 1 moved x -5..-1 to 1..5 (pixels ~35..48); the half-weighted strip -5..-4 to -2..-1
    // (pixels ~22..29); the original place of the quad (-5..-1 => ~16..29 minus the strip) is empty.
    for ((x, y), want) in [((42, 32), [255, 255, 255, 255]), ((26, 32), [0, 255, 0, 255]), ((18, 32), [0, 0, 0, 255])] {
        assert_eq!(px(&img, x, y), want, "{backend:?} skin pixel ({x},{y})");
    }
}

#[test]
fn skin_vulkan() {
    skin(Backend::Vulkan);
}

#[test]
fn skin_gl() {
    skin(Backend::Gl);
}

/// MatFX environment map (gl3 matfx_env): base pass black, env texel chosen by the env matrix
/// and scaled by the coefficient, added on top (source blend ONE).
fn env(backend: Backend) {
    use revc_rw::frame::{EnvMap, Mat4, MeshExtras, MeshVertex, ModelState};
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let (n, f) = (1.0f32, 100.0f32);
    let proj: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, f / (f - n), 1.0, 0.0, 0.0, -n * f / (f - n), 0.0];
    let texels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let env_tex = r.upload_texture(&gpu, 0, 2, 2, TexData::Rgba8, &[&texels], false);
    let v = |x: f32, y: f32| MeshVertex { pos: [x, y, 0.0], normal: [0.0, 0.0, -1.0], color: [0, 0, 0, 255], uv: [0.0, 0.0] };
    let verts = [v(-5.0, -5.0), v(0.0, -5.0), v(0.0, 5.0), v(-5.0, 5.0), v(0.0, -5.0), v(5.0, -5.0), v(5.0, 5.0), v(0.0, 5.0)];
    let geo = r.upload_geometry(&gpu, 0, &verts, &[0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
    let mut world = [0.0f32; 16];
    world[0] = 1.0;
    world[5] = 1.0;
    world[10] = 1.0;
    world[14] = 10.0;
    world[15] = 1.0;
    // The env matrix ignores the normal and lands on a fixed texel (its translation column).
    let mut texm = [0.0f32; 16];
    texm[12] = 0.25;
    texm[13] = 0.25;
    texm[15] = 1.0;
    let first = ModelState { surf_props: [0.0, 0.0, 0.0, 0.0], tex_matrix: texm, color_clamp: [1.0; 4], env_color: [1.0; 4], fx_params: [1.0, 0.0, 0.0, 0.0], ..ModelState::new(world) };
    let mut texm2 = texm;
    texm2[12] = 0.75;
    let second = ModelState { tex_matrix: texm2, fx_params: [0.5, 0.0, 0.0, 0.0], ..first };
    let e = MeshExtras { bones: None, env: Some(EnvMap { texture: env_tex, filter: 1, address_u: 1, address_v: 1 }) };
    let state = DrawState { z_test: 1, z_write: 1, alpha_func: 0, blend: 1, src_blend: 2, dst_blend: 6, ..base() };
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    r.draw_mesh(&state, geo, 0, 6, &proj, &first, &e);
    r.draw_mesh(&state, geo, 6, 6, &proj, &second, &e);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    for ((x, y), want) in [((24, 32), [255, 0, 0, 255]), ((40, 32), [0, 128, 0, 255])] {
        assert!(near(px(&img, x, y), want), "{backend:?} env pixel ({x},{y}): got {:?}, want {want:?}", px(&img, x, y));
    }
}

#[test]
fn env_vulkan() {
    env(Backend::Vulkan);
}

#[test]
fn env_gl() {
    env(Backend::Gl);
}

/// Frame copy (librw rasterRenderFast of the camera into a camera texture, as CMBlur does): the
/// frame so far lands in the texture top row first and can be drawn back after a clear.
fn copy_frame(backend: Backend) {
    let gpu = Gpu::headless(backend).expect("gpu");
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    let blank = vec![0u8; (SIZE * SIZE * 4) as usize];
    let tex = r.upload_texture(&gpu, 0, SIZE, SIZE, TexData::Rgba8, &[&blank], false);
    r.clear(Some([0.0, 0.0, 0.0, 1.0]), true);
    r.draw_2d(&base(), 5, &quad(0.0, 0.0, 32.0, 16.0, [255, 0, 0, 255]), None);
    r.copy_frame(tex, 0, 0);
    r.clear(Some([0.0, 0.0, 1.0, 1.0]), true);
    r.draw_2d(&DrawState { texture: tex, ..base() }, 5, &quad(0.0, 0.0, 64.0, 64.0, [255, 255, 255, 255]), None);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    r.encode(&gpu, &mut enc, &target.create_view(&Default::default()), (SIZE, SIZE));
    r.end_frame();
    let rb = readback_start(&gpu, &mut enc, &target, (SIZE, SIZE));
    gpu.queue.submit([enc.finish()]);
    let img = readback_finish(&gpu, rb).expect("readback");
    for ((x, y), want) in [((8, 4), [255, 0, 0, 255]), ((40, 4), [0, 0, 0, 255]), ((8, 40), [0, 0, 0, 255])] {
        assert_eq!(px(&img, x, y), want, "{backend:?} copy pixel ({x},{y})");
    }
}

#[test]
fn copy_frame_vulkan() {
    copy_frame(Backend::Vulkan);
}

#[test]
fn copy_frame_gl() {
    copy_frame(Backend::Gl);
}

//! Im2D through revc_rw::frame::Renderer on both backends, offscreen: clear, untextured and
//! textured quads (texture orientation, nearest filtering), RW alpha blending and alpha test,
//! and a camera clear in the middle of a frame.

use revc_rw::frame::{DrawState, Renderer, Vertex2D, readback_finish, readback_start};
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
    let tex = r.upload_texture(&gpu, 0, 2, 2, &[&texels], false);

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
        ((8, 8), [255, 0, 0, 255]),      // untextured red
        ((40, 8), [255, 0, 0, 255]),     // texture top-left
        ((56, 8), [0, 255, 0, 255]),     // texture top-right
        ((40, 24), [0, 0, 255, 255]),    // texture bottom-left
        ((56, 24), [255, 255, 255, 255]), // texture bottom-right
        ((8, 48), [128, 128, 255, 191]), // 50 % white over blue; alpha blends with the same factors (glBlendFunc)
        ((40, 48), [0, 0, 255, 255]),    // alpha-tested away
        ((62, 62), [0, 255, 0, 255]),    // drawn after the depth-only clear
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

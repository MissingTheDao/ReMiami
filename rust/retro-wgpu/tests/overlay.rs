//! Output-resolution overlay (`Renderer::begin_overlay`): a HUD drawn sharp at the output's
//! resolution on top of a scene that is drawn at a lower internal resolution, supersampled and
//! post-processed; the output curve applies to both; screenshots (`final_image`) include it.

mod common;
use common::*;
use retro_wgpu::*;

macro_rules! per_backend {
    ($name:ident) => {
        mod $name {
            #[test]
            fn vulkan() {
                super::$name(retro_wgpu::Backend::Vulkan);
            }
            #[test]
            fn gl() {
                super::$name(retro_wgpu::Backend::Gl);
            }
            #[cfg(windows)]
            #[test]
            fn dx12() {
                super::$name(retro_wgpu::Backend::Dx12);
            }
        }
    };
}

const OUT: (u32, u32) = (64, 64);

fn overlay(backend: Backend) {
    // Scene at 16x16, 2x supersampled, FXAA, shown 4x with bilinear filtering.
    let Some(mut c) = ctx(backend, OUT, SceneSettings { internal_size: Some((16, 16)), supersample: 2, msaa: 4 }) else { return };
    c.r.set_post_chain(vec![Box::new(Fxaa::new(&c.gpu))]);
    c.r.output = OutputSettings { mode: ScaleMode::Stretch, filter: ScaleFilter::Bilinear, gamma: 2.0, ..Default::default() };
    let l = c.layout(PC);
    let st = RenderState::default();
    let p = DrawParams::default();
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    // A white triangle over the lower left half (diagonal edge).
    let mut w = VertexWriter::new();
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0)] {
        w.f32s(&[x, y, 0.5]).u8x4([255, 255, 255, 255]);
    }
    c.draw(l, Topology::TriangleList, &w.bytes, Indices::None, &st, &p);
    // A filtered copy of the 32x32 (supersampled) scene at internal size, into the corner of a
    // larger texture.
    let copy = c.r.upload_texture(&c.gpu, 24, 24, TexFormat::Rgba8, &[&[0u8; 24 * 24 * 4]], None, false).unwrap();
    c.r.copy_to_texture_scaled(Target::Scene, copy, [4, 4], [16, 16]);
    c.r.begin_overlay();
    // The overlay's depth starts cleared: a quad at depth 0.9 isn't hidden by the scene's 0.5.
    // Red HUD bar 2 output pixels wide (x 40..42) and a mid-grey one (x 50..52, gamma check).
    let px = |x: f32| x / 32.0 - 1.0;
    c.draw(l, Topology::TriangleFan, &quad_pc(px(40.0), -1.0, px(42.0), 1.0, 0.9, [255, 0, 0, 255]), Indices::None, &st, &p);
    c.draw(l, Topology::TriangleFan, &quad_pc(px(50.0), -1.0, px(52.0), 1.0, 0.9, [64, 64, 64, 255]), Indices::None, &st, &p);
    let shown = c.frame();
    // The HUD's edges are hard at output pixels.
    for y in [4, 30, 60] {
        expect_px(&shown, 40, y, [255, 0, 0, 255], 0, "HUD inside");
        expect_px(&shown, 41, y, [255, 0, 0, 255], 0, "HUD inside");
        let left = shown.pixel(39, y);
        let right = shown.pixel(42, y);
        assert!(left[1] == left[0] && right[1] == right[0], "{backend:?}: HUD edge not hard at y {y}: {left:?} {right:?}");
    }
    // Gamma 2 applies to the overlay too: 64/255 -> sqrt -> ~128.
    expect_px(&shown, 50, 30, [128, 128, 128, 255], 2, "HUD through the output curve");
    // The scene edge is soft (internal resolution, filtered).
    let soft = (0..64).flat_map(|y| (0..36).map(move |x| (x, y))).filter(|&(x, y)| {
        let v = shown.pixel(x, y)[0];
        v > 20 && v < 235
    });
    assert!(soft.count() > 30, "{backend:?}: scene edge should be soft");
    // final_image: output size, with the overlay, before the curve.
    let fi = c.r.final_image().unwrap();
    assert_eq!((fi.width(), fi.height()), OUT);
    let mut enc = c.gpu.device.create_command_encoder(&Default::default());
    let rb = readback::readback_start(&c.gpu, &mut enc, fi);
    c.gpu.queue.submit([enc.finish()]);
    let img = readback::readback_finish(&c.gpu, rb).unwrap();
    expect_px(&img, 41, 10, [255, 0, 0, 255], 0, "final image has the HUD");
    expect_px(&img, 51, 10, [64, 64, 64, 255], 0, "final image before the curve");

    let mut enc = c.gpu.device.create_command_encoder(&Default::default());
    let rb = readback::readback_start(&c.gpu, &mut enc, c.r.texture(copy).unwrap());
    c.gpu.queue.submit([enc.finish()]);
    let cp = readback::readback_finish(&c.gpu, rb).unwrap();
    expect_px(&cp, 2, 2, [0, 0, 0, 0], 0, "copy: outside the rectangle untouched");
    expect_px(&cp, 5, 18, [255, 255, 255, 255], 1, "copy: triangle (lower left)");
    expect_px(&cp, 18, 5, [0, 0, 0, 255], 1, "copy: background (upper right)");
    expect_px(&cp, 21, 21, [0, 0, 0, 0], 0, "copy: below/right of the rectangle untouched");

    // Next frame without an overlay: the plain path again (no HUD).
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleList, &w.bytes, Indices::None, &st, &p);
    let plain = c.frame();
    assert_ne!(plain.pixel(41, 10), [255, 0, 0, 255], "{backend:?}: overlay must not stay on");
    let fi = c.r.final_image().unwrap();
    assert_eq!((fi.width(), fi.height()), (16, 16), "{backend:?}: final image at internal size without an overlay");
}
per_backend!(overlay);

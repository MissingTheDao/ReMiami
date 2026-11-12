//! Stage 3: slang presets as the output stage (feature "slang"). The presets in tests/slang
//! are tiny shaders written for these tests.
#![cfg(feature = "slang")]

mod common;
use common::*;
use retro_wgpu::slang::{SlangOutput, SlangPlacement};
use retro_wgpu::*;
use std::path::PathBuf;

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

fn preset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/slang").join(name)
}

fn game_frame(c: &mut Ctx, rgb: [u8; 3]) -> Image {
    let l = c.layout(PC);
    c.r.clear(Some([0.0, 0.0, 0.0, 1.0]), Some(1.0), Some(0));
    c.draw(l, Topology::TriangleFan, &quad_pc(-1.0, -1.0, 0.0, 1.0, 0.5, [rgb[0], rgb[1], rgb[2], 255]), Indices::None, &RenderState::default(), &DrawParams::default());
    c.frame()
}

/// A one-pass preset on the game image (16x16 internal) placed 4:3 inside a 64x48 output.
fn invert(backend: Backend) {
    let Some(mut c) = ctx(backend, (64, 48), SceneSettings { internal_size: Some((16, 16)), ..Default::default() }) else { return };
    let mut s = SlangOutput::load(&c.gpu, &preset("invert.slangp")).expect("preset");
    s.placement = SlangPlacement::GameRect(OutputSettings { mode: ScaleMode::Integer, ..Default::default() });
    c.r.set_output_stage(Some(Box::new(s)));
    let img = game_frame(&mut c, [255, 0, 0]);
    // 3x integer: 48x48 at x = 8. Left half of the game image red -> cyan, right black -> white.
    expect_px(&img, 12, 24, [0, 255, 255, 255], 1, "inverted red");
    expect_px(&img, 50, 24, [255, 255, 255, 255], 1, "inverted black");
    expect_px(&img, 3, 24, [0, 0, 0, 0], 0, "outside the rect");
    golden(&img, "slang_invert", backend);
}
per_backend!(invert);

/// Frame history and parameters: the output shows the previous frame's original image.
fn history(backend: Backend) {
    let Some(mut c) = ctx(backend, (32, 32), SceneSettings { internal_size: Some((8, 8)), ..Default::default() }) else { return };
    let s = SlangOutput::load(&c.gpu, &preset("history.slangp")).expect("preset");
    assert!(s.parameters().iter().any(|(k, v)| k == "GAIN" && *v == 1.0), "{:?}", s.parameters());
    assert_eq!(s.set_parameter("GAIN", 0.5), Some(1.0));
    c.r.set_output_stage(Some(Box::new(s)));
    let _ = game_frame(&mut c, [255, 0, 0]);
    let img = game_frame(&mut c, [0, 255, 0]);
    // Previous frame: red in the left half, times GAIN 0.5.
    expect_px(&img, 4, 16, [128, 0, 0, 255], 2, "history of frame 1");
    expect_px(&img, 28, 16, [0, 0, 0, 255], 2, "history right half");
}
per_backend!(history);

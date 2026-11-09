//! The same model rendered on Vulkan and on OpenGL must look the same (plan task 12: both
//! backends selectable). Uses the whole textured Infernus (all default parts placed by their
//! frames, infernus.txd) from the game's gta3.img; skipped without a game install (REVC_GAME_DIR or game/ at the repository root). Images are written to
//! rust/target/test-output/revc-rw-<backend>.png.

use revc_formats::{dff, img, txd};
use revc_rw::{Backend, Gpu, Model, render_offscreen};
use std::path::{Path, PathBuf};

const SIZE: u32 = 256;
const BACKGROUND: [u8; 3] = [26, 31, 41]; // the clear colour (0.1, 0.12, 0.16), rounded

fn game_dir() -> Option<PathBuf> {
    let d = std::env::var_os("REVC_GAME_DIR").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game"));
    d.join("models/gta3.img").is_file().then_some(d)
}

fn render(backend: Backend, model: &Model) -> Vec<u8> {
    let gpu = Gpu::headless(backend).unwrap_or_else(|e| panic!("{e}"));
    eprintln!("{backend:?}: {} ({:?}, {})", gpu.info.name, gpu.info.backend, gpu.info.driver);
    let rgba = render_offscreen(&gpu, model, SIZE);
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-output");
    std::fs::create_dir_all(&out).unwrap();
    let file = std::fs::File::create(out.join(format!("revc-rw-{backend:?}.png").to_lowercase())).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SIZE, SIZE);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    rgba
}

fn model_pixels(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().filter(|p| (0..3).any(|c| p[c].abs_diff(BACKGROUND[c]) > 4)).count()
}

#[test]
fn vulkan_and_gl_agree() {
    let Some(game) = game_dir() else {
        eprintln!("SKIPPED: no game install");
        return;
    };
    let a = img::Archive::open(game.join("models/gta3.img")).unwrap();
    let clump = dff::parse(&a.read(a.find("infernus.dff").unwrap()).unwrap()).unwrap();
    let dict = txd::parse(&a.read(a.find("infernus.txd").unwrap()).unwrap()).unwrap();
    let model = Model::from_clump(&clump, Some(&dict)).unwrap();
    assert!(!model.images.is_empty(), "no textures bound");
    eprintln!("{} vertices, {} batches, {} textures", model.vertices.len(), model.batches.len(), model.images.len());

    let vk = render(Backend::Vulkan, &model);
    let gl = render(Backend::Gl, &model);
    let total = (SIZE * SIZE) as usize;
    for (name, img) in [("Vulkan", &vk), ("GL", &gl)] {
        let covered = model_pixels(img) as f64 / total as f64;
        assert!(covered > 0.10, "{name}: model covers only {:.1}% of the image", covered * 100.0);
    }
    let differing = vk.as_chunks::<4>().0.iter().zip(gl.chunks_exact(4)).filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 32)).count();
    let frac = differing as f64 / total as f64;
    eprintln!("Vulkan vs GL: {:.3}% of pixels differ", frac * 100.0);
    assert!(frac < 0.01, "Vulkan and GL images differ in {:.2}% of pixels", frac * 100.0);
}

//! txdexport <file.txd | archive.img:name.txd> <outdir>
//!
//! Writes mip 0 of every texture as an RGBA PNG (<outdir>/<texture>.png, top row first) and
//! prints one line per texture: name, size, depth, DXT kind, palette size, alpha flag and the
//! share of fully transparent and partially transparent pixels. Used by
//! scripts/upscale_hud.py (docs/HUD_UPSCALING.md).
use revc_formats::{img, txd};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: txdexport <file.txd | archive.img:name.txd> <outdir>");
        std::process::exit(2);
    }
    let (src, out) = (&args[1], Path::new(&args[2]));
    let data = if let Some((arc, name)) = src.split_once(".img:") {
        let archive = img::Archive::open(format!("{arc}.img")).unwrap_or_else(|e| panic!("{arc}.img: {e:?}"));
        let entry = archive.find(name).unwrap_or_else(|| panic!("{name} not in {arc}.img"));
        archive.read(entry).unwrap_or_else(|e| panic!("{name}: {e:?}"))
    } else {
        std::fs::read(src).unwrap_or_else(|e| panic!("{src}: {e}"))
    };
    let dict = txd::parse(&data).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    std::fs::create_dir_all(out).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    for t in &dict.textures {
        let px = t.rgba(0).unwrap_or_else(|e| panic!("{}: {e:?}", t.name));
        let n = (px.len() / 4).max(1);
        let zero = px.as_chunks::<4>().0.iter().filter(|p| p[3] == 0).count();
        let partial = px.as_chunks::<4>().0.iter().filter(|p| p[3] > 0 && p[3] < 255).count();
        println!(
            "{:24} {:>4}x{:<4} d{:<2} dxt{} pal{:<3} alpha:{} zero {:.0}% partial {:.0}%",
            t.name,
            t.width,
            t.height,
            t.depth,
            t.compression,
            t.palette.len(),
            t.has_alpha,
            100.0 * zero as f64 / n as f64,
            100.0 * partial as f64 / n as f64
        );
        let path = out.join(format!("{}.png", t.name));
        let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut enc = png::Encoder::new(file, t.width as u32, t.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().and_then(|mut w| w.write_image_data(&px)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

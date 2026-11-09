use revc_formats::{img, txd};
use std::collections::BTreeMap;
fn main() {
    let g = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game");
    let a = img::Archive::open(g.join("models/gta3.img")).unwrap();
    let mut h = a.open_img().unwrap();
    let mut c: BTreeMap<String, usize> = BTreeMap::new();
    for e in a.entries.iter().filter(|e| e.extension() == "txd") {
        let t = txd::parse(&a.read_with(&mut h, e).unwrap()).unwrap();
        for x in &t.textures {
            *c.entry(format!("fmt {:#x} pal8 {} pal4 {} depth {} dxt {}", x.pixel_format(), x.raster_format & txd::raster::PAL8 != 0, x.raster_format & txd::raster::PAL4 != 0, x.depth, x.compression)).or_default() += 1;
        }
    }
    for (k, v) in c { println!("{v:6} {k}"); }
}

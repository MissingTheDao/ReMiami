//! Decodes mip 0 of every texture in gta3.img and the loose TXDs; writes one to a PNG.
use revc_formats::{img, txd};
fn main() {
    let g = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game");
    let a = img::Archive::open(g.join("models/gta3.img")).unwrap();
    let mut h = a.open_img().unwrap();
    let (mut ok, mut bad) = (0, Vec::new());
    let mut all: Vec<(String, txd::Txd)> = a.entries.iter().filter(|e| e.extension() == "txd")
        .map(|e| (e.name.clone(), txd::parse(&a.read_with(&mut h, e).unwrap()).unwrap())).collect();
    for f in std::fs::read_dir(g.join("models")).unwrap().flatten() {
        let p = f.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txd"))
            && let Ok(t) = txd::parse(&std::fs::read(&p).unwrap()) { all.push((p.file_name().unwrap().to_string_lossy().into(), t)); }
    }
    for (name, t) in &all {
        for x in &t.textures {
            match x.rgba(0) { Ok(px) if px.len() == (x.width as usize * x.height as usize * 4) => ok += 1, r => bad.push(format!("{name}/{}: {:?}", x.name, r.err())) }
        }
    }
    println!("decoded {ok}, failed {}", bad.len());
    for b in bad.iter().take(5) { println!("  {b}"); }
    let (_, inf) = all.iter().find(|(n, _)| n.eq_ignore_ascii_case("infernus.txd")).unwrap();
    let t = inf.find(&std::env::args().nth(1).unwrap_or("infernus86body128a".into())).unwrap();
    let px = t.rgba(0).unwrap();
    let mut e = png::Encoder::new(std::fs::File::create(std::env::args().nth(2).unwrap_or("/tmp/tex.png".into())).unwrap(), t.width as u32, t.height as u32);
    e.set_color(png::ColorType::Rgba); e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px).unwrap();
}

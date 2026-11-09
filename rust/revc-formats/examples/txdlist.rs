//! Lists the textures of a TXD: name, size, depth, compression, mip levels.
//! usage: cargo run -p revc-formats --example txdlist -- game/models/hud.txd
//!        cargo run -p revc-formats --example txdlist -- game/models/gta3.img:infernus.txd
use revc_formats::{img, txd};
fn main() {
    let arg = std::env::args().nth(1).expect("usage: txdlist <file.txd | archive.img:name.txd>");
    let data = match arg.split_once(".img:") {
        Some((archive, name)) => {
            let a = img::Archive::open(format!("{archive}.img")).expect("open archive");
            a.read(a.find(name).unwrap_or_else(|| panic!("{name} not in {archive}.img"))).expect("read")
        }
        None => std::fs::read(&arg).expect("read"),
    };
    let t = txd::parse(&data).expect("parse");
    for x in &t.textures {
        println!("{:<32} {:>4}x{:<4} depth {:>2} dxt {} levels {}", x.name, x.width, x.height, x.depth, x.compression, x.levels.len());
    }
}

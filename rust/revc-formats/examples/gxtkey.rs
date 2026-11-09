//! Prints GXT strings: cargo run -p revc-formats --example gxtkey -- file.gxt KEY [KEY...]
use revc_formats::gxt;
fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("usage: gxtkey <file.gxt> KEY...");
    let g = gxt::Gxt::parse(&std::fs::read(&file).expect("read")).expect("parse");
    let main = g.main().expect("no MAIN table");
    for k in args {
        println!("{k}: {:?}", main.get(&k).map(gxt::to_lossy_string));
    }
}

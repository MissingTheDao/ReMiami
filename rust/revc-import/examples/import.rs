//! Imports the manifest's files from a disc image into a folder, as the game's CONSOLE DISCS entry
//! does (for tests and checks): `cargo run -p revc-import --example import -- <image> <dir>`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: import <disc image> <cache dir>");
        std::process::exit(2);
    }
    let image = std::path::Path::new(&args[1]);
    let mut disc = match revc_import::Disc::open(image) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    match revc_import::import(&mut disc, image, std::path::Path::new(&args[2])) {
        Ok(r) => println!("{} files", r.written.len()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

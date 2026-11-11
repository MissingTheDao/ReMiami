//! Lists a console disc image's files (path and size), optionally only those whose path contains
//! a filter, and with `--extract <dir>` copies the matches there (for research; the copies are the
//! player's own data and stay local).
//!
//! usage: cargo run -p revc-import --example list -- <image> [filter] [--extract <dir>]

use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(image) = args.first() else {
        eprintln!("usage: list <image> [filter] [--extract <dir>]");
        std::process::exit(2);
    };
    let mut filter = None;
    let mut extract: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--extract" {
            extract = args.get(i + 1).map(PathBuf::from);
            i += 2;
        } else {
            filter = Some(args[i].to_ascii_lowercase());
            i += 1;
        }
    }
    let mut disc = match revc_import::Disc::open(Path::new(image)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let matches: Vec<_> = disc
        .entries
        .iter()
        .filter(|e| filter.as_ref().is_none_or(|f| e.path.to_ascii_lowercase().contains(f.as_str())))
        .cloned()
        .collect();
    for e in &matches {
        println!("{:>10}  {}", e.size, e.path);
        if let Some(dir) = &extract {
            let out = dir.join(e.path.to_ascii_lowercase());
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match disc.read(e) {
                Ok(data) => {
                    if let Err(err) = std::fs::write(&out, data) {
                        eprintln!("{}: {err}", out.display());
                    }
                }
                Err(err) => eprintln!("{}: {err}", e.path),
            }
        }
    }
}

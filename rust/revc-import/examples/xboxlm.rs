//! Builds the Xbox lightmap sidecar from an extracted gta3.dir/gta3.img (research; the import
//! does this from the disc) and prints a summary.
//!
//! usage: cargo run -p revc-import --example xboxlm -- <gta3.dir> <out.bin> <out.txd> [model]

use std::io::{Read, Seek, SeekFrom};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        eprintln!("usage: xboxlm <gta3.dir> <out.bin> <out.txd> [model]");
        std::process::exit(2);
    }
    let dir = std::fs::read(&a[0]).expect("dir");
    let mut img = std::fs::File::open(a[0].replace(".dir", ".img")).expect("img");
    if let Some(model) = a.get(3) {
        for e in dir.chunks_exact(32) {
            let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap()).to_ascii_lowercase();
            if name != format!("{}.dff", model.to_ascii_lowercase()) {
                continue;
            }
            let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
            let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
            let mut raw = vec![0; size];
            img.seek(SeekFrom::Start(off)).unwrap();
            img.read_exact(&mut raw).unwrap();
            let data = revc_import::xboxlm::img_entry(&raw).unwrap();
            for (i, g) in revc_import::xboxlm::dff_lightmaps(&data).iter().enumerate() {
                println!("geo {i}: {} uv1, lightmaps {:?}, first {:?}", g.uv1.len(), g.lightmaps, g.uv1.iter().take(3).collect::<Vec<_>>());
            }
        }
    }
    let generic = std::fs::read(a[0].replace("gta3.dir", "generic.txd")).unwrap_or_default();
    let (bin, txd, n) = revc_import::xboxlm::img_sidecar(&dir, &[&generic], |off, size| {
        let mut b = vec![0; size];
        img.seek(SeekFrom::Start(off))?;
        img.read_exact(&mut b)?;
        Ok(b)
    })
    .expect("sidecar");
    std::fs::write(&a[1], &bin).unwrap();
    std::fs::write(&a[2], &txd).unwrap();
    let count = u16::from_le_bytes(txd[24..26].try_into().unwrap());
    println!("{n} models with lightmaps, {} bytes; txd {count} lightmaps, {} bytes", bin.len(), txd.len());
}

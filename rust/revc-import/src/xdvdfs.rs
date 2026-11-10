//! Xbox DVD file system (XDVDFS) as on Xbox game discs: a volume descriptor 0x10000 bytes into
//! the game partition, then directories as binary trees of entries. Redump images of dual-layer
//! discs start with the video partition, so the game partition can sit at a few known offsets.

use crate::Entry;
use std::io::{Read, Seek, SeekFrom};

const MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
const SECTOR: u64 = 2048;
/// Where the game partition starts in the common image layouts (plain XISO, redump images).
const BASES: [u64; 4] = [0, 0x1830_0000, 0x0FD9_0000, 0x0208_0000];

/// The file list of an XDVDFS image, or None if `f` isn't one.
pub fn read_tree<R: Read + Seek>(f: &mut R) -> std::io::Result<Option<Vec<Entry>>> {
    for base in BASES {
        let mut magic = [0u8; 20];
        if f.seek(SeekFrom::Start(base + 0x10000)).is_err() || f.read_exact(&mut magic).is_err() || &magic != MAGIC {
            continue;
        }
        let mut root = [0u8; 8];
        f.read_exact(&mut root)?;
        let sector = u32::from_le_bytes(root[0..4].try_into().unwrap()) as u64;
        let size = u32::from_le_bytes(root[4..8].try_into().unwrap()) as u64;
        let mut out = Vec::new();
        walk(f, base, sector, size, "", &mut out, 0)?;
        out.sort_by(|a, b| a.path.cmp(&b.path));
        return Ok(Some(out));
    }
    Ok(None)
}

fn walk<R: Read + Seek>(f: &mut R, base: u64, sector: u64, size: u64, dir: &str, out: &mut Vec<Entry>, depth: u32) -> std::io::Result<()> {
    if depth > 32 || size > 64 << 20 {
        return Ok(()); // corrupt image; don't loop or allocate without bound
    }
    let mut data = vec![0u8; size as usize];
    f.seek(SeekFrom::Start(base + sector * SECTOR))?;
    f.read_exact(&mut data)?;
    let mut stack = vec![0usize];
    let mut seen = std::collections::HashSet::new();
    while let Some(o) = stack.pop() {
        let p = o * 4;
        if !seen.insert(o) || p + 14 > data.len() {
            continue;
        }
        let left = u16::from_le_bytes([data[p], data[p + 1]]);
        let right = u16::from_le_bytes([data[p + 2], data[p + 3]]);
        if left == 0xFFFF {
            continue; // padding
        }
        let start = u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()) as u64;
        let fsize = u32::from_le_bytes(data[p + 8..p + 12].try_into().unwrap()) as u64;
        let attr = data[p + 12];
        let nl = data[p + 13] as usize;
        let Some(name) = data.get(p + 14..p + 14 + nl) else { continue };
        let name: String = name.iter().map(|&c| c as char).collect();
        if left != 0 {
            stack.push(left as usize);
        }
        if right != 0 {
            stack.push(right as usize);
        }
        let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        if attr & 0x10 != 0 {
            if fsize != 0 {
                walk(f, base, start, fsize, &path, out, depth + 1)?;
            }
        } else {
            out.push(Entry { path, offset: base + start * SECTOR, size: fsize });
        }
    }
    Ok(())
}

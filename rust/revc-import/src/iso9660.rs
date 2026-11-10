//! ISO 9660 as on PS2 DVDs and CDs: the primary volume descriptor at sector 16, directory
//! records that never cross a sector boundary.

use crate::Entry;
use std::io::{Read, Seek, SeekFrom};

const SECTOR: u64 = 2048;

/// The file list of an ISO 9660 image, or None if `f` isn't one.
pub fn read_tree<R: Read + Seek>(f: &mut R) -> std::io::Result<Option<Vec<Entry>>> {
    let mut pvd = [0u8; 2048];
    if f.seek(SeekFrom::Start(16 * SECTOR)).is_err() || f.read_exact(&mut pvd).is_err() || pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return Ok(None);
    }
    let root = &pvd[156..190];
    let lba = u32::from_le_bytes(root[2..6].try_into().unwrap()) as u64;
    let size = u32::from_le_bytes(root[10..14].try_into().unwrap()) as u64;
    let mut out = Vec::new();
    walk(f, lba, size, "", &mut out, 0)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(out))
}

/// The volume identifier (PVD bytes 40..72), trimmed.
pub fn volume_id<R: Read + Seek>(f: &mut R) -> std::io::Result<String> {
    let mut pvd = [0u8; 2048];
    f.seek(SeekFrom::Start(16 * SECTOR))?;
    f.read_exact(&mut pvd)?;
    Ok(String::from_utf8_lossy(&pvd[40..72]).trim().to_string())
}

fn walk<R: Read + Seek>(f: &mut R, lba: u64, size: u64, dir: &str, out: &mut Vec<Entry>, depth: u32) -> std::io::Result<()> {
    if depth > 32 || size > 64 << 20 {
        return Ok(());
    }
    let mut data = vec![0u8; size as usize];
    f.seek(SeekFrom::Start(lba * SECTOR))?;
    f.read_exact(&mut data)?;
    let mut p = 0usize;
    while p < data.len() {
        let len = data[p] as usize;
        if len == 0 {
            // Rest of this sector is padding.
            p = (p / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        if p + len > data.len() || len < 34 {
            break;
        }
        let r = &data[p..p + len];
        let extent = u32::from_le_bytes(r[2..6].try_into().unwrap()) as u64;
        let fsize = u32::from_le_bytes(r[10..14].try_into().unwrap()) as u64;
        let flags = r[25];
        let nl = r[32] as usize;
        let name = &r[33..(33 + nl).min(len)];
        p += len;
        if name == [0] || name == [1] {
            continue; // "." and ".."
        }
        let mut name = String::from_utf8_lossy(name).into_owned();
        if let Some(i) = name.find(';') {
            name.truncate(i);
        }
        let name = name.trim_end_matches('.').to_string();
        let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        if flags & 2 != 0 {
            walk(f, extent, fsize, &path, out, depth + 1)?;
        } else {
            out.push(Entry { path, offset: extent * SECTOR, size: fsize });
        }
    }
    Ok(())
}

//! Minimal zip reader for texture packs (texmods.rs): the central directory, and entries that
//! are stored or deflated. No zip64, encryption or multi-disk archives (packs are small).

use std::path::{Path, PathBuf};

/// One file in an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub archive: PathBuf,
    /// Path inside the archive, '/'-separated as stored.
    pub name: String,
    method: u16,
    compressed_size: u32,
    size: u32,
    local_offset: u32,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

/// The files of the archive at `path` (directories left out).
pub fn list(path: &Path) -> Result<Vec<Entry>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    list_bytes(&data, path)
}

fn list_bytes(data: &[u8], path: &Path) -> Result<Vec<Entry>, String> {
    // End of central directory: 22 bytes plus a comment of up to 64 KiB, at the end.
    let lo = data.len().saturating_sub(22 + 0xFFFF);
    let eocd = (lo..=data.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(data, i) == Some(0x0605_4b50))
        .ok_or("not a zip file (no end of central directory)")?;
    let count = u16_at(data, eocd + 10).ok_or("truncated")? as usize;
    let cd_offset = u32_at(data, eocd + 16).ok_or("truncated")? as usize;
    if cd_offset == 0xFFFF_FFFF || count == 0xFFFF {
        return Err("zip64 archives are not supported".into());
    }
    let mut out = Vec::with_capacity(count);
    let mut o = cd_offset;
    for _ in 0..count {
        if u32_at(data, o) != Some(0x0201_4b50) {
            return Err("broken central directory".into());
        }
        let flags = u16_at(data, o + 8).ok_or("truncated")?;
        let method = u16_at(data, o + 10).ok_or("truncated")?;
        let compressed_size = u32_at(data, o + 20).ok_or("truncated")?;
        let size = u32_at(data, o + 24).ok_or("truncated")?;
        let name_len = u16_at(data, o + 28).ok_or("truncated")? as usize;
        let extra_len = u16_at(data, o + 30).ok_or("truncated")? as usize;
        let comment_len = u16_at(data, o + 32).ok_or("truncated")? as usize;
        let local_offset = u32_at(data, o + 42).ok_or("truncated")?;
        let name = data.get(o + 46..o + 46 + name_len).ok_or("truncated")?;
        let name = String::from_utf8_lossy(name).replace('\\', "/");
        o += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') || flags & 1 != 0 {
            continue; // directory, or encrypted
        }
        out.push(Entry { archive: path.to_path_buf(), name, method, compressed_size, size, local_offset });
    }
    Ok(out)
}

impl Entry {
    /// The file's contents.
    /// Reads only this entry's bytes: whole-archive reads made big packs (hundreds of MB, thousands
    /// of textures) take minutes to load.
    pub fn read(&self) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let io = |e: std::io::Error| format!("{}: {e}", self.name);
        let mut f = std::fs::File::open(&self.archive).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(self.local_offset as u64)).map_err(io)?;
        let mut data = vec![0u8; 30];
        f.read_exact(&mut data).map_err(io)?;
        let name_len = u16_at(&data, 26).unwrap_or(0) as usize;
        let extra_len = u16_at(&data, 28).unwrap_or(0) as usize;
        let rest = name_len + extra_len + self.compressed_size as usize;
        f.take(rest as u64).read_to_end(&mut data).map_err(io)?;
        Entry { local_offset: 0, ..self.clone() }.read_from(&data)
    }

    fn read_from(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let o = self.local_offset as usize;
        if u32_at(data, o) != Some(0x0403_4b50) {
            return Err(format!("{}: broken local header", self.name));
        }
        let name_len = u16_at(data, o + 26).ok_or("truncated")? as usize;
        let extra_len = u16_at(data, o + 28).ok_or("truncated")? as usize;
        let start = o + 30 + name_len + extra_len;
        let raw = data.get(start..start + self.compressed_size as usize).ok_or("truncated entry")?;
        let out = match self.method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, self.size as usize)
                .map_err(|e| format!("{}: inflate: {e:?}", self.name))?,
            m => return Err(format!("{}: compression method {m} not supported", self.name)),
        };
        if out.len() != self.size as usize {
            return Err(format!("{}: {} bytes, expected {}", self.name, out.len(), self.size));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn archive() -> Vec<u8> {
        build(&[("a/stored.txt", b"hello", false), ("b/deflated.txt", b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true)])
    }

    /// A zip of (name, contents, deflate) files, built by hand.
    pub(crate) fn build(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for &(name, body, deflate) in files {
            let packed = if deflate { miniz_oxide::deflate::compress_to_vec(body, 6) } else { body.to_vec() };
            let method: u16 = if deflate { 8 } else { 0 };
            let off = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]); // time, date, crc (not checked)
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0; 8]);
            cd.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 12]); // extra, comment, disk, attributes
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn reads_stored_and_deflated() {
        let data = archive();
        let entries = list_bytes(&data, Path::new("x.zip")).unwrap();
        assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a/stored.txt", "b/deflated.txt"]);
        assert_eq!(entries[0].read_from(&data).unwrap(), b"hello");
        assert_eq!(entries[1].read_from(&data).unwrap(), vec![b'a'; 56]);
        assert!(list_bytes(b"not a zip at all, just some bytes here", Path::new("y")).is_err());
    }
}

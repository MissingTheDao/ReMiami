//! Minimal zip reader for texture packs (texmods.rs): the central directory, and entries that
//! are stored or deflated, including ZIP64 archives (more than 65,535 entries or 4 GB). No
//! encryption or multi-disk archives.

use std::path::{Path, PathBuf};

/// One file in an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub archive: PathBuf,
    /// Path inside the archive, '/'-separated as stored.
    pub name: String,
    method: u16,
    compressed_size: u64,
    size: u64,
    local_offset: u64,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
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

    let mut total_entries: u64 = 0;
    let mut cd_size: u64 = 0;
    let mut cd_offset: u64 = 0;

    // Check for ZIP64 EOCD Locator
    let is_zip64 = {
        let locator = eocd.checked_sub(20);
        if let Some(locator_offset) = locator
            && u32_at(data, locator_offset) == Some(0x0706_4b50)
        {
            // Read locator: sig(4), disk(4), eocd_offset(8), total_disks(4)
            let eocd64_offset = u64_at(data, locator_offset + 8).ok_or("truncated zip64 locator")?;
            // Read ZIP64 EOCD
            // sig(4), size of record(8), version made(2), version needed(2), disk(4), cd_disk(4),
            // entries on disk(8), total entries(8), cd size(8), cd offset(8)
            let sig = u32_at(data, eocd64_offset as usize).ok_or("truncated zip64 eocd")?;
            if sig != 0x0606_4b50 {
                false
            } else {
                let entries = u64_at(data, (eocd64_offset + 32) as usize).ok_or("truncated zip64 eocd entries")?;
                let size = u64_at(data, (eocd64_offset + 40) as usize).ok_or("truncated zip64 eocd size")?;
                let offset = u64_at(data, (eocd64_offset + 48) as usize).ok_or("truncated zip64 eocd offset")?;
                total_entries = entries;
                cd_size = size;
                cd_offset = offset;
                true
            }
        } else {
            false
        }
    };

    if !is_zip64 {
        let count = u16_at(data, eocd + 10).ok_or("truncated")? as usize;
        let cd_off = u32_at(data, eocd + 16).ok_or("truncated")? as usize;
        let cd_sz = u32_at(data, eocd + 12).ok_or("truncated")? as usize;
        
        if cd_off == 0xFFFF_FFFF || count == 0xFFFF {
            return Err("zip64 archives are not supported".into());
        }
        total_entries = count as u64;
        cd_size = cd_sz as u64;
        cd_offset = cd_off as u64;
    }

    let mut out = Vec::with_capacity(total_entries.min(1 << 20) as usize);
    let mut o = cd_offset as usize;
    let _ = cd_size;

    for _ in 0..total_entries {
        if u32_at(data, o) != Some(0x0201_4b50) {
            return Err("broken central directory".into());
        }
        let flags = u16_at(data, o + 8).ok_or("truncated")?;
        let method = u16_at(data, o + 10).ok_or("truncated")?;
        let mut compressed_size = u32_at(data, o + 20).ok_or("truncated")? as u64;
        let mut size = u32_at(data, o + 24).ok_or("truncated")? as u64;
        let name_len = u16_at(data, o + 28).ok_or("truncated")? as usize;
        let extra_len = u16_at(data, o + 30).ok_or("truncated")? as usize;
        let comment_len = u16_at(data, o + 32).ok_or("truncated")? as usize;
        let mut local_offset = u32_at(data, o + 42).ok_or("truncated")? as u64;

        // Parse extra field for ZIP64 overrides
        let extra = data.get(o + 46 + name_len..o + 46 + name_len + extra_len).unwrap_or(&[]);
        let mut eo = 0;
        while eo + 4 <= extra_len {
            let id = u16_at(extra, eo).ok_or("truncated extra field")?;
            let elen = u16_at(extra, eo + 2).ok_or("truncated extra field")? as usize;
            let edata = extra.get(eo + 4..eo + 4 + elen).ok_or("truncated extra field")?;
            
            if id == 0x0001 {
                // ZIP64 Extra Field
                // (in this order, only the fields that didn't fit)
                let mut idx = 0;
                for field in [&mut size, &mut compressed_size, &mut local_offset] {
                    if *field == 0xFFFF_FFFF {
                        *field = u64_at(edata, idx).ok_or("truncated zip64 extra")?;
                        idx += 8;
                    }
                }
            }
            eo += 4 + elen;
        }

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
        f.seek(SeekFrom::Start(self.local_offset)).map_err(io)?;
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

    /// The same files as a ZIP64 archive: every size and offset in the 0x0001 extra fields, the
    /// classic records saturated, a ZIP64 end record and its locator.
    pub(crate) fn build64(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        let extra = |a: u64, b: u64, c: Option<u64>| {
            let mut e = vec![1, 0, if c.is_some() { 24 } else { 16 }, 0];
            e.extend_from_slice(&a.to_le_bytes());
            e.extend_from_slice(&b.to_le_bytes());
            if let Some(c) = c {
                e.extend_from_slice(&c.to_le_bytes());
            }
            e
        };
        for &(name, body, deflate) in files {
            let packed = if deflate { miniz_oxide::deflate::compress_to_vec(body, 6) } else { body.to_vec() };
            let method: u16 = if deflate { 8 } else { 0 };
            let off = out.len() as u64;
            let local_extra = extra(body.len() as u64, packed.len() as u64, None);
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[45, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&[0xFF; 8]); // both sizes in the extra field
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(local_extra.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&local_extra);
            out.extend_from_slice(&packed);
            let cd_extra = extra(body.len() as u64, packed.len() as u64, Some(off));
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&[45, 0, 45, 0, 0, 0]);
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0; 8]);
            cd.extend_from_slice(&[0xFF; 8]);
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&(cd_extra.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 10]); // comment, disk, attributes
            cd.extend_from_slice(&[0xFF; 4]); // offset in the extra field
            cd.extend_from_slice(name.as_bytes());
            cd.extend_from_slice(&cd_extra);
        }
        let cd_off = out.len() as u64;
        out.extend_from_slice(&cd);
        let end64 = out.len() as u64;
        out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for v in [files.len() as u64, files.len() as u64, cd.len() as u64, cd_off] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&end64.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[0xFF; 12]); // counts, size, offset: in the ZIP64 record
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

    #[test]
    fn reads_zip64_stored_and_deflated() {
        let data = build64(&[("small_stored.txt", b"hello world stored", false), ("small_deflated.txt", &[b'z'; 300], true)]);
        let entries = list_bytes(&data, Path::new("x64.zip")).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].read_from(&data).unwrap(), b"hello world stored");
        assert_eq!(entries[1].read_from(&data).unwrap(), vec![b'z'; 300]);
    }

    #[test]
    fn lists_70000_entries() {
        let names: Vec<String> = (0..70_000).map(|i| format!("f{i:05}.txt")).collect();
        let files: Vec<(&str, &[u8], bool)> = names.iter().map(|n| (n.as_str(), &b"x"[..], false)).collect();
        let data = build64(&files);
        let entries = list_bytes(&data, Path::new("big.zip")).unwrap();
        assert_eq!(entries.len(), 70_000);
        assert_eq!(entries[69_999].name, "f69999.txt");
        assert_eq!(entries[69_999].read_from(&data).unwrap(), b"x");
    }
}

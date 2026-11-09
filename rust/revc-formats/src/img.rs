//! IMG archives as used by Vice City: a `.dir` file of 32-byte entries next to a `.img` data file.
//!
//! Entry layout (CDirectory / CStreaming::LoadCdDirectory): `u32 offset` and `u32 size`, both in
//! 2048-byte sectors, then a 24-byte NUL-padded name.

use crate::bytes::Reader;
use crate::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SECTOR: u64 = 2048;
pub const ENTRY_SIZE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// In sectors.
    pub offset: u32,
    /// In sectors.
    pub size: u32,
}

impl Entry {
    pub fn byte_offset(&self) -> u64 {
        self.offset as u64 * SECTOR
    }
    pub fn byte_size(&self) -> u64 {
        self.size as u64 * SECTOR
    }
    /// Extension in lower case ("dff", "txd", "col", "ifp", "ipl" ...), empty if none.
    pub fn extension(&self) -> String {
        self.name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default()
    }
}

/// Parses a `.dir` file.
pub fn parse_dir(data: &[u8]) -> Result<Vec<Entry>> {
    if !data.len().is_multiple_of(ENTRY_SIZE) {
        return Err(Error::Invalid(format!(
            "dir size {} is not a multiple of {ENTRY_SIZE}",
            data.len()
        )));
    }
    let mut r = Reader::new(data);
    let mut out = Vec::with_capacity(data.len() / ENTRY_SIZE);
    while r.remaining() > 0 {
        let offset = r.u32("dir offset")?;
        let size = r.u32("dir size")?;
        let name = r.fixed_str(24, "dir name")?;
        out.push(Entry { name, offset, size });
    }
    Ok(out)
}

pub struct Archive {
    pub entries: Vec<Entry>,
    img_path: PathBuf,
    img_len: u64,
}

impl Archive {
    /// Opens `<path>.dir` and `<path>.img` (pass either one, or the path without extension).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let dir_path = path.with_extension("dir");
        let img_path = path.with_extension("img");
        let entries = parse_dir(&std::fs::read(&dir_path)?)?;
        let img_len = std::fs::metadata(&img_path)?.len();
        for e in &entries {
            if e.byte_offset() + e.byte_size() > img_len {
                return Err(Error::Invalid(format!(
                    "{} lies past the end of {}",
                    e.name,
                    img_path.display()
                )));
            }
        }
        Ok(Archive {
            entries,
            img_path,
            img_len,
        })
    }

    pub fn img_len(&self) -> u64 {
        self.img_len
    }

    /// Case-insensitive lookup, as the game does.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Reads one entry (whole sectors, so the data may end in padding).
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut f = File::open(&self.img_path)?;
        self.read_with(&mut f, entry)
    }

    /// Like [`read`](Self::read) but reuses an open `.img` handle (for walking the whole archive).
    pub fn read_with(&self, img: &mut File, entry: &Entry) -> Result<Vec<u8>> {
        img.seek(SeekFrom::Start(entry.byte_offset()))?;
        let mut buf = vec![0; entry.byte_size() as usize];
        img.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn open_img(&self) -> Result<File> {
        Ok(File::open(&self.img_path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries() {
        let mut d = Vec::new();
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&5u32.to_le_bytes());
        let mut name = [0u8; 24];
        name[..11].copy_from_slice(b"radar00.TXD");
        d.extend_from_slice(&name);
        let e = parse_dir(&d).unwrap();
        assert_eq!(
            e[0],
            Entry {
                name: "radar00.TXD".into(),
                offset: 0,
                size: 5
            }
        );
        assert_eq!(e[0].extension(), "txd");
        assert_eq!(e[0].byte_size(), 5 * 2048);
        assert!(parse_dir(&d[..31]).is_err());
    }
}

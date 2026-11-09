//! RenderWare binary stream chunks (RW 3.x as used by Vice City: library 3.3 / 3.4).
//!
//! Every chunk is `u32 type, u32 size, u32 library id` followed by `size` bytes. Containers
//! (clump, geometry, material ...) hold a sequence of child chunks; the first is usually a
//! `STRUCT` with the fixed fields.

use crate::bytes::Reader;
use crate::{Error, Result};

pub const ID_STRUCT: u32 = 0x01;
pub const ID_STRING: u32 = 0x02;
pub const ID_EXTENSION: u32 = 0x03;
pub const ID_TEXTURE: u32 = 0x06;
pub const ID_MATERIAL: u32 = 0x07;
pub const ID_MATLIST: u32 = 0x08;
pub const ID_FRAMELIST: u32 = 0x0E;
pub const ID_GEOMETRY: u32 = 0x0F;
pub const ID_CLUMP: u32 = 0x10;
pub const ID_ATOMIC: u32 = 0x14;
pub const ID_TEXTURENATIVE: u32 = 0x15;
pub const ID_TEXDICTIONARY: u32 = 0x16;
pub const ID_GEOMETRYLIST: u32 = 0x1A;
/// Frame name plugin (Rockstar "node name").
pub const ID_NODENAME: u32 = 0x0253_F2FE;

pub const HEADER_SIZE: usize = 12;

/// Decodes a chunk's library id to the RW version number (`0x34003` for 3.4.0.3, Vice City; `0x33002` for GTA III).
pub fn library_version(libid: u32) -> u32 {
    if libid & 0xFFFF_0000 != 0 {
        (((libid >> 14) & 0x3_FF00) + 0x3_0000) | ((libid >> 16) & 0x3F)
    } else {
        libid << 8
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    pub id: u32,
    pub libid: u32,
    pub data: &'a [u8],
    /// Offset of the header within the slice it was read from.
    pub offset: usize,
}

impl<'a> Chunk<'a> {
    pub fn version(&self) -> u32 {
        library_version(self.libid)
    }

    /// The chunks inside this one.
    pub fn children(&self) -> Chunks<'a> {
        Chunks::new(self.data)
    }

    pub fn reader(&self) -> Reader<'a> {
        Reader::new(self.data)
    }

    /// First child with the given id.
    pub fn child(&self, id: u32) -> Result<Chunk<'a>> {
        for c in self.children() {
            let c = c?;
            if c.id == id {
                return Ok(c);
            }
        }
        Err(Error::Invalid(format!(
            "chunk {:#x} has no child {id:#x}",
            self.id
        )))
    }
}

/// Reads one chunk header at the reader position and returns the chunk, advancing past it.
pub fn read_chunk<'a>(r: &mut Reader<'a>) -> Result<Chunk<'a>> {
    let offset = r.pos();
    let id = r.u32("chunk type")?;
    let size = r.u32("chunk size")?;
    let libid = r.u32("chunk library id")?;
    let data = r.bytes(size as usize, "chunk data")?;
    Ok(Chunk {
        id,
        libid,
        data,
        offset,
    })
}

/// Iterator over consecutive chunks in a slice. Stops at the end or when fewer than a header's
/// worth of bytes remain (IMG entries are padded to whole sectors).
pub struct Chunks<'a> {
    r: Reader<'a>,
    failed: bool,
}

impl<'a> Chunks<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Chunks {
            r: Reader::new(data),
            failed: false,
        }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.r.remaining() < HEADER_SIZE {
            return None;
        }
        let res = read_chunk(&mut self.r);
        self.failed = res.is_err();
        Some(res)
    }
}

/// Top-level chunk of a file (the first one; trailing padding is ignored).
pub fn root(data: &[u8]) -> Result<Chunk<'_>> {
    read_chunk(&mut Reader::new(data))
}

/// Payload of a STRING chunk.
pub fn string(c: &Chunk) -> Result<String> {
    if c.id != ID_STRING {
        return Err(Error::Invalid(format!(
            "expected string chunk, got {:#x}",
            c.id
        )));
    }
    Ok(crate::bytes::c_str(c.data))
}

/// Plugin data from an EXTENSION chunk: (plugin id, payload).
pub fn extensions<'a>(ext: &Chunk<'a>) -> Result<Vec<(u32, &'a [u8])>> {
    ext.children().map(|c| c.map(|c| (c.id, c.data))).collect()
}

#[cfg(test)]
pub(crate) mod build {
    //! Writers for tests: build chunks the way RW streams them.
    pub const LIBID_VC: u32 = 0x1003_FFFF;

    pub fn chunk(id: u32, body: &[u8]) -> Vec<u8> {
        chunk_v(id, LIBID_VC, body)
    }

    pub fn chunk_v(id: u32, libid: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(body.len() + 12);
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&libid.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    pub fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    pub fn u32s(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    pub fn f32s(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(library_version(0x1003_FFFF), 0x34003);
        assert_eq!(library_version(0x0C02_FFFF), 0x33002);
        assert_eq!(library_version(0x0310), 0x31000);
    }

    #[test]
    fn walks_children_and_ignores_padding() {
        let inner = cat(&[
            &chunk(ID_STRUCT, &[1, 2, 3, 4]),
            &chunk(ID_STRING, b"abc\0"),
        ]);
        let mut file = chunk(ID_CLUMP, &inner);
        file.extend_from_slice(&[0; 7]);
        let r = root(&file).unwrap();
        assert_eq!(r.id, ID_CLUMP);
        assert_eq!(r.version(), 0x34003);
        let kids: Vec<_> = r.children().collect::<Result<_>>().unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!(string(&kids[1]).unwrap(), "abc");
        assert!(r.child(ID_GEOMETRY).is_err());
    }

    #[test]
    fn truncated_child_is_an_error() {
        let mut bad = chunk(ID_STRUCT, &[0; 8]);
        bad.truncate(15);
        let file = chunk(ID_CLUMP, &bad);
        assert!(root(&file).unwrap().children().any(|c| c.is_err()));
    }
}

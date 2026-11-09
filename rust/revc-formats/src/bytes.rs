//! Little-endian cursor over a byte slice, shared by all binary parsers.

use crate::{Error, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::Truncated("seek", pos));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n, "skip").map(|_| ())
    }

    pub fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(Error::Truncated(what, self.pos)),
        }
    }

    fn array<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N]> {
        Ok(self.bytes(N, what)?.try_into().unwrap())
    }

    pub fn u8(&mut self, what: &'static str) -> Result<u8> {
        Ok(self.array::<1>(what)?[0])
    }
    pub fn i8(&mut self, what: &'static str) -> Result<i8> {
        Ok(self.u8(what)? as i8)
    }
    pub fn u16(&mut self, what: &'static str) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array(what)?))
    }
    pub fn i16(&mut self, what: &'static str) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array(what)?))
    }
    pub fn u32(&mut self, what: &'static str) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array(what)?))
    }
    pub fn i32(&mut self, what: &'static str) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array(what)?))
    }
    pub fn f32(&mut self, what: &'static str) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array(what)?))
    }
    pub fn vec3(&mut self, what: &'static str) -> Result<[f32; 3]> {
        Ok([self.f32(what)?, self.f32(what)?, self.f32(what)?])
    }

    /// Fixed-size, NUL-padded ASCII string (names in IMG directories, TXD, COL ...).
    pub fn fixed_str(&mut self, n: usize, what: &'static str) -> Result<String> {
        Ok(c_str(self.bytes(n, what)?))
    }
}

/// Text up to the first NUL, bytes outside ASCII mapped as Latin-1.
pub fn c_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_bounds() {
        let d = [1u8, 0, 0, 0, 0xff, 0xff, b'a', b'b', 0, b'z'];
        let mut r = Reader::new(&d);
        assert_eq!(r.u32("x").unwrap(), 1);
        assert_eq!(r.i16("y").unwrap(), -1);
        assert_eq!(r.fixed_str(4, "s").unwrap(), "ab");
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.u8("z"), Err(Error::Truncated("z", 10)));
    }
}

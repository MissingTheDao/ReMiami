//! Entries of the Xbox release's `gta3.img`: every DFF/TXD/IFP (not COL) is wrapped as
//! `u32 magic 0x67A3A1CE, u32 hash, u32 total` (`total`: the bytes of the blocks that follow,
//! headers included), then blocks of `u32 4, u32 csize, u32 csize`
//! followed by `csize` bytes of LZO1X data, each unpacking to at most 128 KiB (research notes:
//! Claude outputs/xbox-research/XBOX_RESEARCH.md; decompressor ported from its tools/lzo.py).

use crate::{Error, Result};

pub const MAGIC: u32 = 0x67A3_A1CE;

/// Whether `data` is a wrapped (compressed) entry.
pub fn is_wrapped(data: &[u8]) -> bool {
    data.len() >= 12 && u32::from_le_bytes(data[..4].try_into().unwrap()) == MAGIC
}

/// The unpacked entry (the RW stream, e.g. a TEXDICTIONARY chunk).
pub fn unwrap(data: &[u8]) -> Result<Vec<u8>> {
    if !is_wrapped(data) {
        return Err(Error::Invalid("not a compressed Xbox IMG entry".into()));
    }
    let total = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let end = 12 + total;
    if end > data.len() {
        return Err(Error::Invalid(format!("Xbox IMG entry says {total} bytes, has {}", data.len() - 12)));
    }
    let mut out = Vec::new();
    let mut p = 12;
    while p < end {
        let head = data.get(p..p + 12).filter(|_| p + 12 <= end).ok_or_else(|| Error::Invalid("Xbox IMG block header past the entry".into()))?;
        let word = |i: usize| u32::from_le_bytes(head[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
        let (four, csize, csize2) = (word(0), word(1), word(2));
        if four != 4 || csize != csize2 || csize == 0 {
            return Err(Error::Invalid(format!("bad Xbox IMG block header ({four}, {csize}, {csize2})")));
        }
        p += 12;
        let block = data.get(p..p + csize).filter(|_| p + csize <= end).ok_or_else(|| Error::Invalid("Xbox IMG block past the entry".into()))?;
        lzo1x_decompress(block, &mut out)?;
        p += csize;
    }
    Ok(out)
}

/// LZO1X decompression of one block, appended to `out`.
pub fn lzo1x_decompress(src: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let bad = || Error::Invalid("corrupt LZO1X data".into());
    let base = out.len();
    let mut ip = 0usize;
    let byte = |ip: usize| src.get(ip).copied().ok_or_else(bad);
    let literal = |out: &mut Vec<u8>, ip: usize, n: usize| -> Result<()> {
        out.extend_from_slice(src.get(ip..ip + n).ok_or_else(bad)?);
        Ok(())
    };
    let mut t = byte(ip)? as usize;
    let mut state;
    if t > 17 {
        t -= 17;
        literal(out, ip + 1, t)?;
        ip += 1 + t;
        state = if t >= 4 { 4 } else { t };
        t = byte(ip)? as usize;
        ip += 1;
    } else {
        state = 0;
        ip += 1;
    }
    loop {
        let (dist, length);
        if t < 16 {
            if state == 0 {
                if t == 0 {
                    while byte(ip)? == 0 {
                        t += 255;
                        ip += 1;
                    }
                    t += 15 + byte(ip)? as usize;
                    ip += 1;
                }
                t += 3;
                literal(out, ip, t)?;
                ip += t;
                state = 4;
                t = byte(ip)? as usize;
                ip += 1;
                continue;
            } else if state == 4 {
                dist = 1 + 0x0800 + (t >> 2) + ((byte(ip)? as usize) << 2);
                ip += 1;
                length = 3;
            } else {
                dist = 1 + (t >> 2) + ((byte(ip)? as usize) << 2);
                ip += 1;
                length = 2;
            }
        } else if t >= 64 {
            dist = 1 + ((t >> 2) & 7) + ((byte(ip)? as usize) << 3);
            ip += 1;
            length = (t >> 5) - 1 + 2;
        } else if t >= 32 {
            let mut l = t & 31;
            if l == 0 {
                while byte(ip)? == 0 {
                    l += 255;
                    ip += 1;
                }
                l += 31 + byte(ip)? as usize;
                ip += 1;
            }
            length = l + 2;
            let d = byte(ip)? as usize | (byte(ip + 1)? as usize) << 8;
            ip += 2;
            dist = 1 + (d >> 2);
        } else {
            let mut d0 = (t & 8) << 11;
            let mut l = t & 7;
            if l == 0 {
                while byte(ip)? == 0 {
                    l += 255;
                    ip += 1;
                }
                l += 7 + byte(ip)? as usize;
                ip += 1;
            }
            length = l + 2;
            let d = byte(ip)? as usize | (byte(ip + 1)? as usize) << 8;
            ip += 2;
            d0 += d >> 2;
            if d0 == 0 {
                return Ok(()); // end of stream
            }
            dist = d0 + 0x4000;
        }
        let start = out.len().checked_sub(dist).filter(|&s| s >= base).ok_or_else(bad)?;
        for i in 0..length {
            let b = out[start + i];
            out.push(b);
        }
        state = (byte(ip - 2)? & 3) as usize;
        if state != 0 {
            literal(out, ip, state)?;
            ip += state;
        }
        t = byte(ip)? as usize;
        ip += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-made LZO1X stream: 4 literals "abcd", a match of length 3 at distance 4 (via the
    /// 2-byte-distance M3 form), then the end marker.
    #[test]
    fn literals_match_and_end() {
        // first byte 17 + 4 = 21: copy 4 literals; then M3: t = 32 | (len-2 = 1), dist-1 = 3 -> d = 3 << 2
        let src = [21, b'a', b'b', b'c', b'd', 32 | 1, 3 << 2, 0, 0x11, 0, 0];
        let mut out = Vec::new();
        lzo1x_decompress(&src, &mut out).unwrap();
        assert_eq!(out, b"abcdabc");
    }

    #[test]
    fn wrapped_entry() {
        let block = [21u8, b'a', b'b', b'c', b'd', 32 | 1, 3 << 2, 0, 0x11, 0, 0];
        let mut d = Vec::new();
        for v in [MAGIC, 0x1234, 12 + block.len() as u32, 4, block.len() as u32, block.len() as u32] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&block);
        assert!(is_wrapped(&d));
        assert_eq!(unwrap(&d).unwrap(), b"abcdabc");
        assert!(unwrap(&d[..d.len() - 3]).is_err());
        assert!(unwrap(b"\x16\0\0\0rest of a TXD").is_err());
    }
}

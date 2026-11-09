//! PC save files (`GTAVCsf<n>.b`): the container around the game state. Each save is a run of
//! blocks (`i32 size`, then the data padded to 4 bytes) followed by an `i32` checksum, the sum of
//! every byte before it. What is inside the blocks is the game's own serialisation.
//!
//! [`check`] is CheckDataNotCorrupt (save/GenericGameStorage.cpp) on the file's bytes, with the
//! original's limits: at most 40 blocks, stop after SIZE_OF_ONE_GAME_IN_BYTES - 4 data bytes, a
//! block larger than the work buffer is read as 54996 bytes, sizes -1..-3 round to 0 (and are
//! accepted), a size that rounds below 0 fails like the original's read. Checked against the
//! original with tests/golden/savecheck.txt.

/// SIZE_OF_ONE_GAME_IN_BYTES.
pub const GAME_SIZE: usize = 201_729;
/// sizeof(work_buff), the game's block buffer.
pub const WORK_BUFF: i32 = 55_000;
const MAX_BLOCKS: usize = 40;

fn align4(size: i32) -> i32 {
    size.wrapping_add(3) & !3
}

/// Result of a successful check: the level stored in the first block (offset 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    pub level: i32,
}

/// CheckDataNotCorrupt on a whole save file. None if a read would fail or the checksum differs.
pub fn check(data: &[u8]) -> Option<Checked> {
    let mut pos = 0usize;
    let mut read = |n: usize| -> Option<&[u8]> {
        let s = data.get(pos..pos.checked_add(n)?)?;
        pos += n;
        Some(s)
    };
    let mut sum: i32 = 0;
    let mut processed: u32 = 0;
    let mut level = 0;
    let mut block = 0;
    while (GAME_SIZE as u32 - 4) > processed && block < MAX_BLOCKS {
        let raw = read(4)?;
        let mut size = i32::from_le_bytes(raw.try_into().unwrap());
        for &b in raw {
            sum = sum.wrapping_add(b as i32);
        }
        if size > align4(WORK_BUFF) {
            size = WORK_BUFF - 4;
        }
        let aligned = align4(size);
        if aligned < 0 {
            return None; // the original asks to read ~4 GB: short read
        }
        let bytes = read(aligned as usize)?;
        for &b in bytes {
            sum = sum.wrapping_add(b as i32);
        }
        processed = processed.wrapping_add(aligned as u32);
        if block == 0 {
            // The original copies 4 bytes at offset 4 of its buffer; a first block shorter than
            // that left older buffer contents there. Treated as 0 here.
            level = bytes.get(4..8).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
        }
        block += 1;
    }
    let stored = i32::from_le_bytes(read(4)?.try_into().unwrap());
    (stored == sum).then_some(Checked { level })
}

/// The start of a save file as the Load menu reads it (C_PcSave::PopulateSlotInfo).
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// Save name (last mission passed), game charset, up to 24 units.
    pub name: Vec<u16>,
    /// Windows SYSTEMTIME: year, month, day of week, day, hour, minute, second, milliseconds.
    pub date: [u16; 8],
}

/// Name and date from the first block (None if the file is too short).
pub fn header(data: &[u8]) -> Option<Header> {
    let units = |off: usize, n: usize| -> Option<Vec<u16>> {
        (0..n).map(|i| data.get(off + 2 * i..off + 2 * i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))).collect()
    };
    let mut name = units(4, 24)?;
    if let Some(end) = name.iter().position(|&u| u == 0) {
        name.truncate(end);
    }
    let date: [u16; 8] = units(4 + 48, 8)?.try_into().unwrap();
    Some(Header { name, date })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid file of the given blocks (sizes as given, data padded) plus the checksum.
    pub fn build(blocks: &[(i32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (size, data) in blocks {
            out.extend_from_slice(&size.to_le_bytes());
            let mut d = data.clone();
            d.resize(align4(*size).max(0) as usize, 0);
            out.extend(d);
        }
        let sum = out.iter().fold(0i32, |s, &b| s.wrapping_add(b as i32));
        out.extend_from_slice(&sum.to_le_bytes());
        out
    }

    #[test]
    fn checks_blocks_and_checksum() {
        let mut first = vec![0u8; 100];
        first[4..8].copy_from_slice(&3i32.to_le_bytes());
        let f = build(&[(100, first), (7, vec![1, 2, 3])]);
        // Two blocks are far from the full game size, so the reader expects more blocks: the
        // checksum is read as a block size and the file ends early.
        assert_eq!(check(&f), None);

        // A real save fills the size; here one big block of exactly GAME_SIZE - 4 bytes.
        let mut big = vec![0u8; GAME_SIZE - 4];
        big[4..8].copy_from_slice(&2i32.to_le_bytes());
        let blocks: Vec<(i32, Vec<u8>)> = big.chunks(50_000).map(|c| (c.len() as i32, c.to_vec())).collect();
        let f = build(&blocks);
        assert_eq!(check(&f), Some(Checked { level: 2 }));
        let mut bad = f.clone();
        bad[100] ^= 1;
        assert_eq!(check(&bad), None);
        assert_eq!(check(&f[..f.len() - 2]), None);
    }

    #[test]
    fn real_test_save() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/saves/GTAVCsf1.b");
        let d = std::fs::read(p).unwrap();
        assert!(check(&d).is_some());
        assert!(!header(&d).unwrap().name.is_empty());
    }

    #[test]
    fn reads_header() {
        let mut d = vec![0u8; 80];
        for (i, c) in "THE PARTY".encode_utf16().enumerate() {
            d[4 + 2 * i..6 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
        d[52..54].copy_from_slice(&2002u16.to_le_bytes());
        let h = header(&d).unwrap();
        assert_eq!(String::from_utf16(&h.name).unwrap(), "THE PARTY");
        assert_eq!(h.date[0], 2002);
        assert!(header(&d[..60]).is_none());
    }
}

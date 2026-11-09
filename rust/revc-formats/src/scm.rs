//! `main.scm` header: the three leading segments (global variable space, used objects, mission
//! table), read the way CTheScripts::Init / ReadMultiScriptFileOffsetsFromScript do. The bytecode
//! itself is left to the script VM (plan task 16).

use crate::bytes::Reader;
use crate::{Error, Result};

const OPCODE_GOTO: u16 = 0x0002;
const ARG_INT32: u8 = 0x01;
const OBJECT_NAME_LEN: usize = 24;

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// Offset of the used-objects segment = size of the global variable space.
    pub var_space_size: u32,
    /// Offset of the mission table segment.
    pub missions_segment: u32,
    /// Models the script refers to by name (index 0 is a dummy entry in the original file).
    pub used_objects: Vec<String>,
    pub main_size: u32,
    pub largest_mission_size: u32,
    pub num_exclusive_missions: u16,
    /// File offsets of the mission scripts.
    pub mission_offsets: Vec<u32>,
}

/// Reads the `GOTO <int32>` at `at` and returns its target.
fn goto_target(data: &[u8], at: usize) -> Result<u32> {
    let mut r = Reader::new(data);
    r.seek(at)?;
    let op = r.u16("segment goto opcode")?;
    let ty = r.u8("segment goto arg type")?;
    let target = r.i32("segment goto target")?;
    if op != OPCODE_GOTO || ty != ARG_INT32 || target < 0 || target as usize >= data.len() {
        return Err(Error::Invalid(format!(
            "no segment jump at {at} (opcode {op:#x}, type {ty}, target {target})"
        )));
    }
    Ok(target as u32)
}

pub fn parse_header(data: &[u8]) -> Result<Header> {
    let var_space_size = goto_target(data, 0)?;
    let missions_segment = goto_target(data, var_space_size as usize)?;
    let mut r = Reader::new(data);
    r.seek(var_space_size as usize + 8)?; // goto (7 bytes) + segment id byte
    let n = r.u32("used object count")? as usize;
    if n > (missions_segment - var_space_size) as usize / OBJECT_NAME_LEN {
        return Err(Error::Invalid(format!(
            "{n} used objects do not fit their segment"
        )));
    }
    let used_objects = (0..n)
        .map(|_| r.fixed_str(OBJECT_NAME_LEN, "used object name"))
        .collect::<Result<_>>()?;

    r.seek(missions_segment as usize + 8)?;
    let main_size = r.u32("main script size")?;
    let largest_mission_size = r.u32("largest mission size")?;
    let num_missions = r.u16("mission count")?;
    let num_exclusive_missions = r.u16("exclusive mission count")?;
    let mission_offsets: Vec<u32> = (0..num_missions)
        .map(|_| r.u32("mission offset"))
        .collect::<Result<_>>()?;
    if main_size as usize > data.len() || main_size < r.pos() as u32 {
        return Err(Error::Invalid(format!(
            "main script size {main_size} out of range"
        )));
    }
    let mut prev = main_size;
    for (i, &off) in mission_offsets.iter().enumerate() {
        if off < prev || off as usize >= data.len() {
            return Err(Error::Invalid(format!(
                "mission {i} offset {off} out of order or past the end"
            )));
        }
        prev = off;
    }
    Ok(Header {
        var_space_size,
        missions_segment,
        used_objects,
        main_size,
        largest_mission_size,
        num_exclusive_missions,
        mission_offsets,
    })
}

impl Header {
    /// Byte range of mission `i` (up to the next mission or the end of the file).
    pub fn mission_range(&self, i: usize, file_len: usize) -> Option<std::ops::Range<usize>> {
        let start = *self.mission_offsets.get(i)? as usize;
        let end = self
            .mission_offsets
            .get(i + 1)
            .map_or(file_len, |&o| o as usize);
        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goto(out: &mut Vec<u8>, target: u32) {
        out.extend_from_slice(&OPCODE_GOTO.to_le_bytes());
        out.push(ARG_INT32);
        out.extend_from_slice(&target.to_le_bytes());
    }

    fn build() -> Vec<u8> {
        let vars = 16u32;
        let objs = vars + 8 + 4 + 2 * 24;
        let mut d = Vec::new();
        goto(&mut d, vars);
        d.resize(vars as usize, 0);
        goto(&mut d, objs);
        d.push(b'm');
        d.extend_from_slice(&2u32.to_le_bytes());
        for name in ["", "BARREL1"] {
            let mut b = [0u8; 24];
            b[..name.len()].copy_from_slice(name.as_bytes());
            d.extend_from_slice(&b);
        }
        let header_end = objs + 8 + 12 + 2 * 4;
        let main_size = header_end + 10;
        goto(&mut d, main_size);
        d.push(0);
        for v in [main_size, 30] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        for v in [main_size, main_size + 20] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.resize(main_size as usize + 50, 0);
        d
    }

    #[test]
    fn parses_header() {
        let d = build();
        let h = parse_header(&d).unwrap();
        assert_eq!(h.var_space_size, 16);
        assert_eq!(h.used_objects, vec!["".to_string(), "BARREL1".into()]);
        assert_eq!(h.mission_offsets.len(), 2);
        assert_eq!(h.num_exclusive_missions, 1);
        let r = h.mission_range(1, d.len()).unwrap();
        assert_eq!(r.len(), 30);
    }

    #[test]
    fn rejects_bad_jump() {
        let mut d = build();
        d[0] = 3;
        assert!(parse_header(&d).is_err());
    }
}

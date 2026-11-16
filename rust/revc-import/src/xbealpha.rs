//! The Xbox style's per-model alpha-test table, read from the player's own Xbox executable at
//! import time (the console build keeps it as code, not as a data file). The executable's hash is
//! checked by the caller before this runs; this module only knows where to look in that version.
//!
//! The table has two parts: a compiled decision chain (compares and branches on the model id; the
//! end of each matching branch loads that model's scale) and plain lists of ids that all share one
//! scale. The chain is run by a small interpreter that knows only the handful of byte forms that
//! occur in that range; anything else stops the import with an error.

use std::collections::BTreeMap;

/// One row: a model id and its scale (0 < scale <= 1).
pub type Row = (u32, f32);

/// Where the table is in the accepted executable (file offsets).
struct Layout {
    /// Expected length of the image; any other length is an error.
    size: usize,
    /// File offset and length of the code range that is interpreted.
    code: (usize, usize),
    /// Reaching this offset means the id has a row: its scale is the last one loaded.
    found: usize,
    /// (file offset, count) of little-endian i32 id lists.
    lists: &'static [(usize, usize)],
    /// File offset of the f32 scale every id of the lists gets.
    list_scale: usize,
}

const LAYOUT: Layout = Layout { size: 3_293_184, code: (0x165d7d, 0x404), found: 0x165eb2, lists: &[(0x2c4150, 137), (0x2c4374, 6)], list_scale: 0x2b0ae8 };

/// Most steps one id may take through the chain.
const MAX_STEPS: usize = 400;

fn bytes<const N: usize>(x: &[u8], at: usize) -> Result<[u8; N], String> {
    x.get(at..at + N).and_then(|b| b.try_into().ok()).ok_or_else(|| format!("read past the end at 0x{at:x}"))
}
fn u8_at(x: &[u8], at: usize) -> Result<u8, String> {
    Ok(bytes::<1>(x, at)?[0])
}
fn i8_at(x: &[u8], at: usize) -> Result<i64, String> {
    Ok(u8_at(x, at)? as i8 as i64)
}
fn i32_at(x: &[u8], at: usize) -> Result<i64, String> {
    Ok(i32::from_le_bytes(bytes(x, at)?) as i64)
}
fn u32_at(x: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(bytes(x, at)?))
}
fn f32_at(x: &[u8], at: usize) -> Result<f32, String> {
    Ok(f32::from_le_bytes(bytes(x, at)?))
}

/// File offset of a virtual address, by the image's section table.
fn file_offset(xbe: &[u8], va: u32) -> Result<usize, String> {
    let base = u32_at(xbe, 0x104)?;
    let count = u32_at(xbe, 0x11c)? as usize;
    let table = u32_at(xbe, 0x120)?.checked_sub(base).ok_or("section table below the image base")? as usize;
    for i in 0..count {
        let h = table + i * 56;
        let (sva, raw_off, raw_size) = (u32_at(xbe, h + 4)?, u32_at(xbe, h + 12)?, u32_at(xbe, h + 16)?);
        if va >= sva && (va - sva) < raw_size {
            return Ok(raw_off as usize + (va - sva) as usize);
        }
    }
    Err(format!("address 0x{va:x} is in no section"))
}

/// The five conditions (low nibble of the jump byte; only these are decoded): > == != <= <.
fn cond(c: u8, flags: i64) -> bool {
    match c & 0x0f {
        0xf => flags > 0,
        0x4 => flags == 0,
        0x5 => flags != 0,
        0xe => flags <= 0,
        _ => flags < 0,
    }
}

/// Where a jump of `len` bytes at `pc` goes.
fn jump(pc: usize, taken: bool, len: usize, rel: i64) -> Result<usize, String> {
    usize::try_from(pc as i64 + len as i64 + if taken { rel } else { 0 }).map_err(|_| format!("jump below the image at 0x{pc:x}"))
}

/// Runs the chain for one id: Some(scale) if it reaches `found`, None if it reaches the end.
fn run(xbe: &[u8], l: &Layout, id: i64) -> Result<Option<f32>, String> {
    let end = l.code.0 + l.code.1;
    let (mut a, mut s, mut flags) = (0i64, id, 0i64);
    let mut scale = None;
    let mut pc = l.code.0;
    for _ in 0..MAX_STEPS {
        if pc == end {
            return Ok(None);
        }
        if pc == l.found {
            return scale.map(Some).ok_or_else(|| "a row was reached before any scale was loaded".to_string());
        }
        if pc < l.code.0 || pc > end {
            return Err(format!("the chain left its range at 0x{pc:x}"));
        }
        let op = u8_at(xbe, pc)?;
        let op2 = u8_at(xbe, pc + 1).unwrap_or(0);
        pc = match (op, op2) {
            (0xB8, _) => {
                a = i32_at(xbe, pc + 1)?;
                pc + 5
            }
            (0x3B, 0xF0) => {
                flags = s - a;
                pc + 2
            }
            (0x83, 0xFE) => {
                flags = s - i8_at(xbe, pc + 2)?;
                pc + 3
            }
            (0x81, 0xFE) => {
                flags = s - i32_at(xbe, pc + 2)?;
                pc + 6
            }
            (0x83, 0xC0) => {
                a += i8_at(xbe, pc + 2)?;
                pc + 3
            }
            (0x83, 0xEE) => {
                s -= i8_at(xbe, pc + 2)?;
                flags = s;
                pc + 3
            }
            (0x81, 0xEE) => {
                s -= i32_at(xbe, pc + 2)?;
                flags = s;
                pc + 6
            }
            (0x4E, _) => {
                s -= 1;
                flags = s;
                pc + 1
            }
            (0xF3, 0x0F) if u8_at(xbe, pc + 2)? == 0x10 && u8_at(xbe, pc + 3)? == 0x05 => {
                scale = Some(f32_at(xbe, file_offset(xbe, u32_at(xbe, pc + 4)?)?)?);
                pc + 8
            }
            (0xE9, _) => jump(pc, true, 5, i32_at(xbe, pc + 1)?)?,
            (0xEB, _) => jump(pc, true, 2, i8_at(xbe, pc + 1)?)?,
            (0x7F | 0x74 | 0x75 | 0x7E | 0x7C, _) => jump(pc, cond(op, flags), 2, i8_at(xbe, pc + 1)?)?,
            (0x0F, 0x8F | 0x84 | 0x85 | 0x8E | 0x8C) => jump(pc, cond(op2, flags), 6, i32_at(xbe, pc + 2)?)?,
            _ => return Err(format!("unexpected code at 0x{pc:x}")),
        };
    }
    Err(format!("id {id}: the chain did not end within {MAX_STEPS} steps"))
}

/// Reads the table from an executable image of the accepted version (the caller has checked its hash).
pub fn extract(xbe: &[u8]) -> Result<Vec<Row>, String> {
    extract_with(xbe, &LAYOUT)
}

fn extract_with(xbe: &[u8], l: &Layout) -> Result<Vec<Row>, String> {
    if xbe.len() != l.size {
        return Err(format!("the executable is {} bytes, expected {}", xbe.len(), l.size));
    }
    let mut rows = BTreeMap::new();
    for id in 0..=0xFFFF_u32 {
        if let Some(scale) = run(xbe, l, id as i64)? {
            rows.insert(id, scale);
        }
    }
    // the lists' ids take the lists' scale, also where the chain gave one
    let list_scale = f32_at(xbe, l.list_scale)?;
    for &(at, count) in l.lists {
        for i in 0..count {
            let id = i32_at(xbe, at + 4 * i)?;
            let id = u32::try_from(id).ok().filter(|&v| v <= 0xFFFF).ok_or_else(|| format!("listed id {id} out of range"))?;
            rows.insert(id, list_scale);
        }
    }
    if rows.is_empty() {
        return Err("the table is empty".into());
    }
    if let Some((id, s)) = rows.iter().find(|(_, s)| !(s.is_finite() && **s > 0.0 && **s <= 1.0)) {
        return Err(format!("id {id}: scale {s} out of range"));
    }
    Ok(rows.into_iter().collect())
}

/// "id scale" per line, '\n' line ends.
pub fn to_text(rows: &[Row]) -> String {
    rows.iter().map(|(id, s)| format!("{id} {s}\n")).collect()
}

/// Parses `to_text` output (blank lines and lines starting with '#' are skipped); a bad line is an error.
pub fn parse_text(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("line {}: {line:?}", n + 1);
        let mut it = line.split_whitespace();
        let (Some(id), Some(s), None) = (it.next(), it.next(), it.next()) else { return Err(bad()) };
        rows.push((id.parse().map_err(|_| bad())?, s.parse().map_err(|_| bad())?));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE_VA: u32 = 0x20000;
    const CODE_AT: usize = 0x1000;
    const DATA_VA: u32 = 0x30000;
    const DATA_AT: usize = 0x2000;
    const SIZE: usize = 0x2100;

    /// A small made-up image: header, two sections (code, data), the code at CODE_AT, the floats
    /// at the start of the data section, then the lists.
    fn image(code: &[u8], found: usize, floats: &[f32], lists: &[&[i32]], list_scale: usize) -> (Vec<u8>, Layout) {
        let mut x = vec![0u8; SIZE];
        let put = |x: &mut Vec<u8>, at: usize, v: u32| x[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut x, 0x104, 0x10000);
        put(&mut x, 0x11c, 2);
        put(&mut x, 0x120, 0x10000 + 0x200);
        for (i, (va, at, size)) in [(CODE_VA, CODE_AT, code.len()), (DATA_VA, DATA_AT, SIZE - DATA_AT)].into_iter().enumerate() {
            let h = 0x200 + i * 56;
            put(&mut x, h + 4, va);
            put(&mut x, h + 8, size as u32);
            put(&mut x, h + 12, at as u32);
            put(&mut x, h + 16, size as u32);
        }
        x[CODE_AT..CODE_AT + code.len()].copy_from_slice(code);
        for (i, f) in floats.iter().enumerate() {
            put(&mut x, DATA_AT + 4 * i, f.to_bits());
        }
        let mut at = DATA_AT + 4 * floats.len();
        let mut spans = Vec::new();
        for list in lists {
            spans.push((at, list.len()));
            for &id in *list {
                put(&mut x, at, id as u32);
                at += 4;
            }
        }
        let layout = Layout { size: SIZE, code: (CODE_AT, code.len()), found: CODE_AT + found, lists: Box::leak(spans.into_boxed_slice()), list_scale: DATA_AT + 4 * list_scale };
        (x, layout)
    }

    /// Scales of the test chain, at DATA_VA + 4 * index: A, B, C, D, then the list scale E.
    const SCALES: [f32; 5] = [0.25, 0.5, 0.75, 1.0, 0.6];

    /// The test chain, with every decoded byte form used at least once:
    /// - id 5 -> A (an exact compare);
    /// - ids below 5 and 6..=299 -> no row;
    /// - ids 300, 301, 302 -> B (a range: >= 300 and <= 302);
    /// - id 303 -> no row (an explicit jump to the end);
    /// - id 1000 -> C (s - 1000 == 0);
    /// - id 999 -> D (s - 1000 = -1, then s - (-1) = 0: the sign-extended short immediate);
    /// - everything else -> no row. Two bytes that are not a known form (0xCC) sit where no id goes.
    fn chain() -> (Vec<u8>, usize) {
        let mut c: Vec<u8> = Vec::new();
        let va = |i: u32| (DATA_VA + 4 * i).to_le_bytes();
        let rel8 = |c: &mut Vec<u8>, at: usize, to: usize| c[at] = (to as i64 - (at as i64 + 1)) as i8 as u8;
        let rel32 = |c: &mut Vec<u8>, at: usize, to: usize| c[at..at + 4].copy_from_slice(&((to as i64 - (at as i64 + 4)) as i32).to_le_bytes());
        c.extend([0x83, 0xFE, 5]); //  0: flags = s - 5
        c.extend([0x74, 0]); //            3: == 0 -> LA
        c.extend([0x7C, 0]); //            5: < 0 -> END_J
        c.extend([0x81, 0xFE, 0x2C, 0x01, 0, 0]); // 7: flags = s - 300
        c.extend([0x0F, 0x8C, 0, 0, 0, 0]); // 13: < 0 -> END_J
        c.extend([0xB8, 0x2E, 0x01, 0, 0]); // 19: a = 302
        c.extend([0x3B, 0xF0]); //        24: flags = s - a
        c.extend([0x0F, 0x8E, 0, 0, 0, 0]); // 26: <= 0 -> LB
        c.extend([0x83, 0xC0, 1]); //     32: a += 1 (303)
        c.extend([0x3B, 0xF0]); //        35: flags = s - a
        c.extend([0x75, 0]); //           37: != 0 -> L2
        c.extend([0xE9, 0, 0, 0, 0]); //  39: -> END (id 303)
        c.extend([0x81, 0xEE, 0xE8, 0x03, 0, 0]); // 44 L2: s -= 1000
        c.extend([0x0F, 0x85, 0, 0, 0, 0]); // 50: != 0 -> L3
        c.extend([0xEB, 0]); //           56: -> LC (id 1000)
        c.extend([0x83, 0xEE, 0xFF]); //  58 L3: s -= -1
        c.extend([0x0F, 0x84, 0, 0, 0, 0]); // 61: == 0 -> LD (id 999)
        c.push(0x4E); //                  67: s -= 1
        c.extend([0x7E, 0]); //           68: <= 0 -> END_J
        c.extend([0x7F, 0]); //           70: > 0 -> L4 (always taken here)
        c.push(0xCC); //                  72: never reached
        c.extend([0x0F, 0x8F, 0, 0, 0, 0]); // 73 L4: > 0 -> END_J (always taken here)
        c.push(0xCC); //                  79: never reached
        for (i, label) in [(0u32, 80usize), (1, 90), (2, 103), (3, 113)] {
            assert_eq!(c.len(), label);
            c.extend([0xF3, 0x0F, 0x10, 0x05]);
            c.extend(va(i));
            if i == 1 {
                c.extend([0xE9, 0, 0, 0, 0]); // 98: -> FOUND (long)
            } else {
                c.extend([0xEB, 0]); // -> FOUND (short)
            }
        }
        assert_eq!(c.len(), 123);
        c.push(0xCC); //                 123 FOUND (never decoded)
        c.extend([0xE9, 0, 0, 0, 0]); // 124 END_J: -> END
        let end = c.len(); //            129 END
        let (la, lb, lc, ld, found, end_j) = (80, 90, 103, 113, 123, 124);
        rel8(&mut c, 4, la);
        rel8(&mut c, 6, end_j);
        rel32(&mut c, 15, end_j);
        rel32(&mut c, 28, lb);
        rel8(&mut c, 38, 44);
        rel32(&mut c, 40, end);
        rel32(&mut c, 52, 58);
        rel8(&mut c, 57, lc);
        rel32(&mut c, 63, ld);
        rel8(&mut c, 69, end_j);
        rel8(&mut c, 71, 73);
        rel32(&mut c, 75, end_j);
        rel8(&mut c, 89, found);
        rel32(&mut c, 99, found);
        rel8(&mut c, 112, found);
        rel8(&mut c, 122, found);
        rel32(&mut c, 125, end);
        (c, found)
    }

    #[test]
    fn file_offsets() {
        let (c, found) = chain();
        let (x, _) = image(&c, found, &SCALES, &[], 4);
        assert_eq!(file_offset(&x, CODE_VA + 7), Ok(CODE_AT + 7));
        assert_eq!(file_offset(&x, DATA_VA + 8), Ok(DATA_AT + 8));
        assert!(file_offset(&x, CODE_VA + c.len() as u32).is_err());
        assert!(file_offset(&x, 0x50000).is_err());
    }

    #[test]
    fn chain_rows() {
        let (c, found) = chain();
        let (x, l) = image(&c, found, &SCALES, &[], 4);
        let rows = extract_with(&x, &l).unwrap();
        assert_eq!(rows, vec![(5, 0.25), (300, 0.5), (301, 0.5), (302, 0.5), (999, 1.0), (1000, 0.75)]);
    }

    #[test]
    fn lists_replace_chain_rows() {
        let (c, found) = chain();
        let (x, l) = image(&c, found, &SCALES, &[&[5, 7], &[999]], 4);
        let rows = extract_with(&x, &l).unwrap();
        assert_eq!(rows, vec![(5, 0.6), (7, 0.6), (300, 0.5), (301, 0.5), (302, 0.5), (999, 0.6), (1000, 0.75)]);
    }

    #[test]
    fn errors() {
        let (c, found) = chain();
        // an unknown byte on a path an id takes (the first byte: every id)
        let mut bad = c.clone();
        bad[0] = 0x90;
        let (x, l) = image(&bad, found, &SCALES, &[], 4);
        assert!(extract_with(&x, &l).unwrap_err().contains("unexpected code"));
        // wrong length
        let (mut x, l) = image(&c, found, &SCALES, &[], 4);
        x.push(0);
        assert!(extract_with(&x, &l).is_err());
        // a scale of 0 and one above 1
        for s in [0.0, 1.5] {
            let (x, l) = image(&c, found, &[s, 0.5, 0.75, 1.0, 0.6], &[], 4);
            assert!(extract_with(&x, &l).unwrap_err().contains("out of range"));
        }
        // a listed id out of range
        let (x, l) = image(&c, found, &SCALES, &[&[-1]], 4);
        assert!(extract_with(&x, &l).is_err());
        // a jump out of the range
        let mut out = c.clone();
        out[4] = 0x7F; // 3: == 0 -> 3 + 2 + 127, past the end
        let (x, l) = image(&out, found, &SCALES, &[], 4);
        assert!(extract_with(&x, &l).unwrap_err().contains("left its range"));
        // a loop that never ends
        let mut lp = c.clone();
        lp[0..2].copy_from_slice(&[0xEB, 0xFE]); // jump to itself
        let (x, l) = image(&lp, found, &SCALES, &[], 4);
        assert!(extract_with(&x, &l).unwrap_err().contains("did not end"));
    }

    #[test]
    fn text_round_trip() {
        let rows = vec![(5, 0.25), (300, 0.5), (1000, 0.75)];
        assert_eq!(parse_text(&to_text(&rows)), Ok(rows));
        assert_eq!(parse_text("# comment\n\n7 0.5\n"), Ok(vec![(7, 0.5)]));
        assert!(parse_text("7\n").is_err());
        assert!(parse_text("7 0.5 1\n").is_err());
        assert!(parse_text("x 0.5\n").is_err());
    }
}

//! Text file parsers for Vice City IDE and IPL formats.

use crate::{Error, Result};

/// A single parsed line from an IDE/IPL section.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// 1-based line number in the original file.
    pub number: usize,
    /// Whitespace-separated tokens.
    pub tokens: Vec<String>,
}

/// A named section containing lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Section name (lowercased).
    pub name: String,
    /// Data lines within the section.
    pub lines: Vec<Line>,
}

/// Parse raw bytes into sections.
///
/// Implements the Vice City IDE/IPL line processing rules:
/// - Map bytes to chars (Latin-1).
/// - Replace control chars (< ' ') and ',' with space.
/// - Strip leading/trailing whitespace.
/// - Skip empty lines and lines starting with '#'.
/// - Detect section boundaries: first token opens a section, "end" closes it.
pub fn sections(data: &[u8]) -> Vec<Section> {
    let mut result: Vec<Section> = Vec::new();
    let mut in_section = false;
    for (i, raw) in data.split(|&b| b == b'\n').enumerate() {
        let processed: String = raw
            .iter()
            .map(|&b| {
                if b < b' ' || b == b',' {
                    ' '
                } else {
                    b as char
                }
            })
            .collect();
        let line = processed.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<String> = line.split_whitespace().map(String::from).collect();
        if !in_section {
            result.push(Section {
                name: tokens[0].to_lowercase(),
                lines: Vec::new(),
            });
            in_section = true;
        } else if line.starts_with("end") {
            in_section = false;
        } else {
            result.last_mut().unwrap().lines.push(Line {
                number: i + 1,
                tokens,
            });
        }
    }
    result
}

/// IDE "objs" / "tobj" entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDef {
    pub id: i32,
    pub model: String,
    pub txd: String,
    pub draw_dists: Vec<f32>,
    pub flags: i32,
    pub time_on_off: Option<(i32, i32)>,
}

/// IPL "inst" entry. `rot` is the quaternion as stored (x, y, z, w).
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub id: i32,
    pub model: String,
    pub area: i32,
    pub pos: [f32; 3],
    pub scale: [f32; 3],
    pub rot: [f32; 4],
}

/// Parse an IDE "objs" or "tobj" entry.
///
/// Format: `id model txd numObjs dist*numObjs flags` (+ `timeOn timeOff` if timed).
/// `numObjs` must be 1..=3. Total token count must match: 4 + numObjs + 1 (+2 if timed).
pub fn parse_object_def(line: &Line, timed: bool) -> Result<ObjectDef> {
    let tokens = &line.tokens;
    let num_tokens = tokens.len();

    // Parse id
    let id: i32 = tokens
        .first()
        .ok_or_else(|| Error::Invalid(format!("line {}: missing id", line.number)))?
        .parse::<i32>()
        .map_err(|_| Error::Invalid(format!("line {}: invalid id '{}'", line.number, tokens[0])))?;

    // Parse model
    let model = tokens
        .get(1)
        .ok_or_else(|| Error::Invalid(format!("line {}: missing model", line.number)))?
        .clone();

    // Parse txd
    let txd = tokens
        .get(2)
        .ok_or_else(|| Error::Invalid(format!("line {}: missing txd", line.number)))?
        .clone();

    // Parse numObjs
    let num_objs: i32 = tokens
        .get(3)
        .ok_or_else(|| Error::Invalid(format!("line {}: missing numObjs", line.number)))?
        .parse::<i32>()
        .map_err(|_| {
            Error::Invalid(format!(
                "line {}: invalid numObjs '{}'",
                line.number, tokens[3]
            ))
        })?;

    if !(1..=3).contains(&num_objs) {
        return Err(Error::Invalid(format!(
            "line {}: numObjs must be 1..=3, got {}",
            line.number, num_objs
        )));
    }

    let num_objs_usize = num_objs as usize;

    // Calculate expected total tokens
    let expected_total = if timed {
        4 + num_objs_usize + 1 + 2 // id, model, txd, numObjs, dists..., flags, timeOn, timeOff
    } else {
        4 + num_objs_usize + 1 // id, model, txd, numObjs, dists..., flags
    };

    if num_tokens != expected_total {
        return Err(Error::Invalid(format!(
            "line {}: expected {} tokens, got {}",
            line.number, expected_total, num_tokens
        )));
    }

    // Parse draw distances
    let mut draw_dists = Vec::with_capacity(num_objs_usize);
    for i in 0..num_objs_usize {
        let dist: f32 = tokens
            .get(4 + i)
            .ok_or_else(|| Error::Invalid(format!("line {}: missing draw dist", line.number)))?
            .parse::<f32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid draw dist '{}'",
                    line.number,
                    tokens[4 + i]
                ))
            })?;
        draw_dists.push(dist);
    }

    // Parse flags
    let flags_idx = 4 + num_objs_usize;
    let flags: i32 = tokens
        .get(flags_idx)
        .ok_or_else(|| Error::Invalid(format!("line {}: missing flags", line.number)))?
        .parse::<i32>()
        .map_err(|_| {
            Error::Invalid(format!(
                "line {}: invalid flags '{}'",
                line.number, tokens[flags_idx]
            ))
        })?;

    // Parse time on/off if timed
    let time_on_off = if timed {
        let time_on: i32 = tokens
            .get(flags_idx + 1)
            .ok_or_else(|| Error::Invalid(format!("line {}: missing timeOn", line.number)))?
            .parse::<i32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid timeOn '{}'",
                    line.number,
                    tokens[flags_idx + 1]
                ))
            })?;
        let time_off: i32 = tokens
            .get(flags_idx + 2)
            .ok_or_else(|| Error::Invalid(format!("line {}: missing timeOff", line.number)))?
            .parse::<i32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid timeOff '{}'",
                    line.number,
                    tokens[flags_idx + 2]
                ))
            })?;
        Some((time_on, time_off))
    } else {
        None
    };

    Ok(ObjectDef {
        id,
        model,
        txd,
        draw_dists,
        flags,
        time_on_off,
    })
}

/// Extract object definitions from "objs" and "tobj" sections.
pub fn object_defs(sections: &[Section]) -> Result<Vec<ObjectDef>> {
    let mut defs = Vec::new();
    for section in sections
        .iter()
        .filter(|s| s.name == "objs" || s.name == "tobj")
    {
        let timed = section.name == "tobj";
        for line in &section.lines {
            defs.push(parse_object_def(line, timed)?);
        }
    }
    Ok(defs)
}

/// Parse an IPL "inst" entry.
///
/// Format: `id model [area] x y z sx sy sz rx ry rz rw`
/// 12 tokens (area absent, defaults to 0) or 13 tokens (area present).
pub fn parse_instance(line: &Line) -> Result<Instance> {
    let tokens = &line.tokens;
    let num_tokens = tokens.len();

    if num_tokens != 12 && num_tokens != 13 {
        return Err(Error::Invalid(format!(
            "line {}: expected 12 or 13 tokens, got {}",
            line.number, num_tokens
        )));
    }

    // Parse id
    let id: i32 = tokens
        .first()
        .ok_or_else(|| Error::Invalid(format!("line {}: missing id", line.number)))?
        .parse::<i32>()
        .map_err(|_| Error::Invalid(format!("line {}: invalid id '{}'", line.number, tokens[0])))?;

    // Parse model
    let model = tokens
        .get(1)
        .ok_or_else(|| Error::Invalid(format!("line {}: missing model", line.number)))?
        .clone();

    // Parse area (optional, index 2 if present)
    let area = if num_tokens == 13 {
        let area_f: f32 = tokens
            .get(2)
            .ok_or_else(|| Error::Invalid(format!("line {}: missing area", line.number)))?
            .parse::<f32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid area '{}'",
                    line.number, tokens[2]
                ))
            })?;
        area_f as i32
    } else {
        0
    };

    // Parse position (x, y, z)
    let pos_start = if num_tokens == 13 { 3 } else { 2 };
    let mut pos = [0.0f32; 3];
    for i in 0..3 {
        pos[i] = tokens
            .get(pos_start + i)
            .ok_or_else(|| {
                Error::Invalid(format!("line {}: missing position component", line.number))
            })?
            .parse::<f32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid position '{}'",
                    line.number,
                    tokens[pos_start + i]
                ))
            })?;
    }

    // Parse scale (sx, sy, sz)
    let scale_start = pos_start + 3;
    let mut scale = [0.0f32; 3];
    for i in 0..3 {
        scale[i] = tokens
            .get(scale_start + i)
            .ok_or_else(|| {
                Error::Invalid(format!("line {}: missing scale component", line.number))
            })?
            .parse::<f32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid scale '{}'",
                    line.number,
                    tokens[scale_start + i]
                ))
            })?;
    }

    // Parse rotation (rx, ry, rz, rw)
    let rot_start = scale_start + 3;
    let mut rot = [0.0f32; 4];
    for i in 0..4 {
        rot[i] = tokens
            .get(rot_start + i)
            .ok_or_else(|| {
                Error::Invalid(format!("line {}: missing rotation component", line.number))
            })?
            .parse::<f32>()
            .map_err(|_| {
                Error::Invalid(format!(
                    "line {}: invalid rotation '{}'",
                    line.number,
                    tokens[rot_start + i]
                ))
            })?;
    }

    Ok(Instance {
        id,
        model,
        area,
        pos,
        scale,
        rot,
    })
}

/// Extract instances from "inst" sections.
pub fn instances(sections: &[Section]) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    for section in sections {
        if section.name == "inst" {
            for line in &section.lines {
                instances.push(parse_instance(line)?);
            }
        }
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commas_tabs_cr() {
        // Test that commas, tabs, and \r are replaced by spaces
        let data = b"section\r\n\t,data,\t7\r\nend\r\n";
        let secs = sections(data);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].name, "section");
        assert_eq!(secs[0].lines.len(), 1);
        assert_eq!(secs[0].lines[0].tokens, vec!["data", "7"]);
        assert_eq!(secs[0].lines[0].number, 2);
    }

    #[test]
    fn test_ignores_other_sections_and_latin1() {
        let data = b"2dfx\n1 2 3\nend\nobjs\n5, caf\xe9, txd, 1, 50, 0\nend\n";
        let defs = object_defs(&sections(data)).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].model, "caf\u{e9}");
    }

    #[test]
    fn test_comment_lines() {
        let data = b"# comment\nsection\ndata\n# another comment\nend";
        let secs = sections(data);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].lines.len(), 1);
        assert_eq!(secs[0].lines[0].tokens, vec!["data"]);
    }

    #[test]
    fn test_end_closing() {
        let data = b"section\ndata1\nend\nother\ndata2\nend";
        let secs = sections(data);
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].name, "section");
        assert_eq!(secs[0].lines.len(), 1);
        assert_eq!(secs[0].lines[0].tokens, vec!["data1"]);
        assert_eq!(secs[1].name, "other");
        assert_eq!(secs[1].lines.len(), 1);
        assert_eq!(secs[1].lines[0].tokens, vec!["data2"]);
    }

    #[test]
    fn test_unclosed_section() {
        let data = b"section\ndata1";
        let secs = sections(data);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].name, "section");
        assert_eq!(secs[0].lines.len(), 1);
        assert_eq!(secs[0].lines[0].tokens, vec!["data1"]);
    }

    #[test]
    fn test_objs_one_distance() {
        let data = b"objs\n1 model txd 1 10.0 0\nend";
        let secs = sections(data);
        let defs = object_defs(&secs).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, 1);
        assert_eq!(defs[0].model, "model");
        assert_eq!(defs[0].txd, "txd");
        assert_eq!(defs[0].draw_dists, vec![10.0]);
        assert_eq!(defs[0].flags, 0);
        assert!(defs[0].time_on_off.is_none());
    }

    #[test]
    fn test_objs_three_distances() {
        let data = b"objs\n1 model txd 3 10.0 20.0 30.0 1\nend";
        let secs = sections(data);
        let defs = object_defs(&secs).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].draw_dists, vec![10.0, 20.0, 30.0]);
        assert_eq!(defs[0].flags, 1);
    }

    #[test]
    fn test_tobj() {
        let data = b"tobj\n1 model txd 1 10.0 0 100 200\nend";
        let secs = sections(data);
        let defs = object_defs(&secs).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].time_on_off, Some((100, 200)));
    }

    #[test]
    fn test_inst_12_tokens() {
        let data = b"inst\n1 model 0 0 0 1 1 1 0 0 0 1\nend";
        let secs = sections(data);
        let insts = instances(&secs).unwrap();
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].id, 1);
        assert_eq!(insts[0].model, "model");
        assert_eq!(insts[0].area, 0);
        assert_eq!(insts[0].pos, [0.0, 0.0, 0.0]);
        assert_eq!(insts[0].scale, [1.0, 1.0, 1.0]);
        assert_eq!(insts[0].rot, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_inst_13_tokens() {
        let data = b"inst\n1 model 5 0 0 0 1 1 1 0 0 0 1\nend";
        let secs = sections(data);
        let insts = instances(&secs).unwrap();
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].area, 5);
    }

    #[test]
    fn test_wrong_token_count() {
        let data = b"objs\n1 model txd 1 10.0\nend"; // Missing flags
        let secs = sections(data);
        let res = object_defs(&secs);
        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            Error::Invalid(msg) => assert!(msg.contains("line 2")),
            _ => panic!("Expected Error::Invalid"),
        }
    }
}

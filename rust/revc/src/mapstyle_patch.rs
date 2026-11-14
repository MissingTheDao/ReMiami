//! Map data per style (roadmap R4), the text part: a PC map file patched with the console's
//! version of it, keeping the PC's ids and line order (scripts and saves refer to both). Objects
//! match by model name, placements by model name and position rounded to a metre (moved ones
//! count as removed and added). See mapstyle.rs for the files and docs/CONSOLE_STYLES.md.

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdeStats {
    pub added: usize,
    pub removed: usize,
    pub txd: usize,
    pub drawdist: usize,
    pub flags: usize,
    pub format: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IplStats {
    pub added: usize,
    pub removed: usize,
    pub area: usize,
}

/// One line of a map file, classified.
enum Line<'a> {
    /// Blank, comment or anything outside a section: copied as it is.
    Other,
    Header(String),
    End,
    /// A data line of the section that is open, split into trimmed fields.
    Data(&'a str, Vec<&'a str>),
}

fn fields(s: &str) -> Vec<&str> {
    s.split(',').map(str::trim).collect()
}

/// The file's lines with their kind and the section each data line is in.
fn classify(text: &str) -> Vec<(&str, Line<'_>, String)> {
    let mut sec = String::new();
    let mut open = false;
    text.lines()
        .map(|raw| {
            let line = raw.trim_end_matches('\r');
            let t = line.split('#').next().unwrap_or("").trim();
            let kind = if t.is_empty() {
                Line::Other
            } else if t.eq_ignore_ascii_case("end") {
                open = false;
                Line::End
            } else if !open && t.chars().all(|c| c.is_ascii_alphanumeric()) {
                sec = t.to_ascii_lowercase();
                open = true;
                Line::Header(sec.clone())
            } else if open {
                Line::Data(t, fields(t))
            } else {
                Line::Other
            };
            (line, kind, sec.clone())
        })
        .collect()
}

fn name_of(f: &[&str]) -> Option<String> {
    f.get(1).map(|n| n.to_ascii_lowercase())
}

fn with_id(id: &str, f: &[&str]) -> String {
    std::iter::once(id).chain(f[1..].iter().copied()).collect::<Vec<_>>().join(", ")
}

/// The PC IDE file with the console's attributes for its objects (objs, tobj) and the console's
/// new objects (ids from `next_id`, recorded in `new_ids`) at the end of their section.
pub(crate) fn patch_ide(pc: &str, console: &str, next_id: &mut dyn FnMut() -> Option<i32>, new_ids: &mut Vec<(String, i32)>) -> (String, IdeStats) {
    let patched = |s: &str| s == "objs" || s == "tobj";
    // the console's objects by section and name, in file order
    let mut con: HashMap<String, Vec<(String, Vec<&str>)>> = HashMap::new();
    for (_, kind, sec) in classify(console) {
        if let Line::Data(_, f) = kind
            && patched(&sec)
            && let Some(n) = name_of(&f)
        {
            con.entry(sec).or_default().push((n, f));
        }
    }
    let pc_lines = classify(pc);
    let mut pc_names: HashMap<String, Vec<String>> = HashMap::new();
    for (_, kind, sec) in &pc_lines {
        if let Line::Data(_, f) = kind
            && let Some(n) = name_of(f)
        {
            pc_names.entry(sec.clone()).or_default().push(n);
        }
    }
    let mut st = IdeStats::default();
    let mut out = Vec::with_capacity(pc_lines.len() + 8);
    for (raw, kind, sec) in &pc_lines {
        match kind {
            Line::Data(_, f) if patched(sec) && f.len() > 1 => {
                let n = f[1].to_ascii_lowercase();
                match con.get(sec).and_then(|v| v.iter().find(|(cn, _)| *cn == n)) {
                    None => {
                        st.removed += 1;
                        out.push(raw.to_string());
                    }
                    Some((_, cf)) => {
                        let (x, y) = (&f[1..], &cf[1..]);
                        if x == y {
                            out.push(raw.to_string());
                            continue;
                        }
                        if x.len() != y.len() {
                            st.format += 1;
                        } else {
                            for i in 1..x.len() {
                                if x[i] != y[i] {
                                    match i {
                                        1 => st.txd += 1,
                                        _ if i < x.len() - 1 => st.drawdist += 1,
                                        _ => st.flags += 1,
                                    }
                                }
                            }
                        }
                        out.push(with_id(f[0], cf));
                    }
                }
            }
            Line::End if patched(sec) => {
                let known = pc_names.get(sec).cloned().unwrap_or_default();
                for (n, cf) in con.get(sec).map(Vec::as_slice).unwrap_or(&[]) {
                    if known.contains(n) || new_ids.iter().any(|(m, _)| m == n) {
                        continue;
                    }
                    st.added += 1;
                    if let Some(id) = next_id() {
                        new_ids.push((n.clone(), id));
                        out.push(with_id(&id.to_string(), cf));
                    }
                }
                out.push(raw.to_string());
            }
            _ => out.push(raw.to_string()),
        }
    }
    (out.join("\n") + "\n", st)
}

/// A placement's key: model name and position rounded to a metre.
fn inst_key(f: &[&str]) -> Option<(String, i64, i64, i64)> {
    let at = if f.len() >= 13 { 3 } else { 2 };
    let p = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok()).map(|v| v.round() as i64);
    Some((f.get(1)?.to_ascii_lowercase(), p(at)?, p(at + 1)?, p(at + 2)?))
}

/// The PC IPL file with the console's placements: matched ones take the console's line (with the
/// PC's id), unmatched ones stay with their name marked `!` (created hidden, without collision, so
/// the pools keep their order), and the console's extra placements come back for the end of the
/// level (ids from `id_of`).
pub(crate) fn patch_ipl(pc: &str, console: &str, id_of: &dyn Fn(&str) -> Option<i32>) -> (String, Vec<String>, IplStats) {
    let mut con: HashMap<(String, i64, i64, i64), VecDeque<usize>> = HashMap::new();
    let con_lines: Vec<Vec<&str>> = classify(console)
        .into_iter()
        .filter_map(|(_, kind, sec)| match kind {
            Line::Data(_, f) if sec == "inst" => Some(f),
            _ => None,
        })
        .collect();
    for (i, f) in con_lines.iter().enumerate() {
        if let Some(k) = inst_key(f) {
            con.entry(k).or_default().push_back(i);
        }
    }
    let mut used = vec![false; con_lines.len()];
    let mut st = IplStats::default();
    let mut out = Vec::new();
    for (raw, kind, sec) in classify(pc) {
        match kind {
            Line::Data(_, f) if sec == "inst" && f.len() > 1 => match inst_key(&f).and_then(|k| con.get_mut(&k)).and_then(VecDeque::pop_front) {
                Some(i) => {
                    used[i] = true;
                    let cf = &con_lines[i];
                    if f.len() >= 13 && cf.len() >= 13 && f[2] != cf[2] {
                        st.area += 1;
                    }
                    out.push(with_id(f[0], cf));
                }
                None => {
                    st.removed += 1;
                    let mut g: Vec<String> = f.iter().map(|s| s.to_string()).collect();
                    g[1] = format!("!{}", g[1]);
                    out.push(g.join(", "));
                }
            },
            _ => out.push(raw.to_string()),
        }
    }
    let mut extra = Vec::new();
    for (i, f) in con_lines.iter().enumerate() {
        if used[i] || f.len() < 2 {
            continue;
        }
        st.added += 1;
        if let Some(id) = id_of(&f[1].to_ascii_lowercase()) {
            extra.push(with_id(&id.to_string(), f));
        }
    }
    (out.join("\n") + "\n", extra, st)
}

/// Every (id, model name) an IDE file declares, in any section.
pub(crate) fn ids_in_use(ide: &str) -> Vec<(i32, String)> {
    classify(ide)
        .into_iter()
        .filter_map(|(_, kind, _)| match kind {
            Line::Data(_, f) => Some((f.first()?.parse().ok()?, name_of(&f)?)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_ide() {
        let pc = "objs\n10, a, t, 1, 100, 0\n11, b, t, 1, 100, 0\nend\ntobj\n20, c, t, 1, 50, 0, 6, 20\nend\n";
        let console = "objs\n500, A, t, 1, 170, 4\n501, newone, t2, 1, 80, 0\nend\ntobj\n600, c, t, 1, 50, 0, 6, 20\nend\n";
        let mut counter = 900;
        let mut next_id = || {
            counter += 1;
            Some(counter - 1)
        };
        let mut new_ids = Vec::new();
        let (output, stats) = patch_ide(pc, console, &mut next_id, &mut new_ids);
        
        assert!(output.contains("10, A, t, 1, 170, 4"));
        assert!(output.contains("11, b, t, 1, 100, 0"));
        assert!(output.contains("900, newone, t2, 1, 80, 0"));
        assert!(output.contains("20, c, t, 1, 50, 0, 6, 20"));
        
        assert_eq!(stats.drawdist, 1);
        assert_eq!(stats.flags, 1);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.added, 1);
        assert_eq!(stats.txd, 0);
        assert_eq!(new_ids, vec![("newone".to_string(), 900)]);
    }

    #[test]
    fn test_patch_ipl() {
        let pc = "inst\n10, a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\n11, b, 0, 5, 5, 5, 1, 1, 1, 0, 0, 0, 1\nend\n";
        let console = "inst\n500, a, 13, 1.2, 2.1, 2.9, 1, 1, 1, 0, 0, 0, 1\n501, c, 0, 9, 9, 9, 1, 1, 1, 0, 0, 0, 1\nend\n";
        let id_of = |name: &str| {
            if name == "c" { Some(77) } else { None }
        };
        let (output, appended, stats) = patch_ipl(pc, console, &id_of);
        
        assert!(output.contains("10, a, 13, 1.2, 2.1, 2.9, 1, 1, 1, 0, 0, 0, 1"));
        assert!(output.contains("11, !b, 0, 5, 5, 5, 1, 1, 1, 0, 0, 0, 1"));
        assert_eq!(appended, vec!["77, c, 0, 9, 9, 9, 1, 1, 1, 0, 0, 0, 1"]);
        assert_eq!(stats.area, 1);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.added, 1);
    }

    #[test]
    fn test_patch_ipl_duplicates() {
        let pc = "inst\n10, a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\n11, a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\nend\n";
        let console = "inst\n500, a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\nend\n";
        let id_of = |_: &str| None;
        let (output, _appended, stats) = patch_ipl(pc, console, &id_of);
        
        // One kept, one removed
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.added, 0);
        assert!(output.contains("10, a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1"));
        assert!(output.contains("11, !a, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1"));
    }

    #[test]
    fn test_patch_ide_preserves_comments_and_sections() {
        let pc = "# comment\nobjs\n10, a, t, 1, 100, 0\nend\ncull\n10, a\nend\n";
        let console = "objs\n500, a, t, 1, 100, 0\nend\ncull\n500, a\nend\n";
        let mut next_id = || None;
        let mut new_ids = Vec::new();
        let (output, _) = patch_ide(pc, console, &mut next_id, &mut new_ids);
        
        assert!(output.contains("# comment"));
        assert!(output.contains("cull"));
        assert!(output.contains("10, a, t, 1, 100, 0"));
    }

    #[test]
    fn test_ids_in_use() {
        let ide = "objs\n10, a, t\nend\nhier\n20, B, x\nend\n";
        let result = ids_in_use(ide);
        assert_eq!(result, vec![(10, "a".to_string()), (20, "b".to_string())]);
    }
}

//! Save files (plan task 15, save): the game's file check (CheckDataNotCorrupt) runs on
//! revc-formats::save. C++ reads the file and hands over the bytes.

use revc_formats::save;

/// Checks a whole save file; on success stores the level from its first block and returns 1.
///
/// # Safety
/// `data` points to `len` bytes; `level` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_check(data: *const u8, len: usize, level: *mut i32) -> i32 {
    if data.is_null() {
        return 0;
    }
    match save::check(unsafe { std::slice::from_raw_parts(data, len) }) {
        Some(c) => {
            unsafe { *level = c.level };
            1
        }
        None => 0,
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/savecheck.txt: mutations of tests/saves/GTAVCsf1.b and what the
    //! original C++ CheckDataNotCorrupt said about each.
    use super::*;

    fn base() -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/saves/GTAVCsf1.b")).unwrap()
    }

    /// Applies a recipe: `set32 off val`, `xor off byte`, `trunc len`, `fixsum`, `append n`,
    /// separated by `+`.
    fn mutate(base: &[u8], recipe: &str) -> Vec<u8> {
        let mut d = base.to_vec();
        for step in recipe.split('+').filter(|s| !s.is_empty() && *s != "none") {
            let f: Vec<&str> = step.split(':').collect();
            match f[0] {
                "set32" => {
                    let (off, v): (usize, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                    if off + 4 <= d.len() {
                        d[off..off + 4].copy_from_slice(&v.to_le_bytes());
                    }
                }
                "xor" => {
                    let (off, x): (usize, u8) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                    if off < d.len() {
                        d[off] ^= x;
                    }
                }
                "trunc" => d.truncate(f[1].parse().unwrap()),
                "fixsum" => {
                    let n = d.len() - 4;
                    let sum = d[..n].iter().fold(0i32, |s, &b| s.wrapping_add(b as i32));
                    d[n..].copy_from_slice(&sum.to_le_bytes());
                }
                "append" => d.extend(std::iter::repeat_n(0x5a, f[1].parse().unwrap())),
                op => panic!("unknown step {op}"),
            }
        }
        d
    }

    #[test]
    fn matches_cpp_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/savecheck.txt");
        let data = std::fs::read_to_string(&path).expect("tests/golden/savecheck.txt");
        let base = base();
        let mut n = 0;
        for line in data.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (recipe, want) = line.split_once(" => ").unwrap();
            let got = match save::check(&mutate(&base, recipe)) {
                Some(c) => format!("ok {}", c.level),
                None => "bad".into(),
            };
            assert_eq!(got, want, "recipe {recipe}");
            n += 1;
        }
        assert!(n >= 300, "only {n} cases");
    }
}

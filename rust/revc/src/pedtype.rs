//! Ped types and ped stats (C++ src/peds/PedType.cpp: CPedType::LoadPedData, CPedStats::LoadPedStats):
//! data/ped.dat and data/pedstats.dat. The objects stay C++ (ms_apPedType, ms_apPedStats); C++
//! loads the files and hands over the bytes. Identical to the original (tests/golden/pedtype.txt).
//! Structs and name tables drafted by the local model.

use crate::cscan::Scan;

/// Number of ped types.
pub const NUM_PEDTYPES: usize = 23;
/// Number of ped stats.
pub const NUM_PEDSTATS: usize = 40;

/// Representation of a ped type entry, matching the C++ `CPedType` struct layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PedType {
    pub flag: u32,
    pub unknown1: f32,
    pub unknown2: f32,
    pub unknown3: f32,
    pub unknown4: f32,
    pub unknown5: f32,
    pub threats: u32,
    pub avoid: u32,
}

/// Representation of a ped stats entry, matching the C++ `CPedStats` struct layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PedStats {
    pub stat_type: i32,
    pub name: [u8; 24],
    pub flee_distance: f32,
    pub heading_change_rate: f32,
    pub fear: i8,
    pub temper: i8,
    pub lawfulness: i8,
    pub sexiness: i8,
    pub attack_strength: f32,
    pub defend_weakness: f32,
    pub flags: i16,
}

// Compile-time size assertions to ensure struct layouts match expectations.
const _: () = assert!(std::mem::size_of::<PedType>() == 32);
const _: () = assert!(std::mem::size_of::<PedStats>() == 52);

/// Maps a ped type word string to its index.
pub fn find_ped_type(word: &str) -> usize {
    match word {
        "PLAYER1" => 0,
        "PLAYER2" => 1,
        "PLAYER3" => 2,
        "PLAYER4" => 3,
        "CIVMALE" => 4,
        "CIVFEMALE" => 5,
        "COP" => 6,
        "GANG1" => 7,
        "GANG2" => 8,
        "GANG3" => 9,
        "GANG4" => 10,
        "GANG5" => 11,
        "GANG6" => 12,
        "GANG7" => 13,
        "GANG8" => 14,
        "GANG9" => 15,
        "EMERGENCY" => 16,
        "FIREMAN" => 17,
        "CRIMINAL" => 18,
        "SPECIAL" => 21,
        "PROSTITUTE" => 20,
        _ => NUM_PEDTYPES,
    }
}

/// Maps a ped type word string to its corresponding flag bit.
pub fn find_ped_flag(word: &str) -> u32 {
    match word {
        "PLAYER1" => 1 << 0,
        "PLAYER2" => 1 << 1,
        "PLAYER3" => 1 << 2,
        "PLAYER4" => 1 << 3,
        "CIVMALE" => 1 << 4,
        "CIVFEMALE" => 1 << 5,
        "COP" => 1 << 6,
        "GANG1" => 1 << 7,
        "GANG2" => 1 << 8,
        "GANG3" => 1 << 9,
        "GANG4" => 1 << 10,
        "GANG5" => 1 << 11,
        "GANG6" => 1 << 12,
        "GANG7" => 1 << 13,
        "GANG8" => 1 << 14,
        "GANG9" => 1 << 15,
        "EMERGENCY" => 1 << 16,
        "PROSTITUTE" => 1 << 17,
        "CRIMINAL" => 1 << 18,
        "SPECIAL" => 1 << 19,
        "GUN" => 1 << 20,
        "COP_CAR" => 1 << 21,
        "FAST_CAR" => 1 << 22,
        "EXPLOSION" => 1 << 23,
        "FIREMAN" => 1 << 24,
        "DEADPEDS" => 1 << 25,
        _ => 0,
    }
}

/// The files' lines as the loaders see them: '\r', ',' and '\t' become spaces; a line whose bytes
/// are all <= ' ' (signed char) or whose first other byte is '#' is skipped. Yields (line, lp).
fn lines(buf: &[u8]) -> impl Iterator<Item = (Vec<u8>, usize)> + '_ {
    buf.split(|&c| c == b'\n')
        .enumerate()
        // A final piece after the last '\n' is a line only if the file doesn't end there.
        .filter(move |(i, piece)| !(piece.is_empty() && *i == buf.iter().filter(|&&c| c == b'\n').count()))
        .filter_map(|(_, piece)| {
            let line: Vec<u8> = piece.iter().map(|&c| if c == b'\r' || c == b',' || c == b'\t' { b' ' } else { c }).collect();
            let lp = line.iter().position(|&c| (c as i8) > 32)?;
            if line[lp] == b'#' { None } else { Some((line, lp)) }
        })
}

fn text(w: &[u8]) -> &str {
    std::str::from_utf8(w).unwrap_or("")
}

/// The flags named after a "Threat"/"Avoid" keyword from `pos` on.
fn flag_list(line: &[u8], mut pos: usize) -> u32 {
    let mut flags = 0;
    loop {
        if pos > line.len() {
            break;
        }
        let Some(w) = Scan::new(&line[pos..]).word() else { break };
        flags |= find_ped_flag(text(&w));
        while pos < line.len() && line[pos] != b' ' {
            pos += 1;
        }
        while pos < line.len() && line[pos] == b' ' {
            pos += 1;
        }
    }
    flags
}

/// CPedType::LoadPedData. Lines for a type the original doesn't know (it would write past the
/// table) are skipped, and so are Threat/Avoid lines before the first known type.
pub fn load_ped_data(buf: &[u8], types: &mut [&mut PedType]) {
    let mut ty = NUM_PEDTYPES;
    let mut f = [0.0f32; 5];
    for (line, lp) in lines(buf) {
        let word = Scan::new(&line[lp..]).word().unwrap_or_default();
        match text(&word) {
            "Threat" => {
                let flags = flag_list(&line, lp + 7);
                if ty < NUM_PEDTYPES {
                    types[ty].threats = flags;
                }
            }
            "Avoid" => {
                let flags = flag_list(&line, lp + 6);
                if ty < NUM_PEDTYPES {
                    types[ty].avoid = flags;
                }
            }
            _ => {
                let mut sc = Scan::new(&line);
                let name = sc.word().unwrap_or_default();
                for v in f.iter_mut() {
                    match sc.float() {
                        Some(x) => *v = x,
                        None => break,
                    }
                }
                ty = find_ped_type(text(&name));
                if ty < NUM_PEDTYPES {
                    let t = &mut *types[ty];
                    t.flag = find_ped_flag(text(&name));
                    t.unknown1 = f[0] / 50.0;
                    t.unknown2 = f[1] / 50.0;
                    t.unknown3 = f[2] / 50.0;
                    t.unknown4 = f[3];
                    t.unknown5 = f[4];
                }
            }
        }
    }
}

/// CPedStats::LoadPedStats: one stat per line, in order. Lines past the table are skipped.
pub fn load_ped_stats(buf: &[u8], stats: &mut [&mut PedStats]) {
    let mut ty = 0usize;
    let mut name: Vec<u8> = Vec::new();
    let (mut flee, mut heading, mut attack, mut defend) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut fear, mut temper, mut lawful, mut sexy, mut flags) = (0i32, 0i32, 0i32, 0i32, 0i32);
    for (line, lp) in lines(buf) {
        let mut sc = Scan::new(&line[lp..]);
        // sscanf stops at the first failure; the rest keep their values.
        let _ = (|| -> Option<()> {
            name = sc.word()?;
            flee = sc.float()?;
            heading = sc.float()?;
            fear = sc.int()?;
            temper = sc.int()?;
            lawful = sc.int()?;
            sexy = sc.int()?;
            attack = sc.float()?;
            defend = sc.float()?;
            flags = sc.int()?;
            Some(())
        })();
        if ty < NUM_PEDSTATS {
            let s = &mut *stats[ty];
            s.stat_type = ty as i32;
            // strncpy(m_name, name, 24)
            let n = name.len().min(24);
            s.name = [0; 24];
            s.name[..n].copy_from_slice(&name[..n]);
            s.flee_distance = flee;
            s.heading_change_rate = heading;
            s.fear = fear as i8;
            s.temper = temper as i8;
            s.lawfulness = lawful as i8;
            s.sexiness = sexy as i8;
            s.attack_strength = attack;
            s.defend_weakness = defend;
            s.flags = flags as i16;
        }
        ty += 1;
    }
}

/// # Safety
/// `data` holds `len` bytes; `types` points to NUM_PEDTYPES valid CPedType pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_ped_types_load(data: *const u8, len: usize, types: *const *mut PedType) {
    unsafe {
        let mut t: Vec<&mut PedType> = std::slice::from_raw_parts(types, NUM_PEDTYPES).iter().map(|&p| &mut *p).collect();
        load_ped_data(std::slice::from_raw_parts(data, len), &mut t);
    }
}

/// # Safety
/// `data` holds `len` bytes; `stats` points to NUM_PEDSTATS valid CPedStats pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_ped_stats_load(data: *const u8, len: usize, stats: *const *mut PedStats) {
    unsafe {
        let mut s: Vec<&mut PedStats> = std::slice::from_raw_parts(stats, NUM_PEDSTATS).iter().map(|&p| &mut *p).collect();
        load_ped_stats(std::slice::from_raw_parts(data, len), &mut s);
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/pedtype.txt: the original loaders on the synthetic files in tests/golden/pedtype/
    //! and the game's files (when present), tables zeroed with Initialise's defaults first.
    use super::*;

    fn text_mode(raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'\r' && raw.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            out.push(raw[i]);
            i += 1;
        }
        out
    }
    fn fnv<T>(x: &T) -> String {
        let b = unsafe { std::slice::from_raw_parts(x as *const T as *const u8, std::mem::size_of::<T>()) };
        format!(" {:016x}", b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3)))
    }

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/pedtype.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (name, want) = line.split_once(" =>").unwrap();
            let (ped, stats) = if name == "game" {
                (game.join("data/ped.dat"), game.join("data/pedstats.dat"))
            } else {
                let d = root.join("tests/golden/pedtype");
                (d.join(format!("{name}.ped")), d.join(format!("{name}.stats")))
            };
            let (Ok(ped), Ok(stats)) = (std::fs::read(&ped), std::fs::read(&stats)) else {
                assert_eq!(name, "game");
                continue;
            };
            let mut types: Vec<PedType> = vec![unsafe { std::mem::zeroed() }; NUM_PEDTYPES];
            let mut st: Vec<PedStats> = vec![unsafe { std::mem::zeroed() }; NUM_PEDSTATS];
            for t in types.iter_mut() {
                t.flag = 1;
            }
            for s in st.iter_mut() {
                s.name[8] = b'R';
                s.flee_distance = 20.0;
                s.heading_change_rate = 15.0;
                (s.fear, s.temper, s.lawfulness, s.sexiness) = (50, 50, 50, 50);
                s.attack_strength = 1.0;
                s.defend_weakness = 1.0;
            }
            load_ped_data(&text_mode(&ped), &mut types.iter_mut().collect::<Vec<_>>());
            load_ped_stats(&text_mode(&stats), &mut st.iter_mut().collect::<Vec<_>>());
            let got = types.iter().map(fnv).collect::<String>() + " |" + &st.iter().map(fnv).collect::<String>();
            let diff: Vec<usize> = got.split(' ').zip(want.split(' ')).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
            assert!(diff.is_empty(), "{name}: tokens {diff:?} differ (1..=23 ped types, then '|', then stats)");
            checked += 1;
        }
        assert!(checked >= 24);
    }
}

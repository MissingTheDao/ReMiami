//! Surface adhesion (C++ src/core/SurfaceTable.cpp, CSurfaceTable): the adhesive limit table from
//! data/surface.dat and the per-surface groups and wet multipliers. The table lives here; C++
//! forwards every method. Identical to the original (tests/golden/surface.txt). The match tables
//! were translated by the local model.
#![allow(dead_code)]

use crate::cscan::Scan;
use std::sync::Mutex;

pub const SURFACE_DEFAULT: u8 = 0;
pub const SURFACE_TARMAC: u8 = 1;
pub const SURFACE_GRASS: u8 = 2;
pub const SURFACE_GRAVEL: u8 = 3;
pub const SURFACE_MUD_DRY: u8 = 4;
pub const SURFACE_PAVEMENT: u8 = 5;
pub const SURFACE_CAR: u8 = 6;
pub const SURFACE_GLASS: u8 = 7;
pub const SURFACE_TRANSPARENT_CLOTH: u8 = 8;
pub const SURFACE_GARAGE_DOOR: u8 = 9;
pub const SURFACE_CAR_PANEL: u8 = 10;
pub const SURFACE_THICK_METAL_PLATE: u8 = 11;
pub const SURFACE_SCAFFOLD_POLE: u8 = 12;
pub const SURFACE_LAMP_POST: u8 = 13;
pub const SURFACE_FIRE_HYDRANT: u8 = 14;
pub const SURFACE_GIRDER: u8 = 15;
pub const SURFACE_METAL_CHAIN_FENCE: u8 = 16;
pub const SURFACE_PED: u8 = 17;
pub const SURFACE_SAND: u8 = 18;
pub const SURFACE_WATER: u8 = 19;
pub const SURFACE_WOOD_CRATES: u8 = 20;
pub const SURFACE_WOOD_BENCH: u8 = 21;
pub const SURFACE_WOOD_SOLID: u8 = 22;
pub const SURFACE_RUBBER: u8 = 23;
pub const SURFACE_PLASTIC: u8 = 24;
pub const SURFACE_HEDGE: u8 = 25;
pub const SURFACE_STEEP_CLIFF: u8 = 26;
pub const SURFACE_CONTAINER: u8 = 27;
pub const SURFACE_NEWS_VENDOR: u8 = 28;
pub const SURFACE_WHEELBASE: u8 = 29;
pub const SURFACE_CARDBOARDBOX: u8 = 30;
pub const SURFACE_TRANSPARENT_STONE: u8 = 31;
pub const SURFACE_METAL_GATE: u8 = 32;
pub const SURFACE_SAND_BEACH: u8 = 33;
pub const SURFACE_CONCRETE_BEACH: u8 = 34;

pub const ADHESIVE_RUBBER: usize = 0;
pub const ADHESIVE_HARD: usize = 1;
pub const ADHESIVE_ROAD: usize = 2;
pub const ADHESIVE_LOOSE: usize = 3;
pub const ADHESIVE_SAND: usize = 4;
pub const ADHESIVE_WET: usize = 5;
pub const NUMADHESIVEGROUPS: usize = 6;

pub fn adhesion_group(surface: u8) -> usize {
    match surface {
        SURFACE_DEFAULT | SURFACE_TARMAC | SURFACE_PAVEMENT | SURFACE_WOOD_CRATES | SURFACE_WOOD_BENCH | SURFACE_WOOD_SOLID | SURFACE_CONCRETE_BEACH => ADHESIVE_ROAD,
        SURFACE_GRASS | SURFACE_GRAVEL | SURFACE_HEDGE | SURFACE_STEEP_CLIFF | SURFACE_CARDBOARDBOX => ADHESIVE_LOOSE,
        SURFACE_MUD_DRY | SURFACE_CAR | SURFACE_GLASS | SURFACE_TRANSPARENT_CLOTH | SURFACE_GARAGE_DOOR | SURFACE_CAR_PANEL | SURFACE_THICK_METAL_PLATE | SURFACE_SCAFFOLD_POLE | SURFACE_LAMP_POST | SURFACE_FIRE_HYDRANT | SURFACE_GIRDER | SURFACE_METAL_CHAIN_FENCE | SURFACE_PLASTIC | SURFACE_CONTAINER | SURFACE_NEWS_VENDOR | SURFACE_TRANSPARENT_STONE | SURFACE_METAL_GATE => ADHESIVE_HARD,
        SURFACE_PED | SURFACE_RUBBER | SURFACE_WHEELBASE => ADHESIVE_RUBBER,
        SURFACE_SAND | SURFACE_SAND_BEACH => ADHESIVE_SAND,
        SURFACE_WATER => ADHESIVE_WET,
        _ => ADHESIVE_ROAD,
    }
}

pub fn wet_multiplier(surface: u8, wet_roads: f32) -> f32 {
    match surface {
        SURFACE_DEFAULT | SURFACE_TARMAC | SURFACE_MUD_DRY | SURFACE_PAVEMENT | SURFACE_TRANSPARENT_CLOTH | SURFACE_WOOD_CRATES | SURFACE_WOOD_BENCH | SURFACE_WOOD_SOLID | SURFACE_HEDGE | SURFACE_CARDBOARDBOX | SURFACE_TRANSPARENT_STONE | SURFACE_CONCRETE_BEACH => 1.0 - wet_roads * 0.25,
        SURFACE_GRASS | SURFACE_CAR | SURFACE_GLASS | SURFACE_GARAGE_DOOR | SURFACE_CAR_PANEL | SURFACE_THICK_METAL_PLATE | SURFACE_SCAFFOLD_POLE | SURFACE_LAMP_POST | SURFACE_FIRE_HYDRANT | SURFACE_GIRDER | SURFACE_METAL_CHAIN_FENCE | SURFACE_PED | SURFACE_RUBBER | SURFACE_PLASTIC | SURFACE_STEEP_CLIFF | SURFACE_CONTAINER | SURFACE_NEWS_VENDOR | SURFACE_WHEELBASE | SURFACE_METAL_GATE => 1.0 - wet_roads * 0.4,
        SURFACE_SAND | SURFACE_SAND_BEACH => 1.0 + wet_roads * 0.5,
        _ => 1.0,
    }
}

pub fn is_soft_landing(surface: u8) -> bool {
    surface == SURFACE_GRASS || surface == SURFACE_SAND || surface == SURFACE_SAND_BEACH
}

/// CSurfaceTable::ms_aAdhesiveLimitTable.
static TABLE: Mutex<[[f32; NUMADHESIVEGROUPS]; NUMADHESIVEGROUPS]> = Mutex::new([[0.0; NUMADHESIVEGROUPS]; NUMADHESIVEGROUPS]);

/// CSurfaceTable::Initialise's parse (the file after CRLF->LF): one line per group, the lower
/// triangle of the table, mirrored. A '-' field is 0. The original walks the buffer without bounds;
/// here the end of the buffer stops every loop.
pub fn parse(buf: &[u8], table: &mut [[f32; NUMADHESIVEGROUPS]; NUMADHESIVEGROUPS]) {
    let at = |i: usize| buf.get(i).copied().unwrap_or(0);
    let mut i = 0usize;
    for line in 0..NUMADHESIVEGROUPS {
        // white space and comments
        while i < buf.len() && matches!(at(i), b' ' | b'\t' | b'\n' | b'\r' | b';') {
            if at(i) == b';' {
                while i < buf.len() && at(i) != b'\n' && at(i) != b'\r' {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        // the surface name
        while i < buf.len() && !matches!(at(i), b' ' | b'\t' | b',') {
            i += 1;
        }
        for field in 0..=line {
            while i < buf.len() && matches!(at(i), b' ' | b'\t' | b',') {
                i += 1;
            }
            let mut limit = 0.0f32;
            if at(i) != b'-' {
                if let Some(v) = Scan::new(buf.get(i..).unwrap_or(&[])).float() {
                    limit = v;
                }
            }
            while i < buf.len() && !matches!(at(i), b' ' | b'\t' | b',' | b'\n') {
                i += 1;
            }
            table[line][field] = limit;
            table[field][line] = limit;
        }
    }
}

/// GetAdhesiveLimit(colpoint): by the groups of surface B (row) and A (column).
pub fn adhesive_limit(table: &[[f32; NUMADHESIVEGROUPS]; NUMADHESIVEGROUPS], surface_a: u8, surface_b: u8) -> f32 {
    table[adhesion_group(surface_b)][adhesion_group(surface_a)]
}

/// # Safety
/// `data` holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_surface_init(data: *const u8, len: usize) {
    let buf = if data.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len) } };
    parse(buf, &mut TABLE.lock().unwrap());
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_surface_adhesion_group(surface: u8) -> i32 {
    adhesion_group(surface) as i32
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_surface_wet_multiplier(surface: u8, wet_roads: f32) -> f32 {
    wet_multiplier(surface, wet_roads)
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_surface_adhesive_limit(surface_a: u8, surface_b: u8) -> f32 {
    adhesive_limit(&TABLE.lock().unwrap(), surface_a, surface_b)
}
#[unsafe(no_mangle)]
pub extern "C" fn revc_surface_is_soft_landing(surface: u8) -> bool {
    is_soft_landing(surface)
}

#[cfg(test)]
mod golden {
    //! tests/golden/surface.txt: the original on synthetic surface files (tests/golden/surface/) and
    //! the game's data/surface.dat: the table, then for every surface byte its group, three wet
    //! multipliers and the soft-landing flag.
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

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/surface.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (name, want) = line.split_once(" =>").unwrap();
            let path = if name == "game" { game.join("data/surface.dat") } else { root.join("tests/golden/surface").join(name) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(name, "game");
                continue;
            };
            let mut t = [[0.0f32; NUMADHESIVEGROUPS]; NUMADHESIVEGROUPS];
            parse(&text_mode(&raw), &mut t);
            let mut got = String::new();
            for row in &t {
                for v in row {
                    got += &format!(" {:08x}", v.to_bits());
                }
            }
            got += " |";
            for s in 0..=255u8 {
                got += &format!(" {}", adhesion_group(s));
                for w in [0.0f32, 0.37, 1.0] {
                    got += &format!(" {:08x}", wet_multiplier(s, w).to_bits());
                }
                got += &format!(" {}", is_soft_landing(s) as i32);
                got += &format!(" {:08x}", adhesive_limit(&t, s, s.wrapping_mul(7)).to_bits());
            }
            assert_eq!(got, want, "{name}");
            checked += 1;
        }
        assert!(checked >= 16);
    }
}

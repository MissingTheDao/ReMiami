//! The time cycle's data (C++ src/renderer/Timecycle.cpp, CTimeCycle::Initialise): the parse of
//! data/timecyc.dat into the 52 hour x weather tables. The tables stay C++ statics (copied over
//! from Tables). Identical to the original (tests/golden/timecyc.txt). Tables struct and C++ glue
//! written by the local model.

/// Rust port of CTimeCycle::Initialise for parsing timecyc.dat
use crate::cscan::Scan;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Tables {
    ambient_red: [[u8; 7]; 24],
    ambient_green: [[u8; 7]; 24],
    ambient_blue: [[u8; 7]; 24],
    ambient_red_obj: [[u8; 7]; 24],
    ambient_green_obj: [[u8; 7]; 24],
    ambient_blue_obj: [[u8; 7]; 24],
    ambient_red_bl: [[u8; 7]; 24],
    ambient_green_bl: [[u8; 7]; 24],
    ambient_blue_bl: [[u8; 7]; 24],
    ambient_red_obj_bl: [[u8; 7]; 24],
    ambient_green_obj_bl: [[u8; 7]; 24],
    ambient_blue_obj_bl: [[u8; 7]; 24],
    directional_red: [[u8; 7]; 24],
    directional_green: [[u8; 7]; 24],
    directional_blue: [[u8; 7]; 24],
    sky_top_red: [[u8; 7]; 24],
    sky_top_green: [[u8; 7]; 24],
    sky_top_blue: [[u8; 7]; 24],
    sky_bottom_red: [[u8; 7]; 24],
    sky_bottom_green: [[u8; 7]; 24],
    sky_bottom_blue: [[u8; 7]; 24],
    sun_core_red: [[u8; 7]; 24],
    sun_core_green: [[u8; 7]; 24],
    sun_core_blue: [[u8; 7]; 24],
    sun_corona_red: [[u8; 7]; 24],
    sun_corona_green: [[u8; 7]; 24],
    sun_corona_blue: [[u8; 7]; 24],
    sun_size: [[i8; 7]; 24],
    sprite_size: [[i8; 7]; 24],
    sprite_brightness: [[i8; 7]; 24],
    shadow_strength: [[u8; 7]; 24],
    light_shadow_strength: [[u8; 7]; 24],
    pole_shadow_strength: [[u8; 7]; 24],
    far_clip: [[i16; 7]; 24],
    fog_start: [[i16; 7]; 24],
    lights_on_ground_brightness: [[u8; 7]; 24],
    low_clouds_red: [[u8; 7]; 24],
    low_clouds_green: [[u8; 7]; 24],
    low_clouds_blue: [[u8; 7]; 24],
    fluffy_clouds_top_red: [[u8; 7]; 24],
    fluffy_clouds_top_green: [[u8; 7]; 24],
    fluffy_clouds_top_blue: [[u8; 7]; 24],
    fluffy_clouds_bottom_red: [[u8; 7]; 24],
    fluffy_clouds_bottom_green: [[u8; 7]; 24],
    fluffy_clouds_bottom_blue: [[u8; 7]; 24],
    blur_red: [[u8; 7]; 24],
    blur_green: [[u8; 7]; 24],
    blur_blue: [[u8; 7]; 24],
    water_red: [[u8; 7]; 24],
    water_green: [[u8; 7]; 24],
    water_blue: [[u8; 7]; 24],
    water_alpha: [[u8; 7]; 24],
}

impl Tables {
    #[allow(dead_code)] // the tests start from it
    pub fn zeroed() -> Tables {
        Tables {
            ambient_red: [[0; 7]; 24],
            ambient_green: [[0; 7]; 24],
            ambient_blue: [[0; 7]; 24],
            ambient_red_obj: [[0; 7]; 24],
            ambient_green_obj: [[0; 7]; 24],
            ambient_blue_obj: [[0; 7]; 24],
            ambient_red_bl: [[0; 7]; 24],
            ambient_green_bl: [[0; 7]; 24],
            ambient_blue_bl: [[0; 7]; 24],
            ambient_red_obj_bl: [[0; 7]; 24],
            ambient_green_obj_bl: [[0; 7]; 24],
            ambient_blue_obj_bl: [[0; 7]; 24],
            directional_red: [[0; 7]; 24],
            directional_green: [[0; 7]; 24],
            directional_blue: [[0; 7]; 24],
            sky_top_red: [[0; 7]; 24],
            sky_top_green: [[0; 7]; 24],
            sky_top_blue: [[0; 7]; 24],
            sky_bottom_red: [[0; 7]; 24],
            sky_bottom_green: [[0; 7]; 24],
            sky_bottom_blue: [[0; 7]; 24],
            sun_core_red: [[0; 7]; 24],
            sun_core_green: [[0; 7]; 24],
            sun_core_blue: [[0; 7]; 24],
            sun_corona_red: [[0; 7]; 24],
            sun_corona_green: [[0; 7]; 24],
            sun_corona_blue: [[0; 7]; 24],
            sun_size: [[0; 7]; 24],
            sprite_size: [[0; 7]; 24],
            sprite_brightness: [[0; 7]; 24],
            shadow_strength: [[0; 7]; 24],
            light_shadow_strength: [[0; 7]; 24],
            pole_shadow_strength: [[0; 7]; 24],
            far_clip: [[0; 7]; 24],
            fog_start: [[0; 7]; 24],
            lights_on_ground_brightness: [[0; 7]; 24],
            low_clouds_red: [[0; 7]; 24],
            low_clouds_green: [[0; 7]; 24],
            low_clouds_blue: [[0; 7]; 24],
            fluffy_clouds_top_red: [[0; 7]; 24],
            fluffy_clouds_top_green: [[0; 7]; 24],
            fluffy_clouds_top_blue: [[0; 7]; 24],
            fluffy_clouds_bottom_red: [[0; 7]; 24],
            fluffy_clouds_bottom_green: [[0; 7]; 24],
            fluffy_clouds_bottom_blue: [[0; 7]; 24],
            blur_red: [[0; 7]; 24],
            blur_green: [[0; 7]; 24],
            blur_blue: [[0; 7]; 24],
            water_red: [[0; 7]; 24],
            water_green: [[0; 7]; 24],
            water_blue: [[0; 7]; 24],
            water_alpha: [[0; 7]; 24],
        }
    }
}

/// CTimeCycle::Initialise's parse of timecyc.dat as read in binary mode: 7 weathers x 24 hours,
/// one line each (lines starting with '/', ' ', CR, LF or NUL are skipped). Values the sscanf
/// doesn't reach keep the previous line's, as the original's locals. Conversions as the original:
/// int to the table's type by truncation, floats through int (x10 for the sizes).
pub fn parse(buf: &[u8], t: &mut Tables) {
    // I = %d, F = %f, in the format's order.
    const KINDS: &[u8; 52] = b"IIIIIIIIIIIIIIIIIIIIIIIIIIIFFFIIIFFFIIIIIIIIIFFFFFFF";
    let at = |i: usize| buf.get(i).copied().unwrap_or(0);
    let mut iv = [0i32; 52];
    let mut fl = [0f32; 52];
    let mut bi = 0usize;
    for w in 0..7 {
        for h in 0..24 {
            if bi >= buf.len() {
                return; // the original reads past the data; never in real files
            }
            while matches!(at(bi), b'/' | b'\n' | 0 | b' ' | b'\r') {
                while !matches!(at(bi), b'\n' | 0 | b'\r') {
                    bi += 1;
                }
                bi += 1;
                if bi >= buf.len() {
                    return;
                }
            }
            let start = bi;
            while bi < buf.len() && at(bi) != b'\n' && at(bi) != 0 {
                bi += 1;
            }
            let line = &buf[start..bi];
            bi += 1;
            let mut sc = Scan::new(line);
            for (k, kind) in KINDS.iter().enumerate() {
                let ok = if *kind == b'I' { sc.int().map(|v| iv[k] = v) } else { sc.float().map(|v| fl[k] = v) };
                if ok.is_none() {
                    break;
                }
            }
            t.ambient_red[h][w] = iv[0] as u8;
            t.ambient_green[h][w] = iv[1] as u8;
            t.ambient_blue[h][w] = iv[2] as u8;
            t.ambient_red_obj[h][w] = iv[3] as u8;
            t.ambient_green_obj[h][w] = iv[4] as u8;
            t.ambient_blue_obj[h][w] = iv[5] as u8;
            t.ambient_red_bl[h][w] = iv[6] as u8;
            t.ambient_green_bl[h][w] = iv[7] as u8;
            t.ambient_blue_bl[h][w] = iv[8] as u8;
            t.ambient_red_obj_bl[h][w] = iv[9] as u8;
            t.ambient_green_obj_bl[h][w] = iv[10] as u8;
            t.ambient_blue_obj_bl[h][w] = iv[11] as u8;
            t.directional_red[h][w] = iv[12] as u8;
            t.directional_green[h][w] = iv[13] as u8;
            t.directional_blue[h][w] = iv[14] as u8;
            t.sky_top_red[h][w] = iv[15] as u8;
            t.sky_top_green[h][w] = iv[16] as u8;
            t.sky_top_blue[h][w] = iv[17] as u8;
            t.sky_bottom_red[h][w] = iv[18] as u8;
            t.sky_bottom_green[h][w] = iv[19] as u8;
            t.sky_bottom_blue[h][w] = iv[20] as u8;
            t.sun_core_red[h][w] = iv[21] as u8;
            t.sun_core_green[h][w] = iv[22] as u8;
            t.sun_core_blue[h][w] = iv[23] as u8;
            t.sun_corona_red[h][w] = iv[24] as u8;
            t.sun_corona_green[h][w] = iv[25] as u8;
            t.sun_corona_blue[h][w] = iv[26] as u8;
            t.sun_size[h][w] = (fl[27] * 10.0) as i32 as i8;
            t.sprite_size[h][w] = (fl[28] * 10.0) as i32 as i8;
            t.sprite_brightness[h][w] = (fl[29] * 10.0) as i32 as i8;
            t.shadow_strength[h][w] = iv[30] as u8;
            t.light_shadow_strength[h][w] = iv[31] as u8;
            t.pole_shadow_strength[h][w] = iv[32] as u8;
            t.far_clip[h][w] = fl[33] as i32 as i16;
            t.fog_start[h][w] = fl[34] as i32 as i16;
            t.lights_on_ground_brightness[h][w] = (fl[35] * 10.0) as i32 as u8;
            t.low_clouds_red[h][w] = iv[36] as u8;
            t.low_clouds_green[h][w] = iv[37] as u8;
            t.low_clouds_blue[h][w] = iv[38] as u8;
            t.fluffy_clouds_top_red[h][w] = iv[39] as u8;
            t.fluffy_clouds_top_green[h][w] = iv[40] as u8;
            t.fluffy_clouds_top_blue[h][w] = iv[41] as u8;
            t.fluffy_clouds_bottom_red[h][w] = iv[42] as u8;
            t.fluffy_clouds_bottom_green[h][w] = iv[43] as u8;
            t.fluffy_clouds_bottom_blue[h][w] = iv[44] as u8;
            t.blur_red[h][w] = fl[45] as i32 as u8;
            t.blur_green[h][w] = fl[46] as i32 as u8;
            t.blur_blue[h][w] = fl[47] as i32 as u8;
            t.water_red[h][w] = fl[48] as i32 as u8;
            t.water_green[h][w] = fl[49] as i32 as u8;
            t.water_blue[h][w] = fl[50] as i32 as u8;
            t.water_alpha[h][w] = fl[51] as i32 as u8;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_timecyc_parse(data: *const u8, len: usize, out: *mut Tables) { unsafe {
    if data.is_null() || out.is_null() {
        return;
    }
    let buf = std::slice::from_raw_parts(data, len);
    let t = &mut *out;
    parse(buf, t);
}}

#[cfg(test)]
mod golden {
    //! tests/golden/timecyc.txt: the original Initialise on synthetic timecyc.dat files
    //! (tests/golden/timecyc/) and the game's (when present): FNV-1a 64 of each of the 52 tables.
    use super::*;

    fn fnv(b: &[u8]) -> String {
        format!(" {:016x}", b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3)))
    }

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/timecyc.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (name, want) = line.split_once(" =>").unwrap();
            let path = if name == "game" { game.join("data/timecyc.dat") } else { root.join("tests/golden/timecyc").join(name) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(name, "game");
                continue;
            };
            let mut t = Tables::zeroed();
            parse(&raw, &mut t);
            // The tables back to back: 24*7 bytes each, 2 bytes per entry for the two i16 ones.
            let bytes = unsafe { std::slice::from_raw_parts(&t as *const Tables as *const u8, std::mem::size_of::<Tables>()) };
            let mut got = String::new();
            let mut at = 0;
            for k in 0..52 {
                let n = if k == 33 || k == 34 { 24 * 7 * 2 } else { 24 * 7 };
                got += &fnv(&bytes[at..at + n]);
                at += n;
            }
            let diff: Vec<usize> = got.split(' ').zip(want.split(' ')).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
            assert!(diff.is_empty(), "{name}: tables {diff:?} differ (1-based)");
            checked += 1;
        }
        assert!(checked >= 12);
    }
}

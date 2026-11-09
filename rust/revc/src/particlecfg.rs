use crate::cscan::{atof, atoi};
use std::ptr::null_mut;

/// Maximum number of particle systems that can be loaded.
pub const MAX_PARTICLES: usize = 83;

/// Represents a single particle system configuration, matching the C++ `tParticleSystemData` layout.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ParticleSystemData {
    type_: i32,
    name: [u8; 20],
    create_range: f32,
    default_initial_radius: f32,
    expansion_rate: f32,
    z_rotation_initial_angle: u16,
    z_rotation_angle_change_amount: i16,
    z_rotation_change_time: u16,
    z_radius_change_time: u16,
    initial_z_radius: f32,
    z_radius_change_amount: f32,
    fade_to_black_time: i16,
    fade_to_black_initial_intensity: u8,
    fade_to_black_amount: i16,
    fade_alpha_initial_intensity: u8,
    fade_alpha_time: i16,
    fade_alpha_amount: i16,
    start_animation_frame: u8,
    final_animation_frame: u8,
    animation_speed: u16,
    rotation_speed: u16,
    gravitational_acceleration: f32,
    friction_decceleration: i32,
    life_span: i32,
    position_random_error: f32,
    velocity_random_error: f32,
    expansion_rate_error: f32,
    rotation_rate_error: i32,
    life_span_error_shape: u32,
    trail_length_multiplier: f32,
    flags: u32,
    render_colouring: [u8; 4],
    initial_color_variation: u8,
    fade_destination_color: [u8; 4],
    color_fade_time: u32,
    texture_stretch_x: f32,
    texture_stretch_y: f32,
    wind_factor: f32,
    raster: *mut u8,
    particles: *mut u8,
}

/// # Safety
/// 
/// `data` must point to `len` bytes of valid memory containing the particle config text.
/// `table` must point to `MAX_PARTICLES` entries of `ParticleSystemData`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_particle_cfg_load(data: *const u8, len: usize, table: *mut ParticleSystemData) {
    unsafe { load(std::slice::from_raw_parts(data, len), std::slice::from_raw_parts_mut(table, MAX_PARTICLES)) }
}

/// LoadParticleData over the file's bytes (CRLF already LF).
pub fn load(buf: &[u8], table_slice: &mut [ParticleSystemData]) {

    let mut entry: Option<usize> = None;
    let mut next_type: usize = 0;
    
    let mut start: usize = 0;
    let mut end: usize = 1;

    loop {
        // Find end of line
        while end < buf.len() && buf[end] != b'\n' {
            end += 1;
        }
        
        // If we reached end of buffer without finding newline, stop (malformed)
        if end >= buf.len() {
            return;
        }

        // Slice for the line, cutting at 0 byte if present
        let line_slice = &buf[start..end];
        let line_end = line_slice.iter().position(|&b| b == 0).unwrap_or(line_slice.len());
        let line = &line_slice[..line_end];

        // Move start to next line
        start = end + 1;
        end = start + 1; // Note: end starts at start+1 for the next iteration search

        // Check for terminator
        if line == b";the end" {
            return;
        }

        // Skip comments
        if line.first() == Some(&b';') {
            continue;
        }

        // Tokenize
        let tokens: Vec<&[u8]> = line.split(|&b| b == b' ' || b == b'\t').collect();
        let non_empty_tokens: Vec<&[u8]> = tokens.into_iter().filter(|t| !t.is_empty()).collect();

        if non_empty_tokens.is_empty() {
            continue;
        }

        let mut param: usize = 0;
        
        for w in &non_empty_tokens {
            // Only write to table if we have a valid entry index
            // Param 0 starts an entry; the others write into the current one.
            if param == 0 || entry.is_some() {
                let idx = entry.unwrap_or(0);
                let e = &mut table_slice[idx];
                
                match param {
                    0 => {
                        // Type/Name
                        if next_type < MAX_PARTICLES {
                            entry = Some(next_type);
                            let e_ptr = &mut table_slice[next_type];
                            e_ptr.type_ = next_type as i32;
                            
                            // strcpy into name: copy min(w.len(), 19) bytes then a 0 byte
                            let copy_len = w.len().min(19);
                            let name_slice = &mut e_ptr.name;
                            name_slice[..copy_len].copy_from_slice(&w[..copy_len]);
                            // Ensure null termination if we didn't fill the whole buffer or if the string was shorter
                            // The note says "bytes after that 0 are left as they were", implying we just write up to 19 and put a 0.
                            // If w is shorter than 19, we put 0 at w.len().
                            if copy_len < 20 {
                                name_slice[copy_len] = 0;
                            }
                        } else {
                            entry = None;
                        }
                        next_type += 1;
                    }
                    1 => e.render_colouring[0] = atoi(w) as u8,
                    2 => e.render_colouring[1] = atoi(w) as u8,
                    3 => e.render_colouring[2] = atoi(w) as u8,
                    4 => {
                        let val = atoi(w);
                        e.initial_color_variation = val.min(100) as u8;
                    }
                    5 => e.fade_destination_color[0] = atoi(w) as u8,
                    6 => e.fade_destination_color[1] = atoi(w) as u8,
                    7 => e.fade_destination_color[2] = atoi(w) as u8,
                    8 => e.color_fade_time = atoi(w) as u32,
                    9 => e.default_initial_radius = atof(w) as f32,
                    10 => e.expansion_rate = atof(w) as f32,
                    11 => e.fade_to_black_initial_intensity = atoi(w) as u8,
                    12 => e.fade_to_black_time = atoi(w) as i16,
                    13 => e.fade_to_black_amount = atoi(w) as i16,
                    14 => e.fade_alpha_initial_intensity = atoi(w) as u8,
                    15 => e.fade_alpha_time = atoi(w) as i16,
                    16 => e.fade_alpha_amount = atoi(w) as i16,
                    17 => e.z_rotation_initial_angle = atoi(w) as u16,
                    18 => e.z_rotation_change_time = atoi(w) as u16,
                    19 => e.z_rotation_angle_change_amount = atoi(w) as i16,
                    20 => e.initial_z_radius = atof(w) as f32,
                    21 => e.z_radius_change_time = atoi(w) as u16,
                    22 => e.z_radius_change_amount = atof(w) as f32,
                    23 => e.animation_speed = atoi(w) as u16,
                    24 => e.start_animation_frame = atoi(w) as u8,
                    25 => e.final_animation_frame = atoi(w) as u8,
                    26 => e.rotation_speed = atoi(w) as u16,
                    27 => e.gravitational_acceleration = atof(w) as f32,
                    28 => e.friction_decceleration = atoi(w),
                    29 => e.life_span = atoi(w),
                    30 => e.position_random_error = atof(w) as f32,
                    31 => e.velocity_random_error = atof(w) as f32,
                    32 => e.expansion_rate_error = atof(w) as f32,
                    33 => e.rotation_rate_error = atoi(w),
                    34 => e.life_span_error_shape = atoi(w) as u32,
                    35 => e.trail_length_multiplier = atof(w) as f32,
                    36 => e.texture_stretch_x = atof(w) as f32,
                    37 => e.texture_stretch_y = atof(w) as f32,
                    38 => e.wind_factor = atof(w) as f32,
                    39 => {
                        let d = atof(w);
                        e.create_range = (d * d) as f32;
                    }
                    40 => e.flags = atoi(w) as u32,
                    _ => {}
                }
            }

            param += 1;
            if param > 40 {
                param = 0;
            }
        }
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/particlecfg.txt: the original LoadParticleData on synthetic particle.cfg files
    //! (tests/golden/particlecfg/) and the game's (when present): FNV-1a 64 of every entry's bytes
    //! before the two pointers.
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
        let golden = std::fs::read_to_string(root.join("tests/golden/particlecfg.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (name, want) = line.split_once(" =>").unwrap();
            let path = if name == "game" { game.join("data/particle.cfg") } else { root.join("tests/golden/particlecfg").join(name) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(name, "game");
                continue;
            };
            let mut table: Vec<ParticleSystemData> = vec![unsafe { std::mem::zeroed() }; MAX_PARTICLES];
            load(&text_mode(&raw), &mut table);
            let got: String = table
                .iter()
                .map(|e| {
                    let b = unsafe { std::slice::from_raw_parts(e as *const ParticleSystemData as *const u8, 144) };
                    format!(" {:016x}", b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3)))
                })
                .collect();
            let diff: Vec<usize> = got.split(' ').zip(want.split(' ')).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i).collect();
            assert!(diff.is_empty(), "{name}: entries {diff:?} differ (1-based)");
            checked += 1;
        }
        assert!(checked >= 16);
    }
}

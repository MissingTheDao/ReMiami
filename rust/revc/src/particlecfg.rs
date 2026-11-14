use crate::cscan::{atof, atoi};

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

/// An animation-speed column between 0 and 1 (exclusive) means "advance a frame every second update";
/// the integer speed field can't hold it, so the merge stores speed 0 and sets this flag
/// (Particle.cpp animates when the speed is non-zero or this flag is set).
pub const FLAG_ANIM_EVERY_OTHER: u32 = 1 << 14;

/// Merge particle config data by name into an existing table.
///
/// `buf` contains the configuration text (CRLF already LF).
/// `table` contains the base configurations.
/// Returns the number of entries that were changed.
pub fn merge_by_name(buf: &[u8], table: &mut [ParticleSystemData]) -> usize {
    let mut changes: usize = 0;
    
    let mut start: usize = 0;
    let mut end: usize = 1;

    loop {
        // Find end of line
        while end < buf.len() && buf[end] != b'\n' {
            end += 1;
        }
        
        // If we reached end of buffer without finding newline, stop
        if end >= buf.len() {
            return changes;
        }

        // Slice for the line, cutting at 0 byte if present
        let line_slice = &buf[start..end];
        let line_end = line_slice.iter().position(|&b| b == 0).unwrap_or(line_slice.len());
        let mut line = &line_slice[..line_end];

        // Remove trailing CR if present
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }

        // Move start to next line
        start = end + 1;
        end = start + 1;

        // Check for terminator
        if line == b";the end" {
            return changes;
        }

        // Skip comments and empty lines
        if line.is_empty() || line.first() == Some(&b';') {
            continue;
        }

        // Tokenize
        let tokens: Vec<&[u8]> = line.split(|&b| b == b' ' || b == b'\t').collect();
        let non_empty_tokens: Vec<&[u8]> = tokens.into_iter().filter(|t| !t.is_empty()).collect();

        if non_empty_tokens.is_empty() {
            continue;
        }

        let name_token = non_empty_tokens[0];
        
        // Find entry by name
        let mut found_idx: Option<usize> = None;
        for (i, entry) in table.iter().enumerate() {
            // Compare name bytes up to first NUL
            let entry_name = entry.name;
            let mut j = 0;
            let mut match_len = 0;
            while j < entry_name.len() && j < name_token.len() {
                if entry_name[j] == 0 {
                    break;
                }
                if entry_name[j] != name_token[j] {
                    break;
                }
                j += 1;
                match_len += 1;
            }
            // Check if entry name ended (NUL) and we matched the full token length
            if j < entry_name.len() && entry_name[j] == 0 && match_len == name_token.len() {
                found_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = found_idx {
            // Build a copy of the entry
            let mut copy = table[idx];
            
            let mut param: usize = 1;
            // The animation-speed column was a fraction (re-applied after the flags column).
            let mut fractional = false;
            
            for w in &non_empty_tokens[1..] {
                if param > 40 {
                    break;
                }
                
                match param {
                    1 => copy.render_colouring[0] = atoi(w) as u8,
                    2 => copy.render_colouring[1] = atoi(w) as u8,
                    3 => copy.render_colouring[2] = atoi(w) as u8,
                    4 => {
                        let val = atoi(w);
                        copy.initial_color_variation = val.min(100) as u8;
                    }
                    5 => copy.fade_destination_color[0] = atoi(w) as u8,
                    6 => copy.fade_destination_color[1] = atoi(w) as u8,
                    7 => copy.fade_destination_color[2] = atoi(w) as u8,
                    8 => copy.color_fade_time = atoi(w) as u32,
                    9 => copy.default_initial_radius = atof(w) as f32,
                    10 => copy.expansion_rate = atof(w) as f32,
                    11 => copy.fade_to_black_initial_intensity = atoi(w) as u8,
                    12 => copy.fade_to_black_time = atoi(w) as i16,
                    13 => copy.fade_to_black_amount = atoi(w) as i16,
                    14 => copy.fade_alpha_initial_intensity = atoi(w) as u8,
                    15 => copy.fade_alpha_time = atoi(w) as i16,
                    16 => copy.fade_alpha_amount = atoi(w) as i16,
                    17 => copy.z_rotation_initial_angle = atoi(w) as u16,
                    18 => copy.z_rotation_change_time = atoi(w) as u16,
                    19 => copy.z_rotation_angle_change_amount = atoi(w) as i16,
                    20 => copy.initial_z_radius = atof(w) as f32,
                    21 => copy.z_radius_change_time = atoi(w) as u16,
                    22 => copy.z_radius_change_amount = atof(w) as f32,
                    23 => {
                        let val = atof(w);
                        fractional = val > 0.0 && val < 1.0;
                        copy.animation_speed = if fractional { 0 } else { val as u16 };
                    }
                    24 => copy.start_animation_frame = atoi(w) as u8,
                    25 => copy.final_animation_frame = atoi(w) as u8,
                    26 => copy.rotation_speed = atoi(w) as u16,
                    27 => copy.gravitational_acceleration = atof(w) as f32,
                    28 => copy.friction_decceleration = atoi(w),
                    29 => copy.life_span = atoi(w),
                    30 => copy.position_random_error = atof(w) as f32,
                    31 => copy.velocity_random_error = atof(w) as f32,
                    32 => copy.expansion_rate_error = atof(w) as f32,
                    33 => copy.rotation_rate_error = atoi(w),
                    34 => copy.life_span_error_shape = atoi(w) as u32,
                    35 => copy.trail_length_multiplier = atof(w) as f32,
                    36 => copy.texture_stretch_x = atof(w) as f32,
                    37 => copy.texture_stretch_y = atof(w) as f32,
                    38 => copy.wind_factor = atof(w) as f32,
                    39 => {
                        let d = atof(w);
                        copy.create_range = (d * d) as f32;
                    }
                    40 => copy.flags = atoi(w) as u32,
                    _ => {}
                }
                param += 1;
            }
            
            if fractional {
                copy.flags |= FLAG_ANIM_EVERY_OTHER;
            } else {
                copy.flags &= !FLAG_ANIM_EVERY_OTHER;
            }
            // Check if change occurred
            let old_bytes = unsafe { std::slice::from_raw_parts(&table[idx] as *const ParticleSystemData as *const u8, std::mem::size_of::<ParticleSystemData>()) };
            let new_bytes = unsafe { std::slice::from_raw_parts(&copy as *const ParticleSystemData as *const u8, std::mem::size_of::<ParticleSystemData>()) };
            
            if old_bytes != new_bytes {
                table[idx] = copy;
                changes += 1;
            }
        }
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    /// A config line: `name` and columns 1..=40, every column `base` (+ the column number for
    /// integers) unless `set` overrides it.
    fn line(name: &str, base: u32, set: &[(usize, &str)]) -> String {
        let mut cols: Vec<String> = (1..=40).map(|c| format!("{}", base + c as u32)).collect();
        cols[39] = "0".into(); // flags
        for &(c, v) in set {
            cols[c - 1] = v.into();
        }
        format!("{name} {}\n", cols.join(" "))
    }

    fn table_of(cfg: &str) -> Vec<ParticleSystemData> {
        let mut table = vec![unsafe { std::mem::zeroed() }; MAX_PARTICLES];
        load(cfg.as_bytes(), &mut table);
        table
    }

    fn bytes(e: &ParticleSystemData) -> Vec<u8> {
        // SAFETY: plain data viewed as bytes.
        unsafe { std::slice::from_raw_parts((e as *const ParticleSystemData).cast::<u8>(), std::mem::size_of::<ParticleSystemData>()) }.to_vec()
    }

    fn base_cfg() -> String {
        format!("{}{}{};the end\n", line("AAA", 0, &[]), line("FLAME", 100, &[]), line("ZZZ", 50, &[]))
    }

    #[test]
    fn only_named_entries_change() {
        let mut table = table_of(&base_cfg());
        let before: Vec<Vec<u8>> = table.iter().map(bytes).collect();
        let cfg = format!("{}{};the end\n", line("NEWTYPE", 7, &[]), line("FLAME", 3, &[(1, "222"), (2, "255"), (3, "244")]));
        assert_eq!(merge_by_name(cfg.as_bytes(), &mut table), 1);
        for (i, e) in table.iter().enumerate() {
            if i == 1 {
                assert_ne!(bytes(e), before[i]);
            } else {
                assert_eq!(bytes(e), before[i], "entry {i} changed");
            }
        }
        assert_eq!(table[1].render_colouring[..3], [222, 255, 244]);
    }

    #[test]
    fn identity_fields_kept() {
        let mut table = table_of(&base_cfg());
        table[1].raster = 0x1000 as *mut u8;
        table[1].particles = 0x2000 as *mut u8;
        let (ty, name) = (table[1].type_, table[1].name);
        merge_by_name(format!("{};the end\n", line("FLAME", 9, &[])).as_bytes(), &mut table);
        assert_eq!((table[1].type_, table[1].name), (ty, name));
        assert_eq!((table[1].raster as usize, table[1].particles as usize), (0x1000, 0x2000));
    }

    #[test]
    fn fractional_speed_sets_the_flag() {
        let mut table = table_of(&base_cfg());
        merge_by_name(format!("{};the end\n", line("FLAME", 0, &[(23, "0.5"), (40, "64")])).as_bytes(), &mut table);
        assert_eq!(table[1].animation_speed, 0);
        assert_eq!(table[1].flags, 64 | FLAG_ANIM_EVERY_OTHER, "the flags column kept, the flag added");
        merge_by_name(format!("{};the end\n", line("FLAME", 0, &[(23, "3"), (40, "64")])).as_bytes(), &mut table);
        assert_eq!(table[1].animation_speed, 3);
        assert_eq!(table[1].flags, 64);
        merge_by_name(format!("{};the end\n", line("FLAME", 0, &[(23, "0"), (40, "0")])).as_bytes(), &mut table);
        assert_eq!((table[1].animation_speed, table[1].flags), (0, 0), "speed 0 means no animation");
    }

    #[test]
    fn order_does_not_matter() {
        let a = format!("{}{}{};the end\n", line("ZZZ", 1, &[]), line("FLAME", 2, &[]), line("AAA", 3, &[]));
        let b = format!("{}{}{};the end\n", line("AAA", 3, &[]), line("ZZZ", 1, &[]), line("FLAME", 2, &[]));
        let (mut t1, mut t2) = (table_of(&base_cfg()), table_of(&base_cfg()));
        assert_eq!(merge_by_name(a.as_bytes(), &mut t1), 3);
        assert_eq!(merge_by_name(b.as_bytes(), &mut t2), 3);
        assert_eq!(t1.iter().map(bytes).collect::<Vec<_>>(), t2.iter().map(bytes).collect::<Vec<_>>());
    }
}

/// Returns true if the current UI style requires merging Xbox particle data.
/// PC (0) and PS2 (1) use the loaded table as-is. Xbox (2) requires merging.
fn merge_wanted(style: i32) -> bool {
    style == 2
}

/// Returns the path to an Xbox-imported file within the cache directory.
fn xbox_file(name: &str) -> std::path::PathBuf {
    crate::discimport::cache_dir().join("xbox").join(name)
}

/// Returns a pointer to the C-string path of the Xbox particle textures file if it exists.
/// The string is cached in a `OnceLock` to remain valid for the lifetime of the program.
/// Returns null if the file does not exist.
#[unsafe(no_mangle)]
pub extern "C" fn revc_particle_xbox_txd_path() -> *const std::ffi::c_char {
    static PATH_CACHE: std::sync::OnceLock<Option<std::ffi::CString>> = std::sync::OnceLock::new();
    
    let cached = PATH_CACHE.get_or_init(|| {
        let path = xbox_file("models/particle.txd");
        if path.exists() {
            std::ffi::CString::new(path.to_string_lossy().into_owned()).ok()
        } else {
            None
        }
    });
    
    match cached {
        Some(cstring) => cstring.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Applies Xbox particle configuration overrides to the table based on the current style.
///
/// # Safety
/// `table` must point to a valid slice of `MAX_PARTICLES` `ParticleSystemData` elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_particle_cfg_apply_style(table: *mut ParticleSystemData, frames_available: i32) -> i32 {
    // If table is null or style doesn't require merging, do nothing.
    if table.is_null() || !merge_wanted(crate::texmods::style()) {
        return 0;
    }

    // Read the Xbox particle config file.
    let path = xbox_file("data/particle.cfg");
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return 0,
    };

    // CRLF to LF, as the game's text-mode load does.
    let normalized: Vec<u8> = data.iter().copied().filter(|&b| b != b'\r').collect();

    // Merge the configuration into the table.
    let n = merge_by_name(&normalized, unsafe { std::slice::from_raw_parts_mut(table, MAX_PARTICLES) });

    // If no animation frames are available, disable animation for specific flame types.
    if frames_available == 0 {
        let names: [&[u8]; 3] = [b"FLAME", b"CARFLAME", b"FIREBALL"];
        for entry in unsafe { std::slice::from_raw_parts_mut(table, MAX_PARTICLES) } {
            // Find the length of the name in the struct (up to first NUL).
            let name_len = entry.name.iter().position(|&b| b == 0).unwrap_or(20);
            let entry_name = &entry.name[..name_len];
            
            if names.iter().any(|n| entry_name == *n) {
                entry.start_animation_frame = 0;
                entry.final_animation_frame = 0;
                entry.animation_speed = 0;
                entry.flags &= !FLAG_ANIM_EVERY_OTHER;
            }
        }
        eprintln!("particles: Xbox values for {} particle types (no Xbox flame frames: single frame)", n);
    } else {
        eprintln!("particles: Xbox values for {} particle types", n);
    }

    n as i32
}

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn test_merge_wanted() {
        assert!(!merge_wanted(0)); // PC
        assert!(!merge_wanted(1)); // PS2
        assert!(merge_wanted(2));  // Xbox
        assert!(!merge_wanted(3)); // Unknown
    }
}

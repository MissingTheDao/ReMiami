use crate::cscan::Scan;

/// Size of the object info array.
pub const NUMOBJECTINFO: usize = 210;
/// Gravity constant used for buoyancy calculations.
pub const GRAVITY: f32 = 0.008;

/// Information about a specific object in the game.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectInfo {
    pub mass: f32,
    pub turn_mass: f32,
    pub air_resistance: f32,
    pub elasticity: f32,
    pub buoyancy: f32,
    pub uproot_limit: f32,
    pub collision_damage_multiplier: f32,
    pub collision_damage_effect: u8,
    pub special_collision_response_cases: u8,
    pub camera_to_avoid_this_object: bool,
}

// Ensure struct size is 32 bytes (0x20) as per notes.
const _: () = assert!(std::mem::size_of::<ObjectInfo>() == 32);

/// Hooks for interacting with the rest of the engine.
pub trait Hooks {
    /// Find model index by name.
    fn find_model(&mut self, name: &[u8]) -> Option<i32>;
    /// Set the object ID for a model.
    fn set_object_id(&mut self, model: i32, id: i16);
}

// External C functions for hooks.
unsafe extern "C" {
    fn revc_cpp_find_model(name: *const u8, len: usize) -> i32;
    fn revc_cpp_set_object_id(model: i32, id: i16);
}

/// C++ implementation of Hooks trait.
struct CppHooks;

impl Hooks for CppHooks {
    fn find_model(&mut self, name: &[u8]) -> Option<i32> {
        let model = unsafe { revc_cpp_find_model(name.as_ptr(), name.len()) };
        if model == -1 {
            None
        } else {
            Some(model)
        }
    }

    fn set_object_id(&mut self, model: i32, id: i16) {
        unsafe { revc_cpp_set_object_id(model, id) };
    }
}

/// Initialise object data from the parsed buffer.
///
/// # Arguments
/// * `buf` - File bytes with CRLF converted to LF.
/// * `infos` - Mutable slice of `ObjectInfo` with `NUMOBJECTINFO` entries.
/// * `hooks` - Hooks implementation.
pub fn initialise(buf: &[u8], infos: &mut [ObjectInfo], hooks: &mut dyn Hooks) {
    // 1. Defaults for entries 0..=3
    for e in 0..4 {
        let info = &mut infos[e];
        info.mass = 99999.0;
        info.turn_mass = 99999.0;
        info.air_resistance = 0.99;
        info.elasticity = 0.1;
        info.uproot_limit = 0.0;
        info.collision_damage_multiplier = 1.0;
        info.collision_damage_effect = 0;
    }

    let base_buoyancy = GRAVITY * infos[0].mass * 2.0;
    infos[0].buoyancy = base_buoyancy;
    for e in 1..4 {
        infos[e].buoyancy = base_buoyancy;
    }

    // Special collision response cases
    let special_response_defaults = [0u8, 0, 4, 4];
    for (i, val) in special_response_defaults.iter().enumerate() {
        infos[i].special_collision_response_cases = *val;
    }

    // Camera avoid defaults
    let cam_avoid_defaults = [false, true, false, true];
    for (i, val) in cam_avoid_defaults.iter().enumerate() {
        infos[i].camera_to_avoid_this_object = *val;
    }

    // 2. Parse
    let mut percent_submerged: f32 = 0.0;
    let mut damage_effect: i32 = 0;
    let mut response_case: i32 = 0;
    let mut cam_avoid: i32 = 0;

    let mut id: usize = 4;
    let mut p: usize = 0;
    let len = buf.len();

    loop {
        if p >= len {
            break;
        }
        if buf[p] == b'*' {
            break;
        }

        // Skip whitespace and comments
        while matches!(buf[p], b' ' | b'\n' | b'\r' | b';') {
            if buf[p] == b';' {
                while p < len && buf[p] != b'\n' && buf[p] != b'*' {
                    p += 1;
                }
            } else {
                p += 1;
            }
            if p >= len {
                break;
            }
        }

        if p >= len || buf[p] == b'*' {
            break;
        }

        // Extract line
        let start = p;
        while p < len && buf[p] != b'\n' && buf[p] != b'*' {
            p += 1;
        }
        if p < len && buf[p] == b'\n' {
            p += 1;
        }

        let line_bytes = &buf[start..p];
        let mut line = Vec::with_capacity(line_bytes.len());
        for &b in line_bytes {
            if b == b',' {
                line.push(b' ');
            } else {
                line.push(b);
            }
        }

        if id >= NUMOBJECTINFO {
            continue;
        }

        let mut scan = Scan::new(&line);
        let name = scan.word().unwrap_or_default();
        
        // Parse fields into infos[id]
        // We need to parse into the struct. The scan reads from the line.
        // The notes say: "into infos[id] directly: mass = float()..."
        // This implies the scan is used to extract values which are then assigned.
        
        let mass = scan.float();
        let turn_mass = scan.float();
        let air_resistance = scan.float();
        let elasticity = scan.float();
        let percent_submerged_scan = scan.float();
        let uproot_limit = scan.float();
        let collision_damage_multiplier = scan.float();
        let damage_effect_scan = scan.int();
        let response_case_scan = scan.int();
        let cam_avoid_scan = scan.int();

        let e = &mut infos[id];

        // Update fields if they were present in the scan
        if let Some(v) = mass { e.mass = v; }
        if let Some(v) = turn_mass { e.turn_mass = v; }
        if let Some(v) = air_resistance { e.air_resistance = v; }
        if let Some(v) = elasticity { e.elasticity = v; }
        if let Some(v) = uproot_limit { e.uproot_limit = v; }
        if let Some(v) = collision_damage_multiplier { e.collision_damage_multiplier = v; }

        // Handle percent_submerged
        if let Some(v) = percent_submerged_scan {
            percent_submerged = v;
        }

        // Handle damage_effect, response_case, cam_avoid
        if let Some(v) = damage_effect_scan {
            damage_effect = v;
        }
        if let Some(v) = response_case_scan {
            response_case = v;
        }
        if let Some(v) = cam_avoid_scan {
            cam_avoid = v;
        }

        // Calculate buoyancy
        if percent_submerged > 0.0 {
            e.buoyancy = 100.0 / percent_submerged * GRAVITY * e.mass;
        }

        e.collision_damage_effect = damage_effect as u8;
        e.special_collision_response_cases = response_case as u8;
        e.camera_to_avoid_this_object = cam_avoid != 0;

        // Hook logic
        if let Some(model) = hooks.find_model(&name) {
            let z = infos[0];
            let two = infos[2];
            let e_ref = infos[id];

            let condition1 = z.mass != e_ref.mass
                || z.collision_damage_multiplier != e_ref.collision_damage_multiplier
                || z.collision_damage_effect != e_ref.collision_damage_effect
                || (z.special_collision_response_cases != e_ref.special_collision_response_cases
                    && two.special_collision_response_cases != e_ref.special_collision_response_cases);

            if condition1 {
                hooks.set_object_id(model, id as i16);
                id += 1;
            } else if z.special_collision_response_cases == e_ref.special_collision_response_cases {
                let id_val = if z.camera_to_avoid_this_object == e_ref.camera_to_avoid_this_object {
                    0
                } else {
                    1
                };
                hooks.set_object_id(model, id_val);
            } else if two.camera_to_avoid_this_object == e_ref.camera_to_avoid_this_object {
                hooks.set_object_id(model, 2);
            } else {
                hooks.set_object_id(model, 3);
            }
        }
    }
}

/// Unsafe external interface to initialise object data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_object_data_init(data: *const u8, len: usize, infos: *mut ObjectInfo) { unsafe {
    let data_slice = std::slice::from_raw_parts(data, len);
    let infos_slice = std::slice::from_raw_parts_mut(infos, NUMOBJECTINFO);
    let mut hooks = CppHooks;
    initialise(data_slice, infos_slice, &mut hooks);
}}

#[cfg(test)]
mod golden {
    //! tests/golden/objectdata.txt: the original Initialise on synthetic object.dat files
    //! (tests/golden/objectdata/; models are "mdl0".."mdl399") and the game's object.dat (no models).
    use super::*;

    struct Log {
        out: String,
        models: bool,
    }
    impl Hooks for Log {
        fn find_model(&mut self, name: &[u8]) -> Option<i32> {
            let n: i32 = std::str::from_utf8(name.strip_prefix(b"mdl")?).ok()?.parse().ok()?;
            (self.models && (0..400).contains(&n)).then_some(n)
        }
        fn set_object_id(&mut self, model: i32, id: i16) {
            self.out += &format!(" set {model} {id}");
        }
    }

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
        let golden = std::fs::read_to_string(root.join("tests/golden/objectdata.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (name, want) = line.split_once(" =>").unwrap();
            let file = name.split(' ').next().unwrap();
            let path = if file == "game" { game.join("data/object.dat") } else { root.join("tests/golden/objectdata").join(file) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(file, "game");
                continue;
            };
            // The recorder's stub finds "mdlN" models in every file; the game's names never match.
            let mut log = Log { out: file.to_string(), models: true };
            let mut infos: Vec<ObjectInfo> = vec![unsafe { std::mem::zeroed() }; NUMOBJECTINFO];
            initialise(&text_mode(&raw), &mut infos, &mut log);
            assert_eq!(log.out, name, "{file}: SetObjectID calls");
            let got: String = infos
                .iter()
                .map(|e| {
                    let b = unsafe { std::slice::from_raw_parts(e as *const ObjectInfo as *const u8, 32) };
                    format!(" {:016x}", b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3)))
                })
                .collect();
            assert_eq!(got, want, "{file}");
            checked += 1;
        }
        assert!(checked >= 16);
    }
}

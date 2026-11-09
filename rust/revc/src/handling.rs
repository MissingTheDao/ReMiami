//! Vehicle handling data (C++ src/vehicles/HandlingMgr.cpp: cHandlingDataMgr::LoadHandlingData and
//! its unit conversions; Transmission.cpp: cTransmission::InitGearRatios). C++ loads
//! data/handling.cfg (text mode: CRLF already LF) and hands over the bytes; the tables stay in the
//! C++ manager. Bit for bit with the original (tests/golden/handling.txt). Structs and names
//! drafted by the local model.

use crate::math::V3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Gear {
    pub max_velocity: f32,
    pub shift_up_velocity: f32,
    pub shift_down_velocity: f32,
}

const _: () = assert!(std::mem::size_of::<Gear>() == 12);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Transmission {
    pub gears: [Gear; 6],
    pub drive_type: u8,
    pub engine_type: u8,
    pub number_of_gears: i8,
    pub flags: u8,
    pub engine_acceleration: f32,
    pub max_velocity: f32,
    pub max_cruise_velocity: f32,
    pub max_reverse_velocity: f32,
    pub cur_velocity: f32,
}

const _: () = assert!(std::mem::size_of::<Transmission>() == 96);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct HandlingData {
    pub identifier: i32,
    pub mass: f32,
    pub inv_mass: f32,
    pub turn_mass: f32,
    pub dimension: V3,
    pub centre_of_mass: V3,
    pub percent_submerged: i8,
    pub buoyancy: f32,
    pub traction_multiplier: f32,
    pub transmission: Transmission,
    pub brake_deceleration: f32,
    pub brake_bias: f32,
    pub abs: i8,
    pub steering_lock: f32,
    pub traction_loss: f32,
    pub traction_bias: f32,
    pub unused: f32,
    pub suspension_force_level: f32,
    pub suspension_damping_level: f32,
    pub suspension_upper_limit: f32,
    pub suspension_lower_limit: f32,
    pub suspension_bias: f32,
    pub suspension_antidive_multiplier: f32,
    pub collision_damage_multiplier: f32,
    pub flags: u32,
    pub seat_offset_distance: f32,
    pub monetary_value: i32,
    pub front_lights: i8,
    pub rear_lights: i8,
}

const _: () = assert!(std::mem::size_of::<HandlingData>() == 220);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BikeHandlingData {
    pub identifier: i32,
    pub lean_fwd_com: f32,
    pub lean_fwd_force: f32,
    pub lean_bak_com: f32,
    pub lean_back_force: f32,
    pub max_lean: f32,
    pub full_anim_lean: f32,
    pub des_lean: f32,
    pub speed_steer: f32,
    pub slip_steer: f32,
    pub no_player_com_z: f32,
    pub wheelie_ang: f32,
    pub stoppie_ang: f32,
    pub wheelie_steer: f32,
    pub wheelie_stab_mult: f32,
    pub stoppie_stab_mult: f32,
}

const _: () = assert!(std::mem::size_of::<BikeHandlingData>() == 64);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BoatHandlingData {
    pub identifier: i32,
    pub thrust_y: f32,
    pub thrust_z: f32,
    pub thrust_app_z: f32,
    pub aq_plane_force: f32,
    pub aq_plane_limit: f32,
    pub aq_plane_offset: f32,
    pub wave_audio_mult: f32,
    pub look_l_r_behind_cam_height: f32,
    pub move_res: V3,
    pub turn_res: V3,
}

const _: () = assert!(std::mem::size_of::<BoatHandlingData>() == 60);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FlyingHandlingData {
    pub identifier: i32,
    pub thrust: f32,
    pub thrust_fall_off: f32,
    pub yaw: f32,
    pub yaw_stab: f32,
    pub side_slip: f32,
    pub roll: f32,
    pub roll_stab: f32,
    pub pitch: f32,
    pub pitch_stab: f32,
    pub form_lift: f32,
    pub attack_lift: f32,
    pub move_res: f32,
    pub turn_res: V3,
    pub speed_res: V3,
}

const _: () = assert!(std::mem::size_of::<FlyingHandlingData>() == 76);

pub const NUMHANDLINGS: usize = 106;
pub const HANDLING_RCBANDIT: usize = 38;
pub const HANDLING_BIKE: usize = 81;
pub const HANDLING_FREEWAY: usize = 85;
pub const HANDLING_PREDATOR: usize = 86;
pub const HANDLING_SEAPLANE: usize = 96;
pub const HANDLING_RCCOPTER: usize = 105;

pub const NUMBIKE: usize = 5;
pub const NUMFLYING: usize = 10;
pub const NUMBOAT: usize = 11;

pub const VEHICLE_NAMES: [&str; NUMHANDLINGS] = [
    "LANDSTAL", "IDAHO", "STINGER", "LINERUN", "PEREN", "SENTINEL", "PATRIOT", "FIRETRUK",
    "TRASH", "STRETCH", "MANANA", "INFERNUS", "PONY", "MULE", "CHEETAH", "AMBULAN", "FBICAR",
    "MOONBEAM", "ESPERANT", "TAXI", "KURUMA", "BOBCAT", "MRWHOOP", "BFINJECT", "POLICE",
    "ENFORCER", "SECURICA", "BANSHEE", "BUS", "RHINO", "BARRACKS", "TRAIN", "HELI", "DODO",
    "COACH", "CABBIE", "STALLION", "RUMPO", "RCBANDIT", "MAFIA", "AIRTRAIN", "DEADDODO",
    "FLATBED", "YANKEE", "GOLFCART", "VOODOO", "WASHING", "CUBAN", "ROMERO", "PACKER",
    "ADMIRAL", "GANGBUR", "ZEBRA", "TOPFUN", "GLENDALE", "OCEANIC", "HERMES", "SABRE1",
    "SABRETUR", "PHEONIX", "WALTON", "REGINA", "COMET", "DELUXO", "BURRITO", "SPAND",
    "BAGGAGE", "KAUFMAN", "RANCHER", "FBIRANCH", "VIRGO", "GREENWOO", "HOTRING", "SANDKING",
    "BLISTAC", "BOXVILLE", "BENSON", "DESPERAD", "LOVEFIST", "BLOODRA", "BLOODRB", "BIKE",
    "MOPED", "DIRTBIKE", "ANGEL", "FREEWAY", "PREDATOR", "SPEEDER", "REEFER", "RIO", "SQUALO",
    "TROPIC", "COASTGRD", "DINGHY", "MARQUIS", "CUPBOAT", "SEAPLANE", "SPARROW", "SEASPAR",
    "MAVERICK", "COASTMAV", "POLMAV", "HUNTER", "RCBARON", "RCGOBLIN", "RCCOPTER",
];

const GRAVITY: f32 = 0.008;

/// FindExactWord over the vehicle names: the index, or NUMHANDLINGS when unknown.
pub fn find_exact_word(word: &[u8]) -> usize {
    VEHICLE_NAMES.iter().position(|n| n.as_bytes() == word).unwrap_or(NUMHANDLINGS)
}

use crate::cscan::{atof, atoi};

/// sscanf("%x"): None when no hex digit follows (the target keeps its value).
fn scan_hex(w: &[u8]) -> Option<u32> {
    let mut i = 0;
    while i < w.len() && (w[i] == b' ' || (b'\t'..=b'\r').contains(&w[i])) {
        i += 1;
    }
    let neg = i < w.len() && w[i] == b'-';
    if i < w.len() && (w[i] == b'+' || w[i] == b'-') {
        i += 1;
    }
    if i + 1 < w.len() && w[i] == b'0' && (w[i + 1] | 0x20) == b'x' && w.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit()) {
        i += 2;
    }
    let s = i;
    let mut v: u64 = 0;
    while i < w.len() && w[i].is_ascii_hexdigit() {
        v = v.wrapping_mul(16).wrapping_add((w[i] as char).to_digit(16).unwrap() as u64);
        i += 1;
    }
    if i == s {
        return None;
    }
    Some((if neg { v.wrapping_neg() } else { v }) as u32)
}

/// The tables LoadHandlingData fills.
pub struct Tables<'a> {
    pub cars: &'a mut [HandlingData],
    pub bikes: &'a mut [BikeHandlingData],
    pub flying: &'a mut [FlyingHandlingData],
    pub boats: &'a mut [BoatHandlingData],
}

/// LoadHandlingData over the file's bytes (CRLF already LF). Where the original would write out
/// of bounds (an unknown vehicle name; a bike line for a non-bike), the line is skipped; a file
/// without a final newline or ";the end" ends where the original would read past its buffer.
pub fn load(data: &[u8], t: &mut Tables) {
    // The current entry of each kind, kept from line to line as the original's pointers.
    let (mut car, mut bike, mut flying, mut boat): (Option<usize>, Option<usize>, Option<usize>, Option<usize>) = (None, None, None, None);
    let mut start = 0;
    let mut end = 1;
    loop {
        while end < data.len() && data[end] != b'\n' {
            end += 1;
        }
        if end >= data.len() {
            return;
        }
        let line = &data[start..end];
        // strncpy stops at a NUL.
        let line = &line[..line.iter().position(|&c| c == 0).unwrap_or(line.len())];
        start = end + 1;
        end = start + 1;
        if line == b";the end" {
            return;
        }
        let first = line.first().copied().unwrap_or(0);
        if first == b';' {
            continue;
        }
        let words = line.split(|&c| c == b' ' || c == b'\t').filter(|w| !w.is_empty());
        let f = |w: &[u8]| atof(w) as f32;
        match first {
            b'!' => {
                let mut skip = false;
                for (field, w) in words.enumerate() {
                    if field == 1 {
                        let id = find_exact_word(w);
                        if !(HANDLING_BIKE..=HANDLING_FREEWAY).contains(&id) {
                            skip = true;
                            break;
                        }
                        bike = Some(id - HANDLING_BIKE);
                        t.bikes[id - HANDLING_BIKE].identifier = id as i32;
                        continue;
                    }
                    let Some(b) = bike.map(|i| &mut t.bikes[i]) else { continue };
                    match field {
                        2 => b.lean_fwd_com = f(w),
                        3 => b.lean_fwd_force = f(w),
                        4 => b.lean_bak_com = f(w),
                        5 => b.lean_back_force = f(w),
                        6 => b.max_lean = f(w),
                        7 => b.full_anim_lean = f(w),
                        8 => b.des_lean = f(w),
                        9 => b.speed_steer = f(w),
                        10 => b.slip_steer = f(w),
                        11 => b.no_player_com_z = f(w),
                        12 => b.wheelie_ang = f(w),
                        13 => b.stoppie_ang = f(w),
                        14 => b.wheelie_steer = f(w),
                        15 => b.wheelie_stab_mult = f(w),
                        16 => b.stoppie_stab_mult = f(w),
                        _ => {}
                    }
                }
                if !skip {
                    if let Some(i) = bike {
                        convert_bike(&mut t.bikes[i]);
                    }
                }
            }
            b'$' => {
                for (field, w) in words.enumerate() {
                    if field == 1 {
                        let id = find_exact_word(w);
                        let i = if (HANDLING_SEAPLANE..=HANDLING_RCCOPTER).contains(&id) { id - HANDLING_SEAPLANE } else { 0 };
                        flying = Some(i);
                        t.flying[i].identifier = id as i32;
                        continue;
                    }
                    let Some(p) = flying.map(|i| &mut t.flying[i]) else { continue };
                    match field {
                        2 => p.thrust = f(w),
                        3 => p.thrust_fall_off = f(w),
                        4 => p.yaw = f(w),
                        5 => p.yaw_stab = f(w),
                        6 => p.side_slip = f(w),
                        7 => p.roll = f(w),
                        8 => p.roll_stab = f(w),
                        9 => p.pitch = f(w),
                        10 => p.pitch_stab = f(w),
                        11 => p.form_lift = f(w),
                        12 => p.attack_lift = f(w),
                        13 => p.move_res = f(w),
                        14 => p.turn_res.x = f(w),
                        15 => p.turn_res.y = f(w),
                        16 => p.turn_res.z = f(w),
                        17 => p.speed_res.x = f(w),
                        18 => p.speed_res.y = f(w),
                        19 => p.speed_res.z = f(w),
                        _ => {}
                    }
                }
            }
            b'%' => {
                for (field, w) in words.enumerate() {
                    if field == 1 {
                        let id = find_exact_word(w);
                        let i = if (HANDLING_PREDATOR..=HANDLING_SEAPLANE).contains(&id) { id - HANDLING_PREDATOR } else { 0 };
                        boat = Some(i);
                        t.boats[i].identifier = id as i32;
                        continue;
                    }
                    let Some(b) = boat.map(|i| &mut t.boats[i]) else { continue };
                    match field {
                        2 => b.thrust_y = f(w),
                        3 => b.thrust_z = f(w),
                        4 => b.thrust_app_z = f(w),
                        5 => b.aq_plane_force = f(w),
                        6 => b.aq_plane_limit = f(w),
                        7 => b.aq_plane_offset = f(w),
                        8 => b.wave_audio_mult = f(w),
                        9 => b.move_res.x = f(w),
                        10 => b.move_res.y = f(w),
                        11 => b.move_res.z = f(w),
                        12 => b.turn_res.x = f(w),
                        13 => b.turn_res.y = f(w),
                        14 => b.turn_res.z = f(w),
                        15 => b.look_l_r_behind_cam_height = f(w),
                        _ => {}
                    }
                }
            }
            _ => {
                let mut skip = false;
                for (field, w) in words.enumerate() {
                    if field == 0 {
                        let id = find_exact_word(w);
                        if id >= NUMHANDLINGS {
                            skip = true;
                            break;
                        }
                        car = Some(id);
                        t.cars[id].identifier = id as i32;
                        continue;
                    }
                    let Some(h) = car.map(|i| &mut t.cars[i]) else { continue };
                    let tr = &mut h.transmission;
                    match field {
                        1 => h.mass = f(w),
                        2 => h.dimension.x = f(w),
                        3 => h.dimension.y = f(w),
                        4 => h.dimension.z = f(w),
                        5 => h.centre_of_mass.x = f(w),
                        6 => h.centre_of_mass.y = f(w),
                        7 => h.centre_of_mass.z = f(w),
                        8 => h.percent_submerged = atoi(w) as i8,
                        9 => h.traction_multiplier = f(w),
                        10 => h.traction_loss = f(w),
                        11 => h.traction_bias = f(w),
                        12 => tr.number_of_gears = atoi(w) as i8,
                        13 => tr.max_velocity = f(w),
                        // In double, as the original.
                        14 => tr.engine_acceleration = (atof(w) * 0.4) as f32,
                        15 => tr.drive_type = w[0],
                        16 => tr.engine_type = w[0],
                        17 => h.brake_deceleration = f(w),
                        18 => h.brake_bias = f(w),
                        19 => h.abs = (atoi(w) != 0) as i8,
                        20 => h.steering_lock = f(w),
                        21 => h.suspension_force_level = f(w),
                        22 => h.suspension_damping_level = f(w),
                        23 => h.seat_offset_distance = f(w),
                        24 => h.collision_damage_multiplier = f(w),
                        25 => h.monetary_value = atoi(w),
                        26 => h.suspension_upper_limit = f(w),
                        27 => h.suspension_lower_limit = f(w),
                        28 => h.suspension_bias = f(w),
                        29 => h.suspension_antidive_multiplier = f(w),
                        30 => {
                            if let Some(v) = scan_hex(w) {
                                h.flags = v;
                            }
                            h.transmission.flags = h.flags as u8;
                        }
                        31 => h.front_lights = atoi(w) as i8,
                        32 => h.rear_lights = atoi(w) as i8,
                        _ => {}
                    }
                }
                // An empty car line converts the current car again, as the original.
                if !skip {
                    if let Some(i) = car {
                        convert(&mut t.cars[i]);
                    }
                }
            }
        }
    }
}

/// ConvertDataToGameUnits: distances to m, time to 1/50 s; the top speed where drag meets the
/// engine; gear ratios.
pub fn convert(h: &mut HandlingData) {
    let t = &mut h.transmission;
    t.engine_acceleration *= 1.0 / (50.0 * 50.0);
    t.max_velocity *= 1000.0 / (60.0 * 60.0 * 50.0);
    h.brake_deceleration *= 1.0 / (50.0 * 50.0);
    h.turn_mass = (h.dimension.x * h.dimension.x + h.dimension.y * h.dimension.y) * h.mass / 12.0;
    if h.turn_mass < 10.0 {
        h.turn_mass *= 5.0;
    }
    h.inv_mass = 1.0 / h.mass;
    h.collision_damage_multiplier *= 2000.0 / h.mass;
    h.buoyancy = 100.0 / h.percent_submerged as f32 * GRAVITY * h.mass;

    let (mut a, mut b) = (0.0f32, 100.0f32);
    let mut velocity = t.max_velocity;
    while a < b && velocity > 0.0 {
        velocity -= 0.01;
        a = t.engine_acceleration / 6.0;
        let a_drag = 0.5 * (velocity * velocity) * h.dimension.x * h.dimension.z / h.mass;
        b = -velocity * (1.0 / (a_drag + 1.0) - 1.0);
    }
    let id = h.identifier as usize;
    if id == HANDLING_RCBANDIT {
        t.max_cruise_velocity = t.max_velocity;
        t.max_reverse_velocity = -t.max_velocity;
    } else if (HANDLING_BIKE..=HANDLING_FREEWAY).contains(&id) {
        t.max_cruise_velocity = velocity;
        t.max_velocity = velocity * 1.2;
        t.max_reverse_velocity = -0.05;
    } else {
        t.max_cruise_velocity = velocity;
        t.max_velocity = velocity * 1.2;
        t.max_reverse_velocity = -0.2;
    }
    if t.drive_type == b'4' {
        t.engine_acceleration /= 4.0;
    } else {
        t.engine_acceleration /= 2.0;
    }
    init_gear_ratios(t);
}

fn deg_to_rad(x: f32) -> f32 {
    x * std::f32::consts::PI / 180.0
}

/// ConvertBikeDataToGameUnits.
pub fn convert_bike(b: &mut BikeHandlingData) {
    b.max_lean = deg_to_rad(b.max_lean).sin();
    b.full_anim_lean = deg_to_rad(b.full_anim_lean);
    b.wheelie_ang = deg_to_rad(b.wheelie_ang).sin();
    b.stoppie_ang = deg_to_rad(b.stoppie_ang).sin();
}

/// cTransmission::InitGearRatios: gear 0 is reverse, 1..=n forward. (More than 4 gears would write
/// past the six gears in the original; those writes are dropped.)
pub fn init_gear_ratios(t: &mut Transmission) {
    t.gears = [Gear::default(); 6];
    let n = t.number_of_gears as i32;
    for i in 1..=n {
        let (i0, i1) = ((i - 1) as usize, i as usize);
        if i1 > 5 {
            break;
        }
        t.gears[i1].max_velocity = i as f32 / n as f32 * t.max_velocity;
        let g0max = t.gears[i0].max_velocity;
        let diff = t.gears[i1].max_velocity - g0max;
        if i >= n {
            t.gears[i1].shift_up_velocity = t.max_velocity;
        } else {
            if i1 + 1 <= 5 {
                t.gears[i1 + 1].shift_down_velocity = diff * 0.42 + g0max;
            }
            t.gears[i1].shift_up_velocity = diff * 0.6667 + g0max;
        }
    }
    t.gears[0].max_velocity = t.max_reverse_velocity;
    t.gears[0].shift_up_velocity = -0.01;
    t.gears[0].shift_down_velocity = t.max_reverse_velocity;
    t.gears[1].shift_down_velocity = -0.01;
}

/// # Safety
/// `data` holds `len` bytes; the table pointers hold NUMHANDLINGS cars, 5 bikes, 10 flying, 11 boats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_handling_load(
    data: *const u8,
    len: usize,
    cars: *mut HandlingData,
    bikes: *mut BikeHandlingData,
    flying: *mut FlyingHandlingData,
    boats: *mut BoatHandlingData,
) {
    unsafe {
        let mut t = Tables {
            cars: std::slice::from_raw_parts_mut(cars, NUMHANDLINGS),
            bikes: std::slice::from_raw_parts_mut(bikes, NUMBIKE),
            flying: std::slice::from_raw_parts_mut(flying, NUMFLYING),
            boats: std::slice::from_raw_parts_mut(boats, NUMBOAT),
        };
        load(std::slice::from_raw_parts(data, len), &mut t);
    }
}

/// # Safety
/// `t` is a valid cTransmission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_transmission_init_gears(t: *mut Transmission) {
    unsafe { init_gear_ratios(&mut *t) }
}

#[cfg(test)]
mod golden {
    //! tests/golden/handling.txt: the original loader on the synthetic files in
    //! tests/golden/handling/ (and on the game's handling.cfg when the game data is there):
    //! a FNV-1a 64 hash of every entry's bytes.
    use super::*;

    fn fnv<T>(x: &T) -> u64 {
        let b = unsafe { std::slice::from_raw_parts(x as *const T as *const u8, std::mem::size_of::<T>()) };
        b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3))
    }

    /// The file as the game's text mode reads it.
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

    fn hashes(raw: &[u8]) -> Vec<String> {
        // Zeroed like the C++ manager (padding included).
        let mut cars: Vec<HandlingData> = vec![unsafe { std::mem::zeroed() }; NUMHANDLINGS];
        let mut bikes: Vec<BikeHandlingData> = vec![unsafe { std::mem::zeroed() }; NUMBIKE];
        let mut flying: Vec<FlyingHandlingData> = vec![unsafe { std::mem::zeroed() }; NUMFLYING];
        let mut boats: Vec<BoatHandlingData> = vec![unsafe { std::mem::zeroed() }; NUMBOAT];
        load(&text_mode(raw), &mut Tables { cars: &mut cars, bikes: &mut bikes, flying: &mut flying, boats: &mut boats });
        let mut out = Vec::new();
        out.extend(cars.iter().enumerate().map(|(i, x)| format!("car {i} {:016x}", fnv(x))));
        out.extend(bikes.iter().enumerate().map(|(i, x)| format!("bike {i} {:016x}", fnv(x))));
        out.extend(flying.iter().enumerate().map(|(i, x)| format!("flying {i} {:016x}", fnv(x))));
        out.extend(boats.iter().enumerate().map(|(i, x)| format!("boat {i} {:016x}", fnv(x))));
        out
    }

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/handling.txt")).unwrap();
        let mut files: Vec<&str> = golden.lines().map(|l| l.split_once(' ').unwrap().0).collect();
        files.dedup();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for file in files {
            let path = if file == "game" { game.join("data/handling.cfg") } else { root.join("tests/golden/handling").join(file) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(file, "game", "missing {}", path.display());
                continue; // no game data here
            };
            let want: Vec<&str> = golden.lines().filter_map(|l| l.strip_prefix(file).map(|r| r.trim_start())).collect();
            let got = hashes(&raw);
            for (w, g) in want.iter().zip(&got) {
                assert_eq!(w, g, "{file}");
            }
            assert_eq!(want.len(), got.len());
            checked += 1;
        }
        assert!(checked >= 16);
    }
}

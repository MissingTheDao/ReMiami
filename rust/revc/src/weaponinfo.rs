use crate::math::V3;

/// A struct representing weapon information, matching the C++ CWeaponInfo layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WeaponInfo {
    pub fire: i32,
    pub range: f32,
    pub firing_rate: u32,
    pub reload: u32,
    pub amount_of_ammunition: i32,
    pub damage: u32,
    pub speed: f32,
    pub radius: f32,
    pub lifespan: f32,
    pub spread: f32,
    pub fire_offset: V3,
    pub anim_to_play: i32,
    pub anim_loop_start: f32,
    pub anim_loop_end: f32,
    pub anim_frame_fire: f32,
    pub anim2_loop_start: f32,
    pub anim2_loop_end: f32,
    pub anim2_frame_fire: f32,
    pub anim_breakout: f32,
    pub model_id: i32,
    pub model2_id: i32,
    pub flags: u32,
    pub weapon_slot: u32,
}

// Layout assertion: 100 bytes
const _: () = assert!(std::mem::size_of::<WeaponInfo>() == 100);

/// Weapon names matching the C++ `ms_aWeaponNames` array.
pub const WEAPON_NAMES: [&str; 37] = [
    "Unarmed",
    "BrassKnuckle",
    "ScrewDriver",
    "GolfClub",
    "NightStick",
    "Knife",
    "BaseballBat",
    "Hammer",
    "Cleaver",
    "Machete",
    "Katana",
    "Chainsaw",
    "Grenade",
    "DetonateGrenade",
    "TearGas",
    "Molotov",
    "Rocket",
    "Colt45",
    "Python",
    "Shotgun",
    "Spas12Shotgun",
    "StubbyShotgun",
    "Tec9",
    "Uzi",
    "SilencedIngram",
    "Mp5",
    "m4",
    "Ruger",
    "SniperRifle",
    "LaserScope",
    "RocketLauncher",
    "FlameThrower",
    "M60",
    "Minigun",
    "Detonator",
    "HeliCannon",
    "Camera",
];

/// Total number of weapon types.
pub const TOTAL: usize = 37;

/// Fire type constants.
pub const WEAPON_FIRE_MELEE: i32 = 0;
pub const WEAPON_FIRE_INSTANT_HIT: i32 = 1;
pub const WEAPON_FIRE_PROJECTILE: i32 = 2;
pub const WEAPON_FIRE_AREA_EFFECT: i32 = 3;
pub const WEAPON_FIRE_CAMERA: i32 = 4;

/// Weapon type constants.
pub const WEAPONTYPE_FLAMETHROWER: usize = 31;
pub const WEAPONTYPE_SHOTGUN: usize = 19;
pub const WEAPONTYPE_SPAS12_SHOTGUN: usize = 20;
pub const WEAPONTYPE_STUBBY_SHOTGUN: usize = 21;

/// Trait for hooks that handle side effects during weapon data loading.
pub trait Hooks {
    /// CWeaponModelInfo::SetWeaponInfo on the model (never called with -1).
    fn weapon_model(&mut self, weapon_type: i32, model_id: i32);
    /// MI_BOMB (a model index known at run time).
    fn bomb_model(&mut self) -> i32;
    /// Called to find the animation group ID by name. Returns None if not found.
    fn anim_group(&mut self, name: &[u8]) -> Option<i32>;
}

use crate::cscan::Scan;


/// Parses the weapon data from a buffer.
///
/// # Arguments
///
/// * `data` - The raw file data (CRLF translated to LF).
/// * `infos` - Mutable slice of WeaponInfo structs to populate.
/// * `hooks` - Hooks for side effects.
pub fn load(data: &[u8], infos: &mut [WeaponInfo], hooks: &mut dyn Hooks) {
    let len = data.len();
    let mut bp = 0;

    // Persistent variables across lines
    let mut anim_to_play: Vec<u8> = Vec::new();
    let mut anim_loop_start: f32 = 0.0;
    let mut anim_loop_end: f32 = 0.0;
    let mut delay_anim_fire: f32 = 0.0;
    let mut anim2_loop_start: f32 = 0.0;
    let mut anim2_loop_end: f32 = 0.0;
    let mut delay_anim2_fire: f32 = 0.0;
    let mut anim_breakout: f32 = 0.0;
    let mut model_id: i32 = 0;
    let mut model_id2: i32 = 0;
    let mut weapon_slot: i32 = 0;

    while bp < len {
        // Find end of line
        let mut line_end = bp;
        while line_end < len && data[line_end] != b'\n' {
            line_end += 1;
        }
        
        let line = &data[bp..line_end];
        bp = line_end + 1; // Skip '\n'

        // Skip whitespace: bytes where (b as i8) <= 32
        let mut lp = 0;
        while lp < line.len() {
            if (line[lp] as i8) <= 32 {
                lp += 1;
            } else {
                break;
            }
        }

        // Skip empty lines or comments
        if lp == line.len() || line[lp] == b'#' {
            continue;
        }

        // Reset per-line variables
        let mut spread: f32 = 0.0;
        let mut flags: u32 = 0;
        let mut speed: f32 = 0.0;
        let mut ammo: i32 = 0;
        let mut lifespan: f32 = 0.0;
        let mut radius: f32 = 0.0;
        let mut range: f32 = 0.0;
        let mut damage: i32 = 0;
        let mut reload: i32 = 0;
        let mut firing_rate: i32 = 0;
        let mut fx: f32 = 0.0;
        let mut fy: f32 = 0.0;
        let mut fz: f32 = 0.0;
        let mut weapon_name: Vec<u8> = Vec::new();
        let mut fire_type: Vec<u8> = Vec::new();

        // Scan the line
        let mut scan = Scan::new(&line[lp..]);
        
        // Format: %s %s %f %d %d %d %d %f %f %f %f %f %f %f %s %f %f %f %f %f %f %f %d %d %x %d
        let res = (|| -> Option<()> {
            weapon_name = scan.word()?;
            fire_type = scan.word()?;
            range = scan.float()?;
            firing_rate = scan.int()?;
            reload = scan.int()?;
            ammo = scan.int()?;
            damage = scan.int()?;
            speed = scan.float()?;
            radius = scan.float()?;
            lifespan = scan.float()?;
            spread = scan.float()?;
            fx = scan.float()?;
            fy = scan.float()?;
            fz = scan.float()?;
            anim_to_play = scan.word()?;
            anim_loop_start = scan.float()?;
            anim_loop_end = scan.float()?;
            delay_anim_fire = scan.float()?;
            anim2_loop_start = scan.float()?;
            anim2_loop_end = scan.float()?;
            delay_anim2_fire = scan.float()?;
            anim_breakout = scan.float()?;
            model_id = scan.int()?;
            model_id2 = scan.int()?;
            flags = scan.hex()?;
            weapon_slot = scan.int()?;
            Some(())
        })();

        // A scan that stops early keeps what it read (and the earlier lines' values for the rest).
        let _ = res;

        // Check for ENDWEAPONDATA
        if weapon_name.len() >= 13 && &weapon_name[..13] == b"ENDWEAPONDATA" {
            return;
        }

        // Find weapon type index
        let mut wt = 0;
        for i in 0..TOTAL {
            if WEAPON_NAMES[i].as_bytes() == weapon_name.as_slice() {
                wt = i;
                break;
            }
        }

        let w = &mut infos[wt];
        
        // Determine fire type index
        let fire_type_idx = match fire_type.as_slice() {
            b"MELEE" => WEAPON_FIRE_MELEE,
            b"INSTANT_HIT" => WEAPON_FIRE_INSTANT_HIT,
            b"PROJECTILE" => WEAPON_FIRE_PROJECTILE,
            b"AREA_EFFECT" => WEAPON_FIRE_AREA_EFFECT,
            b"CAMERA" => WEAPON_FIRE_CAMERA,
            _ => WEAPON_FIRE_INSTANT_HIT, // Default fallback
        };

        w.fire = fire_type_idx;
        w.range = range;
        w.firing_rate = firing_rate as u32;
        w.reload = reload as u32;
        w.amount_of_ammunition = ammo;
        w.damage = damage as u32;
        w.speed = speed;
        w.radius = radius;
        w.lifespan = lifespan;
        w.spread = spread;
        w.fire_offset = V3::new(fx, fy, fz);
        
        // Apply 30.0 divisor
        w.anim_loop_start = anim_loop_start / 30.0;
        w.anim_loop_end = anim_loop_end / 30.0;
        w.anim2_loop_start = anim2_loop_start / 30.0;
        w.anim2_loop_end = anim2_loop_end / 30.0;
        w.anim_frame_fire = delay_anim_fire / 30.0;
        w.anim2_frame_fire = delay_anim2_fire / 30.0;
        w.anim_breakout = anim_breakout / 30.0;
        
        w.model_id = model_id;
        w.model2_id = model_id2;
        w.flags = flags;
        w.weapon_slot = weapon_slot as u32;

        // Recalculate firing rate if conditions met
        // Condition: raw anim_loop_end < 98.0 && wt != 31 && wt not in {19, 20, 21}
        if anim_loop_end < 98.0 && wt != WEAPONTYPE_FLAMETHROWER && wt != WEAPONTYPE_SHOTGUN && wt != WEAPONTYPE_SPAS12_SHOTGUN && wt != WEAPONTYPE_STUBBY_SHOTGUN {
            w.firing_rate = ((w.anim_loop_end - w.anim_loop_start) * 900.0) as i64 as u32;
        }

        // The remap goes into the line-persistent model id, as in the original.
        if wt == 34 || wt == 35 {
            model_id = -1; // detonator, heli cannon
        } else if wt == 13 {
            model_id = hooks.bomb_model(); // detonate grenade
        }
        if model_id != -1 {
            hooks.weapon_model(wt as i32, model_id);
        }

        // Hook: anim_group
        if let Some(g) = hooks.anim_group(&anim_to_play) {
            w.anim_to_play = g;
        }
    }
}

/// External C functions for hooks.
#[allow(unused_imports)]
use std::ffi::c_void;

unsafe extern "C" {
    fn revc_cpp_weapon_model(weapon_type: i32, model_id: i32);
    fn revc_cpp_bomb_model() -> i32;
    fn revc_cpp_anim_group(name: *const u8, len: usize) -> i32;
}

/// Hooks implementation using C++ interop.
struct CppHooks;

impl Hooks for CppHooks {
    fn weapon_model(&mut self, weapon_type: i32, model_id: i32) {
        unsafe {
            revc_cpp_weapon_model(weapon_type, model_id);
        }
    }

    fn bomb_model(&mut self) -> i32 {
        unsafe { revc_cpp_bomb_model() }
    }

    fn anim_group(&mut self, name: &[u8]) -> Option<i32> {
        let result = unsafe {
            revc_cpp_anim_group(name.as_ptr(), name.len())
        };
        if result == -1 {
            None
        } else {
            Some(result)
        }
    }
}

/// Public entry point for loading weapon data.
///
/// # Safety
///
/// * `data` must point to a valid memory region of `len` bytes.
/// * `infos` must point to a valid memory region of at least 37 * `size_of::<WeaponInfo>()` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_weapon_load(data: *const u8, len: usize, infos: *mut WeaponInfo) { unsafe {
    let data_slice = std::slice::from_raw_parts(data, len);
    let infos_slice = std::slice::from_raw_parts_mut(infos, 37);
    let mut hooks = CppHooks;
    load(data_slice, infos_slice, &mut hooks);
}}

#[cfg(test)]
mod golden {
    //! tests/golden/weaponinfo.txt: the original Initialise + LoadWeaponData on the synthetic files in
    //! tests/golden/weaponinfo/ and the game's weapon.dat (when present): the SetWeaponInfo calls and a
    //! FNV-1a 64 of each entry.
    use super::*;

    /// The game's anim group names (AnimManager.cpp), as the recorder's stub has them.
    const GROUPS: &[&str] = &["man","van","coach","bikes","bikev","bikeh","biked","unarmed","screwdrv","knife","baseball","golfclub","chainsaw","python","colt45","shotgun","buddy","tec","uzi","rifle","m60","sniper","grenade","flame","medic","sunbathe","playidles","riot","strip","lance","player","playerrocket","player1armed","player2armed","playerBBBat","playercsaw","shuffle","oldman","gang1","gang2","fatman","oldfatman","jogger","woman","shopping","busywoman","sexywoman","fatwoman","oldwoman","jogwoman","panicchunky","skate","playerback","playerleft","playerright","rocketback","rocketleft","rocketright","csawback","csawleft","csawright"];
    const MI_BOMB: i32 = 291;

    /// The C++ side: model remaps and SetWeaponInfo (logged), anim group lookup.
    struct Log(String);
    impl Hooks for Log {
        fn weapon_model(&mut self, weapon_type: i32, model_id: i32) {
            self.0 += &format!(" model {model_id} {weapon_type}");
        }
        fn bomb_model(&mut self) -> i32 {
            MI_BOMB
        }
        fn anim_group(&mut self, name: &[u8]) -> Option<i32> {
            GROUPS.iter().position(|g| g.as_bytes() == name).map(|i| i as i32)
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

    /// CWeaponInfo::Initialise's defaults.
    fn defaults() -> Vec<WeaponInfo> {
        let mut v: Vec<WeaponInfo> = vec![unsafe { std::mem::zeroed() }; TOTAL];
        for w in v.iter_mut() {
            w.fire = WEAPON_FIRE_INSTANT_HIT;
            w.anim_to_play = 7; // ASSOCGRP_UNARMED
            w.flags = 1 | 2 | 8 | 16 | 32;
        }
        v
    }

    #[test]
    fn matches_cpp_golden() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let golden = std::fs::read_to_string(root.join("tests/golden/weaponinfo.txt")).unwrap();
        let game = std::env::var("REVC_GAME_DIR").map(std::path::PathBuf::from).unwrap_or(root.join("game"));
        let mut checked = 0;
        for line in golden.lines() {
            let (file, want) = line.split_once(' ').unwrap();
            let path = if file == "game" { game.join("data/weapon.dat") } else { root.join("tests/golden/weaponinfo").join(file) };
            let Ok(raw) = std::fs::read(&path) else {
                assert_eq!(file, "game", "missing {}", path.display());
                continue;
            };
            let mut infos = defaults();
            let mut log = Log(String::new());
            load(&text_mode(&raw), &mut infos, &mut log);
            let mut got = log.0.trim_start().to_string();
            got += if got.is_empty() { "=>" } else { " =>" };
            for w in &infos {
                let b = unsafe { std::slice::from_raw_parts(w as *const WeaponInfo as *const u8, 100) };
                got += &format!(" {:016x}", b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3)));
            }
            assert_eq!(got, want, "{file}");
            checked += 1;
        }
        assert!(checked >= 24);
    }
}

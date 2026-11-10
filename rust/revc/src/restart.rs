//! Restart points (C++ src/control/Restart.cpp, CRestart): where the player comes back after
//! dying or being busted. Rust owns the statics (`revc_restart`; the C++ statics are references
//! into it). Identical to the original (tests/golden/restart.txt).
#![allow(non_snake_case, non_upper_case_globals)]

use crate::math::V3;

pub const NUM_RESTART_POINTS: usize = 8;
pub const LEVEL_GENERIC: u8 = 0;
/// Header + body, as SaveAllRestartPoints writes it.
pub const SAVE_SIZE: usize = 8 + 284;

/// The CRestart statics (the C++ mirror RevcRestart matches).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Restart {
    pub OverrideHospitalLevel: u8,
    pub OverridePoliceStationLevel: u8,
    pub bFadeInAfterNextArrest: bool,
    pub bFadeInAfterNextDeath: bool,
    pub bOverrideRestart: bool,
    pub OverridePosition: V3,
    pub OverrideHeading: f32,
    pub HospitalRestartPoints: [V3; NUM_RESTART_POINTS],
    pub HospitalRestartHeadings: [f32; NUM_RESTART_POINTS],
    pub NumberOfHospitalRestarts: u16,
    pub PoliceRestartPoints: [V3; NUM_RESTART_POINTS],
    pub PoliceRestartHeadings: [f32; NUM_RESTART_POINTS],
    pub NumberOfPoliceRestarts: u16,
}

#[unsafe(no_mangle)]
pub static mut revc_restart: Restart = Restart::ZERO;

/// What finding a restart point needs from the world.
pub trait Hooks {
    /// CTheZones::GetLevelFromPosition.
    fn level(&mut self, pos: &V3) -> i32;
    /// The closest ped path node's position (the fallback when no point is in range).
    fn closest_node(&mut self, pos: &V3) -> V3;
}

fn dist2(a: &V3, b: &V3) -> f32 {
    let (x, y, z) = (a.x - b.x, a.y - b.y, a.z - b.z);
    x * x + y * y + z * z
}

impl Restart {
    pub const ZERO: Restart = unsafe { std::mem::zeroed() };

    pub fn initialise(&mut self) {
        *self = Restart::ZERO;
        self.OverridePoliceStationLevel = LEVEL_GENERIC;
        self.OverrideHospitalLevel = LEVEL_GENERIC;
        self.bFadeInAfterNextArrest = true;
        self.bFadeInAfterNextDeath = true;
    }

    /// Add{Hospital,Police}RestartPoint. A ninth point would write past the arrays in the original;
    /// here it is dropped (the count still goes up, as there).
    pub fn add_point(&mut self, police: bool, pos: V3, heading: f32) {
        let (pts, hds, n) = if police {
            (&mut self.PoliceRestartPoints, &mut self.PoliceRestartHeadings, &mut self.NumberOfPoliceRestarts)
        } else {
            (&mut self.HospitalRestartPoints, &mut self.HospitalRestartHeadings, &mut self.NumberOfHospitalRestarts)
        };
        if let (Some(p), Some(h)) = (pts.get_mut(*n as usize), hds.get_mut(*n as usize)) {
            *p = pos;
            *h = heading;
        }
        *n = n.wrapping_add(1);
    }

    pub fn override_next_restart(&mut self, pos: V3, heading: f32) {
        self.bOverrideRestart = true;
        self.OverridePosition = pos;
        self.OverrideHeading = heading;
    }

    /// FindClosest{Hospital,Police}RestartPoint: (position, heading, fell back to a path node;
    /// the original prints a message then).
    pub fn find_closest(&mut self, police: bool, pos: &V3, h: &mut impl Hooks) -> (V3, f32, bool) {
        if self.bOverrideRestart {
            self.bOverrideRestart = false;
            return (self.OverridePosition, self.OverrideHeading, false);
        }
        let (pts, hds, n, ovr) = if police {
            (&self.PoliceRestartPoints, &self.PoliceRestartHeadings, self.NumberOfPoliceRestarts, self.OverridePoliceStationLevel)
        } else {
            (&self.HospitalRestartPoints, &self.HospitalRestartHeadings, self.NumberOfHospitalRestarts, self.OverrideHospitalLevel)
        };
        let n = (n as usize).min(NUM_RESTART_POINTS);
        let curlevel = h.level(pos);
        let want = if ovr != LEVEL_GENERIC { ovr as i32 } else { curlevel };
        let mut min = 4000.0f32 * 4000.0;
        let mut closest = None;
        for i in 0..n {
            if h.level(&pts[i]) == want {
                let d = dist2(pos, &pts[i]);
                if min >= d {
                    min = d;
                    closest = Some(i);
                }
            }
        }
        if closest.is_none() {
            for i in 0..n {
                let d = dist2(pos, &pts[i]);
                if min >= d {
                    min = d;
                    closest = Some(i);
                }
            }
        }
        match closest {
            Some(i) => (pts[i], hds[i], false),
            None => (h.closest_node(pos), 0.0, true),
        }
    }

    pub fn save(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(SAVE_SIZE);
        o.extend_from_slice(b"RST\0");
        o.extend_from_slice(&((SAVE_SIZE - 8) as u32).to_le_bytes());
        let v3 = |o: &mut Vec<u8>, v: &V3| {
            for f in [v.x, v.y, v.z] {
                o.extend_from_slice(&f.to_le_bytes());
            }
        };
        for i in 0..NUM_RESTART_POINTS {
            v3(&mut o, &self.HospitalRestartPoints[i]);
            o.extend_from_slice(&self.HospitalRestartHeadings[i].to_le_bytes());
        }
        for i in 0..NUM_RESTART_POINTS {
            v3(&mut o, &self.PoliceRestartPoints[i]);
            o.extend_from_slice(&self.PoliceRestartHeadings[i].to_le_bytes());
        }
        o.extend_from_slice(&self.NumberOfHospitalRestarts.to_le_bytes());
        o.extend_from_slice(&self.NumberOfPoliceRestarts.to_le_bytes());
        o.push(self.bOverrideRestart as u8);
        o.extend_from_slice(&[0, 0, 0]);
        v3(&mut o, &self.OverridePosition);
        o.extend_from_slice(&self.OverrideHeading.to_le_bytes());
        o.push(self.bFadeInAfterNextDeath as u8);
        o.push(self.bFadeInAfterNextArrest as u8);
        o.push(self.OverrideHospitalLevel);
        o.push(self.OverridePoliceStationLevel);
        debug_assert_eq!(o.len(), SAVE_SIZE);
        o
    }

    /// LoadAllRestartPoints (Initialise, then the block; the header is skipped unchecked).
    pub fn load(&mut self, b: &[u8]) {
        self.initialise();
        let mut at = 8;
        let mut f = || {
            let v = f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
            at += 4;
            v
        };
        let mut pts = [(V3::default(), 0.0f32); 2 * NUM_RESTART_POINTS];
        for p in &mut pts {
            *p = (V3 { x: f(), y: f(), z: f() }, f());
        }
        for i in 0..NUM_RESTART_POINTS {
            (self.HospitalRestartPoints[i], self.HospitalRestartHeadings[i]) = pts[i];
            (self.PoliceRestartPoints[i], self.PoliceRestartHeadings[i]) = pts[NUM_RESTART_POINTS + i];
        }
        let mut at = 8 + 2 * NUM_RESTART_POINTS * 16;
        self.NumberOfHospitalRestarts = u16::from_le_bytes([b[at], b[at + 1]]);
        self.NumberOfPoliceRestarts = u16::from_le_bytes([b[at + 2], b[at + 3]]);
        self.bOverrideRestart = b[at + 4] != 0;
        at += 8;
        let g = |i: usize| f32::from_le_bytes(b[at + 4 * i..at + 4 * i + 4].try_into().unwrap());
        self.OverridePosition = V3 { x: g(0), y: g(1), z: g(2) };
        self.OverrideHeading = g(3);
        at += 16;
        self.bFadeInAfterNextDeath = b[at] != 0;
        self.bFadeInAfterNextArrest = b[at + 1] != 0;
        self.OverrideHospitalLevel = b[at + 2];
        self.OverridePoliceStationLevel = b[at + 3];
    }
}

// --- C ABI ---

fn restart() -> &'static mut Restart {
    // SAFETY: game thread only, like the C++ statics it replaces.
    unsafe { &mut *std::ptr::addr_of_mut!(revc_restart) }
}

unsafe extern "C" {
    fn revc_cpp_restart_level(pos: *const V3) -> i32;
    fn revc_cpp_restart_closest_node(pos: *const V3, out: *mut V3);
}

struct Cpp;
impl Hooks for Cpp {
    fn level(&mut self, pos: &V3) -> i32 {
        unsafe { revc_cpp_restart_level(pos) }
    }
    fn closest_node(&mut self, pos: &V3) -> V3 {
        let mut out = V3::default();
        unsafe { revc_cpp_restart_closest_node(pos, &mut out) };
        out
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_restart_initialise() {
    restart().initialise();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_restart_add(police: bool, pos: *const V3, heading: f32) {
    restart().add_point(police, unsafe { *pos }, heading);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_restart_override(pos: *const V3, heading: f32) {
    restart().override_next_restart(unsafe { *pos }, heading);
}

/// Returns true when it fell back to a path node.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_restart_find_closest(police: bool, pos: *const V3, out_pos: *mut V3, out_heading: *mut f32) -> bool {
    let (p, h, node) = restart().find_closest(police, unsafe { &*pos }, &mut Cpp);
    unsafe {
        *out_pos = p;
        *out_heading = h;
    }
    node
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_restart_save(buf: *mut u8) -> u32 {
    let o = restart().save();
    unsafe { std::ptr::copy_nonoverlapping(o.as_ptr(), buf, o.len()) };
    o.len() as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_restart_load(buf: *const u8, size: u32) {
    debug_assert_eq!(size as usize, SAVE_SIZE);
    restart().load(unsafe { std::slice::from_raw_parts(buf, SAVE_SIZE) });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_restart_size() -> u32 {
    std::mem::size_of::<Restart>() as u32
}

#[cfg(test)]
mod golden {
    //! tests/golden/restart.txt: random op sequences through the original CRestart, state after each.
    use super::*;

    /// The recorder's stubs.
    struct Stub;
    impl Hooks for Stub {
        fn level(&mut self, v: &V3) -> i32 {
            if v.x < -300.0 { 1 } else if v.x > 300.0 { 2 } else { 0 }
        }
        fn closest_node(&mut self, v: &V3) -> V3 {
            V3 { x: v.x as i32 as f32, y: v.y as i32 as f32, z: 7.0 }
        }
    }

    fn hx(v: f32) -> String {
        format!(" {:08x}", v.to_bits())
    }
    fn v3s(v: &V3) -> String {
        hx(v.x) + &hx(v.y) + &hx(v.z)
    }
    fn dump(r: &Restart) -> String {
        let mut s = format!(
            " {} {} {} {} {}",
            r.OverrideHospitalLevel, r.OverridePoliceStationLevel, r.bFadeInAfterNextArrest as u8, r.bFadeInAfterNextDeath as u8, r.bOverrideRestart as u8
        );
        s += &(v3s(&r.OverridePosition) + &hx(r.OverrideHeading));
        for i in 0..NUM_RESTART_POINTS {
            s += &(v3s(&r.HospitalRestartPoints[i]) + &hx(r.HospitalRestartHeadings[i]));
        }
        s += &format!(" {}", r.NumberOfHospitalRestarts);
        for i in 0..NUM_RESTART_POINTS {
            s += &(v3s(&r.PoliceRestartPoints[i]) + &hx(r.PoliceRestartHeadings[i]));
        }
        s + &format!(" {}", r.NumberOfPoliceRestarts)
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/restart.txt")).unwrap();
        let mut r = Restart::ZERO;
        let mut saved: Vec<u8> = Vec::new();
        let mut n = 0;
        for (ln, line) in text.lines().enumerate() {
            let (lhs, rhs) = line.split_once(" =>").unwrap();
            let (want_res, want_state) = rhs.split_once(" | ").unwrap();
            let t: Vec<&str> = lhs.split(' ').collect();
            let f = |k: usize| f32::from_bits(u32::from_str_radix(t[k], 16).unwrap());
            let v = |k: usize| V3 { x: f(k), y: f(k + 1), z: f(k + 2) };
            let mut res = String::new();
            match t[0] {
                "init" => r.initialise(),
                "0" | "1" if t[1] == "full" => {}
                "0" | "1" => r.add_point(t[1] == "1", v(2), f(5)),
                "2" => r.override_next_restart(v(1), f(4)),
                "3" => r.bOverrideRestart = false,
                "4" | "5" | "6" => {
                    let (p, h, node) = r.find_closest(t[1] == "1", &v(2), &mut Stub);
                    res = format!("{}{}{}", if node { " msg" } else { "" }, v3s(&p), hx(h));
                }
                "7" => {
                    r.OverrideHospitalLevel = t[1].parse().unwrap();
                    r.OverridePoliceStationLevel = t[2].parse().unwrap();
                    r.bFadeInAfterNextArrest = t[3] == "1";
                    r.bFadeInAfterNextDeath = t[4] == "1";
                }
                "8" => {
                    saved = r.save();
                    res = format!(" {} {}", saved.len(), saved.iter().map(|b| format!("{b:02x}")).collect::<String>());
                }
                "9" => {
                    if !saved.is_empty() {
                        r.load(&saved);
                    }
                }
                op => panic!("op {op}"),
            }
            assert_eq!(res, want_res, "result, line {}", ln + 1);
            assert_eq!(" ".to_string() + want_state, dump(&r), "state, line {}", ln + 1);
            n += 1;
        }
        assert!(n > 2000);
    }
}

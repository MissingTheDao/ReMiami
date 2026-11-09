//! The wanted level (C++ src/core/Wanted.cpp, CWanted): chaos points, wanted levels and their
//! police resources, the crime queue, the cop pursuit list's order. Works on the C++ object; what
//! it needs from the rest of the game (frenzy, the zone's wanted level drop, police nearby, crime
//! audio, resetting the cops) comes through `Hooks`. Identical to the original
//! (tests/golden/wanted.txt).

use crate::math::V3;

pub const CRIME_NONE: i32 = 0;
const NUM_CRIMES_QD: usize = 16;
const NUM_COPS: usize = 10;

/// CCrimeBeingQd.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrimeBeingQd {
    pub type_: i32,
    pub id: i32,
    pub time: u32,
    pub posn: V3,
    pub reported: bool,
    pub police_doesnt_care: bool,
}

/// CWanted (x86-64 layout).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Wanted {
    pub chaos: i32,
    pub min_chaos: i32,
    pub last_update_time: i32,
    pub last_wanted_level_change: u32,
    pub last_time_suspended: u32,
    pub crime_sensitivity: f32,
    pub current_cops: u8,
    pub max_cops: u8,
    pub maximum_law_enforcer_vehicles: u8,
    pub cops_beating_suspect: u8,
    pub roadblock_density: i16,
    /// Bit 0 ignored by cops, 1 ignored by everyone, 2 SWAT, 3 FBI, 4 army required.
    pub flags: u8,
    pub wanted_level: i32,
    pub min_wanted_level: i32,
    pub crimes: [CrimeBeingQd; NUM_CRIMES_QD],
    pub cops: [*mut u8; NUM_COPS],
}

const _: () = assert!(std::mem::size_of::<CrimeBeingQd>() == 28 && std::mem::size_of::<Wanted>() == 568);

const IGNORED_BY_COPS: u8 = 1;
const IGNORED_BY_EVERYONE: u8 = 2;
const SWAT: u8 = 4;
const FBI: u8 = 8;
const ARMY: u8 = 16;

/// CWanted's statics and the CStats counters it updates, by address.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Statics {
    pub maximum_wanted_level: *mut i32,
    pub n_maximum_wanted_level: *mut i32,
    pub wanted_stars_attained: *mut i32,
    pub wanted_stars_evaded: *mut i32,
}

unsafe impl Send for Statics {}

/// The rest of the game, as CWanted reads it.
pub trait Hooks {
    fn now(&mut self) -> u32;
    /// CDarkel::FrenzyOnGoing.
    fn frenzy(&mut self) -> bool;
    /// CCullZones::GetWantedLevelDrop.
    fn wanted_level_drop(&mut self) -> i32;
    /// DMAudio.ReportCrime.
    fn report_crime_audio(&mut self, type_: i32, coors: &V3);
    /// WorkOutPolicePresence(FindPlayerCoors(), radius).
    fn police_presence(&mut self, radius: f32) -> i32;
    /// ResetPolicePursuit (the cops are C++ peds).
    fn reset_police_pursuit(&mut self, w: &mut Wanted);
    /// printf of the original's pursuit list repairs.
    fn message(&mut self, text: &str);
}

impl Wanted {
    pub fn initialise(&mut self) {
        self.chaos = 0;
        self.min_chaos = 0;
        self.last_update_time = 0;
        self.last_wanted_level_change = 0;
        self.last_time_suspended = 0;
        self.current_cops = 0;
        self.max_cops = 0;
        self.maximum_law_enforcer_vehicles = 0;
        self.roadblock_density = 0;
        self.flags &= !(IGNORED_BY_COPS | IGNORED_BY_EVERYONE | SWAT | FBI | ARMY);
        self.crime_sensitivity = 1.0;
        self.wanted_level = 0;
        self.min_wanted_level = 0;
        self.cops_beating_suspect = 0;
        self.cops = [std::ptr::null_mut(); NUM_COPS];
        self.clear_qd_crimes();
    }

    pub fn are_miami_vice_required(&self) -> bool {
        self.wanted_level >= 3
    }
    pub fn are_swat_required(&self) -> bool {
        self.wanted_level == 4 || self.flags & SWAT != 0
    }
    pub fn are_fbi_required(&self) -> bool {
        self.wanted_level == 5 || self.flags & FBI != 0
    }
    pub fn are_army_required(&self) -> bool {
        self.wanted_level == 6 || self.flags & ARMY != 0
    }
    pub fn num_of_helis_required(&self) -> i32 {
        if self.flags & (IGNORED_BY_COPS | IGNORED_BY_EVERYONE) != 0 {
            return 0;
        }
        match self.wanted_level {
            3..=6 => 1,
            _ => 0,
        }
    }

    pub unsafe fn set_wanted_level(&mut self, mut level: i32, s: &Statics, now: u32) {
        unsafe {
            if level > *s.maximum_wanted_level {
                level = *s.maximum_wanted_level;
            }
            self.clear_qd_crimes();
            match level {
                0 => self.chaos = 0,
                1 => self.chaos = 70,
                2 => self.chaos = 200,
                3 => self.chaos = 570,
                4 => self.chaos = 1220,
                5 => self.chaos = 2420,
                6 => self.chaos = 4820,
                _ => {}
            }
            self.update_wanted_level(s, now);
        }
    }

    pub unsafe fn set_wanted_level_no_drop(&mut self, level: i32, s: &Statics, now: u32) {
        unsafe {
            if self.wanted_level < self.min_wanted_level {
                self.set_wanted_level(self.min_wanted_level, s, now);
            }
            if level > self.wanted_level {
                self.set_wanted_level(level, s, now);
            }
        }
    }

    pub unsafe fn cheat_wanted_level(&mut self, level: i32, s: &Statics, now: u32) {
        unsafe {
            self.set_wanted_level(level, s, now);
            self.update_wanted_level(s, now);
        }
    }

    pub fn clear_qd_crimes(&mut self) {
        for c in self.crimes.iter_mut() {
            c.type_ = CRIME_NONE;
        }
    }

    /// AddCrimeToQ: whether the crime had been reported already.
    pub fn add_crime_to_q(&mut self, type_: i32, id: i32, coors: &V3, reported: bool, police_doesnt_care: bool, now: u32) -> bool {
        for c in self.crimes.iter_mut() {
            if c.type_ == type_ && c.id == id {
                if c.reported {
                    return true;
                }
                if reported {
                    c.reported = reported;
                }
                return false;
            }
        }
        if let Some(c) = self.crimes.iter_mut().find(|c| c.type_ == CRIME_NONE) {
            c.type_ = type_;
            c.id = id;
            c.posn = *coors;
            c.time = now;
            c.reported = reported;
            c.police_doesnt_care = police_doesnt_care;
        }
        false
    }

    pub unsafe fn register_crime_immediately(&mut self, type_: i32, coors: &V3, id: u32, police_doesnt_care: bool, s: &Statics, h: &mut dyn Hooks) {
        unsafe {
            // FIX_SIGNIFICANT_BUGS isn't defined: queued as not reported.
            let now = h.now();
            if !self.add_crime_to_q(type_, id as i32, coors, false, police_doesnt_care, now) {
                self.report_crime_now(type_, coors, police_doesnt_care, s, h);
            }
        }
    }

    pub unsafe fn report_crime_now(&mut self, type_: i32, coors: &V3, police_doesnt_care: bool, s: &Statics, h: &mut dyn Hooks) {
        unsafe {
            let sensitivity = if h.frenzy() { self.crime_sensitivity * 0.3 } else { self.crime_sensitivity };
            let drop = h.wanted_level_drop().min(100);
            let mut chaos = (1.0 - drop as f32 / 100.0) * sensitivity;
            if police_doesnt_care {
                chaos *= 0.333;
            }
            let points: f32 = match type_ {
                2 => 5.0,    // hit ped
                3 => 45.0,   // hit cop
                4 => 30.0,   // shoot ped
                5 => 80.0,   // shoot cop
                6 => 15.0,   // steal car
                7 => 10.0,   // run red light
                8 => 5.0,    // reckless driving
                9 => 5.0,    // speeding
                10 => 18.0,  // run over ped
                11 => 80.0,  // run over cop
                12 => 400.0, // shoot heli
                13 => 20.0,  // ped burned
                14 => 80.0,  // cop burned
                15 => 20.0,  // vehicle burned
                16 => 500.0, // destroyed cessna
                17 => 25.0,  // explosion
                18 => 35.0,  // hit ped, nasty weapon
                19 => 100.0, // hit cop, nasty weapon
                _ => 0.0,    // gun possession; unknown types (an error message in the original)
            };
            if points != 0.0 {
                self.chaos = (self.chaos as f32 + points * chaos) as i32;
            }
            self.chaos = self.chaos.max(self.min_chaos);
            h.report_crime_audio(type_, coors);
            let now = h.now();
            self.update_wanted_level(s, now);
        }
    }

    pub unsafe fn update_wanted_level(&mut self, s: &Statics, now: u32) {
        unsafe {
            let before = self.wanted_level;
            if self.chaos > *s.n_maximum_wanted_level {
                self.chaos = *s.n_maximum_wanted_level;
            }
            let c = self.chaos;
            let set = |w: &mut Wanted, level: i32, vehicles: u8, cops: u8, roadblocks: i16| {
                *s.wanted_stars_attained = (*s.wanted_stars_attained).wrapping_add(level - w.wanted_level);
                w.wanted_level = level;
                w.maximum_law_enforcer_vehicles = vehicles;
                w.max_cops = cops;
                w.roadblock_density = roadblocks;
            };
            if (0..50).contains(&c) {
                if self.wanted_level == 1 {
                    *s.wanted_stars_evaded = (*s.wanted_stars_evaded).wrapping_add(1);
                }
                self.wanted_level = 0;
                self.maximum_law_enforcer_vehicles = 0;
                self.max_cops = 0;
                self.roadblock_density = 0;
            } else if (50..180).contains(&c) {
                set(self, 1, 1, 1, 0);
            } else if (180..550).contains(&c) {
                set(self, 2, 2, 3, 0);
            } else if (550..1200).contains(&c) {
                set(self, 3, 2, 4, 12);
            } else if (1200..2400).contains(&c) {
                set(self, 4, 2, 6, 18);
            } else if (2400..4800).contains(&c) {
                set(self, 5, 3, 8, 24);
            } else if c >= 4800 {
                set(self, 6, 3, 10, 30);
            }
            if before != self.wanted_level {
                self.last_wanted_level_change = now;
            }
        }
    }

    pub unsafe fn update(&mut self, s: &Statics, h: &mut dyn Hooks) {
        unsafe {
            let now = h.now();
            if now > self.last_time_suspended.wrapping_add(20000) {
                self.min_chaos = 0;
                self.min_wanted_level = 0;
            }
            if now.wrapping_sub(self.last_update_time as u32) > 1000 {
                if self.wanted_level > 1 {
                    self.last_update_time = now as i32;
                } else if h.police_presence(18.0) == 0 {
                    self.last_update_time = now as i32;
                    self.chaos = self.chaos.wrapping_sub(1).max(0);
                    self.update_wanted_level(s, now);
                }
                self.update_crimes_q(s, h);
                let (mut order_messed_up, mut count, mut found_empty) = (false, 0u8, false);
                for c in &self.cops {
                    if !c.is_null() {
                        count += 1;
                        if found_empty {
                            order_messed_up = true;
                        }
                    } else {
                        found_empty = true;
                    }
                }
                if count != self.current_cops {
                    h.message("CopPursuit total messed up: re-setting\n");
                    self.current_cops = count;
                }
                if order_messed_up {
                    h.message("CopPursuit pointer list messed up: re-sorting\n");
                    // Moves the first cop after the first gap into it, once.
                    let mut fixed = true;
                    for i in 0..NUM_COPS {
                        if self.cops[i].is_null() {
                            for j in i..NUM_COPS {
                                if !self.cops[j].is_null() {
                                    self.cops[i] = self.cops[j];
                                    self.cops[j] = std::ptr::null_mut();
                                    fixed = false;
                                    break;
                                }
                            }
                            if fixed {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pub unsafe fn update_crimes_q(&mut self, s: &Statics, h: &mut dyn Hooks) {
        unsafe {
            for i in 0..NUM_CRIMES_QD {
                let c = self.crimes[i];
                if c.type_ != CRIME_NONE {
                    if h.now() > c.time.wrapping_add(500) && !c.reported {
                        self.report_crime_now(c.type_, &c.posn, c.police_doesnt_care, s, h);
                        self.crimes[i].reported = true;
                    }
                    if h.now() > c.time.wrapping_add(10000) {
                        self.crimes[i].type_ = CRIME_NONE;
                    }
                }
            }
        }
    }

    pub unsafe fn reset(&mut self, h: &mut dyn Hooks) {
        h.reset_police_pursuit(self);
        self.initialise();
    }

    pub unsafe fn suspend(&mut self, s: &Statics, h: &mut dyn Hooks) {
        unsafe {
            *s.wanted_stars_evaded = (*s.wanted_stars_evaded).wrapping_add(self.wanted_level);
            self.min_chaos = self.chaos;
            self.min_wanted_level = self.wanted_level;
            self.last_time_suspended = h.now();
            self.chaos = 0;
            self.wanted_level = 0;
            h.reset_police_pursuit(self);
        }
    }
}

/// SetMaximumWantedLevel (statics).
pub unsafe fn set_maximum_wanted_level(level: i32, s: &Statics) {
    let (n, max) = match level {
        0 => (0, 0),
        1 => (115, 1),
        2 => (365, 2),
        3 => (875, 3),
        4 => (1800, 4),
        5 => (3600, 5),
        6 => (7200, 6),
        _ => return,
    };
    unsafe {
        *s.n_maximum_wanted_level = n;
        *s.maximum_wanted_level = max;
    }
}

// ---- C ABI ----

static STATICS: std::sync::Mutex<Option<Statics>> = std::sync::Mutex::new(None);

fn statics() -> Statics {
    STATICS.lock().unwrap().expect("revc_wanted_bind not called")
}

unsafe extern "C" {
    fn revc_cpp_wanted_now() -> u32;
    fn revc_cpp_wanted_frenzy() -> bool;
    fn revc_cpp_wanted_level_drop() -> i32;
    fn revc_cpp_wanted_report_audio(type_: i32, coors: *const V3);
    fn revc_cpp_wanted_police_presence(radius: f32) -> i32;
    fn revc_cpp_wanted_reset_pursuit(w: *mut Wanted);
}

struct Cpp;
impl Hooks for Cpp {
    fn now(&mut self) -> u32 {
        unsafe { revc_cpp_wanted_now() }
    }
    fn frenzy(&mut self) -> bool {
        unsafe { revc_cpp_wanted_frenzy() }
    }
    fn wanted_level_drop(&mut self) -> i32 {
        unsafe { revc_cpp_wanted_level_drop() }
    }
    fn report_crime_audio(&mut self, type_: i32, coors: &V3) {
        unsafe { revc_cpp_wanted_report_audio(type_, coors) }
    }
    fn police_presence(&mut self, radius: f32) -> i32 {
        unsafe { revc_cpp_wanted_police_presence(radius) }
    }
    fn reset_police_pursuit(&mut self, w: &mut Wanted) {
        unsafe { revc_cpp_wanted_reset_pursuit(w) }
    }
    fn message(&mut self, text: &str) {
        print!("{text}");
    }
}

#[allow(clippy::missing_safety_doc)]
mod ffi {
    use super::*;
    fn now() -> u32 {
        unsafe { revc_cpp_wanted_now() }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_bind(s: *const Statics) {
        *STATICS.lock().unwrap() = Some(unsafe { *s });
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_initialise(w: *mut Wanted) {
        unsafe { (*w).initialise() }
    }
    /// what: 0 Miami Vice, 1 SWAT, 2 FBI, 3 army required; 4 helis required.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_query(w: *const Wanted, what: i32) -> i32 {
        unsafe {
            let w = &*w;
            match what {
                0 => w.are_miami_vice_required() as i32,
                1 => w.are_swat_required() as i32,
                2 => w.are_fbi_required() as i32,
                3 => w.are_army_required() as i32,
                _ => w.num_of_helis_required(),
            }
        }
    }
    /// how: 0 SetWantedLevel, 1 ...NoDrop, 2 CheatWantedLevel.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_set_level(w: *mut Wanted, level: i32, how: i32) {
        unsafe {
            let s = statics();
            match how {
                0 => (*w).set_wanted_level(level, &s, now()),
                1 => (*w).set_wanted_level_no_drop(level, &s, now()),
                _ => (*w).cheat_wanted_level(level, &s, now()),
            }
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_set_maximum(level: i32) {
        unsafe { set_maximum_wanted_level(level, &statics()) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_clear_qd(w: *mut Wanted) {
        unsafe { (*w).clear_qd_crimes() }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_add_crime(w: *mut Wanted, type_: i32, id: i32, coors: *const V3, reported: bool, pdc: bool) -> bool {
        unsafe { (*w).add_crime_to_q(type_, id, &*coors, reported, pdc, now()) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_register_immediately(w: *mut Wanted, type_: i32, coors: *const V3, id: u32, pdc: bool) {
        unsafe { (*w).register_crime_immediately(type_, &*coors, id, pdc, &statics(), &mut Cpp) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_report_now(w: *mut Wanted, type_: i32, coors: *const V3, pdc: bool) {
        unsafe { (*w).report_crime_now(type_, &*coors, pdc, &statics(), &mut Cpp) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_update_level(w: *mut Wanted) {
        unsafe { (*w).update_wanted_level(&statics(), now()) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_update(w: *mut Wanted) {
        unsafe { (*w).update(&statics(), &mut Cpp) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_update_crimes_q(w: *mut Wanted) {
        unsafe { (*w).update_crimes_q(&statics(), &mut Cpp) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_reset(w: *mut Wanted) {
        unsafe { (*w).reset(&mut Cpp) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_wanted_suspend(w: *mut Wanted) {
        unsafe { (*w).suspend(&statics(), &mut Cpp) }
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/wanted.txt: random op sequences through the original CWanted (its two ped
    //! functions stubbed the same way as these hooks), the state after each.
    use super::*;

    struct Log {
        out: String,
        now: u32,
        frenzy: bool,
        drop: i32,
        presence: i32,
    }
    impl Hooks for Log {
        fn now(&mut self) -> u32 {
            self.now
        }
        fn frenzy(&mut self) -> bool {
            self.frenzy
        }
        fn wanted_level_drop(&mut self) -> i32 {
            self.drop
        }
        fn report_crime_audio(&mut self, type_: i32, coors: &V3) {
            self.out += &format!(" audio {type_} {:08x}", coors.x.to_bits());
        }
        fn police_presence(&mut self, radius: f32) -> i32 {
            self.out += &format!(" presence {:08x}", radius.to_bits());
            self.presence
        }
        fn reset_police_pursuit(&mut self, w: &mut Wanted) {
            self.out += " resetpursuit";
            w.cops = [std::ptr::null_mut(); NUM_COPS];
            w.current_cops = 0;
        }
        fn message(&mut self, _text: &str) {
            self.out += " msg";
        }
    }

    fn state(w: &Wanted, s: &Statics) -> String {
        let b = |f: u8| (w.flags & f != 0) as i32;
        let mut o = format!(
            " | {} {} {} {} {} {:08x} {} {} {} {} {} {}{}{}{}{} {} {}",
            w.chaos,
            w.min_chaos,
            w.last_update_time,
            w.last_wanted_level_change,
            w.last_time_suspended,
            w.crime_sensitivity.to_bits(),
            w.current_cops,
            w.max_cops,
            w.maximum_law_enforcer_vehicles,
            w.cops_beating_suspect,
            w.roadblock_density,
            b(IGNORED_BY_COPS),
            b(IGNORED_BY_EVERYONE),
            b(SWAT),
            b(FBI),
            b(ARMY),
            w.wanted_level,
            w.min_wanted_level
        );
        for c in &w.crimes {
            if c.type_ == CRIME_NONE {
                o += " -";
            } else {
                o += &format!(
                    " {}:{}:{}:{:08x}:{:08x}:{:08x}:{}{}",
                    c.type_,
                    c.id,
                    c.time,
                    c.posn.x.to_bits(),
                    c.posn.y.to_bits(),
                    c.posn.z.to_bits(),
                    c.reported as i32,
                    c.police_doesnt_care as i32
                );
            }
        }
        for p in &w.cops {
            o += &format!(" {}", *p as usize);
        }
        unsafe { o + &format!(" {} {} {} {}", *s.maximum_wanted_level, *s.n_maximum_wanted_level, *s.wanted_stars_attained, *s.wanted_stars_evaded) }
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/wanted.txt")).unwrap();
        let mut stat = Box::new([6i32, 9600, 0, 0]);
        let p = stat.as_mut_ptr();
        let s = unsafe { Statics { maximum_wanted_level: p, n_maximum_wanted_level: p.add(1), wanted_stars_attained: p.add(2), wanted_stars_evaded: p.add(3) } };
        let mut w: Box<Wanted> = Box::new(unsafe { std::mem::zeroed() });
        let mut n = 0;
        for (ln, line) in text.lines().enumerate() {
            let (lhs, want) = line.split_once(" =>").unwrap();
            let a: Vec<&str> = lhs.split(' ').collect();
            let mut h = Log { out: String::new(), now: 0, frenzy: false, drop: 0, presence: 0 };
            if a[0] == "init" {
                h.now = a[1].parse().unwrap();
                w.initialise();
                assert_eq!(state(&w, &s), want, "line {}", ln + 1);
                n += 1;
                continue;
            }
            let num = |i: usize| a[i].parse::<i64>().unwrap();
            let hf = |i: usize| f32::from_bits(u32::from_str_radix(a[i], 16).unwrap());
            h.now = num(1) as u32;
            h.frenzy = num(2) != 0;
            h.drop = num(3) as i32;
            h.presence = num(4) as i32;
            let (type_, id) = (num(5) as i32, num(6) as i32);
            let c = V3::new(hf(7), hf(8), hf(9));
            let (op, pdc) = (num(10), num(11) != 0);
            let r: Vec<&str> = want.split(' ').collect(); // r[0] is empty
            let now = h.now;
            unsafe {
                match op {
                    0..=3 => {
                        let l: i32 = r[2].parse().unwrap();
                        h.out += &format!(" {} {l}", r[1]);
                        match op {
                            0 => w.set_wanted_level(l, &s, now),
                            1 => w.set_wanted_level_no_drop(l, &s, now),
                            2 => w.cheat_wanted_level(l, &s, now),
                            _ => set_maximum_wanted_level(l, &s),
                        }
                    }
                    4 | 5 => {
                        w.add_crime_to_q(type_, id, &c, false, pdc, now);
                    }
                    6 => w.register_crime_immediately(type_, &c, id as u32, pdc, &s, &mut h),
                    7 => {
                        let rep = r[2] == "1";
                        h.out += &format!(" add {}", rep as i32);
                        let res = w.add_crime_to_q(type_, id, &c, rep, pdc, now);
                        h.out += &format!(" r{}", res as i32);
                    }
                    8 => w.report_crime_now(type_, &c, pdc, &s, &mut h),
                    9..=11 => w.update(&s, &mut h),
                    12 => w.update_crimes_q(&s, &mut h),
                    13 => {
                        if r[1] == "suspend" {
                            h.out += " suspend";
                            w.suspend(&s, &mut h);
                        } else {
                            h.out += " clear";
                            w.clear_qd_crimes();
                        }
                    }
                    14 => {
                        let i: usize = r[2].parse().unwrap();
                        w.cops[i] = r[3].parse::<usize>().unwrap() as *mut u8;
                        w.current_cops = r[4].parse().unwrap();
                        let f = r[5].as_bytes();
                        w.flags = (w.flags & !(IGNORED_BY_COPS | SWAT | FBI | ARMY))
                            | if f[0] == b'1' { IGNORED_BY_COPS } else { 0 }
                            | if f[1] == b'1' { SWAT } else { 0 }
                            | if f[2] == b'1' { FBI } else { 0 }
                            | if f[3] == b'1' { ARMY } else { 0 };
                        w.crime_sensitivity = f32::from_bits(u32::from_str_radix(r[6], 16).unwrap());
                        h.out += &format!(" edit {} {} {} {} {}", r[2], r[3], r[4], r[5], r[6]);
                    }
                    _ => {
                        if r[1] == "reset" {
                            h.out += " reset";
                            w.reset(&mut h);
                        } else {
                            h.out += " updlevel";
                            w.update_wanted_level(&s, now);
                        }
                    }
                }
            }
            let got = format!(
                "{} q {} {} {} {} {}{}",
                h.out,
                w.are_miami_vice_required() as i32,
                w.are_swat_required() as i32,
                w.are_fbi_required() as i32,
                w.are_army_required() as i32,
                w.num_of_helis_required(),
                state(&w, &s)
            );
            assert_eq!(got, want, "line {}", ln + 1);
            n += 1;
        }
        assert!(n > 3000);
    }
}

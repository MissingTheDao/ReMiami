//! Map zones (C++ src/core/Zones.cpp, CTheZones): zone creation, the navigation zone hierarchy,
//! audio zones, point lookups, zone info (car/ped densities by day and night) and its setters.
//! The arrays stay C++ statics (other code holds CZone pointers); C++ binds a view of them once and
//! the logic here works on that memory. Save/load stay C++. Identical to the original
//! (tests/golden/zones.txt).

use crate::math::V3;

pub const NUMAUDIOZONES: usize = 14;
pub const NUMINFOZONES: usize = 169;
pub const NUMMAPZONES: usize = 39;
pub const NUMNAVIGZONES: usize = 20;
pub const NUM_CAR_CLASSES: usize = 9;
pub const NUM_BOAT_CLASSES: usize = 2;
pub const NUM_GANGS: usize = 9;

pub const ZONE_DEFAULT: i32 = 0;
pub const ZONE_NAVIG: i32 = 1;
pub const ZONE_INFO: i32 = 2;
pub const ZONE_MAPZONE: i32 = 3;
pub const LEVEL_GENERIC: i32 = 0;

/// CZone.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub name: [u8; 8],
    pub minx: f32,
    pub miny: f32,
    pub minz: f32,
    pub maxx: f32,
    pub maxy: f32,
    pub maxz: f32,
    pub type_: i32,
    pub level: i32,
    pub zoneinfo_day: i16,
    pub zoneinfo_night: i16,
    pub child: *mut Zone,
    pub parent: *mut Zone,
    pub next: *mut Zone,
}

/// CZoneInfo.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZoneInfo {
    pub car_density: i16,
    pub car_threshold: [i16; NUM_CAR_CLASSES],
    pub boat_threshold: [i16; NUM_BOAT_CLASSES],
    pub gang_threshold: [i16; NUM_GANGS],
    pub cop_threshold: i16,
    pub ped_density: u16,
    pub gang_ped_threshold: [u16; NUM_GANGS],
    pub cop_ped_threshold: u16,
    pub ped_group: u16,
}

const _: () = assert!(std::mem::size_of::<Zone>() == 72 && std::mem::size_of::<ZoneInfo>() == 68);

/// CTheZones' statics, by address.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub nav: *mut Zone,
    pub info: *mut Zone,
    pub map: *mut Zone,
    pub zone_infos: *mut ZoneInfo,
    pub audio: *mut i16,
    pub num_audio: *mut u16,
    pub num_map: *mut u16,
    pub num_info: *mut u16,
    pub num_nav: *mut u16,
    pub num_zone_infos: *mut u16,
    pub find_index: *mut i16,
    pub curr_level: *mut i32,
}

unsafe impl Send for View {}

fn zeroed_zone() -> Zone {
    unsafe { std::mem::zeroed() }
}

/// strcmp(a, b) == 0 on a zone name (NUL-terminated within its 8 bytes, or running on).
fn name_is(zone_name: &[u8; 8], name: &[u8]) -> bool {
    let n = zone_name.iter().position(|&c| c == 0).unwrap_or(8);
    &zone_name[..n] == name
}

pub fn point_lies_within_zone(v: &V3, z: &Zone) -> bool {
    z.minx <= v.x && v.x <= z.maxx && z.miny <= v.y && v.y <= z.maxy && z.minz <= v.z && v.z <= z.maxz
}

/// (The original also tests the corners for a debug message.)
pub fn zone_is_entirely_contained_within_other_zone(inner: &Zone, outer: &Zone) -> bool {
    !(inner.minx < outer.minx || inner.maxx > outer.maxx || inner.miny < outer.miny || inner.maxy > outer.maxy || inner.minz < outer.minz || inner.maxz > outer.maxz)
}

/// CClock::GetIsTimeInRange at `hours`.
fn time_in_range(hours: u8, h1: u8, h2: u8) -> bool {
    if h1 > h2 { hours >= h1 || hours < h2 } else { hours >= h1 && hours < h2 }
}

impl View {
    unsafe fn nav(&self, i: usize) -> *mut Zone {
        unsafe { self.nav.add(i) }
    }

    /// CTheZones::Init.
    pub unsafe fn init(&self) {
        unsafe {
            for i in 0..NUMAUDIOZONES {
                *self.audio.add(i) = -1;
            }
            *self.num_audio = 0;
            for i in 0..NUMNAVIGZONES {
                *self.nav.add(i) = zeroed_zone();
            }
            for i in 0..NUMINFOZONES {
                *self.info.add(i) = zeroed_zone();
            }
            let x: i16 = 1000 / 9;
            for i in 0..2 * NUMINFOZONES {
                let zi = &mut *self.zone_infos.add(i);
                zi.car_density = 10;
                zi.car_threshold[0] = x;
                for k in 1..8 {
                    zi.car_threshold[k] = zi.car_threshold[k - 1] + x;
                }
                zi.car_threshold[8] = 1000;
                zi.boat_threshold = [500, 1000];
                zi.cop_threshold = 50;
                zi.gang_threshold = [zi.cop_threshold; NUM_GANGS];
                zi.ped_density = 12;
                zi.cop_ped_threshold = 50;
                zi.gang_ped_threshold = [zi.cop_ped_threshold; NUM_GANGS];
                zi.ped_group = 0;
            }
            *self.num_zone_infos = 1;
            *self.num_nav = 1;
            *self.num_info = 1;
            let whole = |z: &mut Zone, name: &[u8], ty: i32| {
                z.name = [0; 8];
                z.name[..name.len()].copy_from_slice(name);
                (z.minx, z.miny, z.minz, z.maxx, z.maxy, z.maxz) = (-2400.0, -2000.0, -500.0, 1600.0, 2000.0, 500.0);
                z.level = LEVEL_GENERIC;
                z.type_ = ty;
            };
            whole(&mut *self.info, b"CITYINF", ZONE_INFO);
            whole(&mut *self.nav, b"VICE_C", ZONE_DEFAULT);
            *self.curr_level = LEVEL_GENERIC;
            for i in 0..NUMMAPZONES {
                let z = &mut *self.map.add(i);
                *z = zeroed_zone();
                z.type_ = ZONE_MAPZONE;
            }
            *self.num_map = 1;
            whole(&mut *self.map, b"THEMAP", ZONE_MAPZONE);
        }
    }

    /// CTheZones::CreateZone. `name` is upper-cased in place, as the original does to its
    /// argument. A full array drops the zone (the original asserts, then writes past it).
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn create_zone(&self, name: &mut [u8], ty: i32, mut minx: f32, mut miny: f32, mut minz: f32, mut maxx: f32, mut maxy: f32, mut maxz: f32, level: i32) {
        if minx > maxx {
            std::mem::swap(&mut minx, &mut maxx);
        }
        if miny > maxy {
            std::mem::swap(&mut miny, &mut maxy);
        }
        if minz > maxz {
            std::mem::swap(&mut minz, &mut maxz);
        }
        for c in name.iter_mut() {
            *c = c.to_ascii_uppercase();
        }
        let mut tmp = [0u8; 8];
        let n = name.len().min(7);
        tmp[..n].copy_from_slice(&name[..n]);
        unsafe {
            let (arr, count, cap) = match ty {
                ZONE_DEFAULT | ZONE_NAVIG => (self.nav, self.num_nav, NUMNAVIGZONES),
                ZONE_INFO => (self.info, self.num_info, NUMINFOZONES),
                ZONE_MAPZONE => (self.map, self.num_map, NUMMAPZONES),
                _ => return,
            };
            if *count as usize >= cap {
                return;
            }
            let z = &mut *arr.add(*count as usize);
            // strcpy: the name and its NUL; bytes after it stay.
            z.name[..=n].copy_from_slice(&tmp[..=n]);
            z.type_ = ty;
            (z.minx, z.miny, z.minz, z.maxx, z.maxy, z.maxz) = (minx, miny, minz, maxx, maxy, maxz);
            z.level = level;
            if ty == ZONE_INFO {
                z.zoneinfo_day = *self.num_zone_infos as i16;
                *self.num_zone_infos = (*self.num_zone_infos).wrapping_add(1);
                z.zoneinfo_night = *self.num_zone_infos as i16;
                *self.num_zone_infos = (*self.num_zone_infos).wrapping_add(1);
            }
            *count += 1;
        }
    }

    /// CTheZones::PostZoneCreation: the hierarchy, then the audio zones.
    pub unsafe fn post_zone_creation(&self) {
        unsafe {
            for i in 1..*self.num_nav as usize {
                self.insert_zone_into_zone_hierarchy(self.nav(i));
            }
            self.initialise_audio_zone_array();
        }
    }

    unsafe fn insert_zone_into_zone_hierarchy(&self, zone: *mut Zone) {
        unsafe {
            (*zone).child = std::ptr::null_mut();
            (*zone).parent = std::ptr::null_mut();
            (*zone).next = std::ptr::null_mut();
            insert_recursive(zone, self.nav);
        }
    }

    unsafe fn add_zone_to_audio_zone_array(&self, zone: *mut Zone) {
        unsafe {
            if (*zone).type_ != ZONE_DEFAULT {
                return;
            }
            let mut z: i16 = -1;
            for i in 0..NUMNAVIGZONES {
                if self.nav(i) == zone {
                    z = i as i16;
                }
            }
            if (*self.num_audio as usize) < NUMAUDIOZONES {
                *self.audio.add(*self.num_audio as usize) = z;
                *self.num_audio += 1;
            }
        }
    }

    /// Depth first over the hierarchy, a zone after its children.
    unsafe fn initialise_audio_zone_array(&self) {
        unsafe {
            let mut gonext = false;
            let mut zone = self.nav;
            while !zone.is_null() {
                if gonext {
                    self.add_zone_to_audio_zone_array(zone);
                    if !(*zone).next.is_null() {
                        gonext = false;
                        zone = (*zone).next;
                    } else {
                        zone = (*zone).parent;
                    }
                } else if !(*zone).child.is_null() {
                    zone = (*zone).child;
                } else {
                    self.add_zone_to_audio_zone_array(zone);
                    if !(*zone).next.is_null() {
                        zone = (*zone).next;
                    } else {
                        gonext = true;
                        zone = (*zone).parent;
                    }
                }
            }
        }
    }

    pub unsafe fn get_level_from_position(&self, v: &V3) -> i32 {
        unsafe {
            for i in 1..*self.num_map as usize {
                if point_lies_within_zone(v, &*self.map.add(i)) {
                    return (*self.map.add(i)).level;
                }
            }
            (*self.map).level
        }
    }

    pub unsafe fn find_information_zone_for_position(&self, v: &V3) -> *mut Zone {
        unsafe {
            for i in 1..*self.num_info as usize {
                if point_lies_within_zone(v, &*self.info.add(i)) {
                    return self.info.add(i);
                }
            }
            self.info
        }
    }

    pub unsafe fn find_smallest_navigation_zone_for_position(&self, v: &V3, find_default: bool, find_navig: bool) -> *mut Zone {
        unsafe {
            let wanted = |z: &Zone| (find_default && z.type_ == ZONE_DEFAULT) || (find_navig && z.type_ == ZONE_NAVIG);
            let mut best = std::ptr::null_mut();
            if wanted(&*self.nav) {
                best = self.nav;
            }
            let mut zone = (*self.nav).child;
            while !zone.is_null() {
                if point_lies_within_zone(v, &*zone) {
                    if wanted(&*zone) {
                        best = zone;
                    }
                    zone = (*zone).child;
                } else {
                    zone = (*zone).next;
                }
            }
            best
        }
    }

    unsafe fn search_from(&self, name: &[u8], ty: i32) -> i16 {
        unsafe {
            let (arr, count) = match ty {
                ZONE_DEFAULT | ZONE_NAVIG => (self.nav, *self.num_nav),
                ZONE_INFO => (self.info, *self.num_info),
                ZONE_MAPZONE => (self.map, *self.num_map),
                _ => return -1,
            };
            while (*self.find_index as i32) < count as i32 {
                if name_is(&(*arr.add(*self.find_index as usize)).name, name) {
                    return *self.find_index;
                }
                *self.find_index += 1;
            }
            -1
        }
    }

    /// FindZoneByLabelAndReturnIndex (the search position is kept for FindNext...).
    pub unsafe fn find_zone_by_label(&self, name: &[u8], ty: i32) -> i16 {
        unsafe {
            if matches!(ty, ZONE_DEFAULT | ZONE_NAVIG | ZONE_INFO | ZONE_MAPZONE) {
                *self.find_index = 0;
            }
            self.search_from(name, ty)
        }
    }

    pub unsafe fn find_next_zone_by_label(&self, name: &[u8], ty: i32) -> i16 {
        unsafe {
            *self.find_index = (*self.find_index).wrapping_add(1);
            self.search_from(name, ty)
        }
    }

    pub unsafe fn get_zone_info(&self, v: &V3, day: bool) -> *mut ZoneInfo {
        unsafe {
            let zone = self.find_information_zone_for_position(v);
            let i = if day { (*zone).zoneinfo_day } else { (*zone).zoneinfo_night };
            self.zone_infos.offset(i as isize)
        }
    }

    /// GetZoneInfoForTimeOfDay at clock hour `hours` (FIX_BUGS blend between 19-22 and 5-8).
    pub unsafe fn get_zone_info_for_time_of_day(&self, pos: &V3, hours: u8, info: &mut ZoneInfo) {
        unsafe {
            let day = *self.get_zone_info(pos, true);
            let night = *self.get_zone_info(pos, false);
            if time_in_range(hours, 8, 19) {
                *info = day;
            } else if time_in_range(hours, 22, 5) {
                *info = night;
            } else {
                let (d, n);
                if time_in_range(hours, 19, 22) {
                    n = (hours as i32 - 19) as f32 / 3.0;
                    d = 1.0 - n;
                } else {
                    d = (hours as i32 - 5) as f32 / 3.0;
                    n = 1.0 - d;
                }
                let bi = |a: i16, b: i16| (a as f32 * d + b as f32 * n) as i32 as i16;
                let bu = |a: u16, b: u16| (a as f32 * d + b as f32 * n) as i32 as u16;
                info.car_density = bi(day.car_density, night.car_density);
                for i in 0..NUM_CAR_CLASSES {
                    info.car_threshold[i] = bi(day.car_threshold[i], night.car_threshold[i]);
                }
                for i in 0..NUM_BOAT_CLASSES {
                    info.boat_threshold[i] = bi(day.boat_threshold[i], night.boat_threshold[i]);
                }
                for i in 0..NUM_GANGS {
                    info.gang_threshold[i] = bi(day.gang_threshold[i], night.gang_threshold[i]);
                }
                info.cop_threshold = bi(day.cop_threshold, night.cop_threshold);
                info.ped_density = bu(day.ped_density, night.ped_density);
                info.cop_ped_threshold = bu(day.cop_ped_threshold, night.cop_ped_threshold);
                for i in 0..NUM_GANGS {
                    info.gang_ped_threshold[i] = bu(day.gang_ped_threshold[i], night.gang_ped_threshold[i]);
                }
            }
            info.ped_group = if time_in_range(hours, 5, 19) { day.ped_group } else { night.ped_group };
        }
    }

    unsafe fn info_of(&self, zoneid: u16, day: bool) -> &mut ZoneInfo {
        unsafe {
            let z = &*self.info.add(zoneid as usize);
            &mut *self.zone_infos.offset(if day { z.zoneinfo_day } else { z.zoneinfo_night } as isize)
        }
    }

    pub unsafe fn set_zone_car_info(&self, zoneid: u16, day: bool, car_density: i16, cop: i16, gangs: &[i16; NUM_GANGS]) {
        unsafe {
            let info = self.info_of(zoneid, day);
            info.car_density = car_density;
            info.cop_threshold = cop;
            info.gang_threshold[0] = gangs[0].wrapping_add(cop);
            for i in 1..NUM_GANGS {
                info.gang_threshold[i] = gangs[i].wrapping_add(info.gang_threshold[i - 1]);
            }
        }
    }

    pub unsafe fn set_zone_civilian_car_info(&self, zoneid: u16, day: bool, cars: &[i16; NUM_CAR_CLASSES], boats: &[i16; NUM_BOAT_CLASSES]) {
        unsafe {
            let info = self.info_of(zoneid, day);
            info.car_threshold[0] = cars[0];
            for i in 1..NUM_CAR_CLASSES {
                info.car_threshold[i] = cars[i].wrapping_add(info.car_threshold[i - 1]);
            }
            info.boat_threshold[0] = boats[0];
            for i in 1..NUM_BOAT_CLASSES {
                info.boat_threshold[i] = boats[i].wrapping_add(info.boat_threshold[i - 1]);
            }
        }
    }

    /// SetZonePedInfo: the int16 arguments land in uint16 fields.
    pub unsafe fn set_zone_ped_info(&self, zoneid: u16, day: bool, ped_density: i16, gangs: &[i16; NUM_GANGS], cop: i16) {
        unsafe {
            let info = self.info_of(zoneid, day);
            info.ped_density = ped_density as u16;
            info.cop_ped_threshold = cop as u16;
            for i in 0..NUM_GANGS {
                info.gang_ped_threshold[i] = gangs[i] as u16;
            }
            info.gang_ped_threshold[0] = info.gang_ped_threshold[0].wrapping_add(info.cop_ped_threshold);
            for i in 1..NUM_GANGS {
                info.gang_ped_threshold[i] = info.gang_ped_threshold[i].wrapping_add(info.gang_ped_threshold[i - 1]);
            }
        }
    }

    pub unsafe fn set_car_density(&self, zoneid: u16, day: bool, v: u16) {
        unsafe { self.info_of(zoneid, day).car_density = v as i16 }
    }
    pub unsafe fn set_ped_density(&self, zoneid: u16, day: bool, v: u16) {
        unsafe { self.info_of(zoneid, day).ped_density = v }
    }
    pub unsafe fn set_ped_group(&self, zoneid: u16, day: bool, v: u16) {
        unsafe { self.info_of(zoneid, day).ped_group = v }
    }

    pub unsafe fn find_audio_zone(&self, pos: &V3) -> i16 {
        unsafe {
            for i in 0..*self.num_audio as usize {
                let z = *self.audio.add(i);
                if point_lies_within_zone(pos, &*self.nav.offset(z as isize)) {
                    return i as i16;
                }
            }
            -1
        }
    }
}

/// InsertZoneIntoZoneHierRecursive: false if `inner` doesn't fit in `outer`.
unsafe fn insert_recursive(inner: *mut Zone, outer: *mut Zone) -> bool {
    unsafe {
        if outer.is_null() || !zone_is_entirely_contained_within_other_zone(&*inner, &*outer) {
            return false;
        }
        let mut child = (*outer).child;
        while !child.is_null() {
            if insert_recursive(inner, child) {
                return true;
            }
            child = (*child).next;
        }
        // Insert as a child of outer; move outer's children that fit into inner under inner.
        let mut n = 0;
        let mut child = (*outer).child;
        while !child.is_null() {
            if zone_is_entirely_contained_within_other_zone(&*child, &*inner) {
                n += 1;
            }
            child = (*child).next;
        }
        (*inner).next = (*outer).child;
        (*inner).parent = outer;
        (*outer).child = inner;
        if n > 0 {
            let mut insert = inner;
            let mut child = (*inner).next;
            while !child.is_null() {
                let next = (*child).next;
                if zone_is_entirely_contained_within_other_zone(&*child, &*inner) {
                    (*insert).next = (*child).next;
                    (*child).parent = inner;
                    (*child).next = (*inner).child;
                    (*inner).child = child;
                } else {
                    insert = child;
                }
                child = next;
            }
        }
        true
    }
}

// ---- C ABI: CTheZones forwards here with its bound view ----

static VIEW: std::sync::Mutex<Option<View>> = std::sync::Mutex::new(None);

fn view() -> View {
    VIEW.lock().unwrap().expect("revc_zones_bind not called")
}

#[allow(clippy::missing_safety_doc)]
mod ffi {
    use super::*;
    use std::ffi::c_char;

    fn cstr<'a>(p: *const c_char) -> &'a [u8] {
        unsafe { std::ffi::CStr::from_ptr(p).to_bytes() }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_bind(v: *const View) {
        *VIEW.lock().unwrap() = Some(unsafe { *v });
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_init() {
        unsafe { view().init() }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_create(name: *mut c_char, ty: i32, minx: f32, miny: f32, minz: f32, maxx: f32, maxy: f32, maxz: f32, level: i32) {
        unsafe {
            let len = std::ffi::CStr::from_ptr(name).to_bytes().len();
            let name = std::slice::from_raw_parts_mut(name as *mut u8, len);
            view().create_zone(name, ty, minx, miny, minz, maxx, maxy, maxz, level)
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_post_creation() {
        unsafe { view().post_zone_creation() }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_contained(inner: *const Zone, outer: *const Zone) -> bool {
        unsafe { zone_is_entirely_contained_within_other_zone(&*inner, &*outer) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_point_within(v: *const V3, z: *const Zone) -> bool {
        unsafe { point_lies_within_zone(&*v, &*z) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_level(v: *const V3) -> i32 {
        unsafe { view().get_level_from_position(&*v) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_info_zone(v: *const V3) -> *mut Zone {
        unsafe { view().find_information_zone_for_position(&*v) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_nav_zone(v: *const V3, d: bool, n: bool) -> *mut Zone {
        unsafe { view().find_smallest_navigation_zone_for_position(&*v, d, n) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_find_label(name: *const c_char, ty: i32) -> i16 {
        unsafe { view().find_zone_by_label(cstr(name), ty) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_find_next_label(name: *const c_char, ty: i32) -> i16 {
        unsafe { view().find_next_zone_by_label(cstr(name), ty) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_zone_info(v: *const V3, day: u8) -> *mut ZoneInfo {
        unsafe { view().get_zone_info(&*v, day != 0) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_zone_info_tod(v: *const V3, hours: u8, info: *mut ZoneInfo) {
        unsafe { view().get_zone_info_for_time_of_day(&*v, hours, &mut *info) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_set_car_info(zoneid: u16, day: u8, density: i16, cop: i16, gangs: *const [i16; NUM_GANGS]) {
        unsafe { view().set_zone_car_info(zoneid, day != 0, density, cop, &*gangs) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_set_civ_car_info(zoneid: u16, day: u8, cars: *const [i16; NUM_CAR_CLASSES], boats: *const [i16; NUM_BOAT_CLASSES]) {
        unsafe { view().set_zone_civilian_car_info(zoneid, day != 0, &*cars, &*boats) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_set_ped_info(zoneid: u16, day: u8, density: i16, gangs: *const [i16; NUM_GANGS], cop: i16) {
        unsafe { view().set_zone_ped_info(zoneid, day != 0, density, &*gangs, cop) }
    }
    /// what: 0 car density, 1 ped density, 2 ped group.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_set_value(zoneid: u16, day: u8, what: i32, v: u16) {
        unsafe {
            let w = view();
            match what {
                0 => w.set_car_density(zoneid, day != 0, v),
                1 => w.set_ped_density(zoneid, day != 0, v),
                _ => w.set_ped_group(zoneid, day != 0, v),
            }
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_zones_audio_zone(pos: *const V3) -> i16 {
        unsafe { view().find_audio_zone(&*pos) }
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/zones.txt: random scenarios through the original CTheZones logic (create,
    //! hierarchy, lookups, zone info setters, time of day blending) with state dumps.
    use super::*;

    struct Store {
        nav: Vec<Zone>,
        info: Vec<Zone>,
        map: Vec<Zone>,
        infos: Vec<ZoneInfo>,
        audio: Vec<i16>,
        counts: Box<[u16; 6]>, // audio, map, info, nav, zone infos
        find_index: Box<i16>,
        level: Box<i32>,
    }
    impl Store {
        fn new() -> Store {
            Store {
                nav: vec![zeroed_zone(); NUMNAVIGZONES],
                info: vec![zeroed_zone(); NUMINFOZONES],
                map: vec![zeroed_zone(); NUMMAPZONES],
                infos: vec![ZoneInfo::default(); 2 * NUMINFOZONES],
                audio: vec![0; NUMAUDIOZONES],
                counts: Box::new([0; 6]),
                find_index: Box::new(0),
                level: Box::new(0),
            }
        }
        fn view(&mut self) -> View {
            let c = self.counts.as_mut_ptr();
            View {
                nav: self.nav.as_mut_ptr(),
                info: self.info.as_mut_ptr(),
                map: self.map.as_mut_ptr(),
                zone_infos: self.infos.as_mut_ptr(),
                audio: self.audio.as_mut_ptr(),
                num_audio: c,
                num_map: unsafe { c.add(1) },
                num_info: unsafe { c.add(2) },
                num_nav: unsafe { c.add(3) },
                num_zone_infos: unsafe { c.add(4) },
                find_index: &mut *self.find_index,
                curr_level: &mut *self.level,
            }
        }
    }

    fn hf(s: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(s, 16).unwrap())
    }
    fn fmt_info(i: &ZoneInfo) -> String {
        let w: [i16; 34] = unsafe { std::mem::transmute(*i) };
        w.iter().map(|v| format!(" {v}")).collect()
    }

    fn state(v: &View, s: &Store) -> String {
        let nav_index = |p: *mut Zone| if p.is_null() { -1 } else { unsafe { p.offset_from(v.nav) as i64 } };
        let zone = |z: &Zone| {
            let a = u32::from_le_bytes(z.name[..4].try_into().unwrap());
            let b = u32::from_le_bytes(z.name[4..].try_into().unwrap());
            let mut o = format!(" {a:08x}{b:08x}");
            for f in [z.minx, z.miny, z.minz, z.maxx, z.maxy, z.maxz] {
                o += &format!(" {:08x}", f.to_bits());
            }
            o + &format!(
                " {} {} {} {} {} {} {}",
                z.type_,
                z.level,
                z.zoneinfo_day,
                z.zoneinfo_night,
                nav_index(z.child),
                nav_index(z.parent),
                nav_index(z.next)
            )
        };
        let c = &s.counts;
        let mut o = format!(" {} {} {} {} {} {} {}", c[3], c[2], c[1], c[4], c[0], *s.find_index, *s.level);
        for z in s.nav.iter().chain(&s.info).chain(&s.map) {
            o += &zone(z);
        }
        for i in &s.infos {
            o += &fmt_info(i);
        }
        for a in &s.audio {
            o += &format!(" {a}");
        }
        o
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/zones.txt")).unwrap();
        let mut s = Store::new();
        let v = s.view();
        let mut n = 0;
        for (ln, line) in text.lines().enumerate() {
            let (lhs, want) = line.split_once(" =>").unwrap();
            let w: Vec<&str> = lhs.split(' ').collect();
            let p = |i: usize| V3::new(hf(w[i]), hf(w[i + 1]), hf(w[i + 2]));
            let int = |i: usize| w[i].parse::<i32>().unwrap();
            let i16s = |from: usize, k: usize| (0..k).map(|q| w[from + q].parse::<i16>().unwrap()).collect::<Vec<i16>>();
            let got = unsafe {
                match w[0] {
                    "init" => {
                        v.init();
                        String::new()
                    }
                    "create" => {
                        let mut name = w[1].as_bytes().to_vec();
                        v.create_zone(&mut name, int(2), hf(w[3]), hf(w[4]), hf(w[5]), hf(w[6]), hf(w[7]), hf(w[8]), int(9));
                        format!(" {}", String::from_utf8(name).unwrap())
                    }
                    "post" => {
                        v.post_zone_creation();
                        String::new()
                    }
                    "state" => state(&v, &s),
                    "level" => format!(" {}", v.get_level_from_position(&p(1))),
                    "infozone" => format!(" {}", v.find_information_zone_for_position(&p(1)).offset_from(v.info)),
                    "navzone" => {
                        let z = v.find_smallest_navigation_zone_for_position(&p(1), int(4) != 0, int(5) != 0);
                        format!(" {}", if z.is_null() { -1 } else { z.offset_from(v.nav) as i64 })
                    }
                    "audio" => format!(" {}", v.find_audio_zone(&p(1))),
                    "find" => {
                        let a = v.find_zone_by_label(w[1].as_bytes(), int(2));
                        let b = v.find_next_zone_by_label(w[1].as_bytes(), int(2));
                        format!(" {a} {b} {}", *v.find_index)
                    }
                    "carinfo" => {
                        let x = i16s(3, 11);
                        v.set_zone_car_info(int(1) as u16, int(2) != 0, x[0], x[1], x[2..11].try_into().unwrap());
                        String::new()
                    }
                    "civcar" => {
                        let x = i16s(3, 11);
                        v.set_zone_civilian_car_info(int(1) as u16, int(2) != 0, x[0..9].try_into().unwrap(), x[9..11].try_into().unwrap());
                        String::new()
                    }
                    "pedinfo" => {
                        let x = i16s(3, 11);
                        v.set_zone_ped_info(int(1) as u16, int(2) != 0, x[0], x[1..10].try_into().unwrap(), x[10]);
                        String::new()
                    }
                    "setval" => {
                        let (z, d, what, val) = (int(1) as u16, int(2) != 0, int(3), int(4) as u16);
                        match what {
                            0 => v.set_car_density(z, d, val),
                            1 => v.set_ped_density(z, d, val),
                            _ => v.set_ped_group(z, d, val),
                        }
                        String::new()
                    }
                    "tod" => {
                        let mut info = ZoneInfo::default();
                        v.get_zone_info_for_time_of_day(&p(1), int(4) as u8, &mut info);
                        fmt_info(&info)
                    }
                    op => panic!("unknown op {op}"),
                }
            };
            assert_eq!(got, want, "line {}: {}", ln + 1, &lhs[..lhs.len().min(80)]);
            n += 1;
        }
        assert!(n > 1000);
    }
}

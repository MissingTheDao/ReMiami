//! The game clock (C++ src/core/Clock.cpp, CClock). The state stays C++ statics (read inline all
//! over); C++ binds a view of them once and passes the time and pad input in. Identical to the
//! original (tests/golden/clock.txt).

/// CClock's statics (and the globals Update touches), by address.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub hours: *mut u8,
    pub minutes: *mut u8,
    pub seconds: *mut u16,
    pub stored_hours: *mut u8,
    pub stored_minutes: *mut u8,
    pub stored_seconds: *mut u16,
    pub ms_per_game_minute: *mut u32,
    pub last_clock_tick: *mut u32,
    pub clock_has_been_stored: *mut bool,
    /// CStats::DaysPassed.
    pub days_passed: *mut i32,
    pub fast_time: *mut bool,
    /// gbFreezeTime (debug builds).
    pub freeze_time: *mut bool,
}

unsafe impl Send for View {}

impl View {
    /// Initialise (the debug console entries stay C++).
    pub unsafe fn initialise(&self, scale: u32, now: u32) {
        unsafe {
            *self.hours = 12;
            *self.minutes = 0;
            *self.seconds = 0;
            *self.ms_per_game_minute = scale;
            *self.last_clock_tick = now;
            *self.clock_has_been_stored = false;
        }
    }

    /// Update at time `now`; `fast_forward` is pad 2's right shoulder 1 (8 minutes a frame).
    pub unsafe fn update(&self, now: u32, fast_forward: bool) {
        unsafe {
            if fast_forward {
                *self.minutes = (*self.minutes).wrapping_add(8);
                *self.last_clock_tick = now;
                if *self.minutes >= 60 {
                    *self.hours = (*self.hours).wrapping_add(1);
                    *self.minutes = 0;
                    if *self.hours >= 24 {
                        *self.hours = 0;
                    }
                }
            } else if *self.freeze_time {
                *self.last_clock_tick = now;
            } else if now.wrapping_sub(*self.last_clock_tick) > *self.ms_per_game_minute || *self.fast_time {
                *self.minutes = (*self.minutes).wrapping_add(1);
                *self.last_clock_tick = (*self.last_clock_tick).wrapping_add(*self.ms_per_game_minute);
                if *self.fast_time {
                    *self.last_clock_tick = now;
                }
                if *self.minutes >= 60 {
                    *self.hours = (*self.hours).wrapping_add(1);
                    *self.minutes = 0;
                    if *self.hours >= 24 {
                        *self.days_passed = (*self.days_passed).wrapping_add(1);
                        *self.hours = 0;
                    }
                }
            }
            // 60 * elapsed / ms per minute in uint32 (the original divides by zero on a 0 scale).
            let per = *self.ms_per_game_minute;
            if per != 0 {
                *self.seconds = (60u32.wrapping_mul(now.wrapping_sub(*self.last_clock_tick)) / per) as u16;
            }
        }
    }

    pub unsafe fn set_game_clock(&self, mut h: u8, mut m: u8, now: u32) {
        unsafe {
            while m >= 60 {
                m -= 60;
                h = h.wrapping_add(1);
            }
            *self.minutes = m;
            while h >= 24 {
                h -= 24;
            }
            *self.hours = h;
            *self.seconds = 0;
            *self.last_clock_tick = now;
        }
    }

    pub unsafe fn minutes_until(&self, h: u8, m: u8) -> i32 {
        unsafe {
            let now = *self.hours as i32 * 60 + *self.minutes as i32;
            let mut then = h as i32 * 60 + m as i32;
            if then < now {
                then += 24 * 60;
            }
            then - now
        }
    }

    pub unsafe fn is_time_in_range(&self, h1: u8, h2: u8) -> bool {
        let h = unsafe { *self.hours };
        if h1 > h2 { h >= h1 || h < h2 } else { h >= h1 && h < h2 }
    }

    pub unsafe fn store(&self) {
        unsafe {
            *self.stored_hours = *self.hours;
            *self.stored_minutes = *self.minutes;
            *self.stored_seconds = *self.seconds;
            *self.clock_has_been_stored = true;
        }
    }

    pub unsafe fn restore(&self) {
        unsafe {
            *self.hours = *self.stored_hours;
            *self.minutes = *self.stored_minutes;
            *self.seconds = *self.stored_seconds;
        }
    }
}

static VIEW: std::sync::Mutex<Option<View>> = std::sync::Mutex::new(None);

fn view() -> View {
    VIEW.lock().unwrap().expect("revc_clock_bind not called")
}

#[allow(clippy::missing_safety_doc)]
mod ffi {
    use super::*;
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_bind(v: *const View) {
        *VIEW.lock().unwrap() = Some(unsafe { *v });
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_initialise(scale: u32, now: u32) {
        unsafe { view().initialise(scale, now) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_update(now: u32, fast_forward: bool) {
        unsafe { view().update(now, fast_forward) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_set(h: u8, m: u8, now: u32) {
        unsafe { view().set_game_clock(h, m, now) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_minutes_until(h: u8, m: u8) -> i32 {
        unsafe { view().minutes_until(h, m) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_in_range(h1: u8, h2: u8) -> bool {
        unsafe { view().is_time_in_range(h1, h2) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_store() {
        unsafe { view().store() }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_clock_restore() {
        unsafe { view().restore() }
    }
}

#[cfg(test)]
mod golden {
    //! tests/golden/clock.txt: random op sequences through the original CClock, state after each.
    use super::*;

    #[derive(Default)]
    struct S {
        h: u8,
        m: u8,
        s: u16,
        sh: u8,
        sm: u8,
        ss: u16,
        per: u32,
        last: u32,
        stored: bool,
        days: i32,
        fast: bool,
        freeze: bool,
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/clock.txt")).unwrap();
        let mut st = Box::new(S::default());
        let v = View {
            hours: &mut st.h,
            minutes: &mut st.m,
            seconds: &mut st.s,
            stored_hours: &mut st.sh,
            stored_minutes: &mut st.sm,
            stored_seconds: &mut st.ss,
            ms_per_game_minute: &mut st.per,
            last_clock_tick: &mut st.last,
            clock_has_been_stored: &mut st.stored,
            days_passed: &mut st.days,
            fast_time: &mut st.fast,
            freeze_time: &mut st.freeze,
        };
        let mut n = 0;
        for (ln, line) in text.lines().enumerate() {
            let (lhs, want) = line.split_once(" =>").unwrap();
            let w: Vec<u32> = lhs.split(' ').skip(1).map(|x| x.parse().unwrap()).collect();
            let r = unsafe {
                match lhs.split(' ').next().unwrap() {
                    "init" => {
                        v.initialise(w[0], w[1]);
                        String::new()
                    }
                    "update" => {
                        *v.freeze_time = w[2] != 0;
                        *v.fast_time = w[3] != 0;
                        v.update(w[0], w[1] != 0);
                        String::new()
                    }
                    "set" => {
                        v.set_game_clock(w[0] as u8, w[1] as u8, w[2]);
                        String::new()
                    }
                    "until" => format!(" {}", v.minutes_until(w[0] as u8, w[1] as u8)),
                    "range" => format!(" {}", v.is_time_in_range(w[0] as u8, w[1] as u8) as i32),
                    "store" => {
                        v.store();
                        String::new()
                    }
                    "restore" => {
                        v.restore();
                        String::new()
                    }
                    op => panic!("{op}"),
                }
            };
            let s = &*st;
            let got = format!(
                "{r} | {} {} {} {} {} {} {} {} {} {}",
                s.h, s.m, s.s, s.sh, s.sm, s.ss, s.per, s.last, s.stored as i32, s.days
            );
            assert_eq!(got, want, "line {}", ln + 1);
            n += 1;
        }
        assert!(n > 1000);
    }
}

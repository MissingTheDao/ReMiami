//! The pager (plan task 15, text): CPager's queue of scrolling messages (CUserDisplay::Pager).
//! The C++ object keeps its layout and passes itself in; Rust does the queue logic, C++ still
//! draws (CPager::Display) and decides whether scrolling is paused (widescreen, hidden HUD, big
//! messages). Checked against the original with tests/golden/pager.txt.

use crate::messages;
use std::ptr::null;

pub const NUM_PAGER: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagerMessage {
    pub text: *const u16,
    pub speed_ms: u16,
    pub position: i16,
    pub length: u16,
    pub priority: u16,
    pub time_to_change: u32,
    pub unused: i16,
    pub number: [i32; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pager {
    pub num_display_letters: i16,
    pub messages: [PagerMessage; NUM_PAGER],
}

const EMPTY: PagerMessage =
    PagerMessage { text: null(), speed_ms: 0, position: 0, length: 0, priority: 0, time_to_change: 0, unused: 0, number: [0; 6] };

impl Default for Pager {
    fn default() -> Self {
        Pager { num_display_letters: 0, messages: [EMPTY; NUM_PAGER] }
    }
}

/// Where AddToPreviousBriefArray entries go (the game's list; a test can pass its own).
pub trait Briefs {
    fn add_previous(&mut self, text: *const u16, number: [i32; 6]);
}

impl Briefs for messages::Messages {
    fn add_previous(&mut self, text: *const u16, number: [i32; 6]) {
        messages::Messages::add_previous(self, text, number, null());
    }
}

impl Pager {
    /// CPager::Init.
    pub fn init(&mut self) {
        self.clear();
        self.num_display_letters = 8;
    }

    /// CPager::ClearMessages.
    pub fn clear(&mut self) {
        for m in &mut self.messages {
            m.text = null();
        }
    }

    fn start_position(&self) -> i16 {
        -(self.num_display_letters + 10)
    }

    /// CPager::AddMessage / AddMessageWithNumber (`length` of the text as shown, numbers
    /// inserted). The message goes before the first one of lower priority (or into the first empty
    /// slot); with no such slot it is dropped.
    #[allow(clippy::too_many_arguments)] // mirrors CPager's call signature
    pub fn add(&mut self, briefs: &mut impl Briefs, text: *const u16, length: u16, number: [i32; 6], speed: u16, priority: u16, unused: u16, now: u32) {
        for i in 0..NUM_PAGER {
            if !self.messages[i].text.is_null() {
                if self.messages[i].priority >= priority {
                    continue;
                }
                for j in (i + 1..NUM_PAGER).rev() {
                    self.messages[j] = self.messages[j - 1];
                }
            }
            let start = self.start_position();
            self.messages[i] = PagerMessage {
                text,
                speed_ms: speed,
                position: start,
                length,
                priority,
                time_to_change: now.wrapping_add(speed as u32),
                unused: unused as i16,
                number,
            };
            if i == 0 {
                briefs.add_previous(text, number);
            }
            return;
        }
    }

    /// First half of CPager::Process: a message that has scrolled through is replaced by the next.
    pub fn expire(&mut self, briefs: &mut impl Briefs) {
        let top = self.messages[0];
        if top.text.is_null() || (top.position as i32) < top.length as i32 {
            return;
        }
        self.messages[0].text = null();
        let mut i = 0;
        while i < NUM_PAGER - 1 && !self.messages[i + 1].text.is_null() {
            self.messages[i] = self.messages[i + 1];
            i += 1;
        }
        self.messages[i].text = null();
        let next = self.messages[0];
        if !next.text.is_null() {
            briefs.add_previous(next.text, next.number);
        }
    }

    /// Second half of CPager::Process (after Display): scroll one letter when it is time, or
    /// start over while the pager can't be seen (`blocked`).
    pub fn advance(&mut self, blocked: bool, now: u32) {
        if self.messages[0].text.is_null() {
            return;
        }
        if blocked {
            self.restart(now);
        } else if now > self.messages[0].time_to_change {
            let m = &mut self.messages[0];
            m.position = m.position.wrapping_add(1);
            m.time_to_change = now.wrapping_add(m.speed_ms as u32);
        }
    }

    /// CPager::RestartCurrentMessage.
    pub fn restart(&mut self, now: u32) {
        let start = self.start_position();
        let m = &mut self.messages[0];
        if !m.text.is_null() {
            m.position = start;
            m.time_to_change = now.wrapping_add(m.speed_ms as u32);
        }
    }
}

// ---------------------------------------------------------------- C entry points (Pager.cpp)

fn pager<'a>(p: *mut Pager) -> &'a mut Pager {
    unsafe { &mut *p }
}

fn game_briefs() -> &'static mut messages::Messages {
    unsafe { &mut *std::ptr::addr_of_mut!(messages::revc_messages) }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pager_init(p: *mut Pager) {
    pager(p).init();
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pager_clear(p: *mut Pager) {
    pager(p).clear();
}

/// `numbers` null = none (-1s).
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn revc_pager_add(p: *mut Pager, text: *const u16, length: u16, numbers: *const i32, speed: u16, priority: u16, unused: u16, now: u32) {
    let number = if numbers.is_null() { [-1; 6] } else { unsafe { std::slice::from_raw_parts(numbers, 6) }.try_into().unwrap() };
    pager(p).add(game_briefs(), text, length, number, speed, priority, unused, now);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pager_expire(p: *mut Pager) {
    pager(p).expire(game_briefs());
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pager_advance(p: *mut Pager, blocked: i32, now: u32) {
    pager(p).advance(blocked != 0, now);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pager_restart(p: *mut Pager, now: u32) {
    pager(p).restart(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Log(Vec<(*const u16, [i32; 6])>);
    impl Briefs for Log {
        fn add_previous(&mut self, text: *const u16, number: [i32; 6]) {
            self.0.push((text, number));
        }
    }

    #[test]
    fn layout() {
        assert_eq!(std::mem::size_of::<PagerMessage>(), 48);
        assert_eq!(std::mem::offset_of!(PagerMessage, time_to_change), 16);
        assert_eq!(std::mem::offset_of!(PagerMessage, unused), 20);
        assert_eq!(std::mem::offset_of!(PagerMessage, number), 24);
        assert_eq!(std::mem::offset_of!(Pager, messages), 8);
        assert_eq!(std::mem::size_of::<Pager>(), 392);
    }

    #[test]
    fn priority_and_scroll() {
        let (a, b): (Vec<u16>, Vec<u16>) = ("AB\0".encode_utf16().collect(), "C\0".encode_utf16().collect());
        let mut p = Pager::default();
        let mut log = Log::default();
        p.init();
        p.add(&mut log, a.as_ptr(), 2, [-1; 6], 100, 1, 0, 0);
        p.add(&mut log, b.as_ptr(), 1, [-1; 6], 100, 5, 0, 0); // higher priority goes first
        assert_eq!(p.messages[0].text, b.as_ptr());
        assert_eq!(p.messages[1].text, a.as_ptr());
        assert_eq!(p.messages[0].position, -18);
        p.advance(false, 101);
        assert_eq!(p.messages[0].position, -17);
        p.advance(true, 150);
        assert_eq!(p.messages[0].position, -18);
        assert_eq!(log.0.len(), 2);
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/pager.txt (recorded from the original C++).
    use super::*;

    pub(super) fn pool() -> Vec<Vec<u16>> {
        ["CALL ME", "NEW JOB ~1~", "CALL ME", "X"].iter().map(|s| s.encode_utf16().chain([0]).collect()).collect()
    }

    pub(super) fn ptr(pool: &[Vec<u16>], i: i32) -> *const u16 {
        if i < 0 { null() } else { pool[i as usize].as_ptr() }
    }

    fn idx(pool: &[Vec<u16>], p: *const u16) -> String {
        if p.is_null() { "-".into() } else { pool.iter().position(|s| s.as_ptr() == p).map_or("?".into(), |i| i.to_string()) }
    }

    /// Pager state (stale fields included) and the previous-briefs list.
    pub(super) fn fmt_state(pool: &[Vec<u16>], p: &Pager, briefs: &messages::Messages) -> String {
        let mut parts = vec![p.num_display_letters.to_string()];
        for m in &p.messages {
            parts.push(
                format!("{}/{}/{}/{}/{}/{}/{}/{:?}", idx(pool, m.text), m.speed_ms, m.position, m.length, m.priority, m.time_to_change, m.unused, m.number)
                    .replace(' ', ""),
            );
        }
        for b in &briefs.previous {
            parts.push(format!("{}/{:?}", idx(pool, b.text), b.number).replace(' ', ""));
        }
        parts.join(";")
    }

    /// One recorded operation applied to the Rust pager (`blocked` as the C++ found it).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(p: &mut Pager, briefs: &mut messages::Messages, pool: &[Vec<u16>], op: i32, text: i32, n: [i32; 6], speed: u16, priority: u16, unused: u16, blocked: bool, now: u32) {
        let t = ptr(pool, text);
        let shown = |numbers: [i32; 6]| crate::textutil::insert_number(&pool[text as usize][..pool[text as usize].len() - 1], numbers, false).len() as u16;
        match op {
            0 => {
                briefs.init();
                p.init();
            }
            // AddMessage measures the raw text; AddMessageWithNumber the text with numbers in.
            1 => p.add(briefs, t, pool[text as usize].len() as u16 - 1, [-1; 6], speed, priority, unused, now),
            2 => p.add(briefs, t, shown(n), n, speed, priority, unused, now),
            3 => {
                p.expire(briefs);
                p.advance(blocked, now);
            }
            4 => p.clear(),
            5 => p.restart(now),
            _ => panic!("op {op}"),
        }
    }

    #[test]
    fn matches_cpp_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/pager.txt");
        let data = std::fs::read_to_string(&path).expect("tests/golden/pager.txt");
        let pool = pool();
        let (mut p, mut briefs) = (Pager::default(), messages::Messages::default());
        let mut n_ops = 0;
        for line in data.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (input, want) = line.split_once(" => ").unwrap();
            let f: Vec<i64> = input.split(' ').map(|x| x.parse().unwrap()).collect();
            let n: [i32; 6] = [f[2], f[3], f[4], f[5], f[6], f[7]].map(|x| x as i32);
            apply(&mut p, &mut briefs, &pool, f[0] as i32, f[1] as i32, n, f[8] as u16, f[9] as u16, f[10] as u16, f[11] != 0, f[12] as u32);
            assert_eq!(fmt_state(&pool, &p, &briefs), want, "after line {}: {input}", n_ops + 1);
            n_ops += 1;
        }
        assert!(n_ops > 3000, "only {n_ops} operations");
    }
}

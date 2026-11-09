//! On-screen message queues (plan task 15, text): CMessages' brief messages (subtitles,
//! objectives), big messages (mission titles, "passed" etc., six styles with a queue of four
//! each) and the previous-briefs list of the Brief menu.
//!
//! The arrays keep the C layout of tMessage / tBigMessage / tPreviousBrief and live in one exported
//! static, `revc_messages`, because C++ reads them directly (CMessages::BriefMessages etc. are
//! references into it: Script.cpp resets start times, the Brief menu lists PreviousBriefs).
//! Strings are not owned: `text`/`string` point at GXT or script text, compared by address where
//! the original compares addresses and by content in ClearThisPrint / ClearThisBigPrint.
//!
//! Behaviour follows Messages.cpp exactly, quirks included (checked with
//! tests/golden/messages.txt, recorded from the original C++).

use std::ptr::null;

pub const NUM_BRIEF: usize = 8;
pub const NUM_BIG: usize = 6;
pub const NUM_PREVIOUS: usize = 5;
const NO_NUMBERS: [i32; 6] = [-1; 6];

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Message {
    pub text: *const u16,
    pub flag: u16,
    pub time: u32,
    pub start_time: u32,
    pub number: [i32; 6],
    pub string: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BigMessage {
    pub stack: [Message; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviousBrief {
    pub text: *const u16,
    pub number: [i32; 6],
    pub string: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Messages {
    pub brief: [Message; NUM_BRIEF],
    pub big: [BigMessage; NUM_BIG],
    pub previous: [PreviousBrief; NUM_PREVIOUS],
}

const EMPTY: Message = Message { text: null(), flag: 0, time: 0, start_time: 0, number: [0; 6], string: null() };
const EMPTY_PREV: PreviousBrief = PreviousBrief { text: null(), number: [0; 6], string: null() };

impl Default for Messages {
    fn default() -> Self {
        Messages { brief: [EMPTY; NUM_BRIEF], big: [BigMessage { stack: [EMPTY; 4] }; NUM_BIG], previous: [EMPTY_PREV; NUM_PREVIOUS] }
    }
}

/// How a brief message enters the queue (CMessages::AddMessage*, *JumpQ*, *Soon*).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Appended to the queue (dropped if all 8 slots are used).
    Queue,
    /// Replaces the message on screen.
    JumpQ,
    /// Shown next: slot 1 if something is on screen, else at once.
    Soon,
}

fn msg(text: *const u16, flag: u16, time: u32, now: u32, number: [i32; 6], string: *const u16) -> Message {
    Message { text, flag, time, start_time: now, number, string }
}

/// FastWideStringComparison: equal content (both strings non-empty in practice).
fn same_text(a: *const u16, b: *const u16) -> bool {
    if a.is_null() || b.is_null() {
        return false;
    }
    let mut i = 0;
    loop {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return false;
        }
        i += 1;
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x == 0 && y == 0 {
            return true;
        }
    }
}

impl Messages {
    /// CMessages::Init: everything empty.
    pub fn init(&mut self) {
        self.clear();
        for p in &mut self.previous {
            p.text = null();
            p.string = null();
        }
    }

    /// CMessages::ClearMessages (big and brief; the previous-briefs list stays).
    pub fn clear(&mut self) {
        for b in &mut self.big {
            for m in &mut b.stack {
                m.text = null();
                m.string = null();
            }
        }
        self.clear_small();
    }

    /// CMessages::ClearSmallMessagesOnly.
    pub fn clear_small(&mut self) {
        for m in &mut self.brief {
            m.text = null();
            m.string = null();
        }
    }

    /// CMessages::AddToPreviousBriefArray: newest first, no exact duplicates (same text and string
    /// addresses, same numbers); when the list is full the last entry is dropped.
    pub fn add_previous(&mut self, text: *const u16, number: [i32; 6], string: *const u16) {
        let mut i = 0;
        while i < NUM_PREVIOUS && !self.previous[i].text.is_null() {
            let p = &self.previous[i];
            if p.number == number && p.text == text && p.string == string {
                return;
            }
            i += 1;
        }
        if i != 0 {
            let mut j = if i == NUM_PREVIOUS { i as isize - 2 } else { i as isize - 1 };
            while j >= 0 {
                self.previous[j as usize + 1] = self.previous[j as usize];
                j -= 1;
            }
        }
        self.previous[0] = PreviousBrief { text, number, string };
    }

    /// CMessages::AddMessage / AddMessageWithNumber / AddMessageWithString and the JumpQ / Soon
    /// variants (`number` is all -1 and `string` null where the original passes none).
    #[allow(clippy::too_many_arguments)] // mirrors CMessages' call signatures
    pub fn add_brief(&mut self, mode: Mode, text: *const u16, time: u32, flag: u16, number: [i32; 6], string: *const u16, now: u32) {
        let m = msg(text, flag, time, now, number, string);
        match mode {
            Mode::Queue => {
                let Some(i) = self.brief.iter().position(|b| b.text.is_null()) else { return };
                self.brief[i] = m;
                if i == 0 {
                    self.add_previous(text, number, string);
                }
            }
            Mode::JumpQ => {
                self.brief[0] = m;
                self.add_previous(text, number, string);
            }
            Mode::Soon => {
                if !self.brief[0].text.is_null() {
                    for i in (2..NUM_BRIEF).rev() {
                        self.brief[i] = self.brief[i - 1];
                    }
                    self.brief[1] = m;
                } else {
                    self.brief[0] = m;
                    self.add_previous(text, number, null());
                }
            }
        }
    }

    /// CMessages::AddBigMessage(WithNumber) (`queued` false: replaces what is shown in that style)
    /// and AddBigMessageQ / AddBigMessageWithNumberQ (`queued`: next free of four, else dropped).
    pub fn add_big(&mut self, style: usize, text: *const u16, time: u32, number: [i32; 6], queued: bool, now: u32) {
        let Some(big) = self.big.get_mut(style) else { return };
        let i = if queued {
            match big.stack.iter().position(|m| m.text.is_null()) {
                Some(i) => i,
                None => return,
            }
        } else {
            0
        };
        big.stack[i] = msg(text, 0, time, now, number, null());
    }

    /// Moves `stack[from..]` up over a removed entry; returns the index left empty.
    fn close_gap(stack: &mut [Message], mut i: usize) -> usize {
        while i + 1 < stack.len() && !stack[i + 1].text.is_null() {
            stack[i] = stack[i + 1];
            i += 1;
        }
        stack[i].text = null();
        i
    }

    /// CMessages::Process: expire the message shown in each big style and the brief on screen.
    pub fn process(&mut self, now: u32) {
        for big in &mut self.big {
            let top = big.stack[0];
            if !top.text.is_null() && now > top.time.wrapping_add(top.start_time) {
                big.stack[0].text = null();
                Self::close_gap(&mut big.stack, 0);
                big.stack[0].start_time = now;
            }
        }
        let top = self.brief[0];
        if !top.text.is_null() && now > top.time.wrapping_add(top.start_time) {
            self.brief[0].text = null();
            Self::close_gap(&mut self.brief, 0);
            self.brief[0].start_time = now;
            let b = self.brief[0];
            if !b.text.is_null() {
                self.add_previous(b.text, b.number, b.string);
            }
        }
    }

    /// CMessages::ClearThisPrint: removes every brief message with this text (by content).
    pub fn clear_this_print(&mut self, text: *const u16, now: u32) {
        loop {
            let found = (0..NUM_BRIEF).take_while(|&i| !self.brief[i].text.is_null()).find(|&i| same_text(text, self.brief[i].text));
            let Some(i) = found else { break };
            self.brief[i].text = null();
            Self::close_gap(&mut self.brief, i);
            if i == 0 {
                self.brief[0].start_time = now;
                let b = self.brief[0];
                if !b.text.is_null() {
                    self.add_previous(b.text, b.number, b.string);
                }
            }
        }
    }

    /// CMessages::ClearThisBigPrint: removes big messages with this text (by content). The
    /// original's search stops at the first empty slot it meets, even in an earlier style.
    pub fn clear_this_big_print(&mut self, text: *const u16, now: u32) {
        loop {
            let (mut style, mut i, mut equal) = (0, 0, false);
            while style < NUM_BIG {
                if i >= 4 || self.big[style].stack[i].text.is_null() || equal {
                    break;
                }
                equal = same_text(text, self.big[style].stack[i].text);
                if !equal {
                    i += 1;
                    if i == 4 {
                        i = 0;
                        style += 1;
                    }
                }
            }
            if !equal {
                break;
            }
            let stack = &mut self.big[style].stack;
            stack[i].text = null();
            Self::close_gap(stack, i);
            if i == 0 {
                stack[0].start_time = now;
            }
        }
    }
}

// ---------------------------------------------------------------- the game's queues (C++ view)

/// The queues the game uses; C++ refers to the arrays inside (text/Messages.h).
#[unsafe(no_mangle)]
pub static mut revc_messages: Messages = Messages {
    brief: [EMPTY; NUM_BRIEF],
    big: [BigMessage { stack: [EMPTY; 4] }; NUM_BIG],
    previous: [EMPTY_PREV; NUM_PREVIOUS],
};

/// The game's queues. Only the main thread touches them (as in the original).
fn game() -> &'static mut Messages {
    unsafe { &mut *std::ptr::addr_of_mut!(revc_messages) }
}

fn numbers(p: *const i32) -> [i32; 6] {
    if p.is_null() { NO_NUMBERS } else { unsafe { std::slice::from_raw_parts(p, 6) }.try_into().unwrap() }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_init() {
    game().init();
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_clear(small_only: i32) {
    if small_only != 0 { game().clear_small() } else { game().clear() }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_process(now: u32) {
    game().process(now);
}

/// mode 0 queue, 1 jump the queue, 2 soon; `numbers` null = none (-1s).
#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_add(mode: i32, text: *const u16, time: u32, flag: u16, numbers_: *const i32, string: *const u16, now: u32) {
    let mode = match mode {
        1 => Mode::JumpQ,
        2 => Mode::Soon,
        _ => Mode::Queue,
    };
    game().add_brief(mode, text, time, flag, numbers(numbers_), string, now);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_add_big(style: u16, text: *const u16, time: u32, numbers_: *const i32, queued: i32, now: u32) {
    game().add_big(style as usize, text, time, numbers(numbers_), queued != 0, now);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_add_previous(text: *const u16, numbers_: *const i32, string: *const u16) {
    game().add_previous(text, numbers(numbers_), string);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_messages_clear_this(text: *const u16, big: i32, now: u32) {
    if big != 0 { game().clear_this_big_print(text, now) } else { game().clear_this_print(text, now) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layout shared with text/Messages.h (checked there with static_asserts too).
    #[test]
    fn layout() {
        assert_eq!(std::mem::size_of::<Message>(), 56);
        assert_eq!(std::mem::offset_of!(Message, flag), 8);
        assert_eq!(std::mem::offset_of!(Message, time), 12);
        assert_eq!(std::mem::offset_of!(Message, number), 20);
        assert_eq!(std::mem::offset_of!(Message, string), 48);
        assert_eq!(std::mem::size_of::<PreviousBrief>(), 40);
        assert_eq!(std::mem::size_of::<Messages>(), 56 * 8 + 56 * 4 * 6 + 40 * 5);
    }

    #[test]
    fn queue_and_expire() {
        let a: Vec<u16> = "A\0".encode_utf16().collect();
        let b: Vec<u16> = "B\0".encode_utf16().collect();
        let mut m = Messages::default();
        m.add_brief(Mode::Queue, a.as_ptr(), 1000, 0, NO_NUMBERS, null(), 0);
        m.add_brief(Mode::Queue, b.as_ptr(), 500, 0, NO_NUMBERS, null(), 0);
        assert_eq!(m.brief[0].text, a.as_ptr());
        assert_eq!(m.previous[0].text, a.as_ptr());
        m.process(1001);
        assert_eq!(m.brief[0].text, b.as_ptr());
        assert_eq!(m.brief[0].start_time, 1001);
        assert_eq!(m.previous[0].text, b.as_ptr());
        assert!(m.brief[1].text.is_null());
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/messages.txt (recorded from the original C++).
    use super::*;

    /// Strings the operations refer to by index: two different addresses with the same content
    /// (0 and 2) separate address from content comparison.
    pub(super) fn pool() -> Vec<Vec<u16>> {
        ["MISSION", "PASSED", "MISSION", "~1~ cars", "Go to ~a~", "X"]
            .iter()
            .map(|s| s.encode_utf16().chain([0]).collect())
            .collect()
    }

    pub(super) fn ptr(pool: &[Vec<u16>], i: i32) -> *const u16 {
        if i < 0 { null() } else { pool[i as usize].as_ptr() }
    }

    fn idx(pool: &[Vec<u16>], p: *const u16) -> String {
        if p.is_null() {
            return "-".into();
        }
        pool.iter().position(|s| s.as_ptr() == p).map_or("?".into(), |i| i.to_string())
    }

    fn fmt_msg(pool: &[Vec<u16>], m: &Message) -> String {
        format!("{}/{}/{}/{}/{:?}/{}", idx(pool, m.text), m.flag, m.time, m.start_time, m.number, idx(pool, m.string)).replace(' ', "")
    }

    /// Whole state, stale fields of emptied slots included.
    pub(super) fn fmt_state(pool: &[Vec<u16>], s: &Messages) -> String {
        let mut parts: Vec<String> = s.brief.iter().map(|m| fmt_msg(pool, m)).collect();
        for b in &s.big {
            parts.extend(b.stack.iter().map(|m| fmt_msg(pool, m)));
        }
        for p in &s.previous {
            parts.push(format!("{}/{:?}/{}", idx(pool, p.text), p.number, idx(pool, p.string)).replace(' ', ""));
        }
        parts.join(";")
    }

    /// One operation (the recorder's op numbers) applied to `s`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(s: &mut Messages, pool: &[Vec<u16>], op: i32, text: i32, time: u32, flag: u16, n: [i32; 6], string: i32, now: u32) {
        let (t, st) = (ptr(pool, text), ptr(pool, string));
        match op {
            0..=2 => s.add_brief([Mode::Queue, Mode::JumpQ, Mode::Soon][op as usize], t, time, flag, NO_NUMBERS, null(), now),
            3..=5 => s.add_brief([Mode::Queue, Mode::JumpQ, Mode::Soon][op as usize - 3], t, time, flag, n, null(), now),
            6 => s.add_brief(Mode::Queue, t, time, flag, NO_NUMBERS, st, now),
            7 => s.add_brief(Mode::JumpQ, t, time, flag, NO_NUMBERS, st, now),
            8 => s.add_big(flag as usize, t, time, NO_NUMBERS, false, now),
            9 => s.add_big(flag as usize, t, time, NO_NUMBERS, true, now),
            10 => s.add_big(flag as usize, t, time, n, false, now),
            11 => s.add_big(flag as usize, t, time, n, true, now),
            12 => s.process(now),
            13 => s.clear(),
            14 => s.clear_small(),
            15 => s.clear_this_print(t, now),
            16 => s.clear_this_big_print(t, now),
            17 => s.init(),
            18 => s.add_previous(t, n, st),
            _ => panic!("op {op}"),
        }
    }

    #[test]
    fn matches_cpp_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/messages.txt");
        let data = std::fs::read_to_string(&path).expect("tests/golden/messages.txt");
        let pool = pool();
        let mut s = Messages::default();
        let mut n_ops = 0;
        for line in data.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (input, want) = line.split_once(" => ").unwrap();
            let f: Vec<i64> = input.split(' ').map(|x| x.parse().unwrap()).collect();
            let n: [i32; 6] = [f[4], f[5], f[6], f[7], f[8], f[9]].map(|x| x as i32);
            apply(&mut s, &pool, f[0] as i32, f[1] as i32, f[2] as u32, f[3] as u16, n, f[10] as i32, f[11] as u32);
            assert_eq!(fmt_state(&pool, &s), want, "after line {}: {input}", n_ops + 1);
            n_ops += 1;
        }
        assert!(n_ops > 3000, "only {n_ops} operations");
    }
}

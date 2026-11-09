//! Save and load (plan task 15, save): the PC save file container, GenericSave / GenericLoad /
//! PopulateSlotInfo / RestoreForStartLoad and CDate from src/save. The game systems' own blocks
//! (scripts, pools, garages, ... 22 of them) are still written and read by their C++ Save/Load
//! functions, called through `revc_save_block` / `revc_load_block`; the "simple vars" header is
//! gathered from and applied to the C++ globals by `revc_save_gather_vars` / `revc_save_apply_vars`.
//!
//! File layout (all little endian, sizes 4-byte aligned):
//! - chunk 0: simple vars (0xE4 bytes, each field in its own 4-byte-aligned slot), then the
//!   scripts block as [size][data];
//! - chunks 1..=22: [size][data] of each system, in a fixed order;
//! - padding chunks of zeros up to SIZE_OF_ONE_GAME (the original's arithmetic, quirks included);
//! - the checksum: sum of every byte of every chunk (its size word included), as i32.
//!
//! Every chunk is [chunk size u32][chunk bytes]. Differences from the C++: bytes the original
//! filled with leftover memory (field padding in the header, the name after its terminator, the
//! block buffers' unused bytes) are zeros here.

use crate::textutil;
use std::ffi::c_char;

pub const SIZE_OF_ONE_GAME: u32 = 201729;
/// The game's work_buff: the largest chunk the loader accepts and the padding chunks' limit.
pub const WORK_BUFF: usize = 55000;
pub const SIZE_OF_SIMPLE_VARS: usize = 0xE4;
/// Blocks after the header chunk (pools, garages, ..., ped types).
pub const BLOCK_COUNT: i32 = 22;
/// Block index of the scripts, nested in the header chunk.
pub const SCRIPTS: i32 = -1;
/// First bytes of a slot file that was never completed.
const TOP_LINE_EMPTY_FILE: &[u8] = b"THIS FILE IS NOT VALID YET";

pub fn align4(n: u32) -> u32 {
    (n.wrapping_add(3)) & !3
}

/// The header's values, in file order, with the C++ types (shared with
/// cpp/re3-miami/src/save/GenericGameStorage.cpp, struct RevcSimpleVars).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimpleVars {
    pub name: [u16; 24],
    /// SYSTEMTIME: year, month, day of week, day, hour, minute, second, milliseconds.
    pub time: [u16; 8],
    /// SIZE_OF_ONE_GAME, with the quick-save type in the top byte (mission replay).
    pub size_and_quicksave: i32,
    pub level: i32,
    pub camera: [f32; 3],
    pub ms_per_game_minute: u32,
    pub last_clock_tick: u32,
    pub hours: u8,
    pub minutes: u8,
    pub pad_mode: i16,
    pub time_ms: u32,
    pub time_scale: f32,
    pub time_step: f32,
    pub time_step_non_clipped: f32,
    pub frame_counter: u32,
    pub ts_time_step: f32,
    pub ts_frames_per_update: f32,
    pub ts_time_scale: f32,
    pub old_weather: i16,
    pub new_weather: i16,
    pub forced_weather: i16,
    pub weather_interpolation: f32,
    pub weather_in_list: i32,
    pub car_zoom: f32,
    pub ped_zoom: f32,
    pub area: i32,
    pub all_taxis_nitro: u8,
    pub invert_look: u8,
    pub extra_colour: i32,
    pub extra_colour_on: i32,
    pub extra_colour_inter: f32,
    pub radio_positions: [i32; 10],
}

/// Writes `bytes` into a 4-byte-aligned slot (zero padding).
fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
    out.resize(out.len() + (align4(bytes.len() as u32) as usize - bytes.len()), 0);
}

/// Reads fields from 4-byte-aligned slots.
struct Slots<'a> {
    b: &'a [u8],
    at: usize,
}
impl Slots<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let v: [u8; N] = self.b[self.at..self.at + N].try_into().unwrap();
        self.at += align4(N as u32) as usize;
        v
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }
    fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take())
    }
    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }
    fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.take())
    }
    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }
}

impl SimpleVars {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(SIZE_OF_SIMPLE_VARS);
        let u16s = |v: &[u16]| v.iter().flat_map(|c| c.to_le_bytes()).collect::<Vec<u8>>();
        put(&mut o, &u16s(&self.name));
        put(&mut o, &u16s(&self.time));
        put(&mut o, &self.size_and_quicksave.to_le_bytes());
        put(&mut o, &self.level.to_le_bytes());
        for c in self.camera {
            put(&mut o, &c.to_le_bytes());
        }
        put(&mut o, &self.ms_per_game_minute.to_le_bytes());
        put(&mut o, &self.last_clock_tick.to_le_bytes());
        put(&mut o, &[self.hours]);
        put(&mut o, &[self.minutes]);
        put(&mut o, &self.pad_mode.to_le_bytes());
        put(&mut o, &self.time_ms.to_le_bytes());
        put(&mut o, &self.time_scale.to_le_bytes());
        put(&mut o, &self.time_step.to_le_bytes());
        put(&mut o, &self.time_step_non_clipped.to_le_bytes());
        put(&mut o, &self.frame_counter.to_le_bytes());
        put(&mut o, &self.ts_time_step.to_le_bytes());
        put(&mut o, &self.ts_frames_per_update.to_le_bytes());
        put(&mut o, &self.ts_time_scale.to_le_bytes());
        put(&mut o, &self.old_weather.to_le_bytes());
        put(&mut o, &self.new_weather.to_le_bytes());
        put(&mut o, &self.forced_weather.to_le_bytes());
        put(&mut o, &self.weather_interpolation.to_le_bytes());
        put(&mut o, &self.weather_in_list.to_le_bytes());
        put(&mut o, &self.car_zoom.to_le_bytes());
        put(&mut o, &self.ped_zoom.to_le_bytes());
        put(&mut o, &self.area.to_le_bytes());
        put(&mut o, &[self.all_taxis_nitro]);
        put(&mut o, &[self.invert_look]);
        put(&mut o, &self.extra_colour.to_le_bytes());
        put(&mut o, &self.extra_colour_on.to_le_bytes());
        put(&mut o, &self.extra_colour_inter.to_le_bytes());
        put(&mut o, &self.radio_positions.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>());
        debug_assert_eq!(o.len(), SIZE_OF_SIMPLE_VARS);
        o
    }

    /// Parses the first SIZE_OF_SIMPLE_VARS bytes of chunk 0 (None if shorter).
    pub fn from_bytes(b: &[u8]) -> Option<SimpleVars> {
        if b.len() < SIZE_OF_SIMPLE_VARS {
            return None;
        }
        let mut s = Slots { b, at: 0 };
        let mut v = SimpleVars::default();
        let name: [u8; 48] = s.take();
        let time: [u8; 16] = s.take();
        for i in 0..24 {
            v.name[i] = u16::from_le_bytes([name[i * 2], name[i * 2 + 1]]);
        }
        for i in 0..8 {
            v.time[i] = u16::from_le_bytes([time[i * 2], time[i * 2 + 1]]);
        }
        v.size_and_quicksave = s.i32();
        v.level = s.i32();
        v.camera = [s.f32(), s.f32(), s.f32()];
        v.ms_per_game_minute = s.u32();
        v.last_clock_tick = s.u32();
        v.hours = s.u8();
        v.minutes = s.u8();
        v.pad_mode = s.i16();
        v.time_ms = s.u32();
        v.time_scale = s.f32();
        v.time_step = s.f32();
        v.time_step_non_clipped = s.f32();
        v.frame_counter = s.u32();
        v.ts_time_step = s.f32();
        v.ts_frames_per_update = s.f32();
        v.ts_time_scale = s.f32();
        v.old_weather = s.i16();
        v.new_weather = s.i16();
        v.forced_weather = s.i16();
        v.weather_interpolation = s.f32();
        v.weather_in_list = s.i32();
        v.car_zoom = s.f32();
        v.ped_zoom = s.f32();
        v.area = s.i32();
        v.all_taxis_nitro = s.u8();
        v.invert_look = s.u8();
        v.extra_colour = s.i32();
        v.extra_colour_on = s.i32();
        v.extra_colour_inter = s.f32();
        for r in v.radio_positions.iter_mut() {
            *r = s.i32();
        }
        debug_assert_eq!(s.at, SIZE_OF_SIMPLE_VARS);
        Some(v)
    }
}

/// The save name from the last passed mission's title: at most 23 characters; a title that fills
/// them ends in "..." (GenericSave with FIX_BUGS).
pub fn save_name(mission_title: &[u16]) -> [u16; 24] {
    let mut name = [0u16; 24];
    let len = mission_title.len().min(23);
    name[..len].copy_from_slice(&mission_title[..len]);
    if len > 22 {
        // TextCopy of "..." (with its terminator) at 24 - 6.
        name[18..22].copy_from_slice(&[b'.' as u16, b'.' as u16, b'.' as u16, 0]);
    }
    name[23] = 0;
    name
}

/// A save file being built (PcClassSaveRoutine's framing and checksum).
pub struct SaveImage {
    pub data: Vec<u8>,
    pub checksum: i32,
    /// The original's totalSize: chunk bytes written, size words not counted.
    total: u32,
}

impl SaveImage {
    pub(crate) fn new() -> SaveImage {
        SaveImage { data: Vec::with_capacity(SIZE_OF_ONE_GAME as usize + 128), checksum: 0, total: 0 }
    }
    /// One chunk: size word, then the bytes padded to 4; the checksum adds every byte of both.
    pub(crate) fn chunk(&mut self, bytes: &[u8]) {
        let size = bytes.len() as u32;
        self.data.extend_from_slice(&size.to_le_bytes());
        self.data.extend_from_slice(bytes);
        self.data.resize(self.data.len() + (align4(size) - size) as usize, 0);
        let sum = size.to_le_bytes().iter().chain(bytes.iter()).fold(0i32, |s, &b| s.wrapping_add(b as i32));
        self.checksum = self.checksum.wrapping_add(sum);
    }
    /// A chunk that counts towards the game's size (the original's totalSize += size).
    pub(crate) fn block(&mut self, bytes: &[u8]) {
        self.chunk(bytes);
        self.total += align4(bytes.len() as u32);
    }
    /// Pads to one game's size with zero chunks as the original computes it (unsigned
    /// arithmetic, then the int align4bytes, at most work_buff per chunk), then the checksum.
    pub(crate) fn finish(&mut self) {
        let zeros = vec![0u8; WORK_BUFF];
        for _ in 0..4 {
            let raw = SIZE_OF_ONE_GAME.wrapping_sub(self.total).wrapping_sub(4) as i32;
            let mut size = (raw.wrapping_add(3) & !3) as u32;
            if size > WORK_BUFF as u32 {
                size = WORK_BUFF as u32;
            }
            if size > 4 {
                self.chunk(&zeros[..size as usize]);
                self.total += size;
            }
        }
        let sum = self.checksum;
        self.data.extend_from_slice(&sum.to_le_bytes());
    }
}

/// [size][data aligned] of one system's block; `save` fills the buffer and returns the size.
fn block_bytes(prefix: &[u8], room: usize, save: &mut dyn FnMut(&mut [u8]) -> u32) -> Vec<u8> {
    let mut buf = vec![0u8; room];
    let size = save(&mut buf);
    let len = (align4(size) as usize).min(room);
    let mut out = Vec::with_capacity(prefix.len() + 4 + len);
    out.extend_from_slice(prefix);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&buf[..len]);
    out
}

/// GenericSave: the whole file. `save_block(index, buffer)` runs the C++ Save function of block
/// `index` (SCRIPTS, then 0..BLOCK_COUNT) into the buffer and returns its size.
pub fn build_save(vars: &SimpleVars, save_block: &mut dyn FnMut(i32, &mut [u8]) -> u32) -> SaveImage {
    let mut img = SaveImage::new();
    let header = vars.to_bytes();
    let first = block_bytes(&header, WORK_BUFF - SIZE_OF_SIMPLE_VARS - 4, &mut |b| save_block(SCRIPTS, b));
    img.block(&first);
    for i in 0..BLOCK_COUNT {
        let block = block_bytes(&[], WORK_BUFF - 4, &mut |b| save_block(i, b));
        img.block(&block);
    }
    img.finish();
    img
}

/// Why a load stopped.
#[derive(Debug, PartialEq)]
pub enum LoadError {
    /// The file ends (or a chunk is larger than the game's buffer) before the last block.
    Read,
}

/// GenericLoad's walk over a save file: parses the header, then hands over the scripts block and
/// the 22 system blocks in order. Stops with Read at the first chunk the file doesn't hold, after
/// the blocks before it were loaded (as the original, which reads chunk by chunk).
///
/// `load_block(index, chunk, offset, size)` gets the whole chunk the block came in (as the original
/// held it in work_buff), the block's offset in it and its size.
pub fn load(data: &[u8], apply_vars: &mut dyn FnMut(&SimpleVars), load_block: &mut dyn FnMut(i32, &[u8], usize, usize)) -> Result<(), LoadError> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    // Chunk 0. The original reads it without checking the length; a short file leaves the rest of
    // its buffer as it was, here it is zeros.
    let size = word(0).ok_or(LoadError::Read)?;
    let size = (align4(size) as usize).min(WORK_BUFF);
    let mut first = vec![0u8; WORK_BUFF];
    let avail = data.len().saturating_sub(4).min(size);
    first[..avail].copy_from_slice(&data[4..4 + avail]);
    let vars = SimpleVars::from_bytes(&first).unwrap();
    apply_vars(&vars);
    let inner = u32::from_le_bytes(first[SIZE_OF_SIMPLE_VARS..SIZE_OF_SIMPLE_VARS + 4].try_into().unwrap()) as usize;
    let start = SIZE_OF_SIMPLE_VARS + 4;
    load_block(SCRIPTS, &first[..size], start, inner.min(WORK_BUFF - start));
    let mut at = 4 + size;
    for i in 0..BLOCK_COUNT {
        let size = align4(word(at).ok_or(LoadError::Read)?) as usize;
        let chunk = data.get(at + 4..at + 4 + size).filter(|_| size <= WORK_BUFF).ok_or(LoadError::Read)?;
        let inner = chunk.get(..4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).unwrap_or(0);
        load_block(i, chunk, 4.min(chunk.len()), inner.min(chunk.len().saturating_sub(4)));
        at += 4 + size;
    }
    Ok(())
}

/// RestoreForStartLoad: the level and camera position from the first bytes of a save.
pub fn start_info(data: &[u8]) -> Option<(i32, [f32; 3])> {
    let v = SimpleVars::from_bytes(data.get(4..)?)?;
    Some((v.level, v.camera))
}

/// PopulateSlotInfo's per-slot result.
#[derive(Debug, PartialEq)]
pub enum Slot {
    Empty,
    Ok { name: Vec<u16>, date: String },
    Corrupted,
}

/// The date line of a slot: "DD MON YYYY HH:MM:SS" (month name from the text, Latin-1), or with
/// a numeric month in the Japanese game. `time` is the SYSTEMTIME from the header.
pub fn slot_date(time: &[u16; 8], month_name: &dyn Fn(&str) -> Vec<u16>, japanese: bool) -> String {
    const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let [year, month, _, day, hour, minute, second, _] = *time;
    if japanese {
        return format!("{day:02} {month:02} {year:04} {hour:02}:{minute:02}:{second:02}");
    }
    // An invalid month was undefined behaviour in the original (assert only); empty here.
    let name = match MONTHS.get((month as usize).wrapping_sub(1)) {
        Some(key) => textutil::to_ascii_saveload(&month_name(key)).iter().map(|&b| b as char).collect::<String>(),
        None => String::new(),
    };
    format!("{day:02} {name} {year:04} {hour:02}:{minute:02}:{second:02}")
}

/// One slot of PopulateSlotInfo. `file` is the slot's file (None: no such file); `checked` runs
/// CheckDataNotCorrupt and the incompatible-save fix (both C++ for now) and says whether the
/// slot is usable.
pub fn slot_info(file: Option<&[u8]>, checked: &mut dyn FnMut() -> bool, month_name: &dyn Fn(&str) -> Vec<u16>, japanese: bool) -> Slot {
    let Some(file) = file else { return Slot::Empty };
    // The header (size word, name, time) as read; a short file leaves zeros.
    let mut header = [0u8; 4 + 48 + 16];
    let n = file.len().min(header.len());
    header[..n].copy_from_slice(&file[..n]);
    if header.starts_with(TOP_LINE_EMPTY_FILE) {
        return Slot::Empty;
    }
    if !checked() {
        return Slot::Corrupted;
    }
    let name: Vec<u16> = (0..24).map(|i| u16::from_le_bytes([header[4 + i * 2], header[5 + i * 2]])).collect();
    let mut time = [0u16; 8];
    for (i, t) in time.iter_mut().enumerate() {
        *t = u16::from_le_bytes([header[52 + i * 2], header[53 + i * 2]]);
    }
    Slot::Ok { name, date: slot_date(&time, month_name, japanese) }
}

/// CDate's comparison: year, month, day, hour, minute, second.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Date {
    pub second: i32,
    pub minute: i32,
    pub hour: i32,
    pub day: i32,
    pub month: i32,
    pub year: i32,
}
impl Date {
    fn key(&self) -> [i32; 6] {
        [self.year, self.month, self.day, self.hour, self.minute, self.second]
    }
}

// ---- C ABI ----

unsafe extern "C" {
    /// C++ (GenericGameStorage.cpp): fills the header from the game's globals.
    fn revc_save_gather_vars(vars: *mut SimpleVars);
    /// C++: sets the game's globals from a loaded header.
    fn revc_save_apply_vars(vars: *const SimpleVars);
    /// C++: runs the Save function of block `index` into `buf`, sets `size`.
    fn revc_save_block(index: i32, buf: *mut u8, size: *mut u32);
    /// C++: runs the Load function of block `index` (and what the original did right before it).
    fn revc_load_block(index: i32, chunk: *const u8, chunk_len: usize, offset: usize, size: u32);
    /// C++: CText::Get (with its fallbacks).
    fn revc_cpp_text_get(key: *const c_char) -> *const u16;
}

fn text(key: &str) -> Vec<u16> {
    let k = std::ffi::CString::new(key).unwrap();
    unsafe { textutil::wide(revc_cpp_text_get(k.as_ptr())) }.to_vec()
}

/// GenericSave without the file: builds the save image. Returns its bytes through `out`/`len`
/// (free with `revc_save_free`) and the checksum.
///
/// # Safety
/// Pointers valid; called on the game thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_build(out: *mut *mut u8, len: *mut usize, checksum: *mut i32) {
    let mut vars = SimpleVars::default();
    unsafe { revc_save_gather_vars(&mut vars) };
    // The C++ gathers the mission title into the name field; truncate it as GenericSave did.
    let title = vars.name.iter().position(|&c| c == 0).map_or(&vars.name[..], |n| &vars.name[..n]).to_vec();
    vars.name = save_name(&title);
    let img = build_save(&vars, &mut |i, buf| {
        let mut size = 0u32;
        unsafe { revc_save_block(i, buf.as_mut_ptr(), &mut size) };
        size
    });
    let bytes = img.data.into_boxed_slice();
    unsafe {
        *len = bytes.len();
        *checksum = img.checksum;
        *out = Box::into_raw(bytes) as *mut u8;
    }
}

/// # Safety
/// `p`/`len` from `revc_save_build`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_free(p: *mut u8, len: usize) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len)) });
    }
}

/// GenericLoad's walk over a save file read into memory. Returns 1, or 0 on a read error (the
/// blocks before it were loaded).
///
/// # Safety
/// `data` points to `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_load(data: *const u8, len: usize) -> i32 {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let r = load(data, &mut |v| unsafe { revc_save_apply_vars(v) }, &mut |i, chunk, offset, size| unsafe {
        revc_load_block(i, chunk.as_ptr(), chunk.len(), offset, size as u32)
    });
    r.is_ok() as i32
}

/// RestoreForStartLoad's parse. Returns 0 if the data is too short.
///
/// # Safety
/// Pointers valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_start_info(data: *const u8, len: usize, level: *mut i32, pos: *mut f32) -> i32 {
    match start_info(unsafe { std::slice::from_raw_parts(data, len) }) {
        Some((l, p)) => unsafe {
            *level = l;
            std::ptr::copy_nonoverlapping(p.as_ptr(), pos, 3);
            1
        },
        None => 0,
    }
}

/// PopulateSlotInfo for one slot. `file`/`len`: the slot file's first bytes (null: no file).
/// `check(slot)` is CheckDataNotCorrupt plus the save fix. Writes the slot's state (SLOT_OK 0,
/// SLOT_EMPTY 1, SLOT_CORRUPTED 2), name (260 units) and date (70 units).
///
/// # Safety
/// Pointers valid; `name` holds 260 units, `date` 70.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_slot_info(
    slot: i32,
    file: *const u8,
    len: usize,
    check: unsafe extern "C" fn(i32) -> i32,
    japanese: i32,
    state: *mut i32,
    name: *mut u16,
    date: *mut u16,
) {
    let file = (!file.is_null()).then(|| unsafe { std::slice::from_raw_parts(file, len) });
    let name = unsafe { std::slice::from_raw_parts_mut(name, 260) };
    let date = unsafe { std::slice::from_raw_parts_mut(date, 70) };
    name[0] = 0;
    date[0] = 0;
    let put = |dst: &mut [u16], src: &[u16]| {
        let n = src.len().min(dst.len() - 1);
        dst[..n].copy_from_slice(&src[..n]);
        dst[n] = 0;
    };
    let info = slot_info(file, &mut || unsafe { check(slot) } != 0, &text, japanese != 0);
    unsafe {
        *state = match info {
            Slot::Ok { name: n, date: d } => {
                // SlotFileName gets all 24 header units, then a terminator at 24.
                name[..24].copy_from_slice(&n);
                name[24] = 0;
                put(date, &d.bytes().map(|b| b as u16).collect::<Vec<u16>>());
                0
            }
            Slot::Empty => 1,
            Slot::Corrupted => {
                put(name, &textutil::insert_number(&text("FEC_SLC"), [slot + 1, -1, -1, -1, -1, -1], japanese != 0));
                2
            }
        };
    }
}

/// CDate::operator> / < / == as one comparison: -1, 0 or 1.
///
/// # Safety
/// Pointers valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_date_compare(a: *const Date, b: *const Date) -> i32 {
    match unsafe { (*a).key().cmp(&(*b).key()) } {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_save() -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/saves/GTAVCsf1.b")).unwrap()
    }

    /// Splits a save file into its header, scripts block and system blocks (by the loader).
    fn split(data: &[u8]) -> (SimpleVars, Vec<Vec<u8>>) {
        let mut vars = None;
        let mut blocks = Vec::new();
        load(data, &mut |v| vars = Some(*v), &mut |_, c, o, n| blocks.push(c[o..o + n].to_vec())).unwrap();
        (vars.unwrap(), blocks)
    }

    /// The original file with the bytes the C++ filled with leftover memory zeroed (header field
    /// padding; each block's bytes between its size and the next multiple of 4) and the checksum
    /// recomputed.
    fn normalised_original(data: &[u8]) -> Vec<u8> {
        let mut d = data.to_vec();
        let (vars, _) = split(data);
        // Header: rewrite it from the parsed values (zero padding).
        d[4..4 + SIZE_OF_SIMPLE_VARS].copy_from_slice(&vars.to_bytes());
        let word = |d: &[u8], at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap()) as usize;
        // Scripts block (inside chunk 0) and the 22 blocks: [chunk size][inner size][data].
        let mut inner_at = vec![4 + SIZE_OF_SIMPLE_VARS];
        let mut at = 4 + align4(word(&d, 0) as u32) as usize;
        for _ in 0..BLOCK_COUNT {
            inner_at.push(at + 4);
            at += 4 + align4(word(&d, at) as u32) as usize;
        }
        for p in inner_at {
            let inner = word(&d, p);
            let end = p + 4 + align4(inner as u32) as usize;
            d[p + 4 + inner..end].fill(0);
        }
        let n = d.len() - 4;
        // Checksum: every byte before it.
        let sum = d[..n].iter().fold(0i32, |s, &b| s.wrapping_add(b as i32));
        d[n..].copy_from_slice(&sum.to_le_bytes());
        d
    }

    #[test]
    fn header_layout() {
        assert_eq!(std::mem::size_of::<SimpleVars>(), 212); // RevcSimpleVars in GenericGameStorage.cpp
        let data = test_save();
        let (v, blocks) = split(&data);
        assert_eq!(blocks.len(), 23);
        assert_eq!(v.size_and_quicksave, SIZE_OF_ONE_GAME as i32);
        assert_eq!(v.level, 1);
        assert_eq!(v.ms_per_game_minute, 1000);
        assert_eq!(v.hours, 4);
        assert_eq!((v.old_weather, v.new_weather), (4, 4));
        assert_eq!(v.radio_positions[0], 1039111);
        assert_eq!(String::from_utf16_lossy(&v.name[..19]), "In the beginning...");
        assert_eq!(v.time[0..2], [2026, 11]);
        assert_eq!(SimpleVars::from_bytes(&v.to_bytes()), Some(v));
        assert_eq!(start_info(&data), Some((1, v.camera)));
    }

    /// Writing the blocks of the test save back gives the same file (padding chunks, chunk
    /// framing and checksum as the C++ wrote them), up to the leftover-memory header bytes.
    #[test]
    fn rewrite_matches_original() {
        let data = test_save();
        let (vars, blocks) = split(&data);
        let img = build_save(&vars, &mut |i, buf| {
            let b = &blocks[(i + 1) as usize];
            buf[..b.len()].copy_from_slice(b);
            b.len() as u32
        });
        let want = normalised_original(&data);
        assert_eq!(img.data.len(), want.len());
        let first_diff = img.data.iter().zip(&want).position(|(a, b)| a != b);
        assert_eq!(first_diff, None, "differs from the normalised original at byte {first_diff:?}");
        // And the game's own check accepts it.
        assert!(revc_formats::save::check(&img.data).is_some());
    }

    #[test]
    fn truncated_file_stops_with_read_error() {
        let data = test_save();
        let mut loaded = 0;
        let r = load(&data[..100_000], &mut |_| {}, &mut |_, _, _, _| loaded += 1);
        assert_eq!(r, Err(LoadError::Read));
        assert!(loaded > 1 && loaded < 23);
    }

    #[test]
    fn names_and_dates() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let n = save_name(&w("In the beginning..."));
        assert_eq!(&n[..20], &[w("In the beginning..."), vec![0]].concat()[..]);
        let long = save_name(&w("A VERY LONG MISSION TITLE INDEED"));
        assert_eq!(String::from_utf16_lossy(&long[..21]), "A VERY LONG MISSIO...");
        assert_eq!(&long[18..22], &w("...\0")[..]);
        let exact = save_name(&w("TWENTY-THREE CHARS LONG"));
        assert_eq!(String::from_utf16_lossy(&exact[..21]), "TWENTY-THREE CHARS...");
        let short = save_name(&w("TWENTY-TWO CHARS LONG!"));
        assert_eq!(String::from_utf16_lossy(&short[..22]), "TWENTY-TWO CHARS LONG!");
        let month = |k: &str| w(&format!("<{k}>"));
        assert_eq!(slot_date(&[2026, 11, 0, 8, 20, 57, 27, 0], &month, false), "08 <NOV> 2026 20:57:27");
        assert_eq!(slot_date(&[2026, 1, 0, 1, 0, 5, 9, 0], &month, true), "01 01 2026 00:05:09");
        assert_eq!(slot_date(&[2026, 13, 0, 1, 0, 5, 9, 0], &month, false), "01  2026 00:05:09");
    }

    #[test]
    fn slots() {
        let data = test_save();
        let month = |k: &str| k.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(slot_info(None, &mut || true, &month, false), Slot::Empty);
        assert_eq!(slot_info(Some(b"THIS FILE IS NOT VALID YET..."), &mut || true, &month, false), Slot::Empty);
        assert_eq!(slot_info(Some(&data), &mut || false, &month, false), Slot::Corrupted);
        match slot_info(Some(&data), &mut || true, &month, false) {
            Slot::Ok { name, date } => {
                assert_eq!(String::from_utf16_lossy(&name[..19]), "In the beginning...");
                assert_eq!(date, "08 NOV 2026 20:57:27");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dates_compare() {
        let d = |y, mo, da, h, mi, s| Date { year: y, month: mo, day: da, hour: h, minute: mi, second: s };
        let a = d(2026, 11, 9, 20, 0, 0);
        unsafe {
            assert_eq!(revc_date_compare(&a, &a), 0);
            assert_eq!(revc_date_compare(&a, &d(2026, 11, 9, 20, 0, 1)), -1);
            assert_eq!(revc_date_compare(&d(2027, 1, 1, 0, 0, 0), &a), 1);
            assert_eq!(revc_date_compare(&d(2026, 10, 30, 23, 59, 59), &a), -1);
        }
    }
}

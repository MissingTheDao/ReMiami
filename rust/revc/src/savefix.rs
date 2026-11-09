//! Saves from other builds of the game (FIX_INCOMPATIBLE_SAVES in the C++): GetSaveType tells a
//! file's build (pointer width, compiler, Steam) from its bytes, FixSave rewrites the blocks whose
//! layout differs into this build's (the 32-bit MSVC one).
//!
//! Each block conversion is written once, as moves from the other build's layout to ours
//! (`Conv`). The tests run the same moves backwards to make other-build files from a real save.

use crate::savegame::{SaveImage, align4};

pub const TYPE_32_BIT: u8 = 1;
pub const TYPE_64_BIT: u8 = 2;
pub const TYPE_MSVC: u8 = 4;
pub const TYPE_GCC: u8 = 8;
pub const TYPE_STEAM: u8 = 16;

/// Offset of the Steam byte in the first block (after the save name, the date, the level, the
/// camera position).
const STEAM_AT: usize = 0x40 + 4 + 4 + 12;

const TOTAL_COLLECTCARS_GARAGES: usize = 4;
const TOTAL_HIDEOUT_GARAGES: usize = 12;
const NUM_GARAGE_STORED_CARS: usize = 4;
const NUM_GARAGES: usize = 32;
const STORED_CAR: usize = 40;
const NUM_CRANES: usize = 8;
const NUMPICKUPS: usize = 336;
const NUMCOLLECTEDPICKUPS: usize = 20;
const NUMPHONES: usize = 50;
const PLANE_NODE: usize = 20;

/// Garage block size the game writes (our garages are 7340 bytes of it).
const GARAGES_SAVE_SIZE: u32 = 7876;

/// The blocks in file order after the first; the ones a fix can apply to.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Fix {
    Garages,
    Cranes,
    Pickups,
    PhoneInfo,
    Particles,
    ScriptPaths,
}

/// Block index (0 = ped pool .. 21 = ped types) of each fixable block.
fn fix_of(index: usize) -> Option<Fix> {
    Some(match index {
        1 => Fix::Garages,
        6 => Fix::Cranes,
        7 => Fix::Pickups,
        8 => Fix::PhoneInfo,
        14 => Fix::Particles,
        16 => Fix::ScriptPaths,
        _ => return None,
    })
}

const BLOCKS: usize = 22;

fn steam_msvc_32(t: u8) -> bool {
    t & TYPE_STEAM != 0 && t & TYPE_MSVC != 0 && t & TYPE_32_BIT != 0
}

/// FixSave's choice for one block.
fn applies(fix: Fix, t: u8) -> bool {
    match fix {
        Fix::Garages => !steam_msvc_32(t),
        _ => t & TYPE_64_BIT != 0,
    }
}

/// Whether the file needs no fix (FixSave returns at once).
pub fn is_native(t: u8) -> bool {
    t & TYPE_32_BIT != 0 && t & TYPE_MSVC != 0 && t & TYPE_STEAM == 0
}

fn word(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn f32_at(d: &[u8], at: usize) -> Option<f32> {
    word(d, at).map(f32::from_bits)
}

/// Reads chunks as the C++ does: a size word, then that many bytes (aligned unless `exact`).
struct Chunks<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Chunks<'a> {
    fn next(&mut self, exact: bool) -> Option<&'a [u8]> {
        let size = word(self.data, self.pos)?;
        let n = if exact { size } else { align4(size) } as usize;
        let start = self.pos + 4;
        let chunk = self.data.get(start..start.checked_add(n)?)?;
        self.pos = start + n;
        Some(chunk)
    }
}

/// GetSaveType. None when the file ends before the cranes block (the C++ would read stale
/// memory; the file check before it rules that out).
pub fn save_type(data: &[u8]) -> Option<u8> {
    let mut t = 0u8;
    let mut chunks = Chunks { data, pos: 0 };
    // The first block is read with its size as it is, unaligned.
    let first = chunks.next(true)?;
    if *first.get(STEAM_AT)? as i8 == -3 {
        t |= TYPE_STEAM;
    }
    chunks.next(false)?; // ped pool
    let garages = chunks.next(false)?;
    let garages_size = word(garages, 0)? as usize;
    let garages = garages.get(4..4 + garages_size)?;
    for _ in 0..4 {
        chunks.next(false)?; // game logic, vehicles, objects, paths
    }
    let cranes = chunks.next(false)?;
    match word(cranes, 0)? {
        1000 => t |= TYPE_32_BIT,
        1160 => t |= TYPE_64_BIT,
        _ => {} // assert(0) in the original, a no-op in release builds
    }
    // The first garage's corner, at the place it has in MSVC builds; anywhere else it's GCC.
    let at = 1964 + if t & TYPE_32_BIT != 0 { 28 } else { 40 };
    let x = f32_at(garages, at)?;
    let y = f32_at(garages, at + 4)?;
    let inf_z = f32_at(garages, at + 8)?;
    let sup_z = f32_at(garages, at + 12 + 16)?;
    if x == -914.129028 && y == -1263.540039 && inf_z == 10.706000 && sup_z == 14.421000 {
        t |= TYPE_MSVC;
    } else {
        t |= TYPE_GCC;
    }
    Some(t)
}

/// One conversion between the other build's layout (`theirs`) and ours. Forward reads theirs and
/// writes ours; backward (tests) reads ours and writes theirs, filling bytes ours lacks with junk,
/// and keeps in `kept` what of ours survives a round trip.
pub(crate) struct Conv<'a> {
    src: &'a [u8],
    pos: usize,
    out: Vec<u8>,
    backward: bool,
    kept: Vec<u8>,
    junk: u32,
}

impl<'a> Conv<'a> {
    fn new(src: &'a [u8], backward: bool, seed: u32) -> Conv<'a> {
        Conv { src, pos: 0, out: Vec::new(), backward, kept: Vec::new(), junk: seed }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.src.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(b)
    }

    fn junk(&mut self, n: usize) {
        for _ in 0..n {
            self.junk = self.junk.wrapping_mul(1664525).wrapping_add(1013904223);
            self.out.push((self.junk >> 24) as u8);
        }
    }

    /// Bytes both layouts hold.
    fn copy(&mut self, n: usize) -> Option<()> {
        let b = self.take(n)?;
        self.out.extend_from_slice(b);
        self.kept.extend_from_slice(b);
        Some(())
    }

    /// An int both layouts hold; returns it.
    fn count(&mut self) -> Option<i32> {
        let v = i32::from_le_bytes(self.src.get(self.pos..self.pos + 4)?.try_into().unwrap());
        self.copy(4)?;
        Some(v)
    }

    /// Bytes only in theirs (padding, the upper half of pointers): dropped.
    fn theirs(&mut self, n: usize) -> Option<()> {
        if self.backward {
            self.junk(n);
        } else {
            self.take(n)?;
        }
        Some(())
    }

    /// Bytes only in ours: zeros.
    fn ours(&mut self, n: usize) -> Option<()> {
        if self.backward {
            self.take(n)?;
        } else {
            self.out.resize(self.out.len() + n, 0);
        }
        self.kept.resize(self.kept.len() + n, 0);
        Some(())
    }

    /// Bytes at the same place in both whose value doesn't carry over: zeros in ours.
    fn neither(&mut self, n: usize) -> Option<()> {
        if self.backward {
            self.take(n)?;
            self.junk(n);
        } else {
            self.take(n)?;
            self.out.resize(self.out.len() + n, 0);
        }
        self.kept.resize(self.kept.len() + n, 0);
        Some(())
    }

    /// A pointer: 8 bytes in theirs, ours keeps the low 4 (CopyPtr).
    fn ptr(&mut self) -> Option<()> {
        self.copy(4)?;
        self.theirs(4)
    }

    /// A pointer whose value is dropped (SkipPtr).
    fn dead_ptr(&mut self) -> Option<()> {
        self.theirs(4)?;
        self.neither(4)
    }

    /// GCC's 1-byte stored car flags, an int32 in ours.
    fn flags8(&mut self) -> Option<()> {
        if self.backward {
            let b = self.take(4)?;
            self.out.push(b[0]);
            self.kept.extend_from_slice(&(b[0] as i32).to_le_bytes());
        } else {
            let b = self.take(1)?[0] as i32;
            self.out.extend_from_slice(&b.to_le_bytes());
            self.kept.extend_from_slice(&b.to_le_bytes());
        }
        Some(())
    }
}

/// The moves of one block (src past its inner size word). Returns the inner size ours declares.
fn convert(fix: Fix, t: u8, c: &mut Conv) -> Option<u32> {
    let ptrsize = if t & TYPE_32_BIT != 0 { 4 } else { 8 };
    let gcc = t & TYPE_GCC != 0;
    match fix {
        Fix::Garages => {
            c.copy(4 * 6)?;
            c.copy(4 * TOTAL_COLLECTCARS_GARAGES)?;
            c.copy(4)?;
            for _ in 0..NUM_GARAGE_STORED_CARS * TOTAL_HIDEOUT_GARAGES {
                if gcc {
                    c.copy(4 + 12 + 12)?;
                    c.flags8()?;
                    c.copy(6)?;
                    c.theirs(1)?;
                    c.ours(2)?;
                } else {
                    c.copy(STORED_CAR)?;
                }
            }
            for _ in 0..NUM_GARAGES {
                c.copy(7)?;
                c.neither(1)?;
                c.copy(4)?;
                c.theirs(ptrsize - 4)?;
                c.theirs(ptrsize * 2)?;
                c.ours(4 * 2)?;
                c.copy(7)?;
                c.neither(1)?;
                c.copy(8 * 3 + 4 * 17 + 1)?;
                c.neither(3)?;
                c.theirs(ptrsize)?;
                c.ours(4)?;
                // The garage's own stored car (GCC: 36 bytes on 32-bit, 40 on 64-bit).
                c.theirs(if gcc && t & TYPE_64_BIT == 0 { 36 } else { STORED_CAR })?;
                c.ours(STORED_CAR)?;
            }
            Some(GARAGES_SAVE_SIZE)
        }
        Fix::Cranes => {
            c.copy(4 + 4)?;
            for _ in 0..NUM_CRANES {
                c.ptr()?;
                c.ptr()?;
                c.copy(14 * 4 + 12 * 3 + 8)?;
                c.theirs(4)?;
                c.ptr()?;
                c.copy(4 + 7)?;
                c.theirs(5)?;
                c.ours(1)?;
            }
            Some(2 * 4 + 0x3E0)
        }
        Fix::Pickups => {
            for _ in 0..NUMPICKUPS {
                c.copy(12 + 4)?;
                c.ptr()?;
                c.ptr()?;
                c.copy(4 * 2 + 2 * 3 + 8 + 3)?;
                c.theirs(7)?;
                c.ours(3)?;
            }
            c.copy(2)?;
            c.neither(2)?;
            c.copy(NUMCOLLECTEDPICKUPS * 4)?;
            Some((0x4440 + 2 + 2 + 4 * NUMCOLLECTEDPICKUPS) as u32)
        }
        Fix::PhoneInfo => {
            c.copy(4 + 4)?;
            for _ in 0..NUMPHONES {
                c.copy(12)?;
                c.theirs(4)?;
                for _ in 0..6 {
                    c.dead_ptr()?;
                }
                c.copy(4)?;
                c.theirs(4)?;
                c.ptr()?;
                c.copy(4 + 1)?;
                c.neither(3)?;
            }
            Some(0xA30)
        }
        Fix::Particles => {
            let n = c.count()?;
            for _ in 0..n.max(0) {
                // CPlaceable
                c.copy(4 * 4 * 4)?;
                c.dead_ptr()?;
                c.copy(1)?;
                c.theirs(7)?;
                c.ours(3)?;
                // CParticleObject
                for _ in 0..3 {
                    c.dead_ptr()?;
                }
                c.copy(4 * 3 + 2 + 2 * 2)?;
                c.neither(2)?;
                c.copy(12 + 2 * 4 + 4 + 2)?;
                c.neither(2)?;
            }
            // The list's end marker object.
            c.theirs(0x98)?;
            c.ours(0x84)?;
            Some((0x84 * (n + 1) + 4) as u32)
        }
        Fix::ScriptPaths => {
            let mut size = 0x9C;
            for _ in 0..3 {
                let nodes = c.count()?;
                c.theirs(4)?;
                c.dead_ptr()?;
                c.copy(4 * 5)?;
                c.theirs(4)?;
                for _ in 0..6 {
                    c.ptr()?;
                }
                for _ in 0..nodes.max(0) {
                    c.copy(PLANE_NODE)?;
                    size += PLANE_NODE as u32;
                }
            }
            Some(size)
        }
    }
}

/// FixSave: the file rewritten in this build's layout, or None when it can't be read through.
/// Callers skip files that `is_native`.
pub fn fix_save(data: &[u8], t: u8) -> Option<Vec<u8>> {
    let mut img = SaveImage::new();
    let mut chunks = Chunks { data, pos: 0 };
    let first = chunks.next(false)?;
    if steam_msvc_32(t) {
        // Drop the Steam word.
        let mut b = first.get(..STEAM_AT)?.to_vec();
        b.extend_from_slice(first.get(STEAM_AT + 4..)?);
        img.block(&b);
    } else {
        img.block(first);
    }
    for i in 0..BLOCKS {
        let block = chunks.next(false)?;
        match fix_of(i).filter(|&f| applies(f, t)) {
            Some(fix) => {
                let mut c = Conv::new(block.get(4..)?, false, 0);
                let size = convert(fix, t, &mut c)?;
                let mut b = size.to_le_bytes().to_vec();
                b.extend_from_slice(&c.out);
                b.resize(b.len().max(4 + align4(size) as usize), 0);
                b.truncate(4 + align4(size) as usize);
                img.block(&b);
            }
            None => img.block(block),
        }
    }
    img.finish();
    Some(img.data)
}

/// The reverse of fix_save, for tests: one of our saves as a build of type `t` would have written
/// it, and ours with what doesn't survive the round trip zeroed.
#[cfg(test)]
pub(crate) fn unfix_save(data: &[u8], t: u8, seed: u32) -> (Vec<u8>, Vec<u8>) {
    let mut theirs = SaveImage::new();
    let mut kept = SaveImage::new();
    let mut chunks = Chunks { data, pos: 0 };
    let first = chunks.next(false).unwrap();
    kept.block(first);
    if t & TYPE_STEAM != 0 {
        let mut b = first[..STEAM_AT].to_vec();
        b.extend_from_slice(&[0xFD, 0x11, 0x22, 0x33]);
        b.extend_from_slice(&first[STEAM_AT..]);
        theirs.block(&b);
    } else {
        theirs.block(first);
    }
    for i in 0..BLOCKS {
        let block = chunks.next(false).unwrap();
        match fix_of(i).filter(|&f| applies(f, t)) {
            Some(fix) => {
                let mut c = Conv::new(&block[4..], true, seed.wrapping_add(i as u32));
                let size = convert(fix, t, &mut c).unwrap();
                let mut b = (c.out.len() as u32).to_le_bytes().to_vec();
                b.extend_from_slice(&c.out);
                theirs.block(&b);
                let mut k = size.to_le_bytes().to_vec();
                k.extend_from_slice(&c.kept);
                k.resize(4 + align4(size) as usize, 0);
                kept.block(&k);
            }
            None => {
                theirs.block(block);
                kept.block(block);
            }
        }
    }
    theirs.finish();
    kept.finish();
    (theirs.data, kept.data)
}

/// GetSaveType then FixSave over a file's bytes. Returns 0 when the file can't be fixed, 1 when it
/// is already this build's, 2 with the rewritten file in `out` (free with revc_save_free).
///
/// # Safety
/// `data` points to `len` bytes; `out` and `out_len` are valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_save_fix(data: *const u8, len: usize, out: *mut *mut u8, out_len: *mut usize) -> i32 {
    if data.is_null() {
        return 0;
    }
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let Some(t) = save_type(data) else { return 0 };
    if is_native(t) {
        return 1;
    }
    let Some(fixed) = fix_save(data, t) else { return 0 };
    let b = fixed.into_boxed_slice();
    unsafe {
        *out_len = b.len();
        *out = Box::into_raw(b) as *mut u8;
    }
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_save() -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/saves/GTAVCsf1.b")).unwrap()
    }

    /// The builds there are (the Steam release is 32-bit MSVC only).
    const TYPES: [u8; 4] = [
        TYPE_32_BIT | TYPE_GCC,
        TYPE_64_BIT | TYPE_MSVC,
        TYPE_64_BIT | TYPE_GCC,
        TYPE_32_BIT | TYPE_MSVC | TYPE_STEAM,
    ];

    #[test]
    fn our_save_is_native() {
        let t = save_type(&test_save()).unwrap();
        assert_eq!(t, TYPE_32_BIT | TYPE_MSVC);
        assert!(is_native(t));
    }

    /// Block sizes the C++ asserts for the other builds' layouts.
    #[test]
    fn their_block_sizes() {
        let data = test_save();
        let sizes = |t: u8| {
            let (theirs, _) = unfix_save(&data, t, 1);
            let mut chunks = Chunks { data: &theirs, pos: 0 };
            chunks.next(false).unwrap();
            (0..BLOCKS).map(|_| word(chunks.next(false).unwrap(), 0).unwrap()).collect::<Vec<u32>>()
        };
        let s = sizes(TYPE_64_BIT | TYPE_MSVC);
        assert_eq!(s[1], 7852);
        assert_eq!(s[6], 8 + 0x480);
        assert_eq!(s[7], 0x5400 + 4 + 4 * 20);
        assert_eq!(s[8], 0x1138);
        assert_eq!(sizes(TYPE_64_BIT | TYPE_GCC)[1], 7660);
        assert_eq!(sizes(TYPE_32_BIT | TYPE_GCC)[1], 7020);
    }

    #[test]
    fn detects_other_builds() {
        let data = test_save();
        for t in TYPES {
            let (theirs, _) = unfix_save(&data, t, 7);
            assert_eq!(save_type(&theirs), Some(t), "type {t:#x}");
        }
    }

    /// Their file, fixed, is ours again (bar the bytes no layout conversion keeps), and passes the
    /// game's file check.
    #[test]
    fn fixes_other_builds() {
        let data = test_save();
        for t in TYPES {
            for seed in [1, 99] {
                let (theirs, kept) = unfix_save(&data, t, seed);
                assert!(revc_formats::save::check(&theirs).is_some(), "their file {t:#x} fails the check");
                let fixed = fix_save(&theirs, t).unwrap();
                assert_eq!(fixed.len(), kept.len(), "type {t:#x}");
                if let Some(at) = fixed.iter().zip(&kept).position(|(a, b)| a != b) {
                    panic!("type {t:#x}: differs at {at:#x}");
                }
                assert!(revc_formats::save::check(&fixed).is_some());
                assert_eq!(save_type(&fixed), Some(TYPE_32_BIT | TYPE_MSVC));
            }
        }
    }

    #[test]
    fn short_file_fails() {
        let (theirs, _) = unfix_save(&test_save(), TYPE_64_BIT | TYPE_MSVC, 3);
        assert_eq!(fix_save(&theirs[..30000], TYPE_64_BIT | TYPE_MSVC), None);
        assert_eq!(save_type(&theirs[..100]), None);
    }

    const SEEDS: [u32; 2] = [1, 99];

    fn fnv(d: &[u8]) -> u64 {
        d.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    }

    /// Writes the other-build files to $REVC_SAVEFIX_DUMP (for record.sh).
    #[test]
    fn dump() {
        let Ok(dir) = std::env::var("REVC_SAVEFIX_DUMP") else { return };
        let data = test_save();
        std::fs::write(format!("{dir}/native.b"), &data).unwrap();
        for t in TYPES {
            for seed in SEEDS {
                std::fs::write(format!("{dir}/t{t}-s{seed}.b"), unfix_save(&data, t, seed).0).unwrap();
            }
        }
    }

    /// tests/golden/savefix.txt: what the original C++ GetSaveType and FixSave made of each file.
    #[test]
    fn matches_cpp_golden() {
        let golden = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/savefix.txt")).unwrap();
        let data = test_save();
        let mut n = 0;
        for line in golden.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let input = if f[0] == "native.b" {
                data.clone()
            } else {
                let (t, seed) = f[0].trim_end_matches(".b").trim_start_matches('t').split_once("-s").unwrap();
                unfix_save(&data, t.parse().unwrap(), seed.parse().unwrap()).0
            };
            let t = save_type(&input).unwrap();
            assert_eq!(t.to_string(), f[1], "{}", f[0]);
            assert_eq!(f[2], "1");
            let out = if is_native(t) { input } else { fix_save(&input, t).unwrap() };
            assert_eq!((format!("{:016x}", fnv(&out)), out.len().to_string()), (f[3].to_string(), f[4].to_string()), "{}", f[0]);
            n += 1;
        }
        assert_eq!(n, 1 + TYPES.len() * SEEDS.len());
    }
}

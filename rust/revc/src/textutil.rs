//! The game's wide-string helpers (plan task 15, text): CMessages::GetWideStringLength /
//! WideStringCopy / WideStringCompare / InsertNumberInString / InsertStringInString and Text.cpp's
//! UnicodeToAscii* and upper-casing, with the original behaviour (checked against the C++ with
//! tests/golden/textutil.txt).
//!
//! Strings are the game's `wchar` (u16) arrays, NUL-terminated. Where the C++ reads past the end
//! of a string (the token checks look two units ahead; InsertStringInString copies up to
//! len(a) + len(b) units) these read zeros, as the game's zero-filled 256-unit buffers give.

use std::ffi::c_char;

/// Token unit: in Japanese text every ASCII character has bit 15 set.
fn tok(c: u8, japanese: bool) -> u16 {
    if japanese { 0x8000 | c as u16 } else { c as u16 }
}

/// Unit `i` of `s`, zero past its end.
fn at(s: &[u16], i: usize) -> u16 {
    s.get(i).copied().unwrap_or(0)
}

/// Wide string up to (not including) its NUL.
///
/// # Safety
/// `p` must be null or point to a NUL-terminated u16 string.
pub unsafe fn wide<'a>(p: *const u16) -> &'a [u16] {
    if p.is_null() {
        return &[];
    }
    let mut n = 0;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    unsafe { std::slice::from_raw_parts(p, n) }
}

/// CMessages::InsertNumberInString: each `~1~` takes the next of `nums` (printed with %d).
pub fn insert_number(s: &[u16], nums: [i32; 6], japanese: bool) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len() + 16);
    let (t, one) = (tok(b'~', japanese), tok(b'1', japanese));
    let mut next = 0;
    let mut c = 0;
    while c < s.len() {
        if s[c] == t && at(s, c + 1) == one && at(s, c + 2) == t {
            c += 3;
            // After the sixth number the C++ keeps re-inserting n6.
            out.extend(nums[next.min(5)].to_string().encode_utf16());
            next += 1;
        } else {
            out.push(s[c]);
            c += 1;
        }
    }
    out
}

/// CMessages::InsertStringInString: every `~a~` in `a` becomes `b`; the result is cut at
/// len(a) + len(b) units (as the original's loop bound) and the rest of the 256-unit buffer is
/// zeroed by the caller.
pub fn insert_string(a: &[u16], b: &[u16], japanese: bool) -> Vec<u16> {
    let total = a.len() + b.len();
    let (jt, ja) = (tok(b'~', true), tok(b'a', true));
    let mut out = Vec::with_capacity(total);
    let mut p = 0;
    while out.len() < total {
        let token = (at(a, p) == u16::from(b'~') && at(a, p + 1) == u16::from(b'a') && at(a, p + 2) == u16::from(b'~'))
            || (japanese && at(a, p) == jt && at(a, p + 1) == ja && at(a, p + 2) == jt);
        if token {
            p += 3;
            out.extend_from_slice(b);
        } else {
            out.push(at(a, p));
            p += 1;
        }
    }
    out.truncate(total);
    out
}

/// CMessages::WideStringCompare (with FixupChar: Japanese ignores bit 15).
pub fn compare(a: &[u16], b: &[u16], size: u16, japanese: bool) -> bool {
    let size = size as usize;
    let (l1, l2) = (a.len(), b.len());
    if l1 != l2 && (l1 < size || l2 < size) {
        return false;
    }
    let fix = |c: u16| if japanese { c & 0x7fff } else { c };
    for i in 0..size {
        if fix(at(a, i)) == 0 {
            break;
        }
        if fix(at(a, i)) != fix(at(b, i)) {
            return false;
        }
    }
    true
}

/// UnicodeToAscii: game charset to CP1252 for the PC's ASCII paths (at most 255 characters).
/// The C++ stores the sums in a `char`, so values past 255 wrap; one that wraps to 0 (game char
/// 176) ends the string there.
pub fn to_ascii(s: &[u16], russian_or_japanese: bool) -> Vec<u8> {
    let mut out: Vec<u8> = s
        .iter()
        .take(255)
        .map(|&c| {
            let c32 = c as u32;
            (if c < 128 || (russian_or_japanese && c < 256) {
                c32
            } else if c <= 131 {
                c32 + 64
            } else if c <= 141 {
                c32 + 66
            } else if c <= 145 {
                c32 + 68
            } else if c <= 149 {
                c32 + 71
            } else if c <= 154 {
                c32 + 73
            } else if c <= 164 {
                c32 + 75
            } else if c <= 168 {
                c32 + 77
            } else if c <= 204 {
                c32 + 80
            } else {
                match c {
                    205 => 209,
                    206 => 241,
                    207 => 191,
                    _ => b'#' as u32,
                }
            }) as u8
        })
        .collect();
    if let Some(end) = out.iter().position(|&b| b == 0) {
        out.truncate(end);
    }
    out
}

/// UnicodeToAsciiForSaveLoad / ForMemoryCard: Latin-1, '#' for anything above. (The originals
/// could write one byte past their 256-byte buffer; this stops at 255 characters.)
pub fn to_ascii_saveload(s: &[u16]) -> Vec<u8> {
    s.iter().take(255).map(|&c| if c < 256 { c as u8 } else { b'#' }).collect()
}

/// CText::GetUpperCase for the text encoding ('e' English, 'f' French, 'g'/'i'/'s').
pub fn upper(c: u16, encoding: u8) -> u16 {
    const UPPER: [u8; 128] = [
        128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 128,
        129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 173, 173, 175,
        176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199,
        200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
        224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247,
        248, 249, 250, 251, 252, 253, 254, 255,
    ];
    const FRENCH: [u8; 128] = [
        128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 65, 65,
        65, 65, 132, 133, 69, 69, 69, 69, 73, 73, 73, 73, 79, 79, 79, 79, 85, 85, 85, 85, 173, 173, 175, 176, 177, 178, 179, 180,
        181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204,
        205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228,
        229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252,
        253, 254, 255,
    ];
    let ascii = (u16::from(b'a')..=u16::from(b'z')).contains(&c);
    match encoding {
        b'e' | b'f' | b'g' | b'i' | b's' if ascii => c - 32,
        b'f' if (128..=255).contains(&c) => FRENCH[(c - 128) as usize] as u16,
        b'g' | b'i' | b's' if (128..=255).contains(&c) => UPPER[(c - 128) as usize] as u16,
        _ => c,
    }
}

// ---------------------------------------------------------------- C entry points (Messages.cpp, Text.cpp)

fn write_wide(dst: *mut u16, s: &[u16]) {
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
        *dst.add(s.len()) = 0;
    }
}

/// # Safety
/// `s` NUL-terminated (or null); `out` large enough for the result and its NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_insert_number(s: *const u16, nums: *const i32, japanese: i32, out: *mut u16) {
    let nums: [i32; 6] = unsafe { std::slice::from_raw_parts(nums, 6) }.try_into().unwrap();
    let r = if s.is_null() { Vec::new() } else { insert_number(unsafe { wide(s) }, nums, japanese != 0) };
    write_wide(out, &r);
}

/// In place in `a` (a 256-unit buffer): the result, then zeros to unit 256.
///
/// # Safety
/// `a` points to 256 units holding a NUL-terminated string; `b` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_insert_string(a: *mut u16, b: *const u16, japanese: i32) {
    if a.is_null() || b.is_null() {
        return;
    }
    let r = insert_string(unsafe { wide(a) }, unsafe { wide(b) }, japanese != 0);
    let buf = unsafe { std::slice::from_raw_parts_mut(a, 256) };
    let n = r.len().min(256);
    buf[..n].copy_from_slice(&r[..n]);
    buf[n..].fill(0);
}

/// # Safety
/// Both NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_compare(a: *const u16, b: *const u16, size: u16, japanese: i32) -> i32 {
    compare(unsafe { wide(a) }, unsafe { wide(b) }, size, japanese != 0) as i32
}

/// # Safety
/// `s` NUL-terminated; `out` has room for 256 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_to_ascii(s: *const u16, mode: i32, out: *mut c_char) {
    let w = unsafe { wide(s) };
    let r = match mode {
        0 => to_ascii(w, false),
        1 => to_ascii(w, true),
        _ => to_ascii_saveload(w),
    };
    unsafe {
        std::ptr::copy_nonoverlapping(r.as_ptr() as *const c_char, out, r.len());
        *out.add(r.len()) = 0;
    }
}

/// CMessages::GetWideStringLength (u16 like the original).
///
/// # Safety
/// `s` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_len(s: *const u16) -> u16 {
    unsafe { wide(s) }.len() as u16
}

/// CMessages::WideStringCopy: at most `size - 1` units then a NUL; a null `src` clears that many.
///
/// # Safety
/// `dst` has room for `size` units; `src` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_copy(dst: *mut u16, src: *const u16, size: u16) {
    if size == 0 {
        // The original then writes dst[0] = 0 after the (empty) loop.
        unsafe { *dst = 0 };
        return;
    }
    let max = size as usize - 1;
    let n = if src.is_null() {
        unsafe { std::ptr::write_bytes(dst, 0, max) };
        max
    } else {
        let s = unsafe { wide(src) };
        let n = s.len().min(max);
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), dst, n) };
        n
    };
    unsafe { *dst.add(n) = 0 };
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_text_upper(c: u16, encoding: c_char) -> u16 {
    upper(c, encoding as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn len_and_copy() {
        let s: Vec<u16> = w("hello").into_iter().chain([0]).collect();
        assert_eq!(unsafe { revc_text_len(s.as_ptr()) }, 5);
        let mut d = [9u16; 8];
        unsafe { revc_text_copy(d.as_mut_ptr(), s.as_ptr(), 4) };
        assert_eq!(&d[..5], &[b'h' as u16, b'e' as u16, b'l' as u16, 0, 9]);
        unsafe { revc_text_copy(d.as_mut_ptr(), std::ptr::null(), 3) };
        assert_eq!(&d[..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn basics() {
        assert_eq!(insert_number(&w("Cash ~1~ of ~1~"), [5, -2, 0, 0, 0, 0], false), w("Cash 5 of -2"));
        // len(a) + len(b) units: the text, then zeros (the caller's buffer is zero-filled).
        let mut want = w("Hi Tommy!");
        want.resize(12, 0);
        assert_eq!(insert_string(&w("Hi ~a~!"), &w("Tommy"), false), want);
        assert!(compare(&w("ABC"), &w("ABC"), 3, false));
        assert!(!compare(&w("ABC"), &w("ABD"), 3, false));
        assert_eq!(to_ascii(&[0xE9], false), vec![b'#']);
        assert_eq!(upper(u16::from(b'q'), b'e'), u16::from(b'Q'));
    }

    /// Characterisation data recorded from the original C++ (before it was replaced; see
    /// tests/golden/README.md).
    #[test]
    fn matches_cpp_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/textutil.txt");
        let data = std::fs::read_to_string(&path).expect("tests/golden/textutil.txt");
        let mut n = 0;
        for line in data.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (input, want) = line.split_once(" => ").unwrap();
            let got = golden_case(input);
            assert_eq!(got, want, "case {input}");
            n += 1;
        }
        assert!(n > 1000, "only {n} golden cases");
    }

    fn hex_units(s: &str) -> Vec<u16> {
        if s == "-" { Vec::new() } else { s.split(',').map(|x| u16::from_str_radix(x, 16).unwrap()).collect() }
    }

    fn units_hex(v: &[u16]) -> String {
        if v.is_empty() { "-".into() } else { v.iter().map(|u| format!("{u:x}")).collect::<Vec<_>>().join(",") }
    }

    fn bytes_hex(v: &[u8]) -> String {
        if v.is_empty() { "-".into() } else { v.iter().map(|u| format!("{u:x}")).collect::<Vec<_>>().join(",") }
    }

    /// One golden line's input ("op args...") evaluated with the Rust code, formatted like the
    /// recorded C++ output.
    fn golden_case(input: &str) -> String {
        let f: Vec<&str> = input.split(' ').collect();
        let jp = f[1] == "1";
        match f[0] {
            "num" => {
                let nums: Vec<i32> = f[3].split(',').map(|x| x.parse().unwrap()).collect();
                units_hex(&insert_number(&hex_units(f[2]), nums.try_into().unwrap(), jp))
            }
            "str" => units_hex(&insert_string(&hex_units(f[2]), &hex_units(f[3]), jp)),
            "cmp" => (compare(&hex_units(f[2]), &hex_units(f[3]), f[4].parse().unwrap(), jp) as i32).to_string(),
            "asc" => bytes_hex(&to_ascii(&hex_units(f[2]), jp)),
            "asv" => bytes_hex(&to_ascii_saveload(&hex_units(f[2]))),
            "up" => format!("{:x}", upper(u16::from_str_radix(f[2], 16).unwrap(), f[1].as_bytes()[0])),
            op => panic!("unknown op {op}"),
        }
    }
}

//! The game's text (GXT), plan task 15: CText (text/Text.cpp) keeps its interface but its tables
//! live here, parsed by revc-formats::gxt.
//!
//! Semantics kept from the C++ (CKeyArray::BinarySearch over sorted keys, CText::Get,
//! CText::LoadMissionText):
//! - keys are compared exactly (at most 8 bytes); the MAIN table is searched first, then the
//!   mission table if one is loaded;
//! - a mission table is chosen by the first table (in TABL order) whose name is a prefix of the
//!   requested name (`strncmp(table, wanted, strlen(table))`);
//! - returned strings are NUL-terminated u16 arrays that stay valid until the next load.
//!   (The original freed mission text on every LoadMissionText; here all tables of the file stay
//!   in memory, about 0.5 MB, so older pointers stay valid longer, never shorter.)

use revc_formats::gxt::Gxt;
use std::collections::HashMap;
use std::ffi::{CString, c_char, c_int};
use std::sync::Mutex;

struct Table {
    name: String,
    /// Keys in file order (for the test dump) with their NUL-terminated strings.
    keys: Vec<CString>,
    strings: Vec<Box<[u16]>>,
    index: HashMap<Vec<u8>, usize>,
}

impl Table {
    fn get(&self, key: &[u8]) -> Option<*const u16> {
        self.index.get(key).map(|&i| self.strings[i].as_ptr())
    }

    /// The string for `key` without its NUL.
    fn text(&self, key: &[u8]) -> Option<&[u16]> {
        self.index.get(key).map(|&i| &self.strings[i][..self.strings[i].len() - 1])
    }
}

struct Store {
    tables: Vec<Table>,
    main: Option<usize>,
    mission: Option<usize>,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn build(g: Gxt) -> Store {
    let tables: Vec<Table> = g
        .tables
        .into_iter()
        .map(|t| {
            let mut keys = Vec::with_capacity(t.entries.len());
            let mut strings = Vec::with_capacity(t.entries.len());
            let mut index = HashMap::with_capacity(t.entries.len());
            for (i, (k, units)) in t.entries.into_iter().enumerate() {
                index.entry(k.as_bytes().to_vec()).or_insert(i);
                keys.push(CString::new(k).unwrap_or_default());
                let mut s = units;
                s.push(0);
                strings.push(s.into_boxed_slice());
            }
            Table { name: t.name, keys, strings, index }
        })
        .collect();
    let main = tables.iter().position(|t| t.name == "MAIN");
    Store { tables, main, mission: None }
}

/// Key bytes as the C++ compares them: up to the NUL, at most 8 bytes.
fn key_bytes(key: *const c_char) -> Vec<u8> {
    if key.is_null() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(8);
    for i in 0..8 {
        let b = unsafe { *key.add(i) } as u8;
        if b == 0 {
            break;
        }
        out.push(b);
    }
    out
}

/// Parses a whole GXT file (read by the C++ through CFileMgr) and makes it the current text.
/// Returns 0 if the data isn't a valid GXT file; the previous text is then kept.
///
/// # Safety
/// `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_text_load(data: *const u8, len: usize) -> c_int {
    if data.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    match Gxt::parse(bytes) {
        Ok(g) => {
            *STORE.lock().unwrap() = Some(build(g));
            1
        }
        Err(e) => {
            eprintln!("text: can't parse GXT: {e}");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_text_unload() {
    *STORE.lock().unwrap() = None;
}

/// The string for `key` (MAIN table, then the mission table), or null if there is none.
#[unsafe(no_mangle)]
pub extern "C" fn revc_text_get(key: *const c_char) -> *const u16 {
    let k = key_bytes(key);
    let guard = STORE.lock().unwrap();
    let Some(s) = guard.as_ref() else { return std::ptr::null() };
    s.main
        .and_then(|m| s.tables[m].get(&k))
        .or_else(|| s.mission.and_then(|m| s.tables[m].get(&k)))
        .unwrap_or(std::ptr::null())
}

/// Selects the mission table for `name` (see the module doc for the matching rule). Returns 0 and
/// leaves no mission table selected if there is no such table.
#[unsafe(no_mangle)]
pub extern "C" fn revc_text_load_mission(name: *const c_char) -> c_int {
    let wanted = key_bytes(name);
    let mut guard = STORE.lock().unwrap();
    let Some(s) = guard.as_mut() else { return 0 };
    s.mission = s
        .tables
        .iter()
        .position(|t| wanted.starts_with(t.name.as_bytes()));
    s.mission.is_some() as c_int
}

/// Number of keys in the MAIN table (regression test dump).
#[unsafe(no_mangle)]
pub extern "C" fn revc_text_num_keys() -> c_int {
    let guard = STORE.lock().unwrap();
    guard.as_ref().and_then(|s| s.main.map(|m| s.tables[m].keys.len() as c_int)).unwrap_or(0)
}

/// Key `i` of the MAIN table in file order, or null.
#[unsafe(no_mangle)]
pub extern "C" fn revc_text_key(i: c_int) -> *const c_char {
    let guard = STORE.lock().unwrap();
    guard
        .as_ref()
        .and_then(|s| s.main.and_then(|m| s.tables[m].keys.get(i as usize)).map(|k| k.as_ptr()))
        .unwrap_or(std::ptr::null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn chunk(out: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
        out.extend_from_slice(magic);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
    }

    fn name8(s: &str) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    }

    fn body(entries: &[(&str, &str)]) -> Vec<u8> {
        let (mut keys, mut dat) = (Vec::new(), Vec::new());
        for (k, v) in entries {
            keys.extend_from_slice(&(dat.len() as u32).to_le_bytes());
            keys.extend_from_slice(&name8(k));
            for c in v.encode_utf16().chain([0]) {
                dat.extend_from_slice(&c.to_le_bytes());
            }
        }
        let mut out = Vec::new();
        chunk(&mut out, b"TKEY", &keys);
        chunk(&mut out, b"TDAT", &dat);
        out
    }

    /// MAIN (A, B), mission tables COK (X) and COK_2 (X, Y).
    fn gxt() -> Vec<u8> {
        let main = body(&[("A", "Alpha"), ("B", "Beta")]);
        let mut t1 = name8("COK").to_vec();
        t1.extend(body(&[("X", "first")]));
        let mut t2 = name8("COK_2").to_vec();
        t2.extend(body(&[("X", "second"), ("Y", "why")]));
        let at = 8 + 3 * 12;
        let mut tabl = Vec::new();
        for (n, o) in [("MAIN", at), ("COK", at + main.len()), ("COK_2", at + main.len() + t1.len())] {
            tabl.extend_from_slice(&name8(n));
            tabl.extend_from_slice(&(o as u32).to_le_bytes());
        }
        let mut out = Vec::new();
        chunk(&mut out, b"TABL", &tabl);
        out.extend(main);
        out.extend(t1);
        out.extend(t2);
        out
    }

    fn get(key: &str) -> Option<String> {
        let k = CString::new(key).unwrap();
        let p = revc_text_get(k.as_ptr());
        if p.is_null() {
            return None;
        }
        let mut units = Vec::new();
        for i in 0.. {
            let u = unsafe { *p.add(i) };
            if u == 0 {
                break;
            }
            units.push(u);
        }
        Some(String::from_utf16(&units).unwrap())
    }

    // One test: the store is process-global.
    #[test]
    fn load_get_mission_unload() {
        let d = gxt();
        assert_eq!(unsafe { revc_text_load(d.as_ptr(), d.len()) }, 1);
        assert_eq!(get("A").as_deref(), Some("Alpha"));
        assert_eq!(get("X"), None);
        assert_eq!(revc_text_num_keys(), 2);
        assert_eq!(unsafe { CStr::from_ptr(revc_text_key(1)) }.to_str().unwrap(), "B");

        // Prefix rule: "COK_2" matches COK first, like the C++ loop.
        let m = CString::new("COK_2").unwrap();
        assert_eq!(revc_text_load_mission(m.as_ptr()), 1);
        assert_eq!(get("X").as_deref(), Some("first"));
        assert_eq!(get("Y"), None);
        let m = CString::new("NOPE").unwrap();
        assert_eq!(revc_text_load_mission(m.as_ptr()), 0);
        assert_eq!(get("X"), None);

        // Bad data keeps the old text.
        assert_eq!(unsafe { revc_text_load(b"junk".as_ptr(), 4) }, 0);
        assert_eq!(get("B").as_deref(), Some("Beta"));
        revc_text_unload();
        assert_eq!(get("A"), None);
    }
}

// ---------------------------------------------------------------- the consoles' own text

/// Whether the menu language is English (the consoles' text is the American one only).
static ENGLISH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn revc_text_set_english(on: c_int) {
    ENGLISH.store(on != 0, std::sync::atomic::Ordering::Relaxed);
}

/// The Xbox skin's own keys (Text.cpp RVC_X…) with the Xbox's key holding exactly that text.
/// Only exact matches: the Xbox's menu keys aren't what its skin draws (its FET_SG is "SAVE
/// GAME" where the save screen shows "Save Game", FEI_SEL "Select" where the help line shows
/// "select"; checked against the captures), so the skin's other strings stay revc's.
const XBOX_ALIASES: &[(&str, &str)] = &[
    ("RVC_XSF", "FEA_SFX"),
    ("RVC_XOK", "FEM_OK"),
    ("RVC_XNG", "T_0914C"),
];

/// The keys the console's text supplies in its style besides the aliases: the pad names (C_…),
/// which only the Xbox's text has. The rest stays the PC's: the menus' keys partly mean other
/// things on the Xbox (its Stats title key holds a help line), the credits roll walks a fixed key
/// list, and gameplay prompts name the Xbox's own actions.
fn console_key(key: &[u8]) -> bool {
    key.starts_with(b"C_")
}

/// The PS2's keys taken as they are in its style: the Ammu-Nation items, which the PS2 shows in
/// arrows (< Hammer >), and the stunt line with a colon. Its button prompts are left out (they
/// name the PS2's default buttons, wrong once the controls are remapped), and its menu skin draws
/// its own labels.
const PS2_KEYS: &[&str] = &["ARMOUR", "BASEBAT", "CHAINSA", "CLEVER", "GRENADE", "HAMMER", "BSTSTU"];

/// The Xbox's text (imported `xbox/text/american.gxt`), read once.
static XBOX: std::sync::OnceLock<Option<Store>> = std::sync::OnceLock::new();
/// The PS2's text (imported `ps2/text/american.gxt`), read once.
static PS2: std::sync::OnceLock<Option<Store>> = std::sync::OnceLock::new();

/// A console's imported text, read on first use.
fn console_store(lock: &'static std::sync::OnceLock<Option<Store>>, folder: &str, name: &str) -> Option<&'static Store> {
    lock.get_or_init(|| {
        let path = crate::discimport::cache_dir().join(folder).join("text").join("american.gxt");
        let data = std::fs::read(path).ok()?;
        match Gxt::parse(&data) {
            Ok(g) => Some(build(g)),
            Err(e) => {
                eprintln!("text: the {name}'s american.gxt: {e}");
                None
            }
        }
    })
    .as_ref()
}

/// The console's string for `key` in its UI style (PS2 or Xbox), or null: the PC text and revc's
/// own then apply.
#[unsafe(no_mangle)]
pub extern "C" fn revc_text_console_get(key: *const c_char) -> *const u16 {
    if !ENGLISH.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ptr::null();
    }
    let style = crate::texmods::style();
    let key = key_bytes(key);
    let Some((console, k)) = console_source(style, &key) else {
        return if style == 2 { xbox_prompt_get(&key) } else { std::ptr::null() };
    };
    let store = match console {
        Console::Ps2 => console_store(&PS2, "ps2", "PS2"),
        Console::Xbox => console_store(&XBOX, "xbox", "Xbox"),
    };
    store.and_then(|s| s.main.and_then(|m| s.tables[m].get(&k))).unwrap_or(std::ptr::null())
}

/// The Xbox prompts built so far, by key (None: the key keeps the PC text). Never freed: the C++
/// keeps the pointers.
static XBOX_PROMPTS: Mutex<Option<HashMap<Vec<u8>, Option<Box<[u16]>>>>> = Mutex::new(None);

/// The Xbox wording of a gameplay prompt with the PC's actions in it (prompts.rs), or null.
fn xbox_prompt_get(key: &[u8]) -> *const u16 {
    let mut guard = XBOX_PROMPTS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let entry = map.entry(key.to_vec()).or_insert_with(|| {
        let xbox = console_store(&XBOX, "xbox", "Xbox")?;
        let x = xbox.main.and_then(|m| xbox.tables[m].text(key))?;
        let pc_guard = STORE.lock().unwrap();
        let pc_store = pc_guard.as_ref()?;
        let pc = pc_store.main.and_then(|m| pc_store.tables[m].text(key))?;
        let mut s = crate::prompts::xbox_prompt(std::str::from_utf8(key).ok()?, x, pc)?;
        s.push(0);
        Some(s.into_boxed_slice())
    });
    entry.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Console {
    Ps2,
    Xbox,
}

/// Which console's text answers `key` in UI style `style` (1 PS2, 2 Xbox), and under which of
/// its keys; None: the PC text and revc's own apply.
fn console_source(style: i32, key: &[u8]) -> Option<(Console, Vec<u8>)> {
    match style {
        1 if PS2_KEYS.iter().any(|p| p.as_bytes() == key) => Some((Console::Ps2, key.to_vec())),
        2 => match XBOX_ALIASES.iter().find(|(a, _)| a.as_bytes() == key) {
            Some((_, x)) => Some((Console::Xbox, x.as_bytes().to_vec())),
            None if console_key(key) => Some((Console::Xbox, key.to_vec())),
            None => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod console_tests {
    use super::*;

    #[test]
    fn test_style_1_ps2() {
        // Every key in PS2_KEYS gives Some((Console::Ps2, the same key))
        for &ps2_key in PS2_KEYS.iter() {
            let result = console_source(1, ps2_key.as_bytes());
            assert_eq!(result, Some((Console::Ps2, ps2_key.as_bytes().to_vec())), "Failed for PS2 key: {}", ps2_key);
        }

        // "C_ABC" gives None
        assert_eq!(console_source(1, b"C_ABC"), None);

        // "RVC_XSF" gives None
        assert_eq!(console_source(1, b"RVC_XSF"), None);

        // "FEA_MUS" gives None
        assert_eq!(console_source(1, b"FEA_MUS"), None);
    }

    #[test]
    fn test_style_2_xbox() {
        // "RVC_XSF" gives Some((Console::Xbox, b"FEA_SFX"))
        assert_eq!(console_source(2, b"RVC_XSF"), Some((Console::Xbox, b"FEA_SFX".to_vec())));

        // "RVC_XOK" -> "FEM_OK"
        assert_eq!(console_source(2, b"RVC_XOK"), Some((Console::Xbox, b"FEM_OK".to_vec())));

        // "RVC_XNG" -> "T_0914C"
        assert_eq!(console_source(2, b"RVC_XNG"), Some((Console::Xbox, b"T_0914C".to_vec())));

        // "C_FIRE" gives Some((Console::Xbox, b"C_FIRE"))
        assert_eq!(console_source(2, b"C_FIRE"), Some((Console::Xbox, b"C_FIRE".to_vec())));

        // "HAMMER" gives None
        assert_eq!(console_source(2, b"HAMMER"), None);

        // "FEA_MUS" gives None
        assert_eq!(console_source(2, b"FEA_MUS"), None);
    }

    #[test]
    fn test_style_0_and_3_none() {
        let keys_to_test: [&[u8]; 3] = [b"HAMMER", b"C_FIRE", b"RVC_XSF"];

        // Styles 0 and 3 should give None for all specified keys
        for style in [0, 3] {
            for key in &keys_to_test {
                assert_eq!(
                    console_source(style, key),
                    None,
                    "Expected None for style {} and key {:?}",
                    style,
                    String::from_utf8_lossy(key)
                );
            }
        }
    }
}
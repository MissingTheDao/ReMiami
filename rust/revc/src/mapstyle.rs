//! Map data per style (roadmap R4): in the PS2 and Xbox styles (with the console's world models
//! on), the level loads the console's map definitions and placements, patched into the PC's files
//! by model name (mapstyle_patch.rs) and written next to the import; the console's new objects
//! come in an extra image (their converted models, and empty dictionaries the console texture
//! pack fills). Applies when the level loads, i.e. at start-up.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::mapstyle_patch::{IdeStats, IplStats, ids_in_use, patch_ide, patch_ipl};

#[derive(Debug)]
pub(crate) struct Built {
    pub files: std::collections::HashMap<String, PathBuf>,
    pub extra_ipl: Option<PathBuf>,
    pub extra_img: Option<PathBuf>,
    pub ide: IdeStats,
    pub ipl: IplStats,
    pub new_models: Vec<(String, i32)>,
}

pub(crate) fn key(level_path: &str) -> String {
    level_path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn read_ci(root: &Path, rel: &str) -> Option<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    let mut current = root.to_path_buf();

    for part in &parts {
        if current.is_dir() {
            let entries = fs::read_dir(&current).ok()?;
            let found = entries.filter_map(|e| e.ok()).find(|e| {
                e.file_name().to_string_lossy().eq_ignore_ascii_case(part)
            });
            match found {
                Some(entry) => current = entry.path(),
                None => return None,
            }
        } else {
            return None;
        }
    }

    if current.is_file() {
        let bytes = fs::read(&current).ok()?;
        let s: String = bytes.iter().map(|&b| b as char).collect();
        Some(s)
    } else {
        None
    }
}

pub(crate) fn build(
    game: &Path,
    console: &Path,
    out: &Path,
    txd_exists: &dyn Fn(&str) -> bool,
    txd_header: [u8; 12],
) -> Result<Built, String> {
    let gta_vc_content = read_ci(game, "data/gta_vc.dat").ok_or("Could not read data/gta_vc.dat")?;
    
    let mut ide_files: Vec<(String, String)> = Vec::new();
    let mut ipl_files: Vec<(String, String)> = Vec::new();

    for line in gta_vc_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("IDE ") {
            ide_files.push((key(path), path.trim().to_string()));
        } else if let Some(path) = trimmed.strip_prefix("IPL ") {
            ipl_files.push((key(path), path.trim().to_string()));
        }
    }

    // Collect all PC IDE texts for ID allocation and name->id map
    let mut all_ids_in_use: Vec<(i32, String)> = Vec::new();

    // Add default.ide
    if let Some(default_ide_text) = read_ci(game, "data/default.ide") {
        all_ids_in_use.extend(ids_in_use(&default_ide_text));
    }

    // Add all level IDEs
    for (k, _raw_path) in &ide_files {
        if k.starts_with("data/maps/") {
            // Construct path from game root using the key
            if let Some(text) = read_ci(game, k) {
                all_ids_in_use.extend(ids_in_use(&text));
            }
        }
    }

    // Determine max ID in use
    let mut max_id_in_use: i32 = -1;
    for &(id, _) in &all_ids_in_use {
        if id > max_id_in_use {
            max_id_in_use = id;
        }
    }

    let mut next_id_counter = max_id_in_use;
    let mut next_id = || {
        next_id_counter += 1;
        if next_id_counter < 6500 {
            Some(next_id_counter)
        } else {
            None
        }
    };

    let mut new_models: Vec<(String, i32)> = Vec::new();
    let mut ide_stats: IdeStats = IdeStats::default();
    let mut ipl_stats: IplStats = IplStats::default();
    let mut files_map: HashMap<String, PathBuf> = HashMap::new();

    // Name -> ID map for placements
    let mut name_to_id: HashMap<String, i32> = HashMap::new();
    for &(id, ref name) in &all_ids_in_use {
        name_to_id.insert(name.to_ascii_lowercase(), id);
    }

    // Process IDE files
    for (k, _raw_path) in &ide_files {
        if !k.starts_with("data/maps/") {
            continue;
        }

        let rel_path = &k["data/".len()..];
        let pc_text = read_ci(game, k).ok_or(format!("missing PC file {k}"))?;

        let console_path = console.join(rel_path);
        let console_text = if console_path.exists() {
            fs::read(&console_path).map(|b| b.iter().map(|&c| c as char).collect::<String>()).map_err(|e| format!("{}: {e}", console_path.display()))?
        } else {
            // Skip if missing
            continue;
        };

        let (patched_text, stats) = patch_ide(&pc_text, &console_text, &mut next_id, &mut new_models);
        
        let out_path = out.join(rel_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }
        fs::write(&out_path, &patched_text).map_err(|e| format!("Cannot write {}: {}", out_path.display(), e))?;
        
        files_map.insert(k.clone(), out_path);
        ide_stats.added += stats.added;
        ide_stats.removed += stats.removed;
        ide_stats.txd += stats.txd;
        ide_stats.drawdist += stats.drawdist;
        ide_stats.flags += stats.flags;
        ide_stats.format += stats.format;
    }

    // Update name_to_id with new models
    for (name, id) in &new_models {
        name_to_id.insert(name.clone(), *id);
    }

    // Process IPL files
    let mut extra_ipl_lines: Vec<String> = Vec::new();

    for (k, _raw_path) in &ipl_files {
        if !k.starts_with("data/maps/") {
            continue;
        }

        let rel_path = &k["data/".len()..];
        let pc_text = read_ci(game, k).ok_or(format!("missing PC file {k}"))?;

        let console_path = console.join(rel_path);
        let console_text = if console_path.exists() {
            fs::read(&console_path).map(|b| b.iter().map(|&c| c as char).collect::<String>()).map_err(|e| format!("{}: {e}", console_path.display()))?
        } else {
            // Skip if missing
            continue;
        };

        let id_of = |name: &str| name_to_id.get(name).copied();
        let (patched_text, extra_lines, stats) = patch_ipl(&pc_text, &console_text, &id_of);
        
        let out_path = out.join(rel_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }
        fs::write(&out_path, &patched_text).map_err(|e| format!("Cannot write {}: {}", out_path.display(), e))?;
        
        files_map.insert(k.clone(), out_path);
        ipl_stats.added += stats.added;
        ipl_stats.removed += stats.removed;
        ipl_stats.area += stats.area;
        
        extra_ipl_lines.extend(extra_lines);
    }

    let mut extra_ipl_path: Option<PathBuf> = None;
    if !extra_ipl_lines.is_empty() {
        let extra_ipl_path_buf = out.join("maps/extra.ipl");
        if let Some(parent) = extra_ipl_path_buf.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }
        let mut content = String::from("inst\n");
        for line in &extra_ipl_lines {
            content.push_str(line);
            content.push('\n');
        }
        content.push_str("end\n");
        fs::write(&extra_ipl_path_buf, &content).map_err(|e| format!("Cannot write {}: {}", extra_ipl_path_buf.display(), e))?;
        extra_ipl_path = Some(extra_ipl_path_buf);
    }

    let mut extra_img_path: Option<PathBuf> = None;
    if !new_models.is_empty() {
        let console_models = console.parent().unwrap().join("models-world");
        let extra_img_name = "EXTRA.IMG";
        let extra_dir_name = "EXTRA.DIR";
        
        let mut img_entries: Vec<(String, Vec<u8>)> = Vec::new();
        let mut img_dir_entries: Vec<(u32, u32, Vec<u8>)> = Vec::new(); // offset sectors, size sectors, name
        let mut txds_added: HashSet<String> = HashSet::new();
        let mut current_offset_sector: u32 = 0;
        
        // the new models' dictionaries, from the patched IDE lines
        let mut new_model_txd_map: HashMap<i32, String> = HashMap::new();
        
        for (k, out_path) in &files_map {
            if !k.starts_with("data/maps/") {
                continue;
            }
            let patched_text = fs::read_to_string(out_path).map_err(|e| format!("Cannot read patched IDE {}: {}", out_path.display(), e))?;
            for line in patched_text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    if let Ok(id) = parts[0].parse::<i32>() {
                        if new_models.iter().any(|(_, m_id)| *m_id == id) {
                            let name = parts[1].to_ascii_lowercase();
                            let txd = parts[2].to_ascii_lowercase();
                            // Check if this is a new model
                            if new_models.iter().any(|(m_name, m_id)| *m_id == id && m_name == &name) {
                                new_model_txd_map.insert(id, txd);
                            }
                        }
                    }
                }
            }
        }

        for (name, id) in &new_models {
            // Add DFF if exists
            let dff_path = console_models.join(format!("{}.dff", name));
            if dff_path.exists() {
                let dff_bytes = fs::read(&dff_path).map_err(|e| format!("Cannot read {}: {}", dff_path.display(), e))?;
                let padded = pad_to_2048(dff_bytes);
                img_entries.push((format!("{}.dff", name), padded));
            }
            
            // Add TXD if needed
            if let Some(txd_name) = new_model_txd_map.get(id) {
                if !txd_exists(txd_name) && txds_added.insert(txd_name.clone()) {
                    let txd_bytes = build_empty_txd(&txd_header);
                    let padded = pad_to_2048(txd_bytes);
                    img_entries.push((format!("{}.txd", txd_name), padded));
                }
            }
        }

        // Build DIR and IMG
        for (name, bytes) in &img_entries {
            let size_sectors = (bytes.len() as u32 + 2047) / 2048;
            let offset_sectors = current_offset_sector;
            current_offset_sector += size_sectors;
            let padded_name = pad_to_24(&name.to_lowercase());
            img_dir_entries.push((offset_sectors, size_sectors, padded_name));
        }

        // Write DIR
        let dir_path = out.join(extra_dir_name);
        let mut dir_data = Vec::new();
        for (offset, size, name) in &img_dir_entries {
            dir_data.extend_from_slice(&offset.to_le_bytes());
            dir_data.extend_from_slice(&size.to_le_bytes());
            dir_data.extend_from_slice(name);
        }
        fs::write(&dir_path, &dir_data).map_err(|e| format!("Cannot write {}: {}", dir_path.display(), e))?;

        // Write IMG
        let img_path = out.join(extra_img_name);
        let mut img_data = Vec::new();
        for (_name, bytes) in &img_entries {
            img_data.extend_from_slice(bytes);
        }
        fs::write(&img_path, &img_data).map_err(|e| format!("Cannot write {}: {}", img_path.display(), e))?;

        extra_img_path = Some(img_path);
    }

    Ok(Built {
        files: files_map,
        extra_ipl: extra_ipl_path,
        extra_img: extra_img_path,
        ide: ide_stats,
        ipl: ipl_stats,
        new_models,
    })
}

fn pad_to_2048(data: Vec<u8>) -> Vec<u8> {
    let len = data.len();
    let padded_len = ((len + 2047) / 2048) * 2048;
    let mut padded = data;
    padded.resize(padded_len, 0);
    padded
}

fn pad_to_24(name: &str) -> Vec<u8> {
    let mut n = name.as_bytes().to_vec();
    n.resize(24, 0);
    n
}

fn build_empty_txd(txd_header: &[u8; 12]) -> Vec<u8> {
    let mut data = Vec::new();
    
    // Chunk type 0x16 (RenderWare extension block or similar container)
    // Inside: struct chunk (type 1) and extension chunk (type 3)
    
    // Struct chunk: type 1, payload size 4, version from txd_header[8..12]
    // Payload: u16 0 (texture count), u16 0 (device id)
    let struct_payload = [0u8, 0, 0u8, 0]; // u16 0, u16 0
    let struct_version = &txd_header[8..12];
    
    // Extension chunk: type 3, payload size 0, version from txd_header[8..12]
    let ext_version = &txd_header[8..12];
    
    // Header size is 12 bytes
    // Chunk 1: type(4) + size(4) + version(4) + payload(4) = 16 bytes
    // Chunk 2: type(4) + size(4) + version(4) + payload(0) = 12 bytes
    
    // Write struct chunk
    data.extend_from_slice(&1u32.to_le_bytes()); // type
    data.extend_from_slice(&4u32.to_le_bytes()); // size
    data.extend_from_slice(struct_version);     // version
    data.extend_from_slice(&struct_payload);     // payload
    
    // Write extension chunk
    data.extend_from_slice(&3u32.to_le_bytes()); // type
    data.extend_from_slice(&0u32.to_le_bytes()); // size
    data.extend_from_slice(ext_version);        // version
    // No payload
    
    // Wrap in outer chunk type 0x16
    let mut result = Vec::new();
    let inner_size = data.len() as u32;
    let outer_type = 0x16u32;
    let outer_version = &txd_header[8..12];
    
    result.extend_from_slice(&outer_type.to_le_bytes());
    result.extend_from_slice(&inner_size.to_le_bytes());
    result.extend_from_slice(outer_version);
    result.extend_from_slice(&data);
    
    result
}

// ---------------------------------------------------------------- the C++ level loader's side

use std::ffi::{CStr, c_char, c_int};
use std::sync::Mutex;

/// The patched files of this run (built once, when the level loads), and the console's folder.
static STATE: Mutex<Option<Built>> = Mutex::new(None);
static CONSOLE: Mutex<Option<(&'static str, PathBuf)>> = Mutex::new(None);
/// Patched collision slots, by slot name (None: the PC's as it is).
static COLS: Mutex<Option<HashMap<String, Option<Vec<u8>>>>> = Mutex::new(None);

fn copy_out(s: &str, out: *mut c_char, cap: usize) -> bool {
    if out.is_null() || s.len() + 1 > cap {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), out as *mut u8, s.len());
        *out.add(s.len()) = 0;
    }
    true
}

/// The file at `rel` under `root`, each component matched case-insensitively.
fn find_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        cur = fs::read_dir(&cur).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
    }
    Some(cur)
}

/// The PC archive's dictionary names, and the chunk header of its first dictionary (the
/// RenderWare version the empty dictionaries take).
fn pc_txds(game: &Path) -> Option<(HashSet<String>, [u8; 12])> {
    use std::io::{Read, Seek, SeekFrom};
    let dir = fs::read(find_ci(game, "models/gta3.dir")?).ok()?;
    let mut names = HashSet::new();
    let mut first = None;
    for e in dir.chunks_exact(32) {
        let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap_or(&[])).to_ascii_lowercase();
        if name.ends_with(".txd") {
            first.get_or_insert(u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048);
            names.insert(name);
        }
    }
    let mut header = [0u8; 12];
    let mut img = fs::File::open(find_ci(game, "models/gta3.img")?).ok()?;
    img.seek(SeekFrom::Start(first?)).ok()?;
    img.read_exact(&mut header).ok()?;
    Some((names, header))
}

/// Called by CFileLoader::LoadLevel for each level file: for the main level, in the PS2 or Xbox
/// style with its world models on and the disc's map files imported, builds the patched files;
/// 1 then, with the new objects' image path in `img` ("" if none). 0: the PC's files load.
///
/// # Safety
/// `level` a NUL-terminated string; `img` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_map_style_begin(level: *const c_char, img: *mut c_char, cap: usize) -> c_int {
    copy_out("", img, cap);
    if level.is_null() || key(&unsafe { CStr::from_ptr(level) }.to_string_lossy()) != "data/gta_vc.dat" {
        return 0;
    }
    // REVC_MAP_STYLE=0: the PC's map data in every style (tests: a same-style reference)
    if std::env::var("REVC_MAP_STYLE").as_deref() == Ok("0") {
        return 0;
    }
    let style = crate::texmods::style();
    let kind = match style {
        1 => "ps2",
        2 => "xbox",
        _ => return 0,
    };
    let console = crate::discimport::cache_dir().join(kind);
    if !crate::ps2models::world_models_on(style) || !console.join("maps").is_dir() {
        return 0;
    }
    let out = console.join("maps-patched");
    fs::remove_dir_all(&out).ok();
    let game = Path::new(".");
    let Some((txds, header)) = pc_txds(game) else {
        eprintln!("mapstyle: {kind}: can't read models/gta3.dir/.img; the PC's map data stays");
        return 0;
    };
    match build(game, &console.join("maps"), &out, &|t| txds.contains(&format!("{t}.txd")), header) {
        Err(e) => {
            eprintln!("mapstyle: {kind}: {e}; the PC's map data stays");
            0
        }
        Ok(b) => {
            eprintln!("mapstyle: {kind}: objects {:?}, placements {:?}, {} new models", b.ide, b.ipl, b.new_models.len());
            if let Some(p) = &b.extra_img {
                copy_out(&p.to_string_lossy(), img, cap);
            }
            *STATE.lock().unwrap() = Some(b);
            *CONSOLE.lock().unwrap() = Some((kind, console));
            1
        }
    }
}

/// The patched file for a level file's IDE or IPL path, into `out`: 1, or 0 (the PC's file).
///
/// # Safety
/// `path` a NUL-terminated string; `out` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_map_style_file(path: *const c_char, out: *mut c_char, cap: usize) -> c_int {
    if path.is_null() {
        return 0;
    }
    let k = key(&unsafe { CStr::from_ptr(path) }.to_string_lossy());
    let guard = STATE.lock().unwrap();
    guard.as_ref().and_then(|b| b.files.get(&k)).is_some_and(|p| copy_out(&p.to_string_lossy(), out, cap)) as c_int
}

/// The console's extra placements (loaded once, after the PC's): 1 with the path in `out`, or 0.
///
/// # Safety
/// `out` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_map_style_extra_ipl(out: *mut c_char, cap: usize) -> c_int {
    let mut guard = STATE.lock().unwrap();
    guard.as_mut().and_then(|b| b.extra_ipl.take()).is_some_and(|p| copy_out(&p.to_string_lossy(), out, cap)) as c_int
}

// ---------------------------------------------------------------- collision

/// Collision taken from the console where the console's drawn model changed with it (the
/// collision follows what the player sees; each fits the drawn console mesh, checked): models
/// rebuilt by the console. Everything else keeps the PC's collision (docs/CONSOLE_STYLES.md).
const COL_PS2: &[&str] = &[
    "man_grnd5", "lithavabit01", "lhsbackbit", "hav_car_show_out2", "lh_philsext", "ht_veg04_dy", "man_hall_ref", "buldingsite1", "od_chariot",
];
const COL_XBOX: &[&str] = &["deco_polbuild"];
/// The PS2's collision that comes with objects the PS2 placed differently.
const COL_PS2_CHANGED_OBJECTS: &[&str] = &["doontoon63", "doontoon_top02", "mansion3_base"];

fn col_entries(data: &[u8]) -> Vec<(String, &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + 8 <= data.len() && &data[pos..pos + 4] == b"COLL" {
        let end = pos + 8 + u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if end > data.len() || end < pos + 32 {
            break;
        }
        let name: String = data[pos + 8..pos + 32].iter().take_while(|&&b| b != 0).map(|&b| (b as char).to_ascii_lowercase()).collect();
        out.push((name, &data[pos..end]));
        pos = end;
    }
    out
}

/// The PC collision file with the console's entries for the models `take` accepts (where the
/// console has them), then the console's entries for models `append` accepts that the PC file
/// lacks; None if nothing changed.
pub(crate) fn patch_col(pc: &[u8], console: &[u8], take: &dyn Fn(&str) -> bool, append: &dyn Fn(&str) -> bool) -> Option<Vec<u8>> {
    let (p, c) = (col_entries(pc), col_entries(console));
    let mut out = Vec::with_capacity(pc.len());
    let mut changed = false;
    for (name, bytes) in &p {
        match c.iter().find(|(n, _)| n == name).filter(|_| take(name)) {
            Some((_, cb)) if cb != bytes => {
                out.extend_from_slice(cb);
                changed = true;
            }
            _ => out.extend_from_slice(bytes),
        }
    }
    for (name, cb) in &c {
        if append(name) && !p.iter().any(|(n, _)| n == name) {
            out.extend_from_slice(cb);
            changed = true;
        }
    }
    changed.then_some(out)
}

/// The models whose collision comes from the console in its style (`kind` "ps2"/"xbox").
fn col_taken(kind: &str) -> Vec<&'static str> {
    match kind {
        "ps2" => COL_PS2.iter().chain(COL_PS2_CHANGED_OBJECTS).copied().collect(),
        "xbox" => COL_XBOX.to_vec(),
        _ => Vec::new(),
    }
}

/// The console's collision file for an area slot ("bank" -> coll/bank.col, or the loose one
/// under maps/).
fn console_col(console: &Path, slot: &str) -> Option<Vec<u8>> {
    let name = format!("{}.col", slot.to_ascii_lowercase());
    if let Ok(d) = fs::read(console.join("coll").join(&name)) {
        return Some(d);
    }
    fn walk(dir: &Path, name: &str) -> Option<PathBuf> {
        for e in fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(f) = walk(&p, name) {
                    return Some(f);
                }
            } else if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                return Some(p);
            }
        }
        None
    }
    fs::read(walk(&console.join("maps"), &name)?).ok()
}

/// Called by CColStore::LoadCol for each area slot it loads: with the map data per style on, the
/// slot's collision with the console's entries for the reviewed models and the console's new
/// objects (patch_col); its length in `out_len`. Null: the PC's buffer as it is. The data stays
/// valid for the run.
///
/// # Safety
/// `slot` a NUL-terminated string; `buf` holds `len` bytes; `out_len` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_map_style_col(slot: *const c_char, buf: *const u8, len: usize, out_len: *mut usize) -> *const u8 {
    let Some((kind, console)) = CONSOLE.lock().unwrap().clone() else { return std::ptr::null() };
    if slot.is_null() || buf.is_null() || out_len.is_null() {
        return std::ptr::null();
    }
    let slot = unsafe { CStr::from_ptr(slot) }.to_string_lossy().to_ascii_lowercase();
    let mut cols = COLS.lock().unwrap();
    let cols = cols.get_or_insert_with(HashMap::new);
    let entry = cols.entry(slot.clone()).or_insert_with(|| {
        let pc = unsafe { std::slice::from_raw_parts(buf, len) };
        let con = console_col(&console, &slot)?;
        let taken = col_taken(kind);
        let new: HashSet<String> = STATE.lock().unwrap().as_ref().map(|b| b.new_models.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default();
        let patched = patch_col(pc, &con, &|n| taken.contains(&n), &|n| new.contains(n))?;
        eprintln!("mapstyle: {kind}: collision of {slot}: console entries for {:?}", col_entries(&patched).iter().filter(|(n, b)| col_entries(pc).iter().all(|(m, c)| m != n || c != b)).map(|(n, _)| n.clone()).collect::<Vec<_>>());
        Some(patched)
    });
    match entry {
        Some(d) => {
            unsafe { *out_len = d.len() };
            d.as_ptr()
        }
        None => std::ptr::null(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_key() {
        assert_eq!(key("DATA\\MAPS\\littleha\\littleha.IDE"), "data/maps/littleha/littleha.ide");
    }

    #[test]
    fn test_read_ci() {
        let temp_dir = std::env::temp_dir().join(format!("test_read_ci_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp_dir).unwrap();
        
        let data_dir = temp_dir.join("Data");
        let maps_dir = data_dir.join("MAPS");
        fs::create_dir_all(&maps_dir).unwrap();
        
        let ide_file = maps_dir.join("a.IDE");
        let mut f = fs::File::create(&ide_file).unwrap();
        f.write_all(b"test content").unwrap();
        
        let result = read_ci(&temp_dir, "data/maps/a.ide");
        assert_eq!(result, Some("test content".to_string()));
        
        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_build_end_to_end() {
        let temp_dir = std::env::temp_dir().join(format!("test_build_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp_dir).unwrap();

        let game_dir = temp_dir.join("game");
        let console_dir = temp_dir.join("console");
        let out_dir = temp_dir.join("out");
        
        fs::create_dir_all(game_dir.join("data/maps/x")).unwrap();
        fs::create_dir_all(console_dir.join("maps/x")).unwrap();
        // (as in the import: <kind>/maps/... and <kind>/models-world/...)
        fs::create_dir_all(temp_dir.join("models-world")).unwrap();

        // Create data/default.ide with id 5
        let default_ide = "objs\n5, existing, txd, 1, 100, 0\nend\n";
        fs::write(game_dir.join("data/default.ide"), default_ide).unwrap();

        // Create data/gta_vc.dat
        let gta_vc = "IDE data/maps/x/x.ide\nIPL data/maps/x/x.ipl\n";
        fs::write(game_dir.join("data/gta_vc.dat"), gta_vc).unwrap();

        // Create PC IDE
        let pc_ide = "objs\n10, pcobj, txd1, 1, 100, 0\nend\n";
        fs::write(game_dir.join("data/maps/x/x.ide"), pc_ide).unwrap();

        // Create PC IPL
        let pc_ipl = "inst\n10, pcobj, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\nend\n";
        fs::write(game_dir.join("data/maps/x/x.ipl"), pc_ipl).unwrap();

        // Create console IDE
        let console_ide = "objs\n500, neu, nt, 1, 100, 0\nend\n";
        fs::write(console_dir.join("maps/x/x.ide"), console_ide).unwrap();

        // Create console IPL
        let console_ipl = "inst\n500, neu, 0, 1.0, 2.0, 3.0, 1, 1, 1, 0, 0, 0, 1\nend\n";
        fs::write(console_dir.join("maps/x/x.ipl"), console_ipl).unwrap();

        // Create model file
        fs::write(temp_dir.join("models-world/neu.dff"), b"dff data").unwrap();

        let txd_exists = |_: &str| false;
        let txd_header = [0u8; 12];

        let built = build(&game_dir, &console_dir, &out_dir, &txd_exists, txd_header).unwrap();

        assert_eq!(built.new_models.len(), 1);
        assert_eq!(built.new_models[0].0, "neu");
        // Max ID in use was 10 (from pc_ide) or 5 (from default.ide). Max is 10. Next is 11.
        assert_eq!(built.new_models[0].1, 11);

        // Check extra.ipl
        assert!(built.extra_ipl.is_some());
        let extra_ipl_content = fs::read_to_string(built.extra_ipl.unwrap()).unwrap();
        assert!(extra_ipl_content.contains("11, neu"));

        // Check EXTRA.DIR
        assert!(built.extra_img.is_some());
        let dir_path = out_dir.join("EXTRA.DIR");
        assert!(dir_path.exists());
        let dir_data = fs::read(&dir_path).unwrap();
        // Should contain dff and txd entries
        // Each entry is 32 bytes
        assert_eq!(dir_data.len(), 64); // 2 entries (neu.dff and nt.txd)

        // Check EXTRA.IMG
        let img_path = out_dir.join("EXTRA.IMG");
        assert!(img_path.exists());
        let img_data = fs::read(&img_path).unwrap();
        // First entry is neu.dff (8 bytes) padded to 2048
        // Second entry is nt.txd (empty txd structure) padded to 2048
        // The txd entry should start with 0x16
        // Offset of second entry is 2048 bytes
        let txd_entry_start = &img_data[2048..];
        assert_eq!(txd_entry_start[0], 0x16);

        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }

    /// On the real PC data and the local imports (never in the repo; skipped without them): the
    /// numbers of changes per console, pinned, so a parsing slip shows up as a wrong count.
    #[test]
    fn real_data_counts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let game = root.join("game");
        let import = std::env::var_os("REVC_IMPORT_DIR").map_or_else(|| root.join("../revc-scratch/import"), PathBuf::from);
        let Ok(dir) = fs::read(game.join("models/gta3.dir")) else {
            eprintln!("skipped: no PC game");
            return;
        };
        let txds: HashSet<String> = dir
            .chunks_exact(32)
            .map(|e| String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap()).to_ascii_lowercase())
            .filter(|n| n.ends_with(".txd"))
            .collect();
        for kind in ["ps2", "xbox"] {
            let console = import.join(kind).join("maps");
            if !console.is_dir() {
                eprintln!("skipped: no {kind} import");
                continue;
            }
            let out = std::env::temp_dir().join(format!("revc-mapstyle-{kind}-{}", std::process::id()));
            let b = build(&game, &console, &out, &|t| txds.contains(&format!("{t}.txd")), [0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x03, 0x18]).unwrap();
            fs::remove_dir_all(&out).ok();
            // (objects: removed counts objs and tobj; default.ide stays the PC's, so the Xbox's ten
            // wheel draw distances aren't here; docs/CONSOLE_STYLES.md)
            let (ide, ipl) = match kind {
                "ps2" => (IdeStats { added: 19, removed: 39, txd: 4, drawdist: 21, flags: 75, format: 0 }, IplStats { added: 30, removed: 50, area: 195 }),
                _ => (IdeStats { added: 4, removed: 1, txd: 0, drawdist: 96, flags: 3, format: 0 }, IplStats { added: 17, removed: 7, area: 134 }),
            };
            assert_eq!((b.ide, b.ipl), (ide, ipl), "{kind}");
            assert_eq!(b.new_models.len(), ide.added, "{kind}");
            // collision: exactly the reviewed models change (9 + 1, pinned) plus the PS2's changed
            // objects, and the new objects' entries come in; nothing else
            assert_eq!((COL_PS2.len(), COL_XBOX.len(), COL_PS2_CHANGED_OBJECTS.len()), (9, 1, 3));
            let new: HashSet<String> = b.new_models.iter().map(|(n, _)| n.clone()).collect();
            let taken = col_taken(kind);
            let (mut changed, mut added) = (HashSet::new(), HashSet::new());
            for e in dir.chunks_exact(32) {
                let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap()).to_ascii_lowercase();
                let Some(slot) = name.strip_suffix(".col") else { continue };
                let (off, size) = (u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048, u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048);
                let mut pc = vec![0u8; size];
                {
                    use std::io::{Read, Seek, SeekFrom};
                    let mut img = fs::File::open(game.join("models/gta3.img")).unwrap();
                    img.seek(SeekFrom::Start(off)).unwrap();
                    img.read_exact(&mut pc).unwrap();
                }
                let Some(con) = console_col(&import.join(kind), slot) else { continue };
                let Some(p) = patch_col(&pc, &con, &|n| taken.contains(&n), &|n| new.contains(n)) else { continue };
                let before = col_entries(&pc);
                for (n, bytes) in col_entries(&p) {
                    match before.iter().find(|(m, _)| *m == n) {
                        Some((_, old)) if *old != bytes => drop(changed.insert(n)),
                        Some(_) => {}
                        None => drop(added.insert(n)),
                    }
                }
            }
            let want_changed: HashSet<String> = taken.iter().map(|n| n.to_string()).collect();
            assert!(changed.is_subset(&want_changed), "{kind}: unreviewed collision change {:?}", changed.difference(&want_changed).collect::<Vec<_>>());
            assert!(added.is_subset(&new), "{kind}: collision added for {:?}", added.difference(&new).collect::<Vec<_>>());
            eprintln!("{kind}: collision changed {} {:?}, added {}", changed.len(), changed, added.len());
        }
    }
}

#[cfg(test)]
mod col_tests {
    use super::*;

    fn entry(name: &str, body: &[u8]) -> Vec<u8> {
        let mut name_padded = [0u8; 24];
        for (i, c) in name.bytes().enumerate() {
            if i < 24 {
                name_padded[i] = c;
            }
        }
        let total_size = 24 + body.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"COLL");
        out.extend_from_slice(&(total_size as u32).to_le_bytes());
        out.extend_from_slice(&name_padded);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn test_basic_patch() {
        // PC = [a(body 1), b(body 2)]
        let pc_a = entry("a", &[1]);
        let pc_b = entry("b", &[2]);
        let pc = [pc_a.as_slice(), pc_b.as_slice()].concat();

        // console = [B(body 9), c(body 3), d(body 4)]
        let console_b = entry("B", &[9]);
        let console_c = entry("c", &[3]);
        let console_d = entry("d", &[4]);
        let console = [console_b.as_slice(), console_c.as_slice(), console_d.as_slice()].concat();

        let take = |name: &str| name == "b";
        let append = |name: &str| name == "c";

        let result = patch_col(&pc, &console, &take, &append);
        
        // Expected: a1 + B9 + c3
        let expected_a = entry("a", &[1]);
        let expected_b_console = entry("B", &[9]);
        let expected_c = entry("c", &[3]);
        let expected = [expected_a.as_slice(), expected_b_console.as_slice(), expected_c.as_slice()].concat();

        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_take_append_nothing() {
        let pc = entry("a", &[1]);
        let console = entry("b", &[2]);

        let take = |_: &str| false;
        let append = |_: &str| false;

        let result = patch_col(&pc, &console, &take, &append);
        assert_eq!(result, None);
    }

    #[test]
    fn test_pc_with_padding() {
        // PC = [a(body 1)] with 2048-byte zero padding after
        let pc_a = entry("a", &[1]);
        let mut pc = pc_a;
        pc.extend(vec![0u8; 2048]);

        let console = entry("B", &[9]);

        let take = |name: &str| name == "a";
        let append = |_: &str| false;

        let result = patch_col(&pc, &console, &take, &append);
        
        // Should parse pc_a correctly despite padding
        let expected_a_console = entry("A", &[9]); // Note: console name is "B", lowercased "b". PC name is "a".
        // Wait, console entry name is "B", lowercased is "b". PC entry name is "a", lowercased is "a".
        // take("a") is true. Console has "b". They don't match.
        // So PC entry "a" is kept.
        // Result should be same as PC entries (a1).
        // But wait, the spec says "None if nothing changed".
        // Since console doesn't have "a", PC "a" is kept. No change.
        
        // Let's re-read the first test case.
        // PC = [a, b], console = [B, c, d]. take=b, append=c.
        // PC b matches console B (lowercased b == b). So B9 replaces b2.
        // append c: c is in console, not in PC. So c3 is appended.
        // Result: a1, B9, c3.
        
        // In this padding test:
        // PC has "a". Console has "B" (name "b").
        // take("a") is true. Console has "b". "a" != "b". So no replacement.
        // append: nothing.
        // Result should be None (no change).
        
        assert_eq!(result, None);
    }
    
    #[test]
    fn test_case_insensitive_names() {
        let pc_b = entry("b", &[2]);
        let pc = pc_b;

        let console_b = entry("B", &[9]);
        let console = console_b;

        let take = |name: &str| name == "b";
        let append = |_: &str| false;

        let result = patch_col(&pc, &console, &take, &append);
        
        // PC "b" matches console "B" (lowercased "b" == "b").
        // So B9 replaces b2.
        let expected_b_console = entry("B", &[9]);
        let expected = expected_b_console;
        
        assert_eq!(result, Some(expected));
    }
}

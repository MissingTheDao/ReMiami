//! Differential check of revc-formats against the C++ loaders (`REVC_TEST=formats`).
//!
//! After a save is loaded, `revc_test_dump_formats` (skel/host/testhooks.cpp) writes what the C++
//! side holds: model info from the IDE files, collision counts, the GXT key table, geometry of the
//! loaded models and texture sizes of the loaded TXDs. This module parses the same game files with
//! revc-formats and reports every difference. The game directory is the current directory.

use revc_formats::{col, dff, gxt, img, text, txd};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Case-insensitive path lookup relative to `base` (game files are mixed case, the .dat files use
/// DOS paths).
fn find_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let next = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = next.path();
    }
    Some(cur)
}

/// Lines of the form `<KEYWORD> <path>` from the level .dat files, in load order.
fn dat_entries(base: &Path, keyword: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dat in ["data/default.dat", "data/gta_vc.dat"] {
        let Some(p) = find_ci(base, dat) else { continue };
        let Ok(s) = std::fs::read(&p) else { continue };
        for line in String::from_utf8_lossy(&s).lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some(keyword) {
                // COLFILE has a level number before the path.
                let path = if keyword == "COLFILE" { it.nth(1) } else { it.next() };
                if let Some(f) = path.and_then(|p| find_ci(base, p)) {
                    out.push(f);
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Report {
    failures: Vec<String>,
    checked: HashMap<&'static str, usize>,
}

impl Report {
    fn ok(&mut self, what: &'static str) {
        *self.checked.entry(what).or_default() += 1;
    }
    fn fail(&mut self, msg: String) {
        self.failures.push(msg);
    }
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

const MITYPE_VEHICLE: &str = "6";
/// CVehicleModelInfo gives untextured (environment-mapped) vehicle materials the particle.txd
/// "white" texture, renamed "@hite" (VehicleModelInfo.cpp).
const VEHICLE_WHITE: &str = "@hite";

/// The textures of a geometry's materials as the C++ side lists them (textured materials only, in
/// order). "@hite" stands for a material that has no texture in the file; an empty name (the
/// player skin, replaced at run time) matches any texture. A texture the model's TXD lacks leaves
/// the material untextured in C++, so a file texture may be absent from the C++ list, but the C++
/// list may not contain anything else or a different order.
fn textures_match(g: &dff::Geometry, cpp: &[&str]) -> bool {
    let mut cpp = cpp.iter().peekable();
    for m in &g.materials {
        match &m.texture {
            Some(t) => {
                if cpp.peek().is_some_and(|c| c.is_empty() || eq_ci(c, &t.name)) {
                    cpp.next();
                }
            }
            None => {
                // Either untouched (absent from the C++ list) or given the white texture.
                if cpp.peek() == Some(&&VEHICLE_WHITE) {
                    cpp.next();
                }
            }
        }
    }
    cpp.next().is_none()
}

unsafe extern "C" {
    fn revc_test_mission_text(table: *const std::ffi::c_char, key: *const std::ffi::c_char) -> *const u16;
    fn revc_test_restore_mission_text(previous: *const std::ffi::c_char);
    fn revc_test_current_mission_text(out8: *mut std::ffi::c_char);
}

fn wide(p: *const u16) -> Vec<u16> {
    let mut v = Vec::new();
    if !p.is_null() {
        for i in 0.. {
            let u = unsafe { *p.add(i) };
            if u == 0 {
                break;
            }
            v.push(u);
        }
    }
    v
}

/// Every mission table, loaded the way the LOAD_MISSION_TEXT script command does: its first and
/// last key must give the file's strings, from the table the game's prefix rule picks (the first
/// table whose name starts the requested one).
fn check_mission_text(base: &Path, r: &mut Report) -> Result<(), String> {
    let p = find_ci(base, "TEXT/american.gxt").ok_or("TEXT/american.gxt missing")?;
    let g = gxt::Gxt::parse(&std::fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut previous = [0 as std::ffi::c_char; 16];
    unsafe { revc_test_current_mission_text(previous.as_mut_ptr()) };
    for t in g.tables.iter().filter(|t| t.name != "MAIN") {
        let chosen = g.tables.iter().find(|c| t.name.starts_with(&c.name)).unwrap_or(t);
        for (key, _) in [t.entries.first(), t.entries.last()].into_iter().flatten() {
            let ctable = std::ffi::CString::new(t.name.as_str()).unwrap();
            let ckey = std::ffi::CString::new(key.as_str()).unwrap();
            let got = wide(unsafe { revc_test_mission_text(ctable.as_ptr(), ckey.as_ptr()) });
            // MAIN is searched first, so a key in both resolves to MAIN's string.
            let want = g.main().and_then(|m| m.get(key)).or_else(|| chosen.get(key));
            match want {
                Some(w) if w == got.as_slice() => r.ok("mission text keys"),
                _ => r.fail(format!("mission text {}/{key}: C++ {:?}, file {:?}", t.name, gxt::to_lossy_string(&got), want.map(gxt::to_lossy_string))),
            }
        }
    }
    unsafe { revc_test_restore_mission_text(previous.as_ptr()) };
    Ok(())
}

/// Compares the dump at `dump` with revc-formats' view of the game in the current directory.
/// Returns a summary line and the list of differences.
pub fn check(dump: &Path) -> Result<(String, Vec<String>), String> {
    let base = Path::new(".");
    let dump = std::fs::read_to_string(dump).map_err(|e| format!("can't read {}: {e}", dump.display()))?;
    let mut r = Report::default();

    // --- IDE ---------------------------------------------------------------------------------
    let mut defs: HashMap<i32, text::ObjectDef> = HashMap::new();
    for ide in dat_entries(base, "IDE") {
        let data = std::fs::read(&ide).map_err(|e| e.to_string())?;
        for d in text::object_defs(&text::sections(&data)).map_err(|e| format!("{}: {e}", ide.display()))? {
            defs.insert(d.id, d);
        }
    }

    // --- COL ---------------------------------------------------------------------------------
    // Level 0: the COLFILEs, later files replace earlier models of the same name.
    let mut generic_cols: HashMap<String, col::ColModel> = HashMap::new();
    for f in dat_entries(base, "COLFILE") {
        let data = std::fs::read(&f).map_err(|e| e.to_string())?;
        for m in col::parse(&data).map_err(|e| format!("{}: {e}", f.display()))? {
            generic_cols.insert(m.name.to_ascii_lowercase(), m);
        }
    }
    let archive = img::Archive::open(find_ci(base, "models/gta3.img").ok_or("models/gta3.img missing")?)
        .map_err(|e| e.to_string())?;
    let mut imgf = archive.open_img().map_err(|e| e.to_string())?;
    let mut area_cols: HashMap<String, Vec<col::ColModel>> = HashMap::new();
    for e in archive.entries.iter().filter(|e| e.extension() == "col") {
        let data = archive.read_with(&mut imgf, e).map_err(|e| e.to_string())?;
        for m in col::parse(&data).map_err(|err| format!("{}: {err}", e.name))? {
            area_cols.entry(m.name.to_ascii_lowercase()).or_default().push(m);
        }
    }
    let col_matches = |m: &col::ColModel, f: &[&str]| {
        f[3].parse() == Ok(m.spheres.len())
            && f[4].parse() == Ok(m.boxes.len())
            && f[5].parse() == Ok(m.triangles.len())
            // CAutomobile/CBike recompute vehicles' bounding radius from the wheel positions.
            && (f[7] == MITYPE_VEHICLE || f[6].parse::<f32>() == Ok(m.bound_radius))
    };

    // --- caches for DFF / TXD ----------------------------------------------------------------
    let mut dffs: HashMap<String, Option<dff::Clump>> = HashMap::new();
    let mut txds: HashMap<String, Option<txd::Txd>> = HashMap::new();
    let mut model_files: Vec<dff::Geometry> = Vec::new();
    for p in dat_entries(base, "MODELFILE") {
        let data = std::fs::read(&p).map_err(|e| e.to_string())?;
        model_files.extend(dff::parse(&data).map_err(|e| format!("{}: {e}", p.display()))?.geometries);
    }
    let mut gxt_main: Option<gxt::Table> = None;
    let mut gxt_keys = 0usize;

    for line in dump.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "MI" if f.len() >= 9 && (f[2] == "1" || f[2] == "3") => {
                let id: i32 = f[1].parse().unwrap_or(-1);
                let Some(d) = defs.get(&id) else {
                    // Unnamed slots are added in code (CModelInfo::Initialise), not from an IDE.
                    if !f[3].is_empty() {
                        r.fail(format!("model {id} {} not in any IDE", f[3]));
                    }
                    continue;
                };
                let n: usize = f[5].parse().unwrap_or(0);
                let dists: Vec<f32> = f[6..9].iter().map(|x| x.parse().unwrap_or(f32::NAN)).collect();
                let mut same = eq_ci(&d.model, f[3]) && eq_ci(&d.txd, f[4]) && n == d.draw_dists.len()
                    && d.draw_dists.iter().zip(&dists).all(|(a, b)| a == b);
                if f[2] == "3" {
                    let times = (f.get(9).and_then(|x| x.parse().ok()), f.get(10).and_then(|x| x.parse().ok()));
                    same &= d.time_on_off.map(|(a, b)| (Some(a), Some(b))) == Some(times);
                }
                if same {
                    r.ok("model infos");
                } else {
                    r.fail(format!("model {id}: C++ {:?}, IDE {:?}", &f[3..], d));
                }
            }
            "COL" if f.len() == 8 => {
                let name = f[1].to_ascii_lowercase();
                if f[2] == "0" {
                    match generic_cols.get(&name) {
                        Some(m) if col_matches(m, &f) => r.ok("collision (COLFILE)"),
                        Some(m) => r.fail(format!("col {name}: C++ {:?}, file {} spheres {} boxes {} tris r={}",
                            &f[3..], m.spheres.len(), m.boxes.len(), m.triangles.len(), m.bound_radius)),
                        None => {} // built in code (CTempColModels) or from a vehicle/ped
                    }
                } else if f[3..6] != ["0", "0", "0"] {
                    match area_cols.get(&name) {
                        Some(ms) if ms.iter().any(|m| col_matches(m, &f)) => r.ok("collision (gta3.img)"),
                        Some(_) => r.fail(format!("col {name} (slot {}): C++ {:?} matches no model in gta3.img", f[2], &f[3..])),
                        None => r.fail(format!("col {name} (slot {}) not found in gta3.img", f[2])),
                    }
                }
            }
            "GEO" if f.len() == 6 => {
                let name = f[1].to_ascii_lowercase();
                for n in [name.clone(), "wheels".to_string()] {
                    dffs.entry(n.clone()).or_insert_with(|| {
                        let data = match archive.find(&format!("{n}.dff")) {
                            Some(e) => archive.read(e).ok()?,
                            None => std::fs::read(find_ci(base, &format!("models/generic/{n}.dff"))?).ok()?,
                        };
                        dff::parse(&data).ok()
                    });
                }
                // Vehicle wheels (and the wheel_* models) are atomics from models/generic/wheels.dff.
                let wheels = dffs["wheels"].as_ref().map(|c| c.geometries.as_slice()).unwrap_or(&[]);
                // Without a <name>.dff the atomic comes from a MODELFILE (default.dat) or wheels.dff.
                let own = match &dffs[&name] {
                    Some(c) => c.geometries.as_slice(),
                    None => model_files.as_slice(),
                };
                let cpp_tex: Vec<&str> = if f[5] == "-" { Vec::new() } else { f[5].split('|').collect() };
                let found = own.iter().chain(wheels).any(|g| {
                    f[2].parse() == Ok(g.num_vertices)
                        && f[3].parse() == Ok(g.triangles.len())
                        && f[4].parse() == Ok(g.materials.len())
                        && textures_match(g, &cpp_tex)
                });
                if found {
                    r.ok("geometries");
                } else {
                    let ours: Vec<_> = own.iter().map(|g| (g.num_vertices, g.triangles.len(), g.materials.len())).collect();
                    r.fail(format!("{name}: C++ geometry {:?} not in {name}.dff / MODELFILEs / wheels.dff {ours:?}", &f[2..]));
                }
            }
            "TEX" if f.len() == 5 => {
                let slot = f[1].to_ascii_lowercase();
                let dict = txds.entry(slot.clone()).or_insert_with(|| {
                    if slot == "generic" {
                        // CFileLoader::LoadLevel merges every TEXDICTION file into the generic slot.
                        let mut all = txd::Txd { device_id: 0, textures: Vec::new() };
                        for p in dat_entries(base, "TEXDICTION") {
                            all.textures.extend(txd::parse(&std::fs::read(p).ok()?).ok()?.textures);
                        }
                        return Some(all);
                    }
                    let file = format!("{slot}.txd");
                    let data = match archive.find(&file) {
                        Some(e) => archive.read(e).ok()?,
                        None => std::fs::read(find_ci(base, &format!("models/{file}"))?).ok()?,
                    };
                    txd::parse(&data).ok()
                });
                let Some(dict) = dict else { continue }; // dictionary built in code
                let name = if f[2] == VEHICLE_WHITE { "white" } else { f[2] };
                match dict.find(name) {
                    Some(t) if f[3].parse() == Ok(t.width) && f[4].parse() == Ok(t.height) => r.ok("textures"),
                    Some(t) => r.fail(format!("texture {slot}/{}: C++ {}x{}, file {}x{}", f[2], f[3], f[4], t.width, t.height)),
                    None => r.fail(format!("texture {slot}/{} not in {slot}.txd", f[2])),
                }
            }
            "GXT" if f.len() >= 2 => {
                if gxt_main.is_none() {
                    let p = find_ci(base, "TEXT/american.gxt").ok_or("TEXT/american.gxt missing")?;
                    let g = gxt::Gxt::parse(&std::fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    gxt_main = g.main().cloned();
                }
                let main = gxt_main.as_ref().ok_or("american.gxt has no MAIN table")?;
                gxt_keys += 1;
                let cpp: Vec<u16> = f[2..].iter().filter_map(|x| u16::from_str_radix(x, 16).ok()).collect();
                match main.get(f[1]) {
                    Some(units) if units == cpp.as_slice() => r.ok("text keys"),
                    Some(units) => r.fail(format!("text {}: C++ {:?}, file {:?}", f[1], gxt::to_lossy_string(&cpp), gxt::to_lossy_string(units))),
                    None => r.fail(format!("text key {} missing in american.gxt MAIN", f[1])),
                }
            }
            _ => {}
        }
    }
    if let Some(main) = &gxt_main
        && main.entries.len() != gxt_keys
    {
        r.fail(format!("C++ has {gxt_keys} MAIN text keys, file has {}", main.entries.len()));
    }
    check_mission_text(base, &mut r)?;
    for what in ["model infos", "collision (COLFILE)", "geometries", "textures", "text keys", "mission text keys"] {
        if r.checked.get(what).copied().unwrap_or(0) == 0 {
            r.fail(format!("nothing compared for {what}"));
        }
    }
    let mut summary: Vec<_> = r.checked.iter().map(|(k, v)| format!("{v} {k}")).collect();
    summary.sort();
    Ok((format!("formats: matched {}", summary.join(", ")), r.failures))
}

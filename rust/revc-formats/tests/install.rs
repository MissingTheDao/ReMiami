//! Parses every file of a known format in a real Vice City install: loose files and every entry
//! of every IMG archive. The game data is private and not in the repo, so the test is skipped
//! (with a message) when no install is found. Set REVC_GAME_DIR to point at one; by default it
//! looks for `game/` at the repository root.

use revc_formats::{col, dff, gxt, img, scm, text, txd};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn game_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("REVC_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game"));
    dir.join("models").is_dir().then_some(dir)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        // Don't follow symlinked dirs (test.sh builds a game dir of symlinks pointing back here).
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Files in a PC install that the game never loads in that format. Each one is still parsed, and
/// the test fails if it suddenly parses (so the list stays honest).
const NOT_LOADED: &[(&str, &str)] = &[
    (
        "txd.img",
        "texture cache reVC writes on first run (USE_TXD_CDIMAGE), librw GL3 natives, not game data",
    ),
    (
        "intro.txd",
        "PS2 texture dictionary left in the PC release; never loaded",
    ),
    (
        "peds.col",
        "only read for a version stamp (main.cpp); its first record is not valid collision",
    ),
];

fn not_loaded(name: &str) -> Option<&'static str> {
    NOT_LOADED
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, why)| *why)
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

#[derive(Default)]
struct Tally {
    ok: BTreeMap<String, usize>,
    failures: Vec<String>,
}

impl Tally {
    fn check(&mut self, what: &str, kind: &str, data: &[u8]) {
        let res: Result<(), String> = match kind {
            "dff" => dff::parse(data).map(|_| ()).map_err(|e| e.to_string()),
            "txd" => txd::parse(data).map(|_| ()).map_err(|e| e.to_string()),
            "col" => col::parse(data).map(|_| ()).map_err(|e| e.to_string()),
            "gxt" => gxt::Gxt::parse(data).map(|_| ()).map_err(|e| e.to_string()),
            "ide" => text::object_defs(&text::sections(data))
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "ipl" => text::instances(&text::sections(data))
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "scm" => scm::parse_header(data)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            _ => return,
        };
        match res {
            Ok(()) => *self.ok.entry(kind.to_string()).or_default() += 1,
            Err(e) => self.failures.push(format!("{what}: {e}")),
        }
    }
}

#[test]
fn parses_every_file_in_install() {
    let Some(dir) = game_dir() else {
        eprintln!("SKIPPED: no Vice City install found (set REVC_GAME_DIR)");
        return;
    };
    let mut files = Vec::new();
    walk(&dir, &mut files);
    let mut t = Tally::default();
    let mut archives = 0;
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let ext = ext_of(&name);
        if let Some(why) = not_loaded(&name) {
            let mut sub = Tally::default();
            if ext == "img" {
                let a = img::Archive::open(f).unwrap();
                let mut h = a.open_img().unwrap();
                for e in &a.entries {
                    sub.check(&e.name, &e.extension(), &a.read_with(&mut h, e).unwrap());
                }
            } else {
                sub.check(&name, &ext, &std::fs::read(f).unwrap());
            }
            assert!(
                !sub.failures.is_empty(),
                "{name} is listed as not loadable but parses now; update NOT_LOADED"
            );
            eprintln!(
                "skipped {name} ({} parse errors, expected): {why}",
                sub.failures.len()
            );
            continue;
        }
        if ext == "img" && f.with_extension("dir").exists() {
            let a = img::Archive::open(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            let mut h = a.open_img().unwrap();
            archives += 1;
            for e in &a.entries {
                let data = a.read_with(&mut h, e).unwrap();
                t.check(
                    &format!("{}:{}", f.display(), e.name),
                    &e.extension(),
                    &data,
                );
            }
        } else {
            let data = std::fs::read(f).unwrap();
            t.check(&f.display().to_string(), &ext, &data);
        }
    }
    eprintln!("{archives} IMG archives; parsed: {:?}", t.ok);
    for kind in ["dff", "txd", "col", "gxt", "ide", "ipl", "scm"] {
        assert!(
            t.ok.get(kind).copied().unwrap_or(0) > 0,
            "no {kind} files parsed - wrong game dir?"
        );
    }
    assert!(
        t.failures.is_empty(),
        "{} failures:\n{}",
        t.failures.len(),
        t.failures.join("\n")
    );
}

/// Decoded values checked by hand against the game (American PC release) and reVC's limits.
#[test]
fn spot_checks() {
    let Some(dir) = game_dir() else {
        eprintln!("SKIPPED: no Vice City install found (set REVC_GAME_DIR)");
        return;
    };
    let x = gxt::Gxt::parse(&std::fs::read(dir.join("TEXT/american.gxt")).unwrap()).unwrap();
    assert_eq!(
        gxt::to_lossy_string(x.main().unwrap().get("FEP_STG").unwrap()),
        "Start Game"
    );

    let a = img::Archive::open(dir.join("models/gta3.img")).unwrap();
    let c = dff::parse(&a.read(a.find("infernus.dff").unwrap()).unwrap()).unwrap();
    assert!(c.frame_by_name("wheel_lf_dummy").is_some());
    assert_eq!(c.geometries[0].num_vertices, 1721);
    let t = txd::parse(&a.read(a.find("infernus.txd").unwrap()).unwrap()).unwrap();
    let body = t.find("infernus86body128a").unwrap();
    assert_eq!(
        (body.width, body.height, body.compression, body.levels.len()),
        (128, 128, 1, 8)
    );
    assert_eq!(body.levels[0].len(), 128 * 128 / 2); // DXT1: 4 bits per pixel

    let s = std::fs::read(dir.join("data/main.scm")).unwrap();
    let h = scm::parse_header(&s).unwrap();
    assert!(h.main_size <= 225_512 && h.largest_mission_size <= 35_000); // SIZE_MAIN_SCRIPT, SIZE_MISSION_SCRIPT
    assert!(h.used_objects.len() <= 220 && h.mission_offsets.len() <= 120); // MAX_NUM_USED_OBJECTS, MAX_NUM_MISSION_SCRIPTS
}

//! Imports assets from the player's own copies of Vice City for consoles (Xbox, PS2) into a local
//! cache, so optional features can use them (goal "Asset import from original discs"): the Xbox
//! controls screen art and fonts, the PS2 controller-setup sprites, later car textures and more.
//! Nothing from the discs is shipped or committed; the game asks for the disc image (an .iso
//! file) when a feature needs it, and this crate copies the listed files out of it.
//!
//! Formats: Xbox discs use XDVDFS (`xdvdfs`), PS2 discs ISO 9660 (`iso9660`); see
//! Claude outputs/xbox-research/XBOX_RESEARCH.md for what the discs hold.

pub mod iso9660;
pub mod ps2tex;
pub mod xboxlm;
pub mod xdvdfs;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A file on a disc: its path (as on the disc, '/'-separated) and where its bytes are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscKind {
    Xbox,
    Ps2,
}

impl DiscKind {
    /// Folder of this disc's files in the import cache.
    pub fn dir(self) -> &'static str {
        match self {
            DiscKind::Xbox => "xbox",
            DiscKind::Ps2 => "ps2",
        }
    }
}

/// An opened disc image of GTA Vice City.
pub struct Disc {
    pub kind: DiscKind,
    pub entries: Vec<Entry>,
    file: File,
}

fn has(entries: &[Entry], path: &str) -> bool {
    entries.iter().any(|e| e.path.eq_ignore_ascii_case(path))
}

impl Disc {
    /// Opens an image and checks it is Vice City for Xbox or PS2.
    pub fn open(path: &Path) -> Result<Disc, String> {
        let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let io = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(entries) = xdvdfs::read_tree(&mut file).map_err(io)? {
            if has(&entries, "default.xbe") && has(&entries, "neo/ControlMapping1.txt") {
                return Ok(Disc { kind: DiscKind::Xbox, entries, file });
            }
            return Err(format!("{}: an Xbox disc, but not GTA Vice City", path.display()));
        }
        if let Some(entries) = iso9660::read_tree(&mut file).map_err(io)? {
            if has(&entries, "SYSTEM.CNF") && has(&entries, "MODELS/FRONTEN2.TXD") {
                return Ok(Disc { kind: DiscKind::Ps2, entries, file });
            }
            return Err(format!("{}: a disc image, but not GTA Vice City for PS2", path.display()));
        }
        Err(format!("{}: not an Xbox or PS2 disc image", path.display()))
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))
    }

    pub fn read(&mut self, e: &Entry) -> std::io::Result<Vec<u8>> {
        let mut data = vec![0u8; e.size as usize];
        self.file.seek(SeekFrom::Start(e.offset))?;
        self.file.read_exact(&mut data)?;
        Ok(data)
    }

    /// `size` bytes at `offset` inside the file `e` (clamped to its end).
    pub fn read_range(&mut self, e: &Entry, offset: u64, size: usize) -> std::io::Result<Vec<u8>> {
        let size = size.min(e.size.saturating_sub(offset) as usize);
        let mut data = vec![0u8; size];
        self.file.seek(SeekFrom::Start(e.offset + offset))?;
        self.file.read_exact(&mut data)?;
        Ok(data)
    }
}

/// What to lift from each disc: files whose path starts with `dir` (case-insensitive) and ends
/// with one of `exts` (empty = any), copied to `<cache>/<kind>/<dest>/<file name in lower case>`.
pub struct Lift {
    pub kind: DiscKind,
    pub dir: &'static str,
    pub exts: &'static [&'static str],
    pub dest: &'static str,
}

pub const MANIFEST: &[Lift] = &[
    // Xbox front end: menu sprites (pad outline, A/B/X/Y, D-pad, icons, radio logos), the
    // 1536x1536 pause-menu map (UserInterface.txd) and its two bitmap fonts.
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/models/", exts: &[".dds"], dest: "ui" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/", exts: &[".dds", ".txd"], dest: "ui" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/fonts/", exts: &[], dest: "ui/fonts" },
    // The pause menu's "tutorial briefings" and "unlocked items" lists: which text keys each
    // collects (core/BriefLists.cpp).
    Lift { kind: DiscKind::Xbox, dir: "neo/Hints.txt", exts: &[], dest: "neo" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UnlockedItems.txt", exts: &[], dest: "neo" },
    // The pause menu's Maestro animation (R3_Menu.anm): the selection bar's five outline shapes.
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/models/R3_Menu.anm", exts: &[], dest: "ui" },
    // HUD and fonts (DXT5: smoother alpha than the PC's DXT3).
    Lift { kind: DiscKind::Xbox, dir: "models/hud.txd", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Xbox, dir: "models/fonts.txd", exts: &[], dest: "models" },
    // Wheels and the loading, splash and intro screens (with the HUD and fonts, the Xbox style's
    // texture pack: revc/src/ps2pack.rs).
    Lift { kind: DiscKind::Xbox, dir: "models/generic/wheels.txd", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Xbox, dir: "txd/", exts: &[".txd"], dest: "txd" },
    // PS2 front end: the controller-setup sprites (DualShock picture, callouts) re3's PS2 menu uses,
    // the rest of the menu art, the HUD, and the loading, splash and intro screens. They are PS2
    // native (GS swizzled); the game reads them through a texture pack built from them
    // (revc/src/ps2pack.rs, ps2tex.rs).
    Lift { kind: DiscKind::Ps2, dir: "MODELS/FRONTEN2.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "MODELS/FRONTEN1.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "MODELS/HUD.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "TXD/", exts: &[".txd"], dest: "txd" },
    // PS2 time cycle: 5 rows differ from the PC's (the PS2 visual style uses it, plan task 13).
    Lift { kind: DiscKind::Ps2, dir: "DATA/TIMECYC.DAT", exts: &[], dest: "data" },
    // PS2 ped animations: 13 differ from the PC's (walk_start, idle_armed, the woman's walks, ...);
    // the PS2 UI style plays them (animation/AnimManager.cpp ReplacePedAnimsFromPS2).
    Lift { kind: DiscKind::Ps2, dir: "ANIM/PED.IFP", exts: &[], dest: "anim" },
];

fn matches(lift: &Lift, path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let dir = lift.dir.to_ascii_lowercase();
    if !p.starts_with(&dir) {
        return false;
    }
    // A directory lift takes only its own files, not subfolders.
    let rest = &p[dir.len()..];
    if dir.ends_with('/') && rest.contains('/') {
        return false;
    }
    lift.exts.is_empty() || lift.exts.iter().any(|x| p.ends_with(x))
}

/// The result of an import: files written, and where.
#[derive(Debug, Default)]
pub struct Report {
    pub written: Vec<PathBuf>,
}

/// Copies the manifest's files from `disc` into `cache/<kind>/...` and writes
/// `cache/<kind>/IMPORTED.txt` (which image, how many files) last, as the marker that the
/// import is complete.
pub fn import(disc: &mut Disc, image: &Path, cache: &Path) -> Result<Report, String> {
    let root = cache.join(disc.kind.dir());
    let wanted: Vec<(Entry, &Lift)> = disc
        .entries
        .iter()
        .filter_map(|e| MANIFEST.iter().find(|l| l.kind == disc.kind && matches(l, &e.path)).map(|l| (e.clone(), l)))
        .collect();
    if wanted.is_empty() {
        return Err("nothing to import from this disc".into());
    }
    let mut report = Report::default();
    for (e, lift) in wanted {
        let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_ascii_lowercase();
        let dest = root.join(lift.dest).join(name);
        let data = disc.read(&e).map_err(|err| format!("{}: {err}", e.path))?;
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|err| format!("{}: {err}", dest.display()))?;
        std::fs::write(&dest, data).map_err(|err| format!("{}: {err}", dest.display()))?;
        report.written.push(dest);
    }
    if disc.kind == DiscKind::Xbox {
        // World lightmaps (the textures of the Xbox's MatFX dual pass, `<model>@`, and the second
        // coordinates; xboxlm)
        // (skipped if the disc has no gta3.img: not a full game disc)
        let lightmaps = match (disc.find("models/gta3.dir").cloned(), disc.find("models/gta3.img").cloned()) {
            (Some(d), Some(i)) => vec![xbox_lightmaps(disc, &d, &i)?],
            _ => Vec::new(),
        };
        for (bin, txd) in lightmaps {
        for (file, data) in [("lightmaps.bin", bin), ("lightmaps.txd", txd)] {
            let dest = root.join("models").join(file);
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|err| format!("{}: {err}", dest.display()))?;
            std::fs::write(&dest, data).map_err(|err| format!("{}: {err}", dest.display()))?;
            report.written.push(dest);
        }
        }
    }
    let marker = format!(
        "imported from {}\nfiles {}\n(from the player's own disc; not for redistribution)\n",
        image.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        report.written.len()
    );
    std::fs::write(root.join("IMPORTED.txt"), marker).map_err(|e| e.to_string())?;
    Ok(report)
}

/// The Xbox world lightmap sidecar and TXD (xboxlm) from the disc's models/gta3.dir, gta3.img and
/// generic.txd.
fn xbox_lightmaps(disc: &mut Disc, dir_e: &Entry, img_e: &Entry) -> Result<(Vec<u8>, Vec<u8>), String> {
    let dir = disc.read(dir_e).map_err(|e| format!("gta3.dir: {e}"))?;
    let generic = match disc.find("models/generic.txd").cloned() {
        Some(e) => disc.read(&e).map_err(|err| format!("generic.txd: {err}"))?,
        None => Vec::new(),
    };
    let (bin, txd, _) = xboxlm::img_sidecar(&dir, &[&generic], |off, size| disc.read_range(img_e, off, size))?;
    Ok((bin, txd))
}

/// Whether `cache` holds a complete import from this kind of disc.
pub fn imported(cache: &Path, kind: DiscKind) -> bool {
    cache.join(kind.dir()).join("IMPORTED.txt").is_file()
}

#[cfg(test)]
mod tests;

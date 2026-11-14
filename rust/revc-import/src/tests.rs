use super::*;

const S: usize = 2048;

fn put(img: &mut Vec<u8>, at: usize, data: &[u8]) {
    if img.len() < at + data.len() {
        img.resize(at + data.len(), 0);
    }
    img[at..at + data.len()].copy_from_slice(data);
}

/// An XDVDFS directory: entries chained through their right pointers.
fn xdir(entries: &[(&str, bool, u32, u32)]) -> Vec<u8> {
    let mut d = Vec::new();
    let mut offs = Vec::new();
    for (name, _, _, _) in entries {
        offs.push(d.len());
        d.resize(d.len() + (14 + name.len()).next_multiple_of(4), 0);
    }
    for (i, (name, is_dir, start, size)) in entries.iter().enumerate() {
        let p = offs[i];
        let right = offs.get(i + 1).map_or(0, |o| (o / 4) as u16);
        d[p..p + 2].copy_from_slice(&0u16.to_le_bytes());
        d[p + 2..p + 4].copy_from_slice(&right.to_le_bytes());
        d[p + 4..p + 8].copy_from_slice(&start.to_le_bytes());
        d[p + 8..p + 12].copy_from_slice(&size.to_le_bytes());
        d[p + 12] = if *is_dir { 0x10 } else { 0x80 };
        d[p + 13] = name.len() as u8;
        d[p + 14..p + 14 + name.len()].copy_from_slice(name.as_bytes());
    }
    d
}

fn xbox_image() -> Vec<u8> {
    let mut img = Vec::new();
    let root = xdir(&[("default.xbe", false, 60, 4), ("neo", true, 41, 0)]);
    let neo = xdir(&[("ControlMapping1.txt", false, 61, 5), ("UserInterface", true, 42, 0)]);
    let ui = xdir(&[("models", true, 43, 0), ("dpad_f01.dds", false, 62, 3), ("readme.txt", false, 63, 2)]);
    let models = xdir(&[("R3_menu_336.dds", false, 64, 6)]);
    // Directory sizes are filled in now that the children are built.
    let mut root = root;
    let mut neo = neo;
    let mut ui = ui;
    let set = |d: &mut Vec<u8>, entry: usize, size: u32| {
        let mut p = 0;
        for _ in 0..entry {
            let nl = d[p + 13] as usize;
            p += (14 + nl).next_multiple_of(4);
        }
        d[p + 8..p + 12].copy_from_slice(&size.to_le_bytes());
    };
    set(&mut ui, 0, models.len() as u32);
    set(&mut neo, 1, ui.len() as u32);
    set(&mut root, 1, neo.len() as u32);
    let mut vd = b"MICROSOFT*XBOX*MEDIA".to_vec();
    vd.extend_from_slice(&40u32.to_le_bytes());
    vd.extend_from_slice(&(root.len() as u32).to_le_bytes());
    put(&mut img, 0x10000, &vd);
    put(&mut img, 40 * S, &root);
    put(&mut img, 41 * S, &neo);
    put(&mut img, 42 * S, &ui);
    put(&mut img, 43 * S, &models);
    put(&mut img, 60 * S, b"XBEH");
    put(&mut img, 61 * S, b"FIRE ");
    put(&mut img, 62 * S, b"dds");
    put(&mut img, 63 * S, b"hi");
    put(&mut img, 64 * S, b"padpic");
    img
}

fn rec(name: &[u8], lba: u32, size: u32, dir: bool) -> Vec<u8> {
    let len = (33 + name.len()).next_multiple_of(2);
    let mut r = vec![0u8; len];
    r[0] = len as u8;
    r[2..6].copy_from_slice(&lba.to_le_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[25] = if dir { 2 } else { 0 };
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

fn ps2_image() -> Vec<u8> {
    let mut img = Vec::new();
    let mut root = [rec(&[0], 20, 2048, true), rec(&[1], 20, 2048, true), rec(b"SYSTEM.CNF;1", 30, 4, false), rec(b"MODELS", 21, 2048, true)].concat();
    root.resize(2048, 0);
    let mut models = [rec(&[0], 21, 2048, true), rec(&[1], 20, 2048, true), rec(b"FRONTEN2.TXD;1", 31, 8, false), rec(b"HUD.TXD;1", 32, 3, false)].concat();
    models.resize(2048, 0);
    let mut pvd = vec![0u8; 2048];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[40..48].copy_from_slice(b"TESTDISC");
    pvd[156..190].copy_from_slice(&rec(&[0], 20, 2048, true));
    put(&mut img, 16 * S, &pvd);
    put(&mut img, 20 * S, &root);
    put(&mut img, 21 * S, &models);
    put(&mut img, 30 * S, b"BOOT");
    put(&mut img, 31 * S, b"frontend");
    put(&mut img, 32 * S, b"hud");
    img
}

fn temp(name: &str, data: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("revc-import-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{name}.iso"));
    std::fs::write(&p, data).unwrap();
    p
}

#[test]
fn xbox_disc_is_read_detected_and_imported() {
    let iso = temp("xbox", &xbox_image());
    let mut disc = Disc::open(&iso).unwrap();
    assert_eq!(disc.kind, DiscKind::Xbox);
    let e = disc.find("neo/UserInterface/models/R3_menu_336.dds").unwrap().clone();
    assert_eq!(disc.read(&e).unwrap(), b"padpic");
    let cache = iso.parent().unwrap().join("cache");
    let report = import(&mut disc, &iso, &cache).unwrap();
    // The two .dds files; not readme.txt (wrong extension) nor anything outside the manifest.
    assert_eq!(report.written.len(), 2, "{:?}", report.written);
    assert_eq!(std::fs::read(cache.join("xbox/ui/r3_menu_336.dds")).unwrap(), b"padpic");
    assert_eq!(std::fs::read(cache.join("xbox/ui/dpad_f01.dds")).unwrap(), b"dds");
    assert!(imported(&cache, DiscKind::Xbox));
    assert!(!imported(&cache, DiscKind::Ps2));
}

#[test]
fn ps2_disc_is_read_detected_and_imported() {
    let iso = temp("ps2", &ps2_image());
    let mut disc = Disc::open(&iso).unwrap();
    assert_eq!(disc.kind, DiscKind::Ps2);
    assert_eq!(disc.entries.len(), 3, "{:?}", disc.entries);
    let cache = iso.parent().unwrap().join("cache");
    let report = import(&mut disc, &iso, &cache).unwrap();
    // FRONTEN2.TXD and HUD.TXD (both in the manifest since the PS2 texture pack, ps2pack.rs)
    assert_eq!(report.written.len(), 2, "{:?}", report.written);
    assert_eq!(std::fs::read(cache.join("ps2/models/fronten2.txd")).unwrap(), b"frontend");
    assert!(cache.join("ps2/models/hud.txd").is_file());
    assert!(imported(&cache, DiscKind::Ps2));
}

#[test]
fn other_files_are_rejected() {
    let iso = temp("junk", &vec![0u8; 64 * 1024]);
    let err = Disc::open(&iso).err().expect("junk must not open");
    assert!(err.contains("not an Xbox or PS2 disc image"), "{err}");
}

/// The real discs, when REVC_XBOX_ISO / REVC_PS2_ISO point at them (not in CI: they're the
/// player's own copies).
#[test]
fn real_discs_if_given() {
    for (var, kind, must) in [
        ("REVC_XBOX_ISO", DiscKind::Xbox, "neo/UserInterface/models/R3_menu_336.dds"),
        ("REVC_PS2_ISO", DiscKind::Ps2, "MODELS/FRONTEN2.TXD"),
    ] {
        let Some(path) = std::env::var_os(var) else { continue };
        let disc = Disc::open(Path::new(&path)).unwrap();
        assert_eq!(disc.kind, kind);
        assert!(disc.find(must).is_some(), "{must} missing");
        let n = disc.entries.iter().filter(|e| MANIFEST.iter().any(|l| l.kind == kind && matches(l, &e.path))).count();
        eprintln!("{var}: {} files on the disc, {n} to import", disc.entries.len());
        assert!(n > 0);
    }
}

#[test]
fn vehicle_txds_come_from_the_cars_section() {
    let ide = "objs\n1, box, generic, 1, 50\nend\ncars\n# Id, Model, Txd\n130, landstal, landstal, car, LANDSTAL\n131, idaho, Idaho, car, IDAHO\nend\npeds\n0, null, generic, CIVMALE\nend\n";
    let names = crate::vehicle_txds(ide);
    assert_eq!(names.len(), 2);
    assert!(names.contains("landstal") && names.contains("idaho"));
}

#[test]
fn map_files_keep_their_folders() {
    let d = |p: &str| map_file_dest(p).map(|x| x.to_string_lossy().into_owned());
    assert_eq!(d("DATA/MAPS/BANK/BANK.IPL").as_deref(), Some("maps/maps/bank/bank.ipl"));
    assert_eq!(d("data/maps/bank/bank.col").as_deref(), Some("maps/maps/bank/bank.col"));
    assert_eq!(d("DATA/MAPS/GENERIC.IDE").as_deref(), Some("maps/maps/generic.ide"));
    assert_eq!(d("DATA/DEFAULT.IDE").as_deref(), Some("maps/default.ide"));
    assert_eq!(d("DATA/GTA_VC.DAT").as_deref(), Some("maps/gta_vc.dat"));
    assert_eq!(d("DATA/MAP.ZON").as_deref(), Some("maps/map.zon"));
    assert_eq!(d("MODELS/COLL/GENERIC.COL").as_deref(), Some("coll/generic.col"));
    assert_eq!(d("DATA/MAPS/MAP0.DAT"), None);
    assert_eq!(d("DATA/TIMECYC.DAT"), None);
    assert_eq!(d("MODELS/GTA3.IMG"), None);
}

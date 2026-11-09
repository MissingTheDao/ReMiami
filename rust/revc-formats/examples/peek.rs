//! Prints a few decoded values from the game install, for sanity checks.
use revc_formats::{dff, gxt, img, scm, txd};
fn main() {
    let g = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game");
    let x = gxt::Gxt::parse(&std::fs::read(g.join("TEXT/american.gxt")).unwrap()).unwrap();
    let m = x.main().unwrap();
    println!(
        "gxt: {} tables, MAIN {} keys, first {:?} = {:?}",
        x.tables.len(),
        m.entries.len(),
        m.entries[0].0,
        gxt::to_lossy_string(&m.entries[0].1)
    );
    for k in ["FEM_OK", "FEP_STG"] {
        println!("  {k} = {:?}", m.get(k).map(gxt::to_lossy_string));
    }
    let a = img::Archive::open(g.join("models/gta3.img")).unwrap();
    for name in ["player.dff", "infernus.dff", "infernus.txd"] {
        let e = a.find(name).unwrap();
        let d = a.read(e).unwrap();
        if name.ends_with(".dff") {
            let c = dff::parse(&d).unwrap();
            let names: Vec<_> = c
                .frames
                .iter()
                .filter_map(|f| f.name.clone())
                .take(8)
                .collect();
            println!(
                "{name}: {} frames {:?}.., {} geoms, verts {:?}, tris {:?}, mats {:?}",
                c.frames.len(),
                names,
                c.geometries.len(),
                c.geometries
                    .iter()
                    .map(|g| g.num_vertices)
                    .collect::<Vec<_>>(),
                c.geometries
                    .iter()
                    .map(|g| g.triangles.len())
                    .collect::<Vec<_>>(),
                c.geometries[0]
                    .materials
                    .iter()
                    .filter_map(|m| m.texture.as_ref().map(|t| t.name.clone()))
                    .collect::<Vec<_>>()
            );
        } else {
            let t = txd::parse(&d).unwrap();
            println!(
                "{name}: {:?}",
                t.textures
                    .iter()
                    .map(|t| (
                        t.name.clone(),
                        t.width,
                        t.height,
                        t.depth,
                        t.compression,
                        t.levels.len()
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
    let s = std::fs::read(g.join("data/main.scm")).unwrap();
    let h = scm::parse_header(&s).unwrap();
    println!(
        "scm: vars {} objs {} (first {:?}) main {} largest {} missions {} excl {}",
        h.var_space_size,
        h.used_objects.len(),
        &h.used_objects[..3],
        h.main_size,
        h.largest_mission_size,
        h.mission_offsets.len(),
        h.num_exclusive_missions
    );
}

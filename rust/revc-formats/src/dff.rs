//! Models (`.dff`): RenderWare clumps as read by librw's `Clump::streamRead`, for PC (non-native)
//! geometry. Plugin data (skin, HAnim, 2dfx, MatFX ...) is kept raw per object; only the frame
//! node name is decoded, since the game looks models' parts up by name.

use crate::bytes::Reader;
use crate::rw::{
    self, Chunk, ID_ATOMIC, ID_CLUMP, ID_EXTENSION, ID_FRAMELIST, ID_GEOMETRY, ID_GEOMETRYLIST,
    ID_MATERIAL, ID_MATLIST, ID_NODENAME, ID_STRING, ID_STRUCT, ID_TEXTURE,
};
use crate::{Error, Result};

/// Geometry flags (rpGEOMETRY*).
pub mod geo {
    pub const TRISTRIP: u32 = 0x01;
    pub const POSITIONS: u32 = 0x02;
    pub const TEXTURED: u32 = 0x04;
    pub const PRELIT: u32 = 0x08;
    pub const NORMALS: u32 = 0x10;
    pub const LIGHT: u32 = 0x20;
    pub const MODULATE: u32 = 0x40;
    pub const TEXTURED2: u32 = 0x80;
    pub const NATIVE: u32 = 0x0100_0000;
}

pub type Plugins = Vec<(u32, Vec<u8>)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Rows: right, up, at.
    pub rotation: [[f32; 3]; 3],
    pub position: [f32; 3],
    /// Index of the parent frame, -1 for the root.
    pub parent: i32,
    pub name: Option<String>,
    pub plugins: Plugins,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextureRef {
    pub name: String,
    pub mask: String,
    pub filter_addressing: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub color: [u8; 4],
    /// ambient, specular, diffuse.
    pub surface: [f32; 3],
    pub texture: Option<TextureRef>,
    pub plugins: Plugins,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Triangle {
    pub v: [u16; 3],
    pub material: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MorphTarget {
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub flags: u32,
    pub num_vertices: u32,
    pub prelit: Vec<[u8; 4]>,
    pub tex_coords: Vec<Vec<[f32; 2]>>,
    pub triangles: Vec<Triangle>,
    pub morph_targets: Vec<MorphTarget>,
    pub materials: Vec<Material>,
    pub plugins: Plugins,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atomic {
    pub frame: u32,
    pub geometry: u32,
    pub flags: u32,
    pub plugins: Plugins,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clump {
    pub frames: Vec<Frame>,
    pub geometries: Vec<Geometry>,
    pub atomics: Vec<Atomic>,
    pub plugins: Plugins,
}

/// Affine transform as RenderWare stores it: right, up, at (the rotated axes) and position.
/// A point p maps to right*p.x + up*p.y + at*p.z + pos.
pub type Transform = [[f32; 3]; 4];

pub const IDENTITY: Transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]];

/// `outer` applied after `inner`.
pub fn compose(outer: &Transform, inner: &Transform) -> Transform {
    let rot = |v: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|i| outer[0][i] * v[0] + outer[1][i] * v[1] + outer[2][i] * v[2])
    };
    let p = rot(inner[3]);
    [rot(inner[0]), rot(inner[1]), rot(inner[2]), std::array::from_fn(|i| p[i] + outer[3][i])]
}

pub fn transform_point(t: &Transform, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| t[0][i] * p[0] + t[1][i] * p[1] + t[2][i] * p[2] + t[3][i])
}

pub fn transform_dir(t: &Transform, d: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| t[0][i] * d[0] + t[1][i] * d[1] + t[2][i] * d[2])
}

impl Clump {
    /// Frame `i` relative to the clump's root (parents applied).
    pub fn world_transform(&self, i: usize) -> Transform {
        let f = &self.frames[i];
        let local = [f.rotation[0], f.rotation[1], f.rotation[2], f.position];
        if f.parent < 0 { local } else { compose(&self.world_transform(f.parent as usize), &local) }
    }

    /// Whether a part belongs to the normal, close-up look of the model: vehicles also carry a
    /// low-detail copy (`_vlo`) and damaged parts (`_dam`) that the game swaps in.
    pub fn is_default_part(&self, frame: usize) -> bool {
        let n = self.frames[frame].name.as_deref().unwrap_or("").to_ascii_lowercase();
        !n.ends_with("_vlo") && !n.ends_with("_dam") && !n.contains("_lo_")
    }

    pub fn frame_by_name(&self, name: &str) -> Option<usize> {
        self.frames.iter().position(|f| {
            f.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
    }
}

fn plugins(c: &Chunk) -> Result<Plugins> {
    Ok(rw::extensions(c)?
        .into_iter()
        .map(|(id, d)| (id, d.to_vec()))
        .collect())
}

/// Parses the first clump in a DFF file (leading non-clump chunks are skipped).
pub fn parse(data: &[u8]) -> Result<Clump> {
    for c in rw::Chunks::new(data) {
        let c = c?;
        if c.id == ID_CLUMP {
            return parse_clump(&c);
        }
    }
    Err(Error::Invalid("no clump in file".into()))
}

pub fn parse_clump(c: &Chunk) -> Result<Clump> {
    let st = c.child(ID_STRUCT)?;
    let num_atomics = st.reader().i32("clump atomic count")?;
    let mut frames = Vec::new();
    let mut geometries = Vec::new();
    let mut atomics = Vec::new();
    let mut ext = Vec::new();
    for k in c.children() {
        let k = k?;
        match k.id {
            ID_FRAMELIST => frames = parse_frame_list(&k)?,
            ID_GEOMETRYLIST => geometries = parse_geometry_list(&k)?,
            ID_ATOMIC => atomics.push(parse_atomic(&k, &mut geometries)?),
            ID_EXTENSION => ext = plugins(&k)?,
            _ => {} // struct, lights, cameras
        }
    }
    if atomics.len() != num_atomics.max(0) as usize {
        return Err(Error::Invalid(format!(
            "clump says {num_atomics} atomics, found {}",
            atomics.len()
        )));
    }
    for (i, f) in frames.iter().enumerate() {
        if f.parent >= i as i32 {
            return Err(Error::Invalid(format!(
                "frame {i} has parent {} (must come earlier)",
                f.parent
            )));
        }
    }
    for a in &atomics {
        if a.frame as usize >= frames.len() || a.geometry as usize >= geometries.len() {
            return Err(Error::Invalid(format!(
                "atomic refers to frame {} / geometry {} out of range",
                a.frame, a.geometry
            )));
        }
    }
    Ok(Clump {
        frames,
        geometries,
        atomics,
        plugins: ext,
    })
}

fn parse_frame_list(c: &Chunk) -> Result<Vec<Frame>> {
    let mut kids = c.children();
    let st = kids
        .next()
        .ok_or_else(|| Error::Invalid("empty frame list".into()))??;
    let mut r = st.reader();
    let n = r.i32("frame count")?.max(0) as usize;
    let mut frames = Vec::with_capacity(n);
    for _ in 0..n {
        let rotation = [
            r.vec3("frame right")?,
            r.vec3("frame up")?,
            r.vec3("frame at")?,
        ];
        let position = r.vec3("frame pos")?;
        let parent = r.i32("frame parent")?;
        r.skip(4)?; // matrix flags
        frames.push(Frame {
            rotation,
            position,
            parent,
            name: None,
            plugins: Vec::new(),
        });
    }
    // One extension per frame, in order.
    for f in frames.iter_mut() {
        let Some(e) = kids.next() else { break };
        let e = e?;
        if e.id != ID_EXTENSION {
            continue;
        }
        f.plugins = plugins(&e)?;
        f.name = f
            .plugins
            .iter()
            .find(|(id, _)| *id == ID_NODENAME)
            .map(|(_, d)| crate::bytes::c_str(d));
    }
    Ok(frames)
}

fn parse_geometry_list(c: &Chunk) -> Result<Vec<Geometry>> {
    let st = c.child(ID_STRUCT)?;
    let n = st.reader().i32("geometry count")?.max(0) as usize;
    let mut out = Vec::with_capacity(n);
    for k in c.children() {
        let k = k?;
        if k.id == ID_GEOMETRY {
            out.push(parse_geometry(&k)?);
        }
    }
    if out.len() != n {
        return Err(Error::Invalid(format!(
            "geometry list says {n}, found {}",
            out.len()
        )));
    }
    Ok(out)
}

fn parse_atomic(c: &Chunk, geometries: &mut Vec<Geometry>) -> Result<Atomic> {
    let mut r = c.child(ID_STRUCT)?.reader();
    let frame = r.u32("atomic frame")?;
    let mut geometry = r.u32("atomic geometry")?;
    let flags = r.u32("atomic flags")?;
    let mut ext = Vec::new();
    for k in c.children() {
        let k = k?;
        match k.id {
            // Old streams embed the geometry in the atomic.
            ID_GEOMETRY => {
                geometries.push(parse_geometry(&k)?);
                geometry = (geometries.len() - 1) as u32;
            }
            ID_EXTENSION => ext = plugins(&k)?,
            _ => {}
        }
    }
    Ok(Atomic {
        frame,
        geometry,
        flags,
        plugins: ext,
    })
}

pub fn parse_geometry(c: &Chunk) -> Result<Geometry> {
    let st = c.child(ID_STRUCT)?;
    let mut r = st.reader();
    let raw_flags = r.u32("geometry flags")?;
    let num_tris = r.i32("geometry triangles")?.max(0) as usize;
    let num_verts = r.i32("geometry vertices")?.max(0) as usize;
    let num_morph = r.i32("geometry morph targets")?.max(0) as usize;
    if st.version() < 0x34000 {
        r.skip(12)?; // surface properties
    }
    let flags = raw_flags & 0xFF00_FFFF;
    let mut num_sets = ((raw_flags >> 16) & 0xFF) as usize;
    if num_sets == 0 {
        num_sets = if flags & geo::TEXTURED != 0 {
            1
        } else if flags & geo::TEXTURED2 != 0 {
            2
        } else {
            0
        };
    }
    let mut prelit = Vec::new();
    let mut tex_coords = Vec::new();
    let mut triangles = Vec::with_capacity(num_tris);
    if flags & geo::NATIVE == 0 {
        if flags & geo::PRELIT != 0 {
            prelit = r
                .bytes(4 * num_verts, "prelit colours")?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| [c[0], c[1], c[2], c[3]])
                .collect();
        }
        for _ in 0..num_sets {
            let mut set = Vec::with_capacity(num_verts);
            for _ in 0..num_verts {
                set.push([r.f32("uv")?, r.f32("uv")?]);
            }
            tex_coords.push(set);
        }
        for _ in 0..num_tris {
            // librw: tribuf[0] = v0<<16 | v1, tribuf[1] = v2<<16 | matId
            let a = r.u32("triangle")?;
            let b = r.u32("triangle")?;
            triangles.push(Triangle {
                v: [(a >> 16) as u16, a as u16, (b >> 16) as u16],
                material: b as u16,
            });
        }
    }
    let mut morph_targets = Vec::with_capacity(num_morph);
    for _ in 0..num_morph {
        let sphere_center = r.vec3("morph sphere")?;
        let sphere_radius = r.f32("morph radius")?;
        let has_verts = r.i32("morph has vertices")? != 0;
        let has_normals = r.i32("morph has normals")? != 0;
        let read_v3s = |r: &mut Reader, on: bool| -> Result<Vec<[f32; 3]>> {
            if !on || flags & geo::NATIVE != 0 {
                return Ok(Vec::new());
            }
            (0..num_verts).map(|_| r.vec3("morph vertex")).collect()
        };
        let vertices = read_v3s(&mut r, has_verts)?;
        let normals = read_v3s(&mut r, has_normals)?;
        morph_targets.push(MorphTarget {
            sphere_center,
            sphere_radius,
            vertices,
            normals,
        });
    }
    let materials = parse_material_list(&c.child(ID_MATLIST)?, st.version())?;
    for t in &triangles {
        if t.v.iter().any(|&v| v as usize >= num_verts)
            || t.material as usize >= materials.len().max(1)
        {
            return Err(Error::Invalid(format!(
                "triangle {:?} out of range ({num_verts} vertices, {} materials)",
                t,
                materials.len()
            )));
        }
    }
    let ext = match c.child(ID_EXTENSION) {
        Ok(e) => plugins(&e)?,
        Err(_) => Vec::new(),
    };
    Ok(Geometry {
        flags,
        num_vertices: num_verts as u32,
        prelit,
        tex_coords,
        triangles,
        morph_targets,
        materials,
        plugins: ext,
    })
}

fn parse_material_list(c: &Chunk, geo_version: u32) -> Result<Vec<Material>> {
    let mut kids = c.children();
    let st = kids
        .next()
        .ok_or_else(|| Error::Invalid("empty material list".into()))??;
    let mut r = st.reader();
    let n = r.i32("material count")?.max(0) as usize;
    let indices: Vec<i32> = (0..n)
        .map(|_| r.i32("material index"))
        .collect::<Result<_>>()?;
    let mut out: Vec<Material> = Vec::with_capacity(n);
    for idx in indices {
        if idx >= 0 {
            // Instance of an earlier material.
            let m = out
                .get(idx as usize)
                .cloned()
                .ok_or_else(|| Error::Invalid(format!("material index {idx} out of range")))?;
            out.push(m);
            continue;
        }
        let m = loop {
            let k = kids
                .next()
                .ok_or_else(|| Error::Invalid("material list ends early".into()))??;
            if k.id == ID_MATERIAL {
                break k;
            }
        };
        out.push(parse_material(&m, geo_version)?);
    }
    Ok(out)
}

fn parse_material(c: &Chunk, geo_version: u32) -> Result<Material> {
    let st = c.child(ID_STRUCT)?;
    let mut r = st.reader();
    r.skip(4)?; // flags
    let col = r.bytes(4, "material colour")?;
    let color = [col[0], col[1], col[2], col[3]];
    r.skip(4)?; // unused
    let textured = r.i32("material textured")? != 0;
    let surface = if st.version() < 0x30400 {
        [1.0; 3]
    } else {
        [r.f32("ambient")?, r.f32("specular")?, r.f32("diffuse")?]
    };
    let _ = geo_version;
    let texture = if textured {
        Some(parse_texture(&c.child(ID_TEXTURE)?)?)
    } else {
        None
    };
    let ext = match c.child(ID_EXTENSION) {
        Ok(e) => plugins(&e)?,
        Err(_) => Vec::new(),
    };
    Ok(Material {
        color,
        surface,
        texture,
        plugins: ext,
    })
}

fn parse_texture(c: &Chunk) -> Result<TextureRef> {
    let mut kids = c.children();
    let st = kids
        .next()
        .ok_or_else(|| Error::Invalid("empty texture".into()))??;
    let filter_addressing = st.reader().u32("texture filter")?;
    let mut strings = Vec::new();
    for k in kids {
        let k = k?;
        if k.id == ID_STRING {
            strings.push(rw::string(&k)?);
        }
    }
    if strings.len() < 2 {
        return Err(Error::Invalid("texture without name and mask".into()));
    }
    Ok(TextureRef {
        name: strings[0].clone(),
        mask: strings[1].clone(),
        filter_addressing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rw::build::*;

    fn ext(plugins: &[(u32, &[u8])]) -> Vec<u8> {
        let body: Vec<u8> = plugins.iter().flat_map(|(id, d)| chunk(*id, d)).collect();
        chunk(ID_EXTENSION, &body)
    }

    fn frame_list() -> Vec<u8> {
        let mut st = u32s(&[2]);
        for parent in [-1i32, 0] {
            st.extend(f32s(&[
                1.,
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                0.,
                1.,
                0.,
                0.,
                parent as f32 + 1.0,
            ]));
            st.extend(u32s(&[parent as u32, 0]));
        }
        cat(&[
            &chunk(ID_STRUCT, &st),
            &ext(&[(ID_NODENAME, b"root")]),
            &ext(&[(ID_NODENAME, b"wheel_lf")]),
        ])
    }

    fn material(textured: bool) -> Vec<u8> {
        let mut st = u32s(&[0]);
        st.extend_from_slice(&[255, 128, 0, 255]);
        st.extend(u32s(&[0, textured as u32]));
        st.extend(f32s(&[1.0, 0.5, 0.25]));
        let mut body = chunk(ID_STRUCT, &st);
        if textured {
            body.extend(chunk(
                ID_TEXTURE,
                &cat(&[
                    &chunk(ID_STRUCT, &u32s(&[0x1106])),
                    &chunk(ID_STRING, b"wheel\0\0\0"),
                    &chunk(ID_STRING, b"\0\0\0\0"),
                    &ext(&[]),
                ]),
            ));
        }
        body.extend(ext(&[]));
        chunk(ID_MATERIAL, &body)
    }

    fn geometry(bad_index: bool) -> Vec<u8> {
        let flags = geo::TEXTURED | geo::PRELIT | geo::NORMALS | geo::POSITIONS;
        let mut st = u32s(&[flags, 1, 3, 1]);
        st.extend(std::iter::repeat_n(200u8, 12)); // prelit
        st.extend(f32s(&[0., 0., 1., 0., 0., 1.])); // uv
        let v2 = if bad_index { 9 } else { 2 };
        st.extend(u32s(&[1, (v2 << 16) | 1])); // v0=0 v1=1 v2 mat=1
        st.extend(f32s(&[0., 0., 0., 2.]));
        st.extend(u32s(&[1, 1]));
        st.extend(f32s(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]));
        st.extend(f32s(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]));
        let mut ml = u32s(&[2, u32::MAX, u32::MAX]);
        ml = chunk(ID_STRUCT, &ml);
        ml.extend(material(false));
        ml.extend(material(true));
        chunk(
            ID_GEOMETRY,
            &cat(&[
                &chunk(ID_STRUCT, &st),
                &chunk(ID_MATLIST, &ml),
                &ext(&[(0x50E, &[1, 2])]),
            ]),
        )
    }

    fn clump(bad_index: bool) -> Vec<u8> {
        let geolist = cat(&[&chunk(ID_STRUCT, &u32s(&[1])), &geometry(bad_index)]);
        let atomic = cat(&[&chunk(ID_STRUCT, &u32s(&[1, 0, 5, 0])), &ext(&[])]);
        chunk(
            ID_CLUMP,
            &cat(&[
                &chunk(ID_STRUCT, &u32s(&[1, 0, 0])),
                &chunk(ID_FRAMELIST, &frame_list()),
                &chunk(ID_GEOMETRYLIST, &geolist),
                &chunk(ID_ATOMIC, &atomic),
                &ext(&[]),
            ]),
        )
    }

    #[test]
    fn parses_clump() {
        let c = parse(&clump(false)).unwrap();
        assert_eq!(c.frames.len(), 2);
        assert_eq!(c.frame_by_name("WHEEL_LF"), Some(1));
        assert_eq!(c.frames[1].parent, 0);
        assert_eq!(c.frames[1].position, [0., 0., 1.]);
        let g = &c.geometries[0];
        assert_eq!(g.num_vertices, 3);
        assert_eq!(g.prelit[2], [200; 4]);
        assert_eq!(g.tex_coords.len(), 1);
        assert_eq!(
            g.triangles[0],
            Triangle {
                v: [0, 1, 2],
                material: 1
            }
        );
        assert_eq!(g.morph_targets[0].vertices[1], [1., 0., 0.]);
        assert_eq!(g.morph_targets[0].normals.len(), 3);
        assert_eq!(g.materials.len(), 2);
        assert_eq!(g.materials[1].texture.as_ref().unwrap().name, "wheel");
        assert_eq!(g.materials[0].color, [255, 128, 0, 255]);
        assert_eq!(g.plugins, vec![(0x50E, vec![1, 2])]);
        assert_eq!(
            c.atomics[0],
            Atomic {
                frame: 1,
                geometry: 0,
                flags: 5,
                plugins: vec![]
            }
        );
    }

    #[test]
    fn composes_frames() {
        let c = parse(&clump(false)).unwrap();
        // The root is at z 0, frame 1 one unit above it.
        assert_eq!(c.world_transform(0)[3], [0.0, 0.0, 0.0]);
        assert_eq!(c.world_transform(1)[3], [0.0, 0.0, 1.0]);
        let turn: Transform = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [5.0, 0.0, 0.0]];
        assert_eq!(transform_point(&compose(&turn, &IDENTITY), [1.0, 0.0, 0.0]), [5.0, 1.0, 0.0]);
    }

    #[test]
    fn rejects_bad_vertex_index() {
        assert!(parse(&clump(true)).is_err());
    }
}

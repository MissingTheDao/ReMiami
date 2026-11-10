//! DFF clumps read in Rust (plan: docs/RENDERER_PORT.md, stage 2 step 2a).
//!
//! A librw-faithful reader: it keeps everything librw's stream readers use (raw geometry flags,
//! bounding spheres, which material-list entries share a material, struct versions) so the C++
//! side (rw/DffRead.cpp) can build exactly the objects `Clump::streamRead` builds. Extension
//! chunks are passed through whole: librw's plugin readers (skin, MatFX, BinMesh, HAnim, the
//! game's NodeName ...) still run on them, until they move here one by one.

use revc_formats::bytes::Reader;
use revc_formats::rw::{self, Chunk, ID_ATOMIC, ID_CLUMP, ID_EXTENSION, ID_FRAMELIST, ID_GEOMETRY, ID_GEOMETRYLIST, ID_MATERIAL, ID_MATLIST, ID_STRUCT, ID_TEXTURE};

const ID_STRING: u32 = 0x02;

/// Raw extension chunk (header included), or empty when the object has none.
#[repr(C)]
pub struct Ext {
    pub data: *const u8,
    pub len: u32,
}

/// HAnim plugin (0x11E) of a frame, version 0x100 only (else nil: librw's reader).
#[repr(C)]
pub struct DffHAnim {
    pub id: i32,
    pub num_nodes: i32,
    pub flags: i32,
    pub max_key_size: i32,
    pub node_ids: *const i32,
    pub node_flags: *const i32,
}

#[repr(C)]
pub struct DffFrame {
    /// right, up, at, pos
    pub m: [f32; 12],
    pub parent: i32,
    /// the game's NodeName plugin (0x253F2FE): name bytes as stored (not terminated), nil if none
    pub name: *const u8,
    pub name_len: u32,
    pub hanim: *const DffHAnim,
    pub ext: Ext,
}

#[repr(C)]
pub struct DffTexture {
    /// NUL-terminated, as read from the string chunks (at most 127 bytes kept, like librw).
    pub name: [u8; 128],
    pub mask: [u8; 128],
    pub filter_addressing: u32,
    pub ext: Ext,
}

/// One MatFX effect slot (librw readMaterialMatFX): type 1 bump (tex1 bumped, tex2 base), 2 env
/// (tex1), 4 dual (tex1); 0 none.
#[repr(C)]
pub struct DffFxSlot {
    pub kind: u32,
    pub coefficient: f32,
    pub fb_alpha: i32,
    pub src_blend: i32,
    pub dst_blend: i32,
    pub has_tex1: i32,
    pub has_tex2: i32,
    pub tex1: DffTexture,
    pub tex2: DffTexture,
}

#[repr(C)]
pub struct DffMatFx {
    pub effects: u32,
    pub slots: [DffFxSlot; 2],
}

#[repr(C)]
pub struct DffMaterial {
    pub color: [u8; 4],
    /// ambient, specular, diffuse (unused when `default_surface` is set)
    pub surface: [f32; 3],
    /// material struct older than 3.4: librw uses the geometry's (default) surface properties
    pub default_surface: i32,
    pub textured: i32,
    pub texture: DffTexture,
    /// MatFX plugin (0x120), nil if none
    pub matfx: *const DffMatFx,
    /// right-to-render plugin (0x1F): `has_rights` and the two words
    pub has_rights: i32,
    pub rights: [u32; 2],
    pub ext: Ext,
}

#[repr(C)]
pub struct DffMorph {
    pub sphere: [f32; 4],
    pub has_vertices: i32,
    pub has_normals: i32,
    pub vertices: *const f32,
    pub normals: *const f32,
}

/// BinMesh plugin (0x50E): the geometry's triangles as index lists per material.
#[repr(C)]
pub struct DffMesh {
    pub num_indices: u32,
    pub mat_index: u32,
    /// already truncated to 16 bits, as librw's readMesh stores them
    pub indices: *const u16,
}

#[repr(C)]
pub struct DffBinMesh {
    pub flags: u32,
    pub num_meshes: u32,
    pub total_indices: u32,
    pub meshes: *const DffMesh,
}

/// Skin plugin (0x116), as librw's readSkin reads it: raw arrays.
#[repr(C)]
pub struct DffSkin {
    /// numBones, numUsedBones (0: old format), numWeights, unused
    pub header: [u8; 4],
    pub used_bones: *const u8,
    /// 4 bytes per vertex
    pub indices: *const u8,
    /// 4 floats per vertex (raw little-endian bytes)
    pub weights: *const u8,
    /// numBones matrices of 64 bytes, contiguous (old format: the 0xdeaddead markers removed)
    pub inverse_matrices: *const u8,
    /// split data (new format): bone limit, mesh count, RLE size, then numBones + 2*(meshes+rle) bytes
    pub bone_limit: i32,
    pub num_meshes: i32,
    pub rle_size: i32,
    pub split: *const u8,
}

#[repr(C)]
pub struct DffGeometry {
    /// GeoStreamData as stored: flags (with the texture coordinate set count in bits 16-23),
    /// triangles, vertices, morph targets.
    pub flags: u32,
    pub num_triangles: i32,
    pub num_vertices: i32,
    pub num_morph_targets: i32,
    /// surface properties in the geometry struct (version < 3.4), `has_surface` 1
    pub has_surface: i32,
    pub surface: [f32; 3],
    /// raw arrays (not NATIVE): 4 bytes per vertex; 2 floats per vertex and set; 2 u32 per triangle
    pub colors: *const u8,
    pub num_tex_sets: i32,
    pub tex_coords: [*const f32; 8],
    pub triangles: *const u32,
    pub morphs: *const DffMorph,
    /// material list: index per entry (-1: next new material, else an earlier entry)
    pub num_mat_entries: i32,
    pub mat_indices: *const i32,
    pub materials: *const DffMaterial,
    /// parsed BinMesh plugin (nil: none, or NATIVE geometry: librw's reader handles it)
    pub binmesh: *const DffBinMesh,
    pub skin: *const DffSkin,
    pub ext: Ext,
}

#[repr(C)]
pub struct DffAtomic {
    pub frame: i32,
    pub geometry: i32,
    pub flags: i32,
    /// MatFX plugin (0x120): -1 none, else the stored value (non-zero enables effects)
    pub matfx: i32,
    pub has_rights: i32,
    pub rights: [u32; 2],
    pub ext: Ext,
}

#[repr(C)]
pub struct DffLight {
    pub frame: i32,
    /// struct version (minusCosAngle is stored as a tangent before 3.3)
    pub version: u32,
    /// radius, red, green, blue, minusCosAngle
    pub data: [f32; 5],
    pub type_flags: u32,
    pub ext: Ext,
}

#[repr(C)]
pub struct DffClump {
    pub num_atomics: i32,
    pub num_frames: i32,
    pub frames: *const DffFrame,
    pub num_geometries: i32,
    pub geometries: *const DffGeometry,
    pub atomics: *const DffAtomic,
    pub num_lights: i32,
    pub lights: *const DffLight,
    pub ext: Ext,
}

/// Everything the views point into, kept alive until revc_dff_free.
struct Owned {
    clump: DffClump,
    _frames: Vec<DffFrame>,
    _geometries: Vec<DffGeometry>,
    _atomics: Vec<DffAtomic>,
    _lights: Vec<DffLight>,
    _morphs: Vec<Vec<DffMorph>>,
    _materials: Vec<Vec<DffMaterial>>,
    _indices: Vec<Vec<i32>>,
    _floats: Vec<Vec<f32>>,
    _words: Vec<Vec<u32>>,
    _bytes: Vec<Vec<u8>>,
    _binmeshes: Vec<Box<DffBinMesh>>,
    _meshes: Vec<Vec<DffMesh>>,
    _u16s: Vec<Vec<u16>>,
    _hanims: Vec<Box<DffHAnim>>,
    _i32s: Vec<Vec<i32>>,
    _skins: Vec<Box<DffSkin>>,
    _matfx: Vec<Box<DffMatFx>>,
}

const NO_TEXTURE: DffTexture = DffTexture { name: [0; 128], mask: [0; 128], filter_addressing: 0, ext: Ext { data: std::ptr::null(), len: 0 } };

fn empty_slot() -> DffFxSlot {
    DffFxSlot { kind: 0, coefficient: 0.0, fb_alpha: 0, src_blend: 0, dst_blend: 0, has_tex1: 0, has_tex2: 0, tex1: NO_TEXTURE, tex2: NO_TEXTURE }
}

/// Right-to-render (0x1F) words of an object, if it has the plugin.
fn rights_of(parent: &Chunk) -> Option<[u32; 2]> {
    let p = plugin_chunks(parent).into_iter().find(|p| p.id == 0x1F)?;
    let mut r = p.reader();
    Some([r.u32("rights").ok()?, r.u32("rights").ok()?])
}

const NO_EXT: Ext = Ext { data: std::ptr::null(), len: 0 };

fn err(s: impl Into<String>) -> String {
    s.into()
}

/// The chunk's raw bytes including its 12-byte header, found among `parent`'s children.
fn ext_of(parent: &Chunk, all: &[u8]) -> Ext {
    match parent.child(ID_EXTENSION) {
        Ok(e) => {
            let body = e.data;
            // the header is the 12 bytes in front of the body
            let start = body.as_ptr() as usize - all.as_ptr() as usize - 12;
            Ext { data: all[start..].as_ptr(), len: (body.len() + 12) as u32 }
        }
        Err(_) => NO_EXT,
    }
}

fn floats(r: &mut Reader, n: usize, what: &'static str) -> Result<Vec<f32>, String> {
    (0..n).map(|_| r.f32(what).map_err(|e| e.to_string())).collect()
}

/// The plugin chunks inside an object's EXTENSION chunk.
fn plugin_chunks<'a>(parent: &Chunk<'a>) -> Vec<Chunk<'a>> {
    match parent.child(ID_EXTENSION) {
        Ok(e) => e.children().filter_map(|c| c.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn read_string(c: &Chunk, out: &mut [u8; 128]) {
    let n = c.data.len().min(127);
    out[..n].copy_from_slice(&c.data[..n]);
    out[n] = 0;
}

struct Builder<'a> {
    all: &'a [u8],
    o: Owned,
}

impl<'a> Builder<'a> {
    fn texture(&mut self, c: &Chunk<'a>) -> Result<DffTexture, String> {
        let mut kids = c.children();
        let st = kids.next().ok_or("texture without struct")?.map_err(|e| e.to_string())?;
        let filter_addressing = st.reader().u32("texture filter").map_err(|e| e.to_string())?;
        let mut t = DffTexture { name: [0; 128], mask: [0; 128], filter_addressing, ext: ext_of(c, self.all) };
        let strings: Vec<Chunk> = kids.filter_map(|k| k.ok()).filter(|k| k.id == ID_STRING).collect();
        if strings.len() < 2 {
            return Err(err("texture without name/mask strings"));
        }
        read_string(&strings[0], &mut t.name);
        read_string(&strings[1], &mut t.mask);
        Ok(t)
    }

    fn material(&mut self, c: &Chunk<'a>) -> Result<DffMaterial, String> {
        let st = c.child(ID_STRUCT).map_err(|e| e.to_string())?;
        let mut r = st.reader();
        let e = |x: revc_formats::Error| x.to_string();
        r.skip(4).map_err(e)?;
        let col = r.bytes(4, "material colour").map_err(e)?;
        r.skip(4).map_err(e)?;
        let textured = r.i32("material textured").map_err(e)?;
        let default_surface = (st.version() < 0x30400) as i32;
        let surface = if default_surface != 0 { [0.0; 3] } else { [r.f32("ambient").map_err(e)?, r.f32("specular").map_err(e)?, r.f32("diffuse").map_err(e)?] };
        let texture = if textured != 0 {
            self.texture(&c.child(ID_TEXTURE).map_err(|e| e.to_string())?)?
        } else {
            NO_TEXTURE
        };
        let mut matfx = std::ptr::null();
        if let Some(p) = plugin_chunks(c).into_iter().find(|p| p.id == 0x120) {
            let mut r = p.reader();
            let mut fx = Box::new(DffMatFx { effects: r.u32("matfx effects").map_err(e)?, slots: [empty_slot(), empty_slot()] });
            for slot in fx.slots.iter_mut() {
                slot.kind = r.u32("matfx type").map_err(e)?;
                // a nested TEXTURE chunk where the flag says so
                let mut next_texture = |r: &mut Reader<'a>, this: &mut Builder<'a>| -> Result<(i32, DffTexture), String> {
                    if r.i32("matfx has texture").map_err(e)? == 0 {
                        return Ok((0, NO_TEXTURE));
                    }
                    loop {
                        let k = rw::read_chunk(r).map_err(e)?;
                        if k.id == ID_TEXTURE {
                            return Ok((1, this.texture(&k)?));
                        }
                    }
                };
                match slot.kind {
                    1 => {
                        slot.coefficient = r.f32("bump coefficient").map_err(e)?;
                        (slot.has_tex1, slot.tex1) = next_texture(&mut r, self)?;
                        (slot.has_tex2, slot.tex2) = next_texture(&mut r, self)?;
                    }
                    2 => {
                        slot.coefficient = r.f32("env coefficient").map_err(e)?;
                        slot.fb_alpha = r.i32("env fb alpha").map_err(e)?;
                        (slot.has_tex1, slot.tex1) = next_texture(&mut r, self)?;
                    }
                    4 => {
                        slot.src_blend = r.i32("dual src").map_err(e)?;
                        slot.dst_blend = r.i32("dual dst").map_err(e)?;
                        (slot.has_tex1, slot.tex1) = next_texture(&mut r, self)?;
                    }
                    _ => {}
                }
            }
            matfx = &*fx as *const DffMatFx;
            self.o._matfx.push(fx);
        }
        let (has_rights, rights) = match rights_of(c) {
            Some(w) => (1, w),
            None => (0, [0; 2]),
        };
        Ok(DffMaterial { color: [col[0], col[1], col[2], col[3]], surface, default_surface, textured, texture, matfx, has_rights, rights, ext: ext_of(c, self.all) })
    }

    fn geometry(&mut self, c: &Chunk<'a>) -> Result<DffGeometry, String> {
        let e = |x: revc_formats::Error| x.to_string();
        let st = c.child(ID_STRUCT).map_err(e)?;
        let mut r = st.reader();
        let flags = r.u32("geometry flags").map_err(e)?;
        let num_triangles = r.i32("geometry triangles").map_err(e)?;
        let num_vertices = r.i32("geometry vertices").map_err(e)?;
        let num_morph_targets = r.i32("geometry morph targets").map_err(e)?;
        let (nt, nv) = (num_triangles.max(0) as usize, num_vertices.max(0) as usize);
        let has_surface = (st.version() < 0x34000) as i32;
        let surface = if has_surface != 0 { [r.f32("s").map_err(e)?, r.f32("s").map_err(e)?, r.f32("s").map_err(e)?] } else { [0.0; 3] };
        // as Geometry::create: the set count from the flags byte, else TEXTURED / TEXTURED2
        let mut num_tex_sets = ((flags >> 16) & 0xFF) as i32;
        if num_tex_sets == 0 {
            num_tex_sets = if flags & 0x04 != 0 { 1 } else if flags & 0x80 != 0 { 2 } else { 0 };
        }
        let native = flags & 0x0100_0000 != 0;
        let mut colors = std::ptr::null();
        let mut tex_coords = [std::ptr::null(); 8];
        let mut triangles = std::ptr::null();
        if !native {
            if flags & 0x08 != 0 {
                let b = r.bytes(4 * nv, "prelit colours").map_err(e)?.to_vec();
                colors = b.as_ptr();
                self.o._bytes.push(b);
            }
            for set in tex_coords.iter_mut().take(num_tex_sets.min(8) as usize) {
                let f = floats(&mut r, 2 * nv, "uv")?;
                *set = f.as_ptr();
                self.o._floats.push(f);
            }
            let w: Vec<u32> = (0..2 * nt).map(|_| r.u32("triangle").map_err(e)).collect::<Result<_, _>>()?;
            triangles = w.as_ptr();
            self.o._words.push(w);
        }
        let mut morphs = Vec::new();
        for _ in 0..num_morph_targets.max(0) {
            let sphere = [r.f32("s").map_err(e)?, r.f32("s").map_err(e)?, r.f32("s").map_err(e)?, r.f32("s").map_err(e)?];
            let has_vertices = r.i32("has vertices").map_err(e)?;
            let has_normals = r.i32("has normals").map_err(e)?;
            let mut m = DffMorph { sphere, has_vertices, has_normals, vertices: std::ptr::null(), normals: std::ptr::null() };
            if has_vertices != 0 {
                let f = floats(&mut r, 3 * nv, "vertex")?;
                m.vertices = f.as_ptr();
                self.o._floats.push(f);
            }
            if has_normals != 0 {
                let f = floats(&mut r, 3 * nv, "normal")?;
                m.normals = f.as_ptr();
                self.o._floats.push(f);
            }
            morphs.push(m);
        }
        // material list
        let ml = c.child(ID_MATLIST).map_err(e)?;
        let mut kids = ml.children();
        let mst = kids.next().ok_or("empty material list")?.map_err(e)?;
        let mut mr = mst.reader();
        let n = mr.i32("material count").map_err(e)?.max(0) as usize;
        let indices: Vec<i32> = (0..n).map(|_| mr.i32("material index").map_err(e)).collect::<Result<_, _>>()?;
        let mut mats = Vec::new();
        for &idx in &indices {
            if idx >= 0 {
                if idx as usize >= mats.len().max(1) && idx as usize >= n {
                    return Err(err(format!("material index {idx} out of range")));
                }
                continue;
            }
            let m = loop {
                let k = kids.next().ok_or("material list ends early")?.map_err(e)?;
                if k.id == ID_MATERIAL {
                    break k;
                }
            };
            mats.push(self.material(&m)?);
        }
        let mut binmesh = std::ptr::null();
        if !native {
            if let Some(bm) = plugin_chunks(c).into_iter().find(|p| p.id == 0x50E) {
                let mut r = bm.reader();
                let bflags = r.u32("binmesh flags").map_err(e)?;
                let num_meshes = r.u32("binmesh meshes").map_err(e)?;
                let total_indices = r.u32("binmesh indices").map_err(e)?;
                let mut meshes = Vec::with_capacity(num_meshes as usize);
                let mut all = Vec::with_capacity(total_indices as usize);
                let mut starts = Vec::new();
                for _ in 0..num_meshes {
                    let ni = r.u32("mesh indices").map_err(e)?;
                    let mat_index = r.u32("mesh material").map_err(e)?;
                    if mat_index as usize >= n {
                        return Err(err(format!("mesh material {mat_index} of {n}")));
                    }
                    starts.push(all.len());
                    for _ in 0..ni {
                        all.push(r.u32("mesh index").map_err(e)? as u16);
                    }
                    meshes.push(DffMesh { num_indices: ni, mat_index, indices: std::ptr::null() });
                }
                for (m, s) in meshes.iter_mut().zip(starts) {
                    m.indices = all[s..].as_ptr();
                }
                let b = Box::new(DffBinMesh { flags: bflags, num_meshes, total_indices, meshes: meshes.as_ptr() });
                binmesh = &*b as *const DffBinMesh;
                self.o._u16s.push(all);
                self.o._meshes.push(meshes);
                self.o._binmeshes.push(b);
            }
        }
        let mut skin = std::ptr::null();
        if let Some(sk) = plugin_chunks(c).into_iter().find(|p| p.id == 0x116) {
            let mut r = sk.reader();
            let h = r.bytes(4, "skin header").map_err(e)?;
            let header = [h[0], h[1], h[2], h[3]];
            let old = header[1] == 0;
            let nb = header[0] as usize;
            let used = if old { std::ptr::null() } else { r.bytes(header[1] as usize, "used bones").map_err(e)?.as_ptr() };
            let indices = r.bytes(4 * nv, "skin indices").map_err(e)?.as_ptr();
            let weights = r.bytes(16 * nv, "skin weights").map_err(e)?.as_ptr();
            let mut mats = Vec::with_capacity(64 * nb);
            for _ in 0..nb {
                if old {
                    r.skip(4).map_err(e)?;
                }
                mats.extend_from_slice(r.bytes(64, "inverse matrix").map_err(e)?);
            }
            let (mut bone_limit, mut num_meshes, mut rle_size, mut split) = (0, 0, 0, std::ptr::null());
            if !old {
                bone_limit = r.i32("bone limit").map_err(e)?;
                num_meshes = r.i32("skin meshes").map_err(e)?;
                rle_size = r.i32("rle size").map_err(e)?;
                if num_meshes != 0 {
                    let sz = nb + 2 * (num_meshes.max(0) as usize + rle_size.max(0) as usize);
                    split = r.bytes(sz, "skin split data").map_err(e)?.as_ptr();
                }
            }
            let s = Box::new(DffSkin { header, used_bones: used, indices, weights, inverse_matrices: mats.as_ptr(), bone_limit, num_meshes, rle_size, split });
            skin = &*s as *const DffSkin;
            self.o._bytes.push(mats);
            self.o._skins.push(s);
        }
        let g = DffGeometry {
            flags,
            num_triangles,
            num_vertices,
            num_morph_targets,
            has_surface,
            surface,
            colors,
            num_tex_sets,
            tex_coords,
            triangles,
            morphs: morphs.as_ptr(),
            num_mat_entries: n as i32,
            mat_indices: indices.as_ptr(),
            materials: mats.as_ptr(),
            binmesh,
            skin,
            ext: ext_of(c, self.all),
        };
        self.o._morphs.push(morphs);
        self.o._indices.push(indices);
        self.o._materials.push(mats);
        Ok(g)
    }

    /// The clump's extension: inside the CLUMP chunk, or (older files) the EXTENSION chunk right
    /// after it, where librw's reader finds it too.
    fn clump_ext(&self, c: &Chunk<'a>) -> Ext {
        let inside = ext_of(c, self.all);
        if inside.len != 0 {
            return inside;
        }
        let after = 12 + c.data.len();
        match self.all.get(after..after + 12) {
            Some(h) if u32::from_le_bytes(h[0..4].try_into().unwrap()) == ID_EXTENSION => {
                let n = u32::from_le_bytes(h[4..8].try_into().unwrap()) as usize;
                match self.all.get(after..after + 12 + n) {
                    Some(x) => Ext { data: x.as_ptr(), len: x.len() as u32 },
                    None => NO_EXT,
                }
            }
            _ => NO_EXT,
        }
    }

    fn clump(mut self, c: &Chunk<'a>) -> Result<Owned, String> {
        let e = |x: revc_formats::Error| x.to_string();
        let mut kids = c.children();
        let st = kids.next().ok_or("clump without struct")?.map_err(e)?;
        if st.version() <= 0x33000 && st.version() < 0x30400 {
            return Err(err("clump older than RW 3.4"));
        }
        let mut sr = st.reader();
        let num_atomics = sr.i32("atomics").map_err(e)?;
        if st.version() > 0x33000 {
            let lights = sr.i32("lights").map_err(e)?;
            let cameras = sr.i32("cameras").map_err(e)?;
            if cameras != 0 {
                return Err(err("clump with cameras"));
            }
            let _ = lights;
        }
        let mut frames = Vec::new();
        let mut geometries = Vec::new();
        let mut atomics = Vec::new();
        let mut lights = Vec::new();
        let mut light_frame = -1;
        for k in kids {
            let k = k.map_err(e)?;
            match k.id {
                ID_FRAMELIST => {
                    let fst = k.child(ID_STRUCT).map_err(e)?;
                    let mut r = fst.reader();
                    let n = r.i32("frames").map_err(e)?.max(0);
                    for _ in 0..n {
                        let m: Vec<f32> = floats(&mut r, 12, "frame matrix")?;
                        let parent = r.i32("frame parent").map_err(e)?;
                        r.skip(4).map_err(e)?; // matrix flags
                        let mut mm = [0.0; 12];
                        mm.copy_from_slice(&m);
                        frames.push(DffFrame { m: mm, parent, name: std::ptr::null(), name_len: 0, hanim: std::ptr::null(), ext: NO_EXT });
                    }
                    // one extension per frame, in order, after the struct
                    let exts: Vec<Chunk> = k.children().filter_map(|x| x.ok()).filter(|x| x.id == ID_EXTENSION).collect();
                    for (f, x) in frames.iter_mut().zip(exts) {
                        let start = x.data.as_ptr() as usize - self.all.as_ptr() as usize - 12;
                        f.ext = Ext { data: self.all[start..].as_ptr(), len: (x.data.len() + 12) as u32 };
                        for p in x.children().filter_map(|c| c.ok()) {
                            match p.id {
                                0x253F2FE if p.data.len() <= 23 => {
                                    f.name = p.data.as_ptr();
                                    f.name_len = p.data.len() as u32;
                                }
                                0x11E => {
                                    let mut r = p.reader();
                                    if r.i32("hanim version").map_err(e)? != 0x100 {
                                        continue;
                                    }
                                    let id = r.i32("hanim id").map_err(e)?;
                                    let num_nodes = r.i32("hanim nodes").map_err(e)?;
                                    let (mut flags, mut max_key_size) = (0, 0);
                                    let (mut ids, mut nflags) = (Vec::new(), Vec::new());
                                    if num_nodes != 0 {
                                        flags = r.i32("hanim flags").map_err(e)?;
                                        max_key_size = r.i32("hanim key size").map_err(e)?;
                                        for _ in 0..num_nodes.max(0) {
                                            ids.push(r.i32("node id").map_err(e)?);
                                            r.skip(4).map_err(e)?; // index, unused
                                            nflags.push(r.i32("node flags").map_err(e)?);
                                        }
                                    }
                                    let h = Box::new(DffHAnim { id, num_nodes, flags, max_key_size, node_ids: ids.as_ptr(), node_flags: nflags.as_ptr() });
                                    f.hanim = &*h as *const DffHAnim;
                                    self.o._i32s.push(ids);
                                    self.o._i32s.push(nflags);
                                    self.o._hanims.push(h);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ID_GEOMETRYLIST => {
                    for g in k.children() {
                        let g = g.map_err(e)?;
                        if g.id == ID_GEOMETRY {
                            geometries.push(self.geometry(&g)?);
                        }
                    }
                }
                ID_ATOMIC => {
                    let ast = k.child(ID_STRUCT).map_err(e)?;
                    let mut r = ast.reader();
                    let frame = r.i32("atomic frame").map_err(e)?;
                    let geometry = r.i32("atomic geometry").map_err(e)?;
                    let flags = r.i32("atomic flags").map_err(e)?;
                    if frame < 0 || frame as usize >= frames.len() || geometry < 0 || geometry as usize >= geometries.len() {
                        return Err(err("atomic frame/geometry out of range"));
                    }
                    let matfx = match plugin_chunks(&k).into_iter().find(|p| p.id == 0x120) {
                        Some(p) => p.reader().i32("atomic matfx").map_err(e)?,
                        None => -1,
                    };
                    let (has_rights, rights) = match rights_of(&k) {
                        Some(w) => (1, w),
                        None => (0, [0; 2]),
                    };
                    atomics.push(DffAtomic { frame, geometry, flags, matfx, has_rights, rights, ext: ext_of(&k, self.all) });
                }
                // a light: STRUCT with its frame index, then the LIGHT chunk
                ID_STRUCT => light_frame = k.reader().i32("light frame").map_err(e)?,
                0x12 => {
                    let lst = k.child(ID_STRUCT).map_err(e)?;
                    let mut r = lst.reader();
                    let data = [r.f32("l").map_err(e)?, r.f32("l").map_err(e)?, r.f32("l").map_err(e)?, r.f32("l").map_err(e)?, r.f32("l").map_err(e)?];
                    let type_flags = r.u32("light type").map_err(e)?;
                    if light_frame < 0 || light_frame as usize >= frames.len() {
                        return Err(err("light frame out of range"));
                    }
                    lights.push(DffLight { frame: light_frame, version: lst.version(), data, type_flags, ext: ext_of(&k, self.all) });
                }
                _ => {}
            }
        }
        if frames.is_empty() || atomics.len() != num_atomics.max(0) as usize {
            return Err(err(format!("clump says {num_atomics} atomics, has {} ({} frames)", atomics.len(), frames.len())));
        }
        self.o.clump = DffClump {
            num_atomics,
            num_frames: frames.len() as i32,
            frames: frames.as_ptr(),
            num_geometries: geometries.len() as i32,
            geometries: geometries.as_ptr(),
            atomics: atomics.as_ptr(),
            num_lights: lights.len() as i32,
            lights: lights.as_ptr(),
            ext: self.clump_ext(c),
        };
        self.o._lights = lights;
        self.o._frames = frames;
        self.o._geometries = geometries;
        self.o._atomics = atomics;
        Ok(self.o)
    }
}

/// A clump read from `data`, which must start with the CLUMP chunk header and stay alive (and
/// unchanged) until revc_dff_free: extension views point into it.
fn read(data: &[u8]) -> Result<Box<Owned>, String> {
    let c = rw::root(data).map_err(|e| e.to_string())?;
    if c.id != ID_CLUMP {
        return Err(err(format!("not a clump (chunk {:#x})", c.id)));
    }
    let b = Builder {
        all: data,
        o: Owned {
            clump: DffClump { num_atomics: 0, num_frames: 0, frames: std::ptr::null(), num_geometries: 0, geometries: std::ptr::null(), atomics: std::ptr::null(), num_lights: 0, lights: std::ptr::null(), ext: NO_EXT },
            _frames: Vec::new(),
            _geometries: Vec::new(),
            _atomics: Vec::new(),
            _lights: Vec::new(),
            _morphs: Vec::new(),
            _materials: Vec::new(),
            _indices: Vec::new(),
            _floats: Vec::new(),
            _words: Vec::new(),
            _bytes: Vec::new(),
            _binmeshes: Vec::new(),
            _meshes: Vec::new(),
            _u16s: Vec::new(),
            _hanims: Vec::new(),
            _i32s: Vec::new(),
            _skins: Vec::new(),
            _matfx: Vec::new(),
        },
    };
    b.clump(&c).map(Box::new)
}

/// # Safety
/// `data` must point to `len` bytes that outlive the returned handle. Returns nil on error
/// (logged); otherwise `*out` points at the clump view until revc_dff_free(handle).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_dff_read(data: *const u8, len: usize, out: *mut *const DffClump) -> *mut std::ffi::c_void {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    match read(data) {
        Ok(o) => {
            let p = Box::into_raw(o);
            unsafe { *out = &(*p).clump };
            p.cast()
        }
        Err(e) => {
            eprintln!("revc: DFF: {e}");
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `handle` from revc_dff_read, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_dff_free(handle: *mut std::ffi::c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Owned>()) });
    }
}

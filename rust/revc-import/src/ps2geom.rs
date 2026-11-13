//! PS2 native geometry (Vice City's PS2 DFFs) converted to ordinary RenderWare geometry, so the
//! game loads the PS2 models like the PC ones (the "exact PS2 models" option: the PS2 style with
//! the PS2's own vertex lighting).
//!
//! A PS2 geometry keeps its counts and morph target header but no vertex arrays; each mesh is a
//! DMA chain of VIF packets (the `NativeData` extension) that unpacks the vertices batch by batch
//! into VU memory. Converted here is the default material pipeline, the one the map models use:
//! per batch position (V3-32), UV (V2-32), RGBA (V4-8, 0..255) and normal (V3-8, n/128), strip
//! batches overlapping by two vertices. Anything else (skinned peds, vehicles, broken-out
//! attributes) is an error, and the PC model stays in use. Ported from librw's PS2 uninstance
//! (src/ps2/ps2.cpp, MIT).

use std::collections::HashMap;

const ID_STRUCT: u32 = 1;
const ID_EXT: u32 = 3;
const ID_CLUMP: u32 = 0x10;
const ID_GEOMLIST: u32 = 0x1a;
const ID_GEOMETRY: u32 = 0x0f;
const ID_BINMESH: u32 = 0x50e;
const ID_NATIVEDATA: u32 = 0x510;

const GEO_TRISTRIP: u16 = 0x01;
const GEO_PRELIT: u16 = 0x08;
const GEO_NORMALS: u16 = 0x10;

fn u16_at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "truncated".to_string())
}
fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "truncated".to_string())
}
fn f32_at(d: &[u8], o: usize) -> Result<f32, String> {
    u32_at(d, o).map(f32::from_bits)
}

/// RenderWare's library id to version (librw libraryIDUnpackVersion).
fn unpack_version(libid: u32) -> u32 {
    if libid & 0xFFFF_0000 != 0 { (((libid >> 14) & 0x3FF00) + 0x30000) | ((libid >> 16) & 0x3F) } else { libid << 8 }
}

/// A chunk: type, version stamp, body range.
#[derive(Clone, Copy)]
struct Chunk {
    ty: u32,
    ver: u32,
    start: usize,
    end: usize,
}

fn chunks(d: &[u8], mut o: usize, end: usize) -> Result<Vec<Chunk>, String> {
    let mut out = Vec::new();
    while o + 12 <= end {
        let (ty, size, ver) = (u32_at(d, o)?, u32_at(d, o + 4)? as usize, u32_at(d, o + 8)?);
        if size > end - o - 12 {
            return Err(format!("chunk {ty:#x} overruns its parent"));
        }
        out.push(Chunk { ty, ver, start: o + 12, end: o + 12 + size });
        o += 12 + size;
    }
    Ok(out)
}

fn put_chunk(out: &mut Vec<u8>, ty: u32, ver: u32, body: &[u8]) {
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&ver.to_le_bytes());
    out.extend_from_slice(body);
}

/// One decoded vertex: position, UV, prelit colour, normal (as in the stream).
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    rgba: [u8; 4],
    normal: [i8; 3],
}

impl Vertex {
    fn key(&self) -> ([u32; 5], [u8; 4], [i8; 3]) {
        let b = |f: f32| f.to_bits();
        ([b(self.pos[0]), b(self.pos[1]), b(self.pos[2]), b(self.uv[0]), b(self.uv[1])], self.rgba, self.normal)
    }
}

/// UNPACK commands of the default pipeline (V3-32 xyz, V2-32 uv, V4-8 unsigned rgba, V3-8
/// normal), their VU addresses 0..3.
const UNPACK_XYZ: u8 = 0x68;
const UNPACK_UV: u8 = 0x64;
const UNPACK_RGBA: u8 = 0x6e;
const UNPACK_NORMAL: u8 = 0x6a;

/// The vertices of one mesh's DMA chain, batch after batch (strip batches overlap by two).
fn mesh_vertices(data: &[u8], strip: bool) -> Result<Vec<Vertex>, String> {
    let word = |i: usize| u32_at(data, i * 4);
    let mut out: Vec<Vertex> = Vec::new();
    let mut batch: Vec<(u8, usize, usize)> = Vec::new(); // (command, count, byte offset)
    let flush = |batch: &mut Vec<(u8, usize, usize)>, out: &mut Vec<Vertex>| -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let find = |cmd: u8| batch.iter().find(|b| b.0 == cmd).copied();
        let xyz = find(UNPACK_XYZ).ok_or("batch without positions")?;
        let n = xyz.1;
        let mut verts = Vec::with_capacity(n);
        for i in 0..n {
            let p = xyz.2 + i * 12;
            let pos = [f32_at(data, p)?, f32_at(data, p + 4)?, f32_at(data, p + 8)?];
            let uv = match find(UNPACK_UV) {
                Some((_, m, o)) if i < m => [f32_at(data, o + i * 8)?, f32_at(data, o + i * 8 + 4)?],
                _ => [0.0, 0.0],
            };
            let rgba = match find(UNPACK_RGBA) {
                Some((_, m, o)) if i < m => data.get(o + i * 4..o + i * 4 + 4).ok_or("truncated colours")?.try_into().unwrap(),
                _ => [255; 4],
            };
            let normal = match find(UNPACK_NORMAL) {
                Some((_, m, o)) if i < m => {
                    let b = data.get(o + i * 3..o + i * 3 + 3).ok_or("truncated normals")?;
                    [b[0] as i8, b[1] as i8, b[2] as i8]
                }
                _ => [0; 3],
            };
            verts.push(Vertex { pos, uv, rgba, normal });
        }
        let skip = if strip && !out.is_empty() { 2.min(verts.len()) } else { 0 };
        out.extend_from_slice(&verts[skip..]);
        batch.clear();
        Ok(())
    };
    // VIF codes from word `i` to `end`: unpacks inline, batches end at MSCAL/MSCALF/MSCNT
    let vif = |mut i: usize, end: usize, batch: &mut Vec<(u8, usize, usize)>, out: &mut Vec<Vertex>| -> Result<(), String> {
        while i < end {
            let c = word(i)?;
            let cmd = (c >> 24) as u8;
            i += 1;
            if cmd & 0x60 == 0x60 {
                let (vn, vl) = ((cmd >> 2) & 3, cmd & 3);
                let num = match (c >> 16) & 0xff {
                    0 => 256,
                    n => n as usize,
                };
                let words = ((32usize >> vl) * (vn as usize + 1) * num).div_ceil(32);
                let cmd = cmd & !0x10; // (the mask bit)
                match cmd {
                    UNPACK_XYZ | UNPACK_UV | UNPACK_RGBA | UNPACK_NORMAL => batch.push((cmd, num, i * 4)),
                    other => return Err(format!("unpack {other:#x} (not the default pipeline)")),
                }
                i += words;
            } else {
                match cmd {
                    0x14 | 0x15 | 0x17 => flush(batch, out)?,
                    0x20 => i += 1,
                    0x30 | 0x31 => i += 4,
                    0x4a => i += 2 * ((c >> 16) & 0xff) as usize,
                    0x50 | 0x51 => i += 4 * match c & 0xffff {
                        0 => 65536,
                        n => n as usize,
                    },
                    _ => {}
                }
            }
        }
        Ok(())
    };
    let mut q = 0usize;
    while q * 16 < data.len() {
        let t0 = word(q * 4)?;
        let qwc = (t0 & 0xffff) as usize;
        match t0 & 0x7000_0000 {
            0x1000_0000 => {
                vif(q * 4 + 2, (q + 1 + qwc) * 4, &mut batch, &mut out)?;
                q += 1 + qwc;
            }
            0x6000_0000 => {
                vif(q * 4 + 2, (q + 1 + qwc) * 4, &mut batch, &mut out)?;
                break;
            }
            0x3000_0000 => return Err("broken-out attributes (DMA ref)".into()),
            _ => return Err(format!("unknown DMA tag {t0:#x}")),
        }
    }
    flush(&mut batch, &mut out)?;
    Ok(out)
}

/// A native PS2 geometry chunk body converted to a generic one (same chunk version).
fn convert_geometry(d: &[u8], g: Chunk) -> Result<Vec<u8>, String> {
    let kids = chunks(d, g.start, g.end)?;
    let st = kids.iter().find(|c| c.ty == ID_STRUCT).ok_or("geometry without struct")?;
    let flags = u16_at(d, st.start)?;
    let num_tex = d[st.start + 2];
    if d[st.start + 3] == 0 {
        return Err("not native".into());
    }
    if num_tex > 1 {
        return Err("two UV sets".into());
    }
    let version = unpack_version(st.ver);
    let mut p = st.start + 16;
    let surface = if version < 0x34000 {
        let s = d[p..p + 12].to_vec();
        p += 12;
        Some(s)
    } else {
        None
    };
    let sphere = d.get(p..p + 16).ok_or("truncated morph target")?.to_vec();
    let ext = kids.iter().find(|c| c.ty == ID_EXT).ok_or("geometry without extensions")?;
    let ext_kids = chunks(d, ext.start, ext.end)?;
    let bin = ext_kids.iter().find(|c| c.ty == ID_BINMESH).ok_or("no mesh header")?;
    let nat = ext_kids.iter().find(|c| c.ty == ID_NATIVEDATA).ok_or("no native data")?;
    let (mesh_flags, num_meshes) = (u32_at(d, bin.start)?, u32_at(d, bin.start + 4)? as usize);
    let strip = mesh_flags == 1;
    // (the struct's own size runs past the extension in the game's files; librw doesn't read it
    // either: the mesh entries say how long they are)
    if u32_at(d, nat.start)? != ID_STRUCT {
        return Err("native data without struct".into());
    }
    let nat_struct = Chunk { ty: ID_STRUCT, ver: 0, start: nat.start + 12, end: nat.end };
    if u32_at(d, nat_struct.start)? != 4 {
        return Err("not a PS2 geometry".into());
    }
    // vertices (shared between meshes when equal) and per-mesh index lists
    let mut verts: Vec<Vertex> = Vec::new();
    let mut lookup: HashMap<([u32; 5], [u8; 4], [i8; 3]), u32> = HashMap::new();
    let mut meshes: Vec<(u32, Vec<u32>)> = Vec::new();
    let mut o = nat_struct.start + 4;
    for m in 0..num_meshes {
        let (num_indices, material) = (u32_at(d, bin.start + 12 + 8 * m)? as usize, u32_at(d, bin.start + 16 + 8 * m)?);
        let size = u32_at(d, o)? as usize;
        let data = d.get(o + 8..o + 8 + size).ok_or("truncated mesh data")?;
        o += 8 + size;
        let mv = mesh_vertices(data, strip)?;
        if mv.len() != num_indices {
            return Err(format!("mesh {m}: {} vertices for {num_indices} indices", mv.len()));
        }
        let idx = mv
            .iter()
            .map(|v| {
                *lookup.entry(v.key()).or_insert_with(|| {
                    verts.push(*v);
                    verts.len() as u32 - 1
                })
            })
            .collect();
        meshes.push((material, idx));
    }
    if verts.len() > 0xFFFF {
        return Err("more than 65535 vertices".into());
    }
    // triangles from the strips (or lists), degenerate ones left out
    let mut tris: Vec<[u32; 4]> = Vec::new();
    for (mat, idx) in &meshes {
        if strip {
            for j in 0..idx.len().saturating_sub(2) {
                let (a, b, c) = if j & 1 == 1 { (idx[j + 1], idx[j], idx[j + 2]) } else { (idx[j], idx[j + 1], idx[j + 2]) };
                if a != b && b != c && a != c {
                    tris.push([a, b, c, *mat]);
                }
            }
        } else {
            for t in idx.chunks_exact(3) {
                tris.push([t[0], t[1], t[2], *mat]);
            }
        }
    }
    // the generic geometry struct
    let n = verts.len();
    let mut s = Vec::new();
    s.extend_from_slice(&flags.to_le_bytes());
    s.push(num_tex);
    s.push(0);
    s.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    s.extend_from_slice(&(n as u32).to_le_bytes());
    s.extend_from_slice(&1u32.to_le_bytes());
    if let Some(sp) = &surface {
        s.extend_from_slice(sp);
    }
    if flags & GEO_PRELIT != 0 {
        for v in &verts {
            s.extend_from_slice(&v.rgba);
        }
    }
    for _ in 0..num_tex {
        for v in &verts {
            s.extend_from_slice(&v.uv[0].to_le_bytes());
            s.extend_from_slice(&v.uv[1].to_le_bytes());
        }
    }
    for t in &tris {
        s.extend_from_slice(&((t[0] << 16) | t[1]).to_le_bytes());
        s.extend_from_slice(&((t[2] << 16) | t[3]).to_le_bytes());
    }
    s.extend_from_slice(&sphere);
    s.extend_from_slice(&1u32.to_le_bytes());
    let normals = flags & GEO_NORMALS != 0;
    s.extend_from_slice(&(normals as u32).to_le_bytes());
    for v in &verts {
        for c in v.pos {
            s.extend_from_slice(&c.to_le_bytes());
        }
    }
    if normals {
        for v in &verts {
            for c in v.normal {
                s.extend_from_slice(&(c as f32 / 128.0).to_le_bytes());
            }
        }
    }
    // the mesh header with its indices; the other extensions as they are, native data dropped
    let mut b = Vec::new();
    b.extend_from_slice(&mesh_flags.to_le_bytes());
    b.extend_from_slice(&(num_meshes as u32).to_le_bytes());
    b.extend_from_slice(&(meshes.iter().map(|m| m.1.len()).sum::<usize>() as u32).to_le_bytes());
    for (mat, idx) in &meshes {
        b.extend_from_slice(&(idx.len() as u32).to_le_bytes());
        b.extend_from_slice(&mat.to_le_bytes());
        for i in idx {
            b.extend_from_slice(&i.to_le_bytes());
        }
    }
    let mut ext_body = Vec::new();
    for c in &ext_kids {
        match c.ty {
            ID_NATIVEDATA => {}
            ID_BINMESH => put_chunk(&mut ext_body, ID_BINMESH, c.ver, &b),
            _ => put_chunk(&mut ext_body, c.ty, c.ver, &d[c.start..c.end]),
        }
    }
    let mut body = Vec::new();
    for c in &kids {
        match c.ty {
            ID_STRUCT if c.start == st.start => put_chunk(&mut body, ID_STRUCT, c.ver, &s),
            ID_EXT if c.start == ext.start => put_chunk(&mut body, ID_EXT, c.ver, &ext_body),
            _ => put_chunk(&mut body, c.ty, c.ver, &d[c.start..c.end]),
        }
    }
    let _ = GEO_TRISTRIP;
    Ok(body)
}

/// Rebuilds the chunks in `start..end`, converting every native geometry on the way.
fn rebuild(d: &[u8], start: usize, end: usize, out: &mut Vec<u8>, converted: &mut usize) -> Result<(), String> {
    for c in chunks(d, start, end)? {
        match c.ty {
            ID_CLUMP | ID_GEOMLIST => {
                let mut body = Vec::new();
                rebuild(d, c.start, c.end, &mut body, converted)?;
                put_chunk(out, c.ty, c.ver, &body);
            }
            ID_GEOMETRY => {
                let st = chunks(d, c.start, c.end)?.into_iter().find(|k| k.ty == ID_STRUCT).ok_or("geometry without struct")?;
                if d.get(st.start + 3).copied().unwrap_or(0) != 0 {
                    put_chunk(out, c.ty, c.ver, &convert_geometry(d, c)?);
                    *converted += 1;
                } else {
                    put_chunk(out, c.ty, c.ver, &d[c.start..c.end]);
                }
            }
            _ => put_chunk(out, c.ty, c.ver, &d[c.start..c.end]),
        }
    }
    Ok(())
}

/// A PS2 DFF (clump) with its native geometries converted; Err when it has none or one of them
/// uses another pipeline than the default (the PC model stays in use then).
pub fn convert_dff(d: &[u8]) -> Result<Vec<u8>, String> {
    // the file may carry padding after the clump (IMG sectors)
    let first = chunks(d, 0, d.len()).ok().and_then(|c| c.first().copied()).ok_or("not a RenderWare file")?;
    if first.ty != ID_CLUMP {
        return Err("not a clump".into());
    }
    let mut out = Vec::new();
    let mut converted = 0;
    rebuild(d, 0, first.end, &mut out, &mut converted)?;
    if converted == 0 {
        return Err("no PS2 geometry".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-mesh PS2 geometry holding a strip of 4 vertices in two batches of 3 (overlap 2).
    fn native_clump() -> Vec<u8> {
        let mut stream = Vec::new();
        let batch = |stream: &mut Vec<u8>, pts: &[[f32; 3]]| {
            let n = pts.len() as u32;
            let unpack = |stream: &mut Vec<u8>, cmd: u32, data: &[u8]| {
                stream.extend_from_slice(&(cmd << 24 | n << 16).to_le_bytes());
                stream.extend_from_slice(data);
                while stream.len() % 4 != 0 {
                    stream.push(0);
                }
            };
            let mut xyz = Vec::new();
            for p in pts {
                for c in p {
                    xyz.extend_from_slice(&c.to_le_bytes());
                }
            }
            unpack(stream, 0x68, &xyz);
            let uv: Vec<u8> = pts.iter().flat_map(|p| [p[0], p[1]]).flat_map(|f| f.to_le_bytes()).collect();
            unpack(stream, 0x64, &uv);
            let rgba: Vec<u8> = pts.iter().flat_map(|_| [10u8, 20, 30, 255]).collect();
            unpack(stream, 0x6e, &rgba);
            stream.extend_from_slice(&(0x15u32 << 24).to_le_bytes()); // MSCALF: batch end
        };
        batch(&mut stream, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        batch(&mut stream, &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]]);
        while stream.len() % 16 != 8 {
            stream.push(0);
        }
        let qwc = (stream.len() + 8) / 16 - 1; // (qwords after the tag qword)
        let mut dma = Vec::new();
        dma.extend_from_slice(&(0x6000_0000u32 | qwc as u32).to_le_bytes());
        dma.extend_from_slice(&0u32.to_le_bytes());
        dma.extend_from_slice(&stream);
        let ver = 0x0c02ffff;
        let mut nat = Vec::new();
        nat.extend_from_slice(&4u32.to_le_bytes());
        nat.extend_from_slice(&(dma.len() as u32).to_le_bytes());
        nat.extend_from_slice(&1u32.to_le_bytes());
        nat.extend_from_slice(&dma);
        let mut nat_chunk = Vec::new();
        put_chunk(&mut nat_chunk, ID_STRUCT, ver, &nat);
        let mut bin = Vec::new();
        for w in [1u32, 1, 4, 4, 0] {
            bin.extend_from_slice(&w.to_le_bytes());
        }
        let mut ext = Vec::new();
        put_chunk(&mut ext, ID_BINMESH, ver, &bin);
        put_chunk(&mut ext, ID_NATIVEDATA, ver, &nat_chunk);
        let mut st = Vec::new();
        st.extend_from_slice(&(0x0bu16).to_le_bytes()); // tristrip, positions, prelit
        st.extend_from_slice(&[1, 1]);
        for w in [2u32, 4, 1] {
            st.extend_from_slice(&w.to_le_bytes());
        }
        for f in [1.0f32, 1.0, 1.0, 0.5, 0.5, 0.0, 1.0] {
            st.extend_from_slice(&f.to_le_bytes());
        }
        st.extend_from_slice(&[0; 8]);
        let mut geo = Vec::new();
        put_chunk(&mut geo, ID_STRUCT, ver, &st);
        put_chunk(&mut geo, ID_EXT, ver, &ext);
        let mut list = Vec::new();
        put_chunk(&mut list, ID_STRUCT, ver, &1u32.to_le_bytes());
        put_chunk(&mut list, ID_GEOMETRY, ver, &geo);
        let mut clump = Vec::new();
        put_chunk(&mut clump, ID_GEOMLIST, ver, &list);
        let mut out = Vec::new();
        put_chunk(&mut out, ID_CLUMP, ver, &clump);
        out
    }

    #[test]
    fn strip_batches_become_a_quad() {
        let out = convert_dff(&native_clump()).unwrap();
        let clump = chunks(&out, 0, out.len()).unwrap()[0];
        let list = chunks(&out, clump.start, clump.end).unwrap()[0];
        let geo = chunks(&out, list.start, list.end).unwrap()[1];
        let st = chunks(&out, geo.start, geo.end).unwrap()[0];
        let s = &out[st.start..st.end];
        assert_eq!(s[3], 0, "no longer native");
        let (ntri, nvert) = (u32_at(s, 4).unwrap(), u32_at(s, 8).unwrap());
        assert_eq!((ntri, nvert), (2, 4));
        // after header (16), surface (12), colours (16), uvs (32), triangles (16): sphere, flags, positions
        let pos = 16 + 12 + 16 + 32 + 16 + 16 + 8;
        assert_eq!(f32_at(s, pos + 3 * 12).unwrap(), 1.0); // the fourth vertex (1,1,0)
        assert_eq!(f32_at(s, pos + 3 * 12 + 4).unwrap(), 1.0);
        assert_eq!(s[16 + 12], 10, "prelit colour kept");
    }

    #[test]
    fn other_pipelines_are_refused() {
        let mut d = native_clump();
        // turn the RGBA unpack (0x6e) into a V4-16 one
        let i = d.windows(4).position(|w| w == [0, 0, 3, 0x6e]).unwrap();
        d[i + 3] = 0x6d;
        assert!(convert_dff(&d).is_err());
    }
}

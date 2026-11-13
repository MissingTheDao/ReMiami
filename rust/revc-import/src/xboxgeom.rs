//! Xbox native geometry (Vice City's Xbox DFFs, RenderWare 3.5) converted to ordinary geometry,
//! so the game loads the Xbox's own models in the Xbox style ("exact Xbox models"): its map
//! models were partly rebuilt and re-lit (vertex colours, the second coordinate set of the world
//! lightmaps), so the PC models with the Xbox lightmaps grafted on can't look the same.
//!
//! The native data (extension 0x510) holds a header, the index buffers (u16, one per mesh, strips
//! or lists) and one vertex buffer, `stride` bytes per vertex: position f32x3, [normal packed
//! 11:11:10], [colour B,G,R,A], then f32x2 per coordinate set. After librw's Xbox reader and
//! uninstance (src/d3d/xbox.cpp, MIT). Other layouts (skinned peds, vehicles) are an error and
//! the PC model stays in use.

use crate::ps2geom::{Chunk, chunks, f32_at, put_chunk, u16_at, u32_at, unpack_version};

const ID_STRUCT: u32 = 1;
const ID_EXT: u32 = 3;
const ID_CLUMP: u32 = 0x10;
const ID_GEOMLIST: u32 = 0x1a;
const ID_GEOMETRY: u32 = 0x0f;
const ID_BINMESH: u32 = 0x50e;
const ID_NATIVEDATA: u32 = 0x510;
const ID_VERTEXFMT: u32 = 0x511;

const GEO_PRELIT: u32 = 0x08;
const GEO_NORMALS: u32 = 0x10;
const PLATFORM_XBOX: u32 = 5;
const D3DPT_TRIANGLESTRIP: u32 = 6;

/// An 11:11:10 packed normal (D3DVSDT_NORMPACKED3).
fn unpack_normal(v: u32) -> [f32; 3] {
    let x = ((v << 21) as i32 >> 21) as f32 / 1023.0;
    let y = ((v << 10) as i32 >> 21) as f32 / 1023.0;
    let z = (v as i32 >> 22) as f32 / 511.0;
    [x, y, z]
}

fn convert_geometry(d: &[u8], g: Chunk) -> Result<Vec<u8>, String> {
    let kids = chunks(d, g.start, g.end)?;
    let st = *kids.iter().find(|c| c.ty == ID_STRUCT).ok_or("geometry without struct")?;
    let word = u32_at(d, st.start)?;
    let (flags, sets) = (word & 0xFFFF, ((word >> 16) & 0xFF) as usize);
    if (word >> 24) & 0xFF == 0 {
        return Err("not native".into());
    }
    let mut p = st.start + 16;
    if unpack_version(st.ver) < 0x34000 {
        p += 12; // (surface properties: not in the Xbox's 3.5 files)
    }
    let sphere = d.get(p..p + 16).ok_or("truncated morph target")?.to_vec();
    let ext = *kids.iter().find(|c| c.ty == ID_EXT).ok_or("geometry without extensions")?;
    let ext_kids = chunks(d, ext.start, ext.end)?;
    let bin = ext_kids.iter().find(|c| c.ty == ID_BINMESH).ok_or("no mesh header")?;
    let nat = ext_kids.iter().find(|c| c.ty == ID_NATIVEDATA).ok_or("no native data")?;
    let num_meshes = u32_at(d, bin.start + 4)? as usize;
    // native data: struct header, platform, then the size field the offsets count from (less the
    // 0x18 bytes of the resource entry header that preceded it in memory)
    if u32_at(d, nat.start)? != ID_STRUCT || u32_at(d, nat.start + 12)? != PLATFORM_XBOX {
        return Err("not an Xbox geometry".into());
    }
    let q = nat.start + 16;
    let size = u32_at(d, q)? as usize;
    let base = q.checked_sub(0x18).ok_or("bad native data")?;
    let (header_meshes, prim, nv, stride) =
        (u16_at(d, q + 6)? as usize, u32_at(d, q + 8)?, u32_at(d, q + 12)? as usize, u32_at(d, q + 16)? as usize);
    if header_meshes != num_meshes {
        return Err("mesh counts differ".into());
    }
    let normals = flags & GEO_NORMALS != 0;
    let prelit = flags & GEO_PRELIT != 0;
    let expect = 12 + if normals { 4 } else { 0 } + if prelit { 4 } else { 0 } + 8 * sets;
    if stride != expect {
        return Err(format!("vertex stride {stride}, expected {expect} (not a plain mesh)"));
    }
    let vb = q + size;
    if vb + nv * stride > nat.end || nv > 0xFFFF {
        return Err("truncated vertex buffer".into());
    }
    // meshes: (index count, material, indices)
    let strip = prim == D3DPT_TRIANGLESTRIP;
    let mut meshes: Vec<(u32, Vec<u32>)> = Vec::new();
    for m in 0..num_meshes {
        let e = q + 4 + 32 + m * 0x18;
        let (num_indices, off) = (u32_at(d, e + 8)? as usize, u32_at(d, e + 12)? as usize);
        let material = u32_at(d, bin.start + 16 + 8 * m)?;
        let at = base + off;
        let idx: Vec<u32> = (0..num_indices).map(|i| u16_at(d, at + 2 * i).map(|v| v as u32)).collect::<Result<_, _>>()?;
        if idx.iter().any(|&i| i as usize >= nv) {
            return Err(format!("mesh {m}: index past the vertices"));
        }
        meshes.push((material, idx));
    }
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
    // vertex arrays out of the buffer
    let (mut pos, mut nrm, mut col, mut uvs) = (Vec::new(), Vec::new(), Vec::new(), vec![Vec::new(); sets]);
    for i in 0..nv {
        let mut o = vb + i * stride;
        pos.push([f32_at(d, o)?, f32_at(d, o + 4)?, f32_at(d, o + 8)?]);
        o += 12;
        if normals {
            nrm.push(unpack_normal(u32_at(d, o)?));
            o += 4;
        }
        if prelit {
            col.push([d[o + 2], d[o + 1], d[o], d[o + 3]]); // B,G,R,A to R,G,B,A
            o += 4;
        }
        for set in uvs.iter_mut() {
            set.push([f32_at(d, o)?, f32_at(d, o + 4)?]);
            o += 8;
        }
    }
    let mut s = Vec::new();
    s.extend_from_slice(&((flags as u16).to_le_bytes()));
    s.push(sets as u8);
    s.push(0);
    s.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    s.extend_from_slice(&(nv as u32).to_le_bytes());
    s.extend_from_slice(&1u32.to_le_bytes());
    if unpack_version(st.ver) < 0x34000 {
        s.extend_from_slice(&[0, 0, 128, 63, 0, 0, 128, 63, 0, 0, 128, 63]);
    }
    if prelit {
        for c in &col {
            s.extend_from_slice(c);
        }
    }
    for set in &uvs {
        for uv in set {
            s.extend_from_slice(&uv[0].to_le_bytes());
            s.extend_from_slice(&uv[1].to_le_bytes());
        }
    }
    for t in &tris {
        s.extend_from_slice(&((t[0] << 16) | t[1]).to_le_bytes());
        s.extend_from_slice(&((t[2] << 16) | t[3]).to_le_bytes());
    }
    s.extend_from_slice(&sphere);
    s.extend_from_slice(&1u32.to_le_bytes());
    s.extend_from_slice(&(normals as u32).to_le_bytes());
    for v in &pos {
        for c in v {
            s.extend_from_slice(&c.to_le_bytes());
        }
    }
    for n in &nrm {
        for c in n {
            s.extend_from_slice(&c.to_le_bytes());
        }
    }
    let mut b = Vec::new();
    b.extend_from_slice(&(strip as u32).to_le_bytes());
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
            ID_NATIVEDATA | ID_VERTEXFMT => {}
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
    Ok(body)
}

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

/// An Xbox DFF (clump, unpacked from the archive) with its native geometries converted; Err when
/// it has none or one of them isn't a plain mesh.
pub fn convert_dff(d: &[u8]) -> Result<Vec<u8>, String> {
    let first = chunks(d, 0, d.len()).ok().and_then(|c| c.first().copied()).ok_or("not a RenderWare file")?;
    if first.ty != ID_CLUMP {
        return Err("not a clump".into());
    }
    let mut out = Vec::new();
    let mut converted = 0;
    rebuild(d, 0, first.end, &mut out, &mut converted)?;
    if converted == 0 {
        return Err("no Xbox geometry".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-mesh Xbox geometry: a strip of 4 prelit vertices with two coordinate sets.
    fn native_clump() -> Vec<u8> {
        let ver = 0x1400ffff;
        let (nv, stride) = (4u32, 12 + 4 + 16);
        let idx: [u16; 4] = [0, 1, 2, 3];
        // header from the size field on: size, serial u16, meshes u16, prim, vertices, stride,
        // vb pointer, vertex alpha, begin, end; one mesh entry (0x18); then the indices
        let mut h = Vec::new();
        h.extend_from_slice(&0u32.to_le_bytes()); // size, patched below
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        for w in [6u32, nv, stride, 0, 0, 0, 0] {
            h.extend_from_slice(&w.to_le_bytes());
        }
        let idx_off = 0x18 + h.len() + 0x18; // (offsets count from 0x18 before the size field)
        for w in [0u32, nv, 4, idx_off as u32, 0, 0] {
            h.extend_from_slice(&w.to_le_bytes());
        }
        for i in idx {
            h.extend_from_slice(&i.to_le_bytes());
        }
        let size = h.len() as u32;
        h[0..4].copy_from_slice(&size.to_le_bytes());
        for i in 0..nv {
            let (x, y) = ((i & 1) as f32, (i >> 1) as f32);
            for f in [x, y, 0.0] {
                h.extend_from_slice(&f.to_le_bytes());
            }
            h.extend_from_slice(&[30, 20, 10, 255]); // B,G,R,A
            for f in [x, y, x * 0.5, y * 0.5] {
                h.extend_from_slice(&f.to_le_bytes());
            }
        }
        let mut nat = Vec::new();
        nat.extend_from_slice(&PLATFORM_XBOX.to_le_bytes());
        nat.extend_from_slice(&h);
        let mut nat_chunk = Vec::new();
        put_chunk(&mut nat_chunk, ID_STRUCT, ver, &nat);
        let mut bin = Vec::new();
        for w in [1u32, 1, 4, 4, 0] {
            bin.extend_from_slice(&w.to_le_bytes());
        }
        let mut ext = Vec::new();
        put_chunk(&mut ext, ID_BINMESH, ver, &bin);
        put_chunk(&mut ext, ID_NATIVEDATA, ver, &nat_chunk);
        put_chunk(&mut ext, ID_VERTEXFMT, ver, &[0; 4]);
        let mut st = Vec::new();
        st.extend_from_slice(&(0x0100_0000u32 | 2 << 16 | 0x8b).to_le_bytes()); // native, 2 sets, strip+pos+tex+prelit+tex2
        for w in [2u32, nv, 1] {
            st.extend_from_slice(&w.to_le_bytes());
        }
        for f in [0.5f32, 0.5, 0.0, 1.0] {
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
    fn strip_becomes_two_triangles_with_both_sets() {
        let out = convert_dff(&native_clump()).unwrap();
        let clump = chunks(&out, 0, out.len()).unwrap()[0];
        let list = chunks(&out, clump.start, clump.end).unwrap()[0];
        let geo = chunks(&out, list.start, list.end).unwrap()[1];
        let parts = chunks(&out, geo.start, geo.end).unwrap();
        let s = &out[parts[0].start..parts[0].end];
        assert_eq!((s[2], s[3]), (2, 0), "two coordinate sets, no longer native");
        assert_eq!((u32_at(s, 4).unwrap(), u32_at(s, 8).unwrap()), (2, 4));
        assert_eq!(&s[16..20], &[10, 20, 30, 255], "colour as R,G,B,A");
        // colours (16), set 0 (32), set 1 (32), triangles (16), sphere (16), flags (8): positions
        let set1 = 16 + 16 + 32;
        assert_eq!(f32_at(s, set1 + 3 * 8).unwrap(), 0.5, "the fourth vertex's second coordinates");
        let pos = 16 + 16 + 64 + 16 + 16 + 8;
        assert_eq!(f32_at(s, pos + 3 * 12 + 4).unwrap(), 1.0);
        let ext = chunks(&out, parts[1].start, parts[1].end).unwrap();
        assert!(ext.iter().all(|c| c.ty != ID_NATIVEDATA && c.ty != ID_VERTEXFMT));
    }

    #[test]
    fn normals_unpack() {
        let n = unpack_normal(1023 | (0 << 11) | ((-511i32 as u32 & 0x3FF) << 22));
        assert!((n[0] - 1.0).abs() < 1e-6 && n[1] == 0.0 && (n[2] + 1.0).abs() < 1e-6);
    }
}

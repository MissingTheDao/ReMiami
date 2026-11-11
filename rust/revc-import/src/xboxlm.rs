//! Xbox world lightmaps (plan task 13b/13i; FixMods S11). Every world material on the Xbox has a
//! MatFX dual pass with a per-model lightmap `<model>@` (in the model's TXD or generic.txd) and a
//! second texture coordinate set in the geometry. The PC models have neither. Their vertices are
//! the Xbox's, but often in another order and with fewer duplicates (sampled: 97 % of lightmapped
//! geometries have every PC vertex's position and first coordinates among the Xbox's, bit for
//! bit, but 62 % differ in count or order). So the import keeps, per model and geometry, each Xbox
//! vertex's key (`vertex_key`: its position and first coordinates) with its second coordinates,
//! and each material's lightmap name; the game gives each PC vertex the coordinates of the Xbox
//! vertex with its key as the model loads (rwgeomobj geo_add_lightmap_coords, core
//! XboxLightmaps.cpp), or leaves the geometry alone if any vertex has no match. All lightmaps go
//! into one TXD, `lightmaps.txd`.
//!
//! Xbox `models/gta3.img` entries are compressed: magic 0x67A3A1CE, then blocks of LZO1X
//! (research xbox-research/tools/xboximg.py). The geometry is Xbox-native (MatFX dual, native
//! data chunk 0x510): a header (its first word is its own size from that word on) followed by the
//! vertices, `stride` bytes each: position f32x3, [packed normal u32], [colour u32], uv0 f32x2,
//! uv1 f32x2.
//!
//! The sidecar `lightmaps.bin` (in the import's `models` folder): "RVLM", u32 version 1,
//! u32 models; per model: u8 name length + name (lower case, no extension), u16 geometries; per
//! geometry: u32 vertices, vertices x (u32 key, f32x2 uv1), u16 materials; per material: u8
//! length + lightmap name (0: none). Version 2 (version 1 had no keys).

const MAGIC: u32 = 0x67A3_A1CE;

/// LZO1X decompression (the reference lzo1x_d.ch decoder, safe indexing). `src` starts at the
/// first instruction.
pub fn lzo1x(src: &[u8]) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len() * 3);
    let mut ip = 0usize;
    let byte = |ip: usize| -> Result<u8, String> { src.get(ip).copied().ok_or_else(|| "lzo: input overrun".to_string()) };
    let copy_match = |out: &mut Vec<u8>, dist: usize, n: usize| -> Result<(), String> {
        if dist == 0 || dist > out.len() {
            return Err("lzo: bad distance".into());
        }
        let start = out.len() - dist;
        for i in 0..n {
            let b = out[start + i];
            out.push(b);
        }
        Ok(())
    };
    let literals = |out: &mut Vec<u8>, ip: &mut usize, n: usize| -> Result<(), String> {
        let s = src.get(*ip..*ip + n).ok_or("lzo: literal overrun")?;
        out.extend_from_slice(s);
        *ip += n;
        Ok(())
    };
    let long = |ip: &mut usize, base: usize| -> Result<usize, String> {
        let mut t = 0usize;
        while byte(*ip)? == 0 {
            t += 255;
            *ip += 1;
        }
        t += base + byte(*ip)? as usize;
        *ip += 1;
        Ok(t)
    };

    enum S {
        Loop,
        FirstLiteralRun,
        Match(usize),
        MatchDone,
        MatchNext(usize),
    }
    let mut state;
    let b0 = byte(0)?;
    if b0 > 17 {
        ip = 1;
        let t = (b0 - 17) as usize;
        if t < 4 {
            state = S::MatchNext(t);
        } else {
            literals(&mut out, &mut ip, t)?;
            state = S::FirstLiteralRun;
        }
    } else {
        state = S::Loop;
    }
    loop {
        state = match state {
            S::Loop => {
                let mut t = byte(ip)? as usize;
                ip += 1;
                if t >= 16 {
                    S::Match(t)
                } else {
                    if t == 0 {
                        t = long(&mut ip, 15)?;
                    }
                    literals(&mut out, &mut ip, t + 3)?;
                    S::FirstLiteralRun
                }
            }
            S::FirstLiteralRun => {
                let t = byte(ip)? as usize;
                ip += 1;
                if t >= 16 {
                    S::Match(t)
                } else {
                    let d = 1 + 0x0800 + (t >> 2) + ((byte(ip)? as usize) << 2);
                    ip += 1;
                    copy_match(&mut out, d, 3)?;
                    S::MatchDone
                }
            }
            S::Match(t) => {
                if t >= 64 {
                    let d = 1 + ((t >> 2) & 7) + ((byte(ip)? as usize) << 3);
                    ip += 1;
                    copy_match(&mut out, d, (t >> 5) - 1 + 2)?;
                } else if t >= 32 {
                    let mut n = t & 31;
                    if n == 0 {
                        n = long(&mut ip, 31)?;
                    }
                    let d = 1 + ((byte(ip)? as usize) >> 2) + ((byte(ip + 1)? as usize) << 6);
                    ip += 2;
                    copy_match(&mut out, d, n + 2)?;
                } else if t >= 16 {
                    let mut d = (t & 8) << 11;
                    let mut n = t & 7;
                    if n == 0 {
                        n = long(&mut ip, 7)?;
                    }
                    d += ((byte(ip)? as usize) >> 2) + ((byte(ip + 1)? as usize) << 6);
                    ip += 2;
                    if d == 0 {
                        return Ok(out); // end of stream
                    }
                    copy_match(&mut out, d + 0x4000, n + 2)?;
                } else {
                    let d = 1 + (t >> 2) + ((byte(ip)? as usize) << 2);
                    ip += 1;
                    copy_match(&mut out, d, 2)?;
                }
                S::MatchDone
            }
            S::MatchDone => {
                let t = (byte(ip - 2)? & 3) as usize;
                if t == 0 { S::Loop } else { S::MatchNext(t) }
            }
            S::MatchNext(t) => {
                literals(&mut out, &mut ip, t)?;
                let t = byte(ip)? as usize;
                ip += 1;
                S::Match(t)
            }
        };
    }
}

/// An entry of the Xbox gta3.img, decompressed if it is.
pub fn img_entry(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.len() < 12 || u32::from_le_bytes(raw[0..4].try_into().unwrap()) != MAGIC {
        return Ok(raw.to_vec());
    }
    let total = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
    let mut pos = 12usize;
    let mut out = Vec::new();
    while pos + 12 <= 12 + total && pos + 12 <= raw.len() {
        let typ = u32::from_le_bytes(raw[pos..pos + 4].try_into().unwrap());
        let n = u32::from_le_bytes(raw[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 12;
        let block = raw.get(pos..pos + n).ok_or("img: block past the end")?;
        if typ == 4 {
            out.extend(lzo1x(block)?);
        } else {
            out.extend_from_slice(block);
        }
        pos += n;
    }
    Ok(out)
}

/// A vertex's key: FNV-1a over the bits of its position and first texture coordinates.
pub fn vertex_key(pos: [f32; 3], uv: [f32; 2]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for f in [pos[0], pos[1], pos[2], uv[0], uv[1]] {
        for b in f.to_le_bytes() {
            h = (h ^ b as u32).wrapping_mul(0x0100_0193);
        }
    }
    h
}

/// One geometry's lightmap data.
#[derive(Debug, Default, Clone)]
pub struct GeoLightmap {
    /// per vertex: vertex_key
    pub keys: Vec<u32>,
    pub uv1: Vec<[f32; 2]>,
    pub lightmaps: Vec<String>,
}

fn u32at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

/// Chunks (type, body start, body size) directly inside [start, end).
fn chunks(b: &[u8], start: usize, end: usize) -> Vec<(u32, usize, usize)> {
    let mut v = Vec::new();
    let mut o = start;
    while o + 12 <= end {
        let (Some(t), Some(sz)) = (u32at(b, o), u32at(b, o + 4)) else { break };
        let sz = sz as usize;
        if o + 12 + sz > end {
            break;
        }
        v.push((t, o + 12, sz));
        o += 12 + sz;
    }
    v
}

fn first(b: &[u8], start: usize, end: usize, t: u32) -> Option<(usize, usize)> {
    chunks(b, start, end).into_iter().find(|c| c.0 == t).map(|c| (c.1, c.2))
}

/// The lightmap name of a material: the texture name in its MatFX dual effect.
fn material_lightmap(b: &[u8], body: usize, size: usize) -> String {
    let Some((ext, esz)) = first(b, body, body + size, 0x03) else { return String::new() };
    let Some((fx, fsz)) = first(b, ext, ext + esz, 0x120) else { return String::new() };
    // the dual effect's texture: the first string after a Texture chunk inside the MatFX data
    let data = &b[fx..fx + fsz];
    let mut o = 0;
    while o + 12 <= data.len() {
        if u32at(data, o) == Some(0x06) {
            let tsz = u32at(data, o + 4).unwrap_or(0) as usize;
            let tb = o + 12;
            if let Some((s, ssz)) = first(data, tb, (tb + tsz).min(data.len()), 0x02) {
                let raw = &data[s..s + ssz];
                let name = raw.split(|&c| c == 0).next().unwrap_or(&[]);
                return String::from_utf8_lossy(name).to_string();
            }
        }
        o += 4;
    }
    String::new()
}

/// The lightmap data of every geometry of an Xbox DFF (clump), in geometry order. Geometries
/// that aren't native with a second coordinate set give an empty entry.
pub fn dff_lightmaps(b: &[u8]) -> Vec<GeoLightmap> {
    let mut out = Vec::new();
    let Some((clump, csz)) = first(b, 0, b.len(), 0x10) else { return out };
    let Some((gl, glsz)) = first(b, clump, clump + csz, 0x1A) else { return out };
    for (t, g, gsz) in chunks(b, gl, gl + glsz) {
        if t != 0x0F {
            continue;
        }
        let mut lm = GeoLightmap::default();
        let gend = g + gsz;
        let Some((st, _)) = first(b, g, gend, 0x01) else {
            out.push(lm);
            continue;
        };
        let flags = u32at(b, st).unwrap_or(0);
        let nv = u32at(b, st + 8).unwrap_or(0) as usize;
        if let Some((ml, mlsz)) = first(b, g, gend, 0x08) {
            for (mt, m, msz) in chunks(b, ml, ml + mlsz) {
                if mt == 0x07 {
                    lm.lightmaps.push(material_lightmap(b, m, msz));
                }
            }
        }
        let native = flags & 0x0100_0000 != 0;
        let sets = (flags >> 16) & 0xFF;
        if native && sets >= 2 {
            if let Some((ext, esz)) = first(b, g, gend, 0x03) {
                if let Some((nd, ndsz)) = first(b, ext, ext + esz, 0x510) {
                    // struct header (12) + platform (4), then the header that sizes itself
                    let h = nd + 16;
                    if let (Some(hsize), Some(hv), Some(stride)) = (u32at(b, h), u32at(b, h + 12), u32at(b, h + 16)) {
                        let (stride, hv) = (stride as usize, hv as usize);
                        let vstart = h + hsize as usize;
                        // position, [normal], [colour], uv0, uv1
                        let uv0 = 12 + if flags & 0x10 != 0 { 4 } else { 0 } + if flags & 0x08 != 0 { 4 } else { 0 };
                        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                        if hv == nv && stride == uv0 + 16 && vstart + nv * stride <= nd + ndsz {
                            for i in 0..nv {
                                let o = vstart + i * stride;
                                lm.keys.push(vertex_key([f(o), f(o + 4), f(o + 8)], [f(o + uv0), f(o + uv0 + 4)]));
                                lm.uv1.push([f(o + uv0 + 8), f(o + uv0 + 12)]);
                            }
                        }
                    }
                }
            }
        }
        out.push(lm);
    }
    out
}

/// Builds the sidecar from (model name, geometries) pairs; models without any lightmap left out.
pub fn sidecar(models: &[(String, Vec<GeoLightmap>)]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"RVLM");
    v.extend_from_slice(&2u32.to_le_bytes());
    let keep: Vec<_> = models.iter().filter(|m| m.1.iter().any(|g| !g.uv1.is_empty())).collect();
    v.extend_from_slice(&(keep.len() as u32).to_le_bytes());
    let name = |v: &mut Vec<u8>, s: &str| {
        let s = &s.as_bytes()[..s.len().min(255)];
        v.push(s.len() as u8);
        v.extend_from_slice(s);
    };
    for (m, geos) in keep {
        name(&mut v, &m.to_ascii_lowercase());
        v.extend_from_slice(&(geos.len() as u16).to_le_bytes());
        for g in geos {
            v.extend_from_slice(&(g.uv1.len() as u32).to_le_bytes());
            for (k, uv) in g.keys.iter().zip(&g.uv1) {
                v.extend_from_slice(&k.to_le_bytes());
                v.extend_from_slice(&uv[0].to_le_bytes());
                v.extend_from_slice(&uv[1].to_le_bytes());
            }
            v.extend_from_slice(&(g.lightmaps.len() as u16).to_le_bytes());
            for l in &g.lightmaps {
                name(&mut v, l);
            }
        }
    }
    v
}

/// The lightmap texture natives (whole 0x15 chunks, names ending in '@') of a TXD.
pub fn txd_lightmaps(b: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    let Some((td, tdsz)) = first(b, 0, b.len(), 0x16) else { return v };
    for (t, n, nsz) in chunks(b, td, td + tdsz) {
        if t != 0x15 {
            continue;
        }
        // native struct: platform, filter/addressing, name[32]
        let Some((st, stsz)) = first(b, n, n + nsz, 0x01) else { continue };
        if stsz < 40 {
            continue;
        }
        let name = b[st + 8..st + 40].split(|&c| c == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name).to_string();
        if name.ends_with('@') {
            v.push((name, b[n - 12..n + nsz].to_vec()));
        }
    }
    v
}

/// A TXD (texture dictionary) of whole texture native chunks.
pub fn build_txd(natives: &[Vec<u8>]) -> Vec<u8> {
    // the library version stamp of the natives' own headers
    let ver = natives.first().map_or(0x1803_FFFF, |n| u32::from_le_bytes(n[8..12].try_into().unwrap()));
    let mut body = Vec::new();
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&4u32.to_le_bytes());
    body.extend_from_slice(&ver.to_le_bytes());
    body.extend_from_slice(&(natives.len() as u16).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    for n in natives {
        body.extend_from_slice(n);
    }
    body.extend_from_slice(&3u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&ver.to_le_bytes());
    let mut v = Vec::with_capacity(body.len() + 12);
    v.extend_from_slice(&0x16u32.to_le_bytes());
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(&ver.to_le_bytes());
    v.extend(body);
    v
}

/// The sidecar and the lightmap TXD for a whole Xbox gta3.img: `dir` is gta3.dir,
/// `read(offset, size)` reads bytes of gta3.img, `extra` further TXDs (generic.txd). Only the
/// .dff and .txd entries are read. Returns (sidecar, txd, models with lightmaps).
pub fn img_sidecar(dir: &[u8], extra: &[&[u8]], mut read: impl FnMut(u64, usize) -> std::io::Result<Vec<u8>>) -> Result<(Vec<u8>, Vec<u8>, usize), String> {
    let mut models = Vec::new();
    let mut lms: std::collections::BTreeMap<String, Vec<u8>> = Default::default();
    for t in extra {
        for (n, d) in txd_lightmaps(t) {
            lms.entry(n.to_ascii_lowercase()).or_insert(d);
        }
    }
    for e in dir.chunks_exact(32) {
        let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
        let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
        let name = e[8..32].split(|&c| c == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name).to_ascii_lowercase();
        let (dff, txd) = (name.strip_suffix(".dff"), name.ends_with(".txd"));
        if dff.is_none() && !txd {
            continue;
        }
        let raw = read(off, size).map_err(|err| format!("{name}: {err}"))?;
        let data = img_entry(&raw).map_err(|err| format!("{name}: {err}"))?;
        match dff {
            Some(stem) => models.push((stem.to_string(), dff_lightmaps(&data))),
            None => {
                for (n, d) in txd_lightmaps(&data) {
                    lms.entry(n.to_ascii_lowercase()).or_insert(d);
                }
            }
        }
    }
    // only the lightmaps some model uses
    let used: std::collections::HashSet<String> = models.iter().flat_map(|m| m.1.iter().flat_map(|g| g.lightmaps.iter().map(|l| l.to_ascii_lowercase()))).collect();
    let natives: Vec<Vec<u8>> = lms.into_iter().filter(|(n, _)| used.contains(n)).map(|(_, d)| d).collect();
    let n = models.iter().filter(|m| m.1.iter().any(|g| !g.uv1.is_empty())).count();
    Ok((sidecar(&models), build_txd(&natives), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzo_literals_and_match() {
        // 4 literals, then the end marker
        assert_eq!(lzo1x(&[21, b'a', b'b', b'c', b'd', 0x11, 0, 0]).unwrap(), b"abcd");
        // after a literal run, t < 16 copies 3 bytes from at least 0x801 back: too far here
        assert!(lzo1x(&[21, b'a', b'b', b'c', b'd', 0x00, 0x00]).is_err());
        // a short match after literals: t = ((len-1)<<5) | (((dist-1)&7)<<2), next byte
        // (dist-1)>>3; len 4, dist 4: t = (3 << 5) | (3 << 2) = 0x6C, then 0
        assert_eq!(lzo1x(&[21, b'a', b'b', b'c', b'd', 0x6C, 0x00, 0x11, 0, 0]).unwrap(), b"abcdabcd");
    }

    #[test]
    fn sidecar_layout() {
        let g = GeoLightmap { keys: vec![7], uv1: vec![[0.5, 0.25]], lightmaps: vec!["m@".into()] };
        let v = sidecar(&[("M".into(), vec![g]), ("none".into(), vec![GeoLightmap::default()])]);
        assert_eq!(&v[0..4], b"RVLM");
        assert_eq!(u32::from_le_bytes(v[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(v[8..12].try_into().unwrap()), 1);
        assert_eq!(&v[12..14], b"\x01m");
        assert_eq!(u32::from_le_bytes(v[16..20].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(v[20..24].try_into().unwrap()), 7);
        assert_eq!(f32::from_le_bytes(v[24..28].try_into().unwrap()), 0.5);
        assert_eq!(&v[32..37], b"\x01\x00\x02m@");
    }
}

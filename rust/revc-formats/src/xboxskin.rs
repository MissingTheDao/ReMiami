//! The Xbox build's native skin data (geometry plugin 0x116 on platform 5), converted to the
//! platform-independent arrays the PC files store.
//!
//! The layout is read from the files themselves (the Xbox cutscene hands), not from documentation.
//! Certain, from the data: the struct chunk and platform 5; the bone count; a 256-entry table whose
//! first entries list the used bones (the rest -1); the used-bone count; the weights per vertex;
//! the per-vertex stride; one u8 weight per slot (they always sum to 255); 64-byte inverse bone
//! matrices, one per bone, at the end. Inferred: the second 256-entry table (bone to used slot; not
//! needed), the field after the weight count (a pointer, ignored), and the per-slot u16 bone
//! references being shader register numbers, used slot * 3 (every value seen is a multiple of 3
//! below 3 * used bones). Anything that doesn't fit this layout is refused, never guessed.

use crate::Error;

/// Plugin chunk id of the struct inside a native skin.
const ID_STRUCT: u32 = 1;
/// RenderWare's platform id of the Xbox.
const PLATFORM_XBOX: u32 = 5;

/// The platform-independent form (as a PC file's skin plugin holds it).
#[derive(Debug, Clone, PartialEq)]
pub struct PlainSkin {
    /// num_bones, num_used_bones, num_weights, 0
    pub header: [u8; 4],
    pub used_bones: Vec<u8>,
    /// 4 bone indices per vertex (unused slots 0)
    pub indices: Vec<u8>,
    /// 4 weights per vertex (unused slots 0.0)
    pub weights: Vec<f32>,
    /// num_bones matrices, 64 bytes each, as stored
    pub inverse_matrices: Vec<u8>,
}

/// Whether a skin plugin payload is in the Xbox-native layout (a struct chunk, platform 5).
pub fn is_xbox_native(data: &[u8]) -> bool {
    data.len() >= 16 && u32_at(data, 0) == ID_STRUCT && u32_at(data, 12) == PLATFORM_XBOX
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Error> {
        if n > self.d.len() - self.at {
            return Err(Error::Truncated(what, self.at));
        }
        let b = &self.d[self.at..self.at + n];
        self.at += n;
        Ok(b)
    }
    fn u32(&mut self, what: &'static str) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.bytes(4, what)?.try_into().unwrap()))
    }
}

/// Converts a native skin plugin payload of a geometry with `num_vertices` vertices.
pub fn xbox_native_skin(data: &[u8], num_vertices: usize) -> Result<PlainSkin, Error> {
    let bad = |m: String| Err(Error::Invalid(format!("Xbox skin: {m}")));
    let mut h = Reader { d: data, at: 0 };
    let id = h.u32("skin struct header")?;
    let size = h.u32("skin struct header")? as usize;
    h.u32("skin struct header")?;
    if id != ID_STRUCT {
        return bad(format!("chunk {id:#x}, not a struct"));
    }
    let mut r = Reader { d: h.bytes(size, "skin struct")?, at: 0 };
    let platform = r.u32("platform")?;
    if platform != PLATFORM_XBOX {
        return bad(format!("platform {platform}, not the Xbox's"));
    }
    let num_bones = r.u32("bone count")?;
    if !(1..=255).contains(&num_bones) {
        return bad(format!("{num_bones} bones"));
    }
    let table = r.bytes(256 * 4, "used bone table")?;
    r.bytes(256 * 4, "bone slot table")?;
    let num_used = r.u32("used bone count")?;
    let num_weights = r.u32("weight count")?;
    r.u32("vertex buffer pointer")?;
    let stride = r.u32("vertex stride")? as usize;
    if num_used == 0 || num_used > num_bones {
        return bad(format!("{num_used} used bones of {num_bones}"));
    }
    if !(1..=4).contains(&num_weights) {
        return bad(format!("{num_weights} weights per vertex"));
    }
    let nw = num_weights as usize;
    if stride < 3 * nw {
        return bad(format!("vertex stride {stride} for {nw} weights"));
    }
    let mut used = Vec::with_capacity(num_used as usize);
    for i in 0..num_used as usize {
        let b = u32_at(table, 4 * i) as i32;
        if b < 0 || b >= num_bones as i32 {
            return bad(format!("used bone {i} is {b} of {num_bones}"));
        }
        used.push(b as u8);
    }
    let vb = r.bytes(num_vertices * stride, "vertex buffer")?;
    let mut indices = Vec::with_capacity(4 * num_vertices);
    let mut weights = Vec::with_capacity(4 * num_vertices);
    for (v, rec) in vb.chunks_exact(stride).enumerate() {
        let mut sum = 0u32;
        for s in 0..4 {
            if s >= nw || rec[s] == 0 {
                indices.push(0);
                weights.push(0.0);
                continue;
            }
            sum += rec[s] as u32;
            let reg = u16::from_le_bytes([rec[nw + 2 * s], rec[nw + 2 * s + 1]]) as usize;
            if reg % 3 != 0 || reg / 3 >= used.len() {
                return bad(format!("vertex {v}: bone register {reg} (used bones {})", used.len()));
            }
            indices.push(used[reg / 3]);
            weights.push(rec[s] as f32 / 255.0);
        }
        if sum != 255 {
            return bad(format!("vertex {v}: weights sum to {sum}/255"));
        }
    }
    let mats = r.bytes(64 * num_bones as usize, "inverse bone matrices")?;
    Ok(PlainSkin {
        header: [num_bones as u8, num_used as u8, nw as u8, 0],
        used_bones: used,
        indices,
        weights,
        inverse_matrices: mats.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A native skin payload: `used` bone list, 3 weights, stride 9, per vertex (weights,
    /// registers); matrix k filled with the byte k.
    fn payload(num_bones: u32, used: &[i32], verts: &[([u8; 3], [u16; 3])], platform: u32) -> Vec<u8> {
        let mut s = Vec::new();
        s.extend_from_slice(&platform.to_le_bytes());
        s.extend_from_slice(&num_bones.to_le_bytes());
        for i in 0..256 {
            s.extend_from_slice(&used.get(i).copied().unwrap_or(-1).to_le_bytes());
        }
        s.extend_from_slice(&[0u8; 1024]);
        for v in [used.len() as u32, 3, 0x1234, 9] {
            s.extend_from_slice(&v.to_le_bytes());
        }
        for (w, r) in verts {
            s.extend_from_slice(w);
            for x in r {
                s.extend_from_slice(&x.to_le_bytes());
            }
        }
        for k in 0..num_bones {
            s.extend_from_slice(&[k as u8; 64]);
        }
        s.extend_from_slice(&[0u8; 12]); // trailing padding, as the real files have
        let mut d = Vec::new();
        for v in [ID_STRUCT, s.len() as u32, 0x1400ffff] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&s);
        d
    }

    #[test]
    fn converts_a_synthetic_skin() {
        let d = payload(3, &[0, 2], &[([255, 0, 0], [3, 27, 0]), ([133, 122, 0], [0, 3, 0])], 5);
        assert!(is_xbox_native(&d));
        let s = xbox_native_skin(&d, 2).unwrap();
        assert_eq!(s.header, [3, 2, 3, 0]);
        assert_eq!(s.used_bones, vec![0, 2]);
        // a zero weight's register (27 above) is not looked at
        assert_eq!(s.indices, vec![2, 0, 0, 0, 0, 2, 0, 0]);
        assert_eq!(s.weights, vec![1.0, 0.0, 0.0, 0.0, 133.0 / 255.0, 122.0 / 255.0, 0.0, 0.0]);
        assert_eq!(s.inverse_matrices.len(), 3 * 64);
        assert_eq!(s.inverse_matrices[64], 1);
        assert_eq!(s.inverse_matrices[191], 2);
    }

    #[test]
    fn refuses_what_it_does_not_know() {
        let ok = [([255, 0, 0], [3, 0, 0])];
        assert!(xbox_native_skin(&payload(3, &[0, 2], &ok, 4), 1).is_err());
        assert!(!is_xbox_native(&payload(3, &[0, 2], &ok, 4)));
        // register not a multiple of 3, register past the used bones, weights not summing to 255
        assert!(xbox_native_skin(&payload(3, &[0, 2], &[([255, 0, 0], [4, 0, 0])], 5), 1).is_err());
        assert!(xbox_native_skin(&payload(3, &[0, 2], &[([255, 0, 0], [6, 0, 0])], 5), 1).is_err());
        assert!(xbox_native_skin(&payload(3, &[0, 2], &[([200, 0, 0], [0, 0, 0])], 5), 1).is_err());
        // used bone outside the skeleton
        assert!(xbox_native_skin(&payload(3, &[0, 3], &ok, 5), 1).is_err());
        // truncated: more vertices than stored
        assert!(xbox_native_skin(&payload(3, &[0, 2], &ok, 5), 300).is_err());
    }

    /// The real Xbox models (local import, skipped without it): every skinned geometry of the
    /// cutscene hands and the cutscene characters converts, weights sum to 1, indices are used
    /// bones, one inverse matrix per bone.
    #[test]
    fn real_xbox_models() {
        let Some(dir) = std::env::var_os("REVC_XBOX_MODELS") else { return };
        let mut files = 0;
        let mut skins = 0;
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            if !(name.starts_with("cs") && name.ends_with(".dff")) {
                continue;
            }
            let d = std::fs::read(&p).unwrap();
            files += 1;
            // geometry chunks (0x0f) and their skin plugin (0x116) found by walking the tree
            for (nv, sk) in skins_of(&d) {
                assert!(is_xbox_native(sk), "{name}: skin not in the Xbox layout");
                let s = xbox_native_skin(sk, nv).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(s.inverse_matrices.len(), 64 * s.header[0] as usize);
                for v in 0..nv {
                    let w: f32 = s.weights[4 * v..4 * v + 4].iter().sum();
                    assert!((w - 1.0).abs() < 1e-5, "{name}: vertex {v} weight {w}");
                    for k in 0..4 {
                        if s.weights[4 * v + k] > 0.0 {
                            assert!(s.used_bones.contains(&s.indices[4 * v + k]), "{name}: vertex {v}");
                        }
                    }
                }
                skins += 1;
            }
        }
        eprintln!("{files} cs*.dff files, {skins} Xbox skins converted");
        assert!(files > 0);
    }

    /// (vertex count, skin plugin payload) of every geometry in a DFF.
    fn skins_of(d: &[u8]) -> Vec<(usize, &[u8])> {
        fn walk<'a>(d: &'a [u8], mut at: usize, end: usize, out: &mut Vec<(usize, &'a [u8])>, nv: usize) {
            while at + 12 <= end {
                let (id, size) = (u32_at(d, at), u32_at(d, at + 4) as usize);
                let body = at + 12;
                let stop = (body + size).min(end);
                match id {
                    0x0f => {
                        // geometry struct: flags, triangles, vertices
                        let v = u32_at(d, body + 12 + 8) as usize;
                        walk(d, body, stop, out, v);
                    }
                    0x10 | 0x1a | 0x03 => walk(d, body, stop, out, nv),
                    0x116 => out.push((nv, &d[body..stop])),
                    _ => {}
                }
                at = body + size;
            }
        }
        let mut out = Vec::new();
        walk(d, 0, d.len(), &mut out, 0);
        out
    }
}

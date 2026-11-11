//! librw's geometry math, stage 2 step 4c: V3d::transformPoints/transformVectors (base.cpp; also
//! the atomics' world bounding spheres), MorphTarget::calculateBoundingSphere, and the mesh
//! indices Geometry::buildMeshes makes from the triangle list: per-material lists, or triangle
//! strips (tristrip.cpp: the dual graph, the simple strips and their stitching; librw's tunnel
//! pass is disabled there and here). The game builds strips for the water at start-up;
//! rwgeomobj's buildMeshes and buildTristrips use these.
//!
//! Bit-identical to librw compiled like the game; the strips come out index for index. Checked
//! against tests/golden/rwgeometry.txt, recorded from the C++ (tests/golden/recorders/rwgeometry).

use crate::math::{RwMatrix, V3};
use crate::rwcamera::Sphere;

/// rw::Triangle
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Triangle {
    pub v: [u16; 3],
    pub mat_id: u16,
}

/// MeshHeader::TRISTRIP
pub const MESH_TRISTRIP: u32 = 1;

// ---------------------------------------------------------------- transforms (base.cpp)

pub fn transform_point(p: &V3, m: &RwMatrix) -> V3 {
    V3 {
        x: p.x * m.right.x + p.y * m.up.x + p.z * m.at.x + m.pos.x,
        y: p.x * m.right.y + p.y * m.up.y + p.z * m.at.y + m.pos.y,
        z: p.x * m.right.z + p.y * m.up.z + p.z * m.at.z + m.pos.z,
    }
}

pub fn transform_vector(p: &V3, m: &RwMatrix) -> V3 {
    V3 {
        x: p.x * m.right.x + p.y * m.up.x + p.z * m.at.x,
        y: p.x * m.right.y + p.y * m.up.y + p.z * m.at.y,
        z: p.x * m.right.z + p.y * m.up.z + p.z * m.at.z,
    }
}

// ---------------------------------------------------------------- bounding sphere (geometry.cpp)

/// MorphTarget::calculateBoundingSphere: the centre of the vertices' bounding box and its
/// half-diagonal (with librw's ±1e6 start values, so no vertices give a huge sphere).
pub fn bounding_sphere(verts: &[V3]) -> Sphere {
    let mut min = V3 { x: 1000000.0, y: 1000000.0, z: 1000000.0 };
    let mut max = V3 { x: -1000000.0, y: -1000000.0, z: -1000000.0 };
    for v in verts {
        if v.x > max.x {
            max.x = v.x;
        }
        if v.x < min.x {
            min.x = v.x;
        }
        if v.y > max.y {
            max.y = v.y;
        }
        if v.y < min.y {
            min.y = v.y;
        }
        if v.z > max.z {
            max.z = v.z;
        }
        if v.z < min.z {
            min.z = v.z;
        }
    }
    let half = 1.0f32 / 2.0;
    let center = V3 { x: (min.x + max.x) * half, y: (min.y + max.y) * half, z: (min.z + max.z) * half };
    let d = V3 { x: max.x - center.x, y: max.y - center.y, z: max.z - center.z };
    Sphere { center, radius: (d.x * d.x + d.y * d.y + d.z * d.z).sqrt() }
}

// ---------------------------------------------------------------- mesh lists (buildMeshes)

/// Geometry::buildMeshes without TRISTRIP: each material's triangles' indices, in order.
pub fn list_meshes(tris: &[Triangle], num_meshes: usize) -> Vec<Vec<u16>> {
    let mut meshes = vec![Vec::new(); num_meshes];
    for t in tris {
        meshes[t.mat_id as usize].extend_from_slice(&t.v);
    }
    meshes
}

// ---------------------------------------------------------------- triangle strips (tristrip.cpp)

#[derive(Clone, Copy, Default)]
struct Edge {
    node: usize,
    connected: bool,
    other: usize,
    strip: bool,
}

#[derive(Clone, Copy)]
struct Node {
    v: [u16; 3],
    e: [Edge; 3],
    strip_id: i32,
}

struct StripMesh {
    nodes: Vec<Node>,
    lone: Vec<usize>,
    ends: Vec<usize>,
}

impl StripMesh {
    /// collectFaces
    fn new(tris: &[Triangle], mat: usize) -> StripMesh {
        let nodes = tris
            .iter()
            .filter(|t| t.mat_id as usize == mat)
            .map(|t| Node { v: t.v, e: [Edge::default(); 3], strip_id: -1 })
            .collect();
        StripMesh { nodes, lone: Vec::new(), ends: Vec::new() }
    }

    /// findEdge: the first node with a free edge e0 -> e1.
    fn find_edge(&self, e0: u16, e1: u16) -> Option<(usize, usize)> {
        for (i, n) in self.nodes.iter().enumerate() {
            for j in 0..3 {
                if n.e[j].connected {
                    continue;
                }
                if e0 == n.v[j] && e1 == n.v[(j + 1) % 3] {
                    return Some((i, j));
                }
            }
        }
        None
    }

    /// connectNodesPreserve: link triangles sharing an edge in opposite directions.
    fn connect(&mut self) {
        for i in 0..self.nodes.len() {
            for j in 0..3 {
                if self.nodes[i].e[j].connected {
                    continue;
                }
                let n = self.nodes[i];
                if let Some((node, other)) = self.find_edge(n.v[(j + 1) % 3], n.v[j]) {
                    self.nodes[i].e[j] = Edge { node, connected: true, other, strip: false };
                    self.nodes[node].e[other] = Edge { node: i, connected: true, other: j, strip: false };
                }
            }
        }
    }

    fn connections(&self, n: usize) -> usize {
        self.nodes[n].e.iter().filter(|e| e.connected).count()
    }

    fn strip_edges(&self, n: usize) -> usize {
        self.nodes[n].e.iter().filter(|e| e.strip).count()
    }

    /// complementEdge
    fn complement(&mut self, n: usize, j: usize) {
        let e = self.nodes[n].e[j];
        self.nodes[n].e[j].strip = !e.strip;
        let o = &mut self.nodes[e.node].e[e.other].strip;
        *o = !*o;
    }

    /// extendStrip: walk from start over unclaimed neighbours.
    fn extend(&mut self, start: usize) {
        if self.connections(start) == 0 {
            self.lone.push(start);
            return;
        }
        self.ends.push(start);
        let mut n = start;
        'walk: loop {
            for i in 0..3 {
                let e = self.nodes[n].e[i];
                if !e.connected || self.nodes[e.node].strip_id >= 0 {
                    continue;
                }
                self.nodes[e.node].strip_id = self.nodes[n].strip_id;
                self.complement(n, i);
                n = e.node;
                continue 'walk;
            }
            break;
        }
        if n != start {
            self.ends.push(n);
        }
    }

    /// buildStrips
    fn build(&mut self) {
        for i in 0..self.nodes.len() {
            if self.nodes[i].strip_id >= 0 {
                continue;
            }
            self.nodes[i].strip_id = i as i32;
            self.extend(i);
        }
    }

    fn next_edge(&self, n: usize, last: i32) -> i32 {
        (0..3).find(|&i| self.nodes[n].e[i].strip && i as i32 != last).map_or(-1, |i| i as i32)
    }

    /// makeMesh: the strips' indices, stitched with degenerate triangles, then the single
    /// triangles.
    fn make_mesh(&self) -> Vec<u16> {
        let next = |x: i32| (x + 1) % 3;
        let left = |x: i32| (x + 2) % 3;
        let right = next;
        let mut idx = vec![0xFFFFu16; self.nodes.len() * 5];
        let mut num = 0usize;
        let mut even = true;
        for &start in &self.ends {
            let mut n = start;
            if self.nodes[n].strip_id != n as i32 {
                continue;
            }
            let mut j = self.next_edge(n, -1);
            if j < 0 {
                continue;
            }
            let seam = num;
            if seam != 0 {
                num += 2;
            }
            let mut i;
            if even {
                i = left(j);
                idx[num] = self.nodes[n].v[i as usize];
                idx[num + 1] = self.nodes[n].v[next(i) as usize];
            } else {
                i = right(j);
                idx[num] = self.nodes[n].v[next(i) as usize];
                idx[num + 1] = self.nodes[n].v[i as usize];
            }
            num += 2;
            let mut lastrightturn = -1;
            while j >= 0 {
                let rightturn = (right(i) == j) as i32;
                if rightturn == lastrightturn {
                    idx[num] = idx[num - 2];
                    num += 1;
                    even = !even;
                }
                lastrightturn = rightturn;
                idx[num] = if rightturn != 0 { self.nodes[n].v[next(j) as usize] } else { self.nodes[n].v[j as usize] };
                num += 1;
                even = !even;
                let e = self.nodes[n].e[j as usize];
                i = e.other as i32;
                n = e.node;
                j = self.next_edge(n, i);
            }
            idx[num] = self.nodes[n].v[left(i) as usize];
            num += 1;
            even = !even;
            if seam != 0 {
                idx[seam] = idx[seam - 1];
                idx[seam + 1] = idx[seam + 2];
            }
        }
        let singles = self.ends.iter().filter(|&&n| self.strip_edges(n) == 0).chain(self.lone.iter());
        for &n in singles {
            let v = self.nodes[n].v;
            let (a, b) = (v[!even as usize], v[even as usize]);
            if num != 0 {
                idx[num] = idx[num - 1];
                idx[num + 1] = a;
                num += 2;
            }
            idx[num] = a;
            idx[num + 1] = b;
            idx[num + 2] = v[2];
            num += 3;
            even = !even;
        }
        idx.truncate(num);
        idx
    }
}

/// Geometry::buildTristrips: each material's triangles as strips.
pub fn strip_meshes(tris: &[Triangle], num_meshes: usize) -> Vec<Vec<u16>> {
    (0..num_meshes)
        .map(|m| {
            let mut sm = StripMesh::new(tris, m);
            sm.connect();
            sm.build();
            sm.make_mesh()
        })
        .collect()
}

// ---------------------------------------------------------------- C ABI

/// # Safety
/// `n` points at `out` (may be the same array).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_v3d_transform_points(out: *mut V3, pts: *const V3, n: i32, m: *const RwMatrix) {
    for i in 0..n.max(0) as usize {
        unsafe { *out.add(i) = transform_point(&*pts.add(i), &*m) };
    }
}

/// # Safety
/// `n` vectors at `out` (may be the same array).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_v3d_transform_vectors(out: *mut V3, vs: *const V3, n: i32, m: *const RwMatrix) {
    for i in 0..n.max(0) as usize {
        unsafe { *out.add(i) = transform_vector(&*vs.add(i), &*m) };
    }
}

/// # Safety
/// `n` vertices.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_geo_bounding_sphere(verts: *const V3, n: i32, out: *mut Sphere) {
    let verts = if n > 0 { unsafe { std::slice::from_raw_parts(verts, n as usize) } } else { &[] };
    unsafe { *out = bounding_sphere(verts) };
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwgeometry.txt (recorded from librw's C++, see the recorder).
    use super::*;

    struct In<'a>(std::str::SplitWhitespace<'a>);
    impl In<'_> {
        fn word(&mut self) -> &str {
            self.0.next().expect("short line")
        }
        fn u(&mut self) -> u32 {
            u32::from_str_radix(self.word(), 16).unwrap()
        }
        fn i(&mut self) -> i64 {
            self.word().parse().unwrap()
        }
        fn f(&mut self) -> f32 {
            f32::from_bits(self.u())
        }
        fn v3(&mut self) -> V3 {
            V3 { x: self.f(), y: self.f(), z: self.f() }
        }
        fn m(&mut self) -> RwMatrix {
            let right = self.v3();
            let flags = self.u();
            let up = self.v3();
            self.word();
            let at = self.v3();
            self.word();
            let pos = self.v3();
            self.word();
            RwMatrix { right, flags, up, at, pos, ..Default::default() }
        }
    }

    fn hex(v: f32) -> String {
        format!(" {:08x}", v.to_bits())
    }
    fn hv(a: &V3) -> String {
        hex(a.x) + &hex(a.y) + &hex(a.z)
    }

    #[test]
    fn matches_librw_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwgeometry.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwgeometry.txt");
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let mut w = In(input.split_whitespace());
            let op = w.word().to_string();
            let got = match op.as_str() {
                "tpoints" | "tvectors" => {
                    let m = w.m();
                    let k = w.i() as usize;
                    let mut pts: Vec<V3> = (0..k).map(|_| w.v3()).collect();
                    let p = pts.as_mut_ptr();
                    unsafe {
                        if op == "tpoints" {
                            revc_v3d_transform_points(p, p, k as i32, &m)
                        } else {
                            revc_v3d_transform_vectors(p, p, k as i32, &m)
                        }
                    }
                    pts.iter().map(hv).collect::<String>()
                }
                "sphere" => {
                    let k = w.i() as usize;
                    let v: Vec<V3> = (0..k).map(|_| w.v3()).collect();
                    let s = bounding_sphere(&v);
                    hv(&s.center) + &hex(s.radius)
                }
                "meshes" => {
                    let (_nverts, tristrip, nmats) = (w.i(), w.i(), w.i() as usize);
                    assert_eq!(w.word(), "|");
                    let mut tris = Vec::new();
                    while let Some(a) = w.0.next() {
                        let a: u16 = a.parse().unwrap();
                        tris.push(Triangle { v: [a, w.i() as u16, w.i() as u16], mat_id: w.i() as u16 });
                    }
                    let meshes = if tristrip != 0 { strip_meshes(&tris, nmats) } else { list_meshes(&tris, nmats) };
                    let mut o = format!(" {} |", if tristrip != 0 { MESH_TRISTRIP } else { 0 });
                    for m in &meshes {
                        o += &format!(" {}", m.len());
                        for i in m {
                            o += &format!(" {i}");
                        }
                    }
                    o
                }
                other => panic!("unknown op {other}"),
            };
            n += 1;
            if got != want && bad.len() < 5 {
                bad.push(format!("{}\n  want{}\n  got {}", &input[..input.len().min(200)], &want[..want.len().min(300)], &got[..got.len().min(300)]));
            }
        }
        assert!(n >= 5600, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

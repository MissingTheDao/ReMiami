//! Collision tests (plan task: collision module; C++ src/collision/Collision.cpp, PC paths: no
//! VU_COLLISION, uncompressed vertices). Shapes against shapes, lines of sight, vertical lines and
//! one col model against another, bit for bit as the original (tests/golden/collision.txt).
//!
//! The triangle plane cache (CCollision::CalculateTrianglePlanes, a linked list of models whose
//! planes are allocated) stays in C++ for now: the model functions call back into it.

use crate::math::{Matrix, V3, cross};

// ---- C++ layouts (static_asserts in cpp/re3-miami/src/collision/Collision.cpp) ----

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub center: V3,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColSphere {
    pub center: V3,
    pub radius: f32,
    pub surface: u8,
    pub piece: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BBox {
    pub min: V3,
    pub max: V3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColBox {
    pub min: V3,
    pub max: V3,
    pub surface: u8,
    pub piece: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColLine {
    pub p0: V3,
    pub pad0: i32,
    pub p1: V3,
    pub pad1: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColPoint {
    pub point: V3,
    pub pad1: i32,
    pub normal: V3,
    pub pad2: i32,
    pub surface_a: u8,
    pub piece_a: u8,
    pub surface_b: u8,
    pub piece_b: u8,
    pub depth: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColTriangle {
    pub a: u16,
    pub b: u16,
    pub c: u16,
    pub surface: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColTrianglePlane {
    pub normal: V3,
    pub dist: f32,
    pub dir: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StoredCollPoly {
    pub verts: [V3; 3],
    pub valid: bool,
}

#[repr(C)]
#[derive(Debug)]
pub struct ColModel {
    pub bounding_sphere: Sphere,
    pub bounding_box: BBox,
    pub num_spheres: i16,
    pub num_boxes: i16,
    pub num_triangles: i16,
    pub num_lines: i8,
    pub level: u8,
    pub owns_collision_volumes: bool,
    pub spheres: *mut ColSphere,
    pub lines: *mut ColLine,
    pub boxes: *mut ColBox,
    pub vertices: *mut V3,
    pub triangles: *mut ColTriangle,
    pub triangle_planes: *mut ColTrianglePlane,
}

impl Sphere {
    /// CSphere::Set(radius, center).
    pub fn new(radius: f32, center: V3) -> Sphere {
        Sphere { center, radius }
    }
}

impl BBox {
    pub fn new(min: V3, max: V3) -> BBox {
        BBox { min, max }
    }
}

impl ColSphere {
    fn sphere(&self) -> Sphere {
        Sphere { center: self.center, radius: self.radius }
    }
}

impl ColBox {
    fn bbox(&self) -> BBox {
        BBox { min: self.min, max: self.max }
    }
}

impl ColLine {
    pub fn new(p0: V3, p1: V3) -> ColLine {
        ColLine { p0, p1, ..Default::default() }
    }
}

/// A col model's arrays as slices (empty when the count is 0 or the pointer null).
unsafe fn arr<'a, T>(p: *const T, n: i32) -> &'a [T] {
    if p.is_null() || n <= 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n as usize) } }
}

impl ColModel {
    fn spheres(&self) -> &[ColSphere] {
        unsafe { arr(self.spheres, self.num_spheres as i32) }
    }
    fn boxes(&self) -> &[ColBox] {
        unsafe { arr(self.boxes, self.num_boxes as i32) }
    }
    fn lines(&self) -> &[ColLine] {
        unsafe { arr(self.lines, self.num_lines as i32) }
    }
    fn triangles(&self) -> &[ColTriangle] {
        unsafe { arr(self.triangles, self.num_triangles as i32) }
    }
    fn planes(&self) -> &[ColTrianglePlane] {
        unsafe { arr(self.triangle_planes, self.num_triangles as i32) }
    }
    fn verts(&self) -> *const V3 {
        self.vertices
    }
}

// ---- vector arithmetic in the original's operation order ----

fn add(a: V3, b: V3) -> V3 {
    V3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn sub(a: V3, b: V3) -> V3 {
    V3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn scale(a: V3, s: f32) -> V3 {
    V3::new(a.x * s, a.y * s, a.z * s)
}
fn dot(a: V3, b: V3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn mag_sqr(a: V3) -> f32 {
    a.x * a.x + a.y * a.y + a.z * a.z
}
fn mag(a: V3) -> f32 {
    mag_sqr(a).sqrt()
}
fn normalised(mut a: V3) -> V3 {
    a.normalise();
    a
}
/// MultiplyInverse(mat, vec): by the transposed rotation after removing the translation.
fn multiply_inverse(m: &Matrix, v: V3) -> V3 {
    let f = &m.f;
    let v = V3::new(v.x - f[12], v.y - f[13], v.z - f[14]);
    V3::new(f[0] * v.x + f[1] * v.y + f[2] * v.z, f[4] * v.x + f[5] * v.y + f[6] * v.z, f[8] * v.x + f[9] * v.y + f[10] * v.z)
}

#[derive(Clone, Copy)]
struct V2 {
    x: f32,
    y: f32,
}
fn sub2(a: V2, b: V2) -> V2 {
    V2 { x: a.x - b.x, y: a.y - b.y }
}
fn cross2(a: V2, b: V2) -> f32 {
    a.x * b.y - a.y * b.x
}

// Surface types (SurfaceTable.h).
const SURFACE_GLASS: u8 = 7;
const SURFACE_TRANSPARENT_CLOTH: u8 = 8;
const SURFACE_SCAFFOLD_POLE: u8 = 12;
const SURFACE_METAL_CHAIN_FENCE: u8 = 16;
const SURFACE_TRANSPARENT_STONE: u8 = 31;

pub fn is_see_through(s: u8) -> bool {
    matches!(s, SURFACE_GLASS | SURFACE_TRANSPARENT_CLOTH | SURFACE_METAL_CHAIN_FENCE | SURFACE_TRANSPARENT_STONE | SURFACE_SCAFFOLD_POLE)
}
pub fn is_see_through_vertical(s: u8) -> bool {
    matches!(s, SURFACE_GLASS | SURFACE_TRANSPARENT_CLOTH)
}
pub fn is_shoot_through(s: u8) -> bool {
    matches!(s, SURFACE_TRANSPARENT_CLOTH | SURFACE_METAL_CHAIN_FENCE | SURFACE_TRANSPARENT_STONE | SURFACE_SCAFFOLD_POLE)
}

// ---- triangle planes ----

const DIR_X_POS: u8 = 0;
const DIR_X_NEG: u8 = 1;
const DIR_Y_POS: u8 = 2;
const DIR_Y_NEG: u8 = 3;
const DIR_Z_POS: u8 = 4;

impl ColTrianglePlane {
    /// CColTrianglePlane::Set.
    pub fn set(va: V3, vb: V3, vc: V3) -> ColTrianglePlane {
        let normal = normalised(cross(&sub(vc, va), &sub(vb, va)));
        let dist = dot(normal, va);
        let an = V3::new(normal.x.abs(), normal.y.abs(), normal.z.abs());
        let dir = if an.x > an.y && an.x > an.z {
            if normal.x < 0.0 { DIR_X_NEG } else { DIR_X_POS }
        } else if an.y > an.z {
            if normal.y < 0.0 { DIR_Y_NEG } else { DIR_Y_POS }
        } else if normal.z < 0.0 {
            DIR_Z_POS + 1
        } else {
            DIR_Z_POS
        };
        ColTrianglePlane { normal, dist, dir }
    }
    fn calc_point(&self, v: V3) -> f32 {
        dot(self.normal, v) - self.dist
    }
}

/// The 2D point-in-triangle test of the line/triangle functions: projects on the plane's main
/// axis (winding by its sign) and checks P against the three edges. (The original asserts on a
/// direction outside 0..5; that can't come out of Set, here it counts as DIR_Z_NEG.)
fn inside_triangle(dir: u8, va: V3, vb: V3, vc: V3, p: V3) -> bool {
    let pr = |v: V3| match dir {
        DIR_X_POS | DIR_X_NEG => V2 { x: v.y, y: v.z },
        DIR_Y_POS | DIR_Y_NEG => V2 { x: v.z, y: v.x },
        _ => V2 { x: v.x, y: v.y },
    };
    let (v1, v2, v3, vt) = match dir {
        DIR_X_POS | DIR_Y_POS | DIR_Z_POS => (pr(va), pr(vc), pr(vb), pr(p)),
        _ => (pr(va), pr(vb), pr(vc), pr(p)),
    };
    if cross2(sub2(v2, v1), sub2(vt, v1)) < 0.0 {
        return false;
    }
    if cross2(sub2(v3, v1), sub2(vt, v1)) > 0.0 {
        return false;
    }
    if cross2(sub2(v3, v2), sub2(vt, v2)) < 0.0 {
        return false;
    }
    true
}

unsafe fn tri_verts(verts: *const V3, tri: &ColTriangle) -> (V3, V3, V3) {
    unsafe { (*verts.add(tri.a as usize), *verts.add(tri.b as usize), *verts.add(tri.c as usize)) }
}

// ---- tests ----

pub fn test_sphere_sphere(s1: &Sphere, s2: &Sphere) -> bool {
    let d = s1.radius + s2.radius;
    mag_sqr(sub(s1.center, s2.center)) < d * d
}

pub fn test_sphere_box(sph: &Sphere, b: &BBox) -> bool {
    let (c, r) = (sph.center, sph.radius);
    !(c.x + r < b.min.x || c.x - r > b.max.x || c.y + r < b.min.y || c.y - r > b.max.y || c.z + r < b.min.z || c.z - r > b.max.z)
}

fn inside_box(p: V3, b: &BBox) -> bool {
    p.x > b.min.x && p.x < b.max.x && p.y > b.min.y && p.y < b.max.y && p.z > b.min.z && p.z < b.max.z
}

/// The six face crossings of ProcessLineBox/TestLineBox, in the original's order and formulas:
/// (t, point, normal) of each face the segment crosses inside the face.
fn line_box_faces(l: &ColLine, b: &BBox, mut hit: impl FnMut(f32, V3, V3) -> bool) {
    let (p0, p1) = (l.p0, l.p1);
    // min x
    if (b.min.x - p1.x) * (b.min.x - p0.x) < 0.0 {
        let t = (b.min.x - p0.x) / (p1.x - p0.x);
        let y = p0.y + (p1.y - p0.y) * t;
        if y > b.min.y && y < b.max.y {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z && hit(t, V3::new(b.min.x, y, z), V3::new(-1.0, 0.0, 0.0)) {
                return;
            }
        }
    }
    // max x
    if (p1.x - b.max.x) * (p0.x - b.max.x) < 0.0 {
        let t = (p0.x - b.max.x) / (p0.x - p1.x);
        let y = p0.y + (p1.y - p0.y) * t;
        if y > b.min.y && y < b.max.y {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z && hit(t, V3::new(b.max.x, y, z), V3::new(1.0, 0.0, 0.0)) {
                return;
            }
        }
    }
    // min y
    if (b.min.y - p0.y) * (b.min.y - p1.y) < 0.0 {
        let t = (b.min.y - p0.y) / (p1.y - p0.y);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z && hit(t, V3::new(x, b.min.y, z), V3::new(0.0, -1.0, 0.0)) {
                return;
            }
        }
    }
    // max y
    if (p0.y - b.max.y) * (p1.y - b.max.y) < 0.0 {
        let t = (p0.y - b.max.y) / (p0.y - p1.y);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z && hit(t, V3::new(x, b.max.y, z), V3::new(0.0, 1.0, 0.0)) {
                return;
            }
        }
    }
    // min z
    if (b.min.z - p0.z) * (b.min.z - p1.z) < 0.0 {
        let t = (b.min.z - p0.z) / (p1.z - p0.z);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let y = p0.y + (p1.y - p0.y) * t;
            if y > b.min.y && y < b.max.y && hit(t, V3::new(x, y, b.min.z), V3::new(0.0, 0.0, -1.0)) {
                return;
            }
        }
    }
    // max z
    if (p0.z - b.max.z) * (p1.z - b.max.z) < 0.0 {
        let t = (p0.z - b.max.z) / (p0.z - p1.z);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let y = p0.y + (p1.y - p0.y) * t;
            if y > b.min.y && y < b.max.y {
                hit(t, V3::new(x, y, b.max.z), V3::new(0.0, 0.0, 1.0));
            }
        }
    }
}

pub fn test_line_box(l: &ColLine, b: &BBox) -> bool {
    if inside_box(l.p0, b) || inside_box(l.p1, b) {
        return true;
    }
    let mut found = false;
    line_box_faces(l, b, |_, _, _| {
        found = true;
        true
    });
    found
}

pub fn test_vertical_line_box(l: &ColLine, b: &BBox) -> bool {
    if l.p0.x <= b.min.x || l.p0.y <= b.min.y || l.p0.x >= b.max.x || l.p0.y >= b.max.y {
        return false;
    }
    if l.p0.z < l.p1.z {
        if l.p0.z > b.max.z || l.p1.z < b.min.z {
            return false;
        }
    } else if l.p1.z > b.max.z || l.p0.z < b.min.z {
        return false;
    }
    true
}

/// # Safety
/// `verts` holds the triangle's vertices.
pub unsafe fn test_line_triangle(l: &ColLine, verts: *const V3, tri: &ColTriangle, plane: &ColTrianglePlane) -> bool {
    let normal = plane.normal;
    if plane.calc_point(l.p0) * plane.calc_point(l.p1) > 0.0 {
        return false;
    }
    let p0dist = dot(sub(l.p1, l.p0), normal);
    if p0dist == 0.0 {
        return false; // FIX_BUGS: the line lies in the plane
    }
    let t = -plane.calc_point(l.p0) / p0dist;
    let p = add(l.p0, scale(sub(l.p1, l.p0), t));
    let (va, vb, vc) = unsafe { tri_verts(verts, tri) };
    inside_triangle(plane.dir, va, vb, vc, p)
}

pub fn test_line_sphere(l: &ColLine, sph: &Sphere) -> bool {
    let v01 = sub(l.p1, l.p0);
    let v0c = sub(sph.center, l.p0);
    let linesq = mag_sqr(v01);
    let projline = -2.0 * dot(v01, v0c);
    let tansq = 4.0 * linesq * (mag_sqr(sph.center) - 2.0 * dot(sph.center, l.p0) + mag_sqr(l.p0) - sph.radius * sph.radius);
    let diffsq = projline * projline - tansq;
    if diffsq < 0.0 {
        return false;
    }
    let f = (-projline - diffsq.sqrt()) / (2.0 * linesq);
    (0.0..=1.0).contains(&f)
}

/// Which of the six sectors around the triangle (in its plane) the sphere centre is in, as the
/// sphere/triangle functions compute it: (insideAB, insideAC, insideBC, plane distance, a, b, c).
fn sphere_tri_sectors(center: V3, va: V3, vb: V3, vc: V3, normal: V3) -> (bool, bool, bool) {
    let mut vec2 = sub(vb, va);
    let len = mag(vec2);
    vec2 = scale(vec2, 1.0 / len);
    let vec1 = cross(&vec2, &normal);
    let vac = sub(vc, va);
    let vas = sub(center, va);
    let b = V2 { x: 0.0, y: len };
    let c = V2 { x: dot(vec1, vac), y: dot(vec2, vac) };
    let s = V2 { x: dot(vec1, vas), y: dot(vec2, vas) };
    (cross2(s, b) >= 0.0, cross2(c, s) >= 0.0, cross2(sub2(s, b), sub2(c, b)) >= 0.0)
}

/// # Safety
/// `verts` holds the triangle's vertices.
pub unsafe fn test_sphere_triangle(sphere: &Sphere, verts: *const V3, tri: &ColTriangle, plane: &ColTrianglePlane) -> bool {
    let planedist = plane.calc_point(sphere.center);
    if planedist.abs() > sphere.radius {
        return false;
    }
    let (va, vb, vc) = unsafe { tri_verts(verts, tri) };
    let (ab, ac, bc) = sphere_tri_sectors(sphere.center, va, vb, vc, plane.normal);
    let dist = match ab as i32 + ac as i32 + bc as i32 {
        0 => return false,
        1 => {
            if ab {
                mag(sub(sphere.center, vc))
            } else if ac {
                mag(sub(sphere.center, vb))
            } else {
                mag(sub(sphere.center, va))
            }
        }
        2 => {
            if !ab {
                dist_to_line(va, vb, sphere.center)
            } else if !ac {
                dist_to_line(va, vc, sphere.center)
            } else {
                dist_to_line(vb, vc, sphere.center)
            }
        }
        _ => planedist.abs(),
    };
    dist < sphere.radius
}

// ---- process (find the closest collision point) ----

pub fn process_sphere_sphere(s1: &ColSphere, s2: &ColSphere, point: &mut ColPoint, mindistsq: &mut f32) -> bool {
    let mut dist = sub(s1.center, s2.center);
    let mut d = mag(dist) - s2.radius;
    let depth = s1.radius - d;
    if d < 0.0 {
        d = 0.0;
    }
    if d * d < *mindistsq && d < s1.radius {
        dist.normalise();
        point.point = sub(s1.center, scale(dist, d));
        point.normal = dist;
        point.surface_a = s1.surface;
        point.piece_a = s1.piece;
        point.surface_b = s2.surface;
        point.piece_b = s2.piece;
        point.depth = depth;
        *mindistsq = d * d;
        return true;
    }
    false
}

pub fn process_sphere_box(sph: &ColSphere, b: &ColBox, point: &mut ColPoint, mindistsq: &mut f32) -> bool {
    if !test_sphere_box(&sph.sphere(), &b.bbox()) {
        return false;
    }
    let c = sph.center;
    let pos = |v: f32, lo: f32, hi: f32| if v < lo { 1 } else if v > hi { 2 } else { 0 };
    let (xpos, ypos, zpos) = (pos(c.x, b.min.x, b.max.x), pos(c.y, b.min.y, b.max.y), pos(c.z, b.min.z, b.max.z));
    if xpos == 0 && ypos == 0 && zpos == 0 {
        // Centre inside the box. (The original doesn't lower mindistsq here.)
        let p = scale(add(b.min, b.max), 0.5);
        let dist = sub(c, p);
        let lensq = mag_sqr(dist);
        if lensq < *mindistsq {
            point.normal = scale(dist, 1.0 / lensq.sqrt());
            point.point = sub(c, point.normal);
            point.surface_a = sph.surface;
            point.piece_a = sph.piece;
            point.surface_b = b.surface;
            point.piece_b = b.piece;
            let dx = if dist.x > 0.0 { b.max.x - c.x } else { c.x - b.min.x };
            let dy = if dist.y > 0.0 { b.max.y - c.y } else { c.y - b.min.y };
            let dz = if dist.z > 0.0 { b.max.z - c.z } else { c.z - b.min.z };
            point.depth = if dx > dy && dx > dz {
                dx
            } else if dy > dz {
                dy
            } else {
                dz
            };
            return true;
        }
    } else {
        let pick = |s: i32, lo: f32, hi: f32, v: f32| match s {
            1 => lo,
            2 => hi,
            _ => v,
        };
        let p = V3::new(pick(xpos, b.min.x, b.max.x, c.x), pick(ypos, b.min.y, b.max.y, c.y), pick(zpos, b.min.z, b.max.z, c.z));
        let dist = sub(c, p);
        let lensq = mag_sqr(dist);
        if lensq < *mindistsq {
            let len = lensq.sqrt();
            point.point = p;
            point.normal = scale(dist, 1.0 / len);
            point.surface_a = sph.surface;
            point.piece_a = sph.piece;
            point.surface_b = b.surface;
            point.piece_b = b.piece;
            point.depth = sph.radius - len;
            *mindistsq = lensq;
            return true;
        }
    }
    false
}

pub fn process_line_box(l: &ColLine, b: &ColBox, point: &mut ColPoint, mindist: &mut f32) -> bool {
    let mut mint = 1.0f32;
    let mut p = V3::default();
    let mut normal = V3::default();
    line_box_faces(l, &b.bbox(), |t, q, n| {
        if t < mint {
            mint = t;
            p = q;
            normal = n;
        }
        false
    });
    if mint >= *mindist {
        return false;
    }
    point.point = p;
    point.normal = normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = b.surface;
    point.piece_b = b.piece;
    *mindist = mint;
    true
}

pub fn process_line_sphere(l: &ColLine, sphere: &ColSphere, point: &mut ColPoint, mindist: &mut f32) -> bool {
    let v01 = sub(l.p1, l.p0);
    let v0c = sub(sphere.center, l.p0);
    let linesq = mag_sqr(v01);
    let projline = dot(v01, v0c);
    let tansq = (mag_sqr(v0c) - sphere.radius * sphere.radius) * linesq;
    let diffsq = projline * projline - tansq;
    if diffsq < 0.0 {
        return false;
    }
    let t = (projline - diffsq.sqrt()) / linesq;
    if t < 0.0 || t > 1.0 || t >= *mindist {
        return false;
    }
    point.point = add(l.p0, scale(v01, t));
    point.normal = normalised(sub(point.point, sphere.center));
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = sphere.surface;
    point.piece_b = sphere.piece;
    *mindist = t;
    true
}

fn store_poly(poly: Option<&mut StoredCollPoly>, va: V3, vb: V3, vc: V3) {
    if let Some(poly) = poly {
        poly.verts = [va, vb, vc];
        poly.valid = true;
    }
}

/// (Unused by the game.)
///
/// # Safety
/// `verts` holds the triangle's vertices.
pub unsafe fn process_vertical_line_triangle(
    l: &ColLine,
    verts: *const V3,
    tri: &ColTriangle,
    plane: &ColTrianglePlane,
    point: &mut ColPoint,
    mindist: &mut f32,
    poly: Option<&mut StoredCollPoly>,
) -> bool {
    let p0 = l.p0;
    let (va, vb, vc) = unsafe { tri_verts(verts, tri) };
    if p0.x < va.x && p0.x < vb.x && p0.x < vc.x {
        return false;
    }
    if p0.x > va.x && p0.x > vb.x && p0.x > vc.x {
        return false;
    }
    if p0.y < va.y && p0.y < vb.y && p0.y < vc.y {
        return false;
    }
    if p0.y > va.y && p0.y > vb.y && p0.y > vc.y {
        return false;
    }
    let normal = plane.normal;
    if plane.calc_point(p0) * plane.calc_point(l.p1) > 0.0 {
        return false;
    }
    let h = sub(l.p1, p0).z;
    let t = -plane.calc_point(p0) / (h * normal.z);
    if t >= *mindist {
        return false;
    }
    let p = V3::new(p0.x, p0.y, p0.z + h * t);
    if !inside_triangle(plane.dir, va, vb, vc, p) {
        return false;
    }
    point.point = p;
    point.normal = normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    store_poly(poly, va, vb, vc);
    *mindist = t;
    true
}

pub fn is_stored_poly_still_valid_vertical_line(pos: V3, z: f32, point: &mut ColPoint, poly: &mut StoredCollPoly) -> bool {
    if !poly.valid {
        return false;
    }
    let [va, vb, vc] = poly.verts;
    let plane = ColTrianglePlane::set(va, vb, vc);
    let p0 = pos;
    let p1 = V3::new(pos.x, pos.y, z);
    if plane.calc_point(p0) * plane.calc_point(p1) > 0.0 {
        poly.valid = false;
        return false;
    }
    let t = -plane.calc_point(p0) / dot(sub(p1, p0), plane.normal);
    let p = add(p0, scale(sub(p1, p0), t));
    if !inside_triangle(plane.dir, va, vb, vc, p) {
        poly.valid = false;
        return false;
    }
    point.point = p;
    poly.valid = true;
    true
}

/// # Safety
/// `verts` holds the triangle's vertices.
pub unsafe fn process_line_triangle(
    l: &ColLine,
    verts: *const V3,
    tri: &ColTriangle,
    plane: &ColTrianglePlane,
    point: &mut ColPoint,
    mindist: &mut f32,
    poly: Option<&mut StoredCollPoly>,
) -> bool {
    let normal = plane.normal;
    if plane.calc_point(l.p0) * plane.calc_point(l.p1) > 0.0 {
        return false;
    }
    let p0dist = dot(sub(l.p1, l.p0), normal);
    if p0dist == 0.0 {
        return false; // FIX_BUGS
    }
    let t = -plane.calc_point(l.p0) / p0dist;
    if t >= *mindist {
        return false;
    }
    let p = add(l.p0, scale(sub(l.p1, l.p0), t));
    let (va, vb, vc) = unsafe { tri_verts(verts, tri) };
    if !inside_triangle(plane.dir, va, vb, vc, p) {
        return false;
    }
    point.point = p;
    point.normal = normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    store_poly(poly, va, vb, vc);
    *mindist = t;
    true
}

/// # Safety
/// `verts` holds the triangle's vertices.
pub unsafe fn process_sphere_triangle(
    sphere: &ColSphere,
    verts: *const V3,
    tri: &ColTriangle,
    plane: &ColTrianglePlane,
    point: &mut ColPoint,
    mindistsq: &mut f32,
) -> bool {
    let planedist = plane.calc_point(sphere.center);
    let distsq = planedist * planedist;
    if planedist.abs() > sphere.radius || distsq > *mindistsq {
        return false;
    }
    let (va, vb, vc) = unsafe { tri_verts(verts, tri) };
    let normal = plane.normal;
    let (ab, ac, bc) = sphere_tri_sectors(sphere.center, va, vb, vc, normal);
    let (dist, p) = match ab as i32 + ac as i32 + bc as i32 {
        0 => return false,
        1 => {
            let p = if ab {
                vc
            } else if ac {
                vb
            } else {
                va
            };
            (mag(sub(sphere.center, p)), p)
        }
        2 => {
            if !ab {
                dist_to_line_closest(va, vb, sphere.center)
            } else if !ac {
                dist_to_line_closest(va, vc, sphere.center)
            } else {
                dist_to_line_closest(vb, vc, sphere.center)
            }
        }
        _ => (planedist.abs(), sub(sphere.center, scale(normal, planedist))),
    };
    if dist >= sphere.radius || dist * dist >= *mindistsq {
        return false;
    }
    point.point = p;
    point.normal = normalised(sub(sphere.center, p));
    point.surface_a = sphere.surface;
    point.piece_a = sphere.piece;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    point.depth = sphere.radius - dist;
    *mindistsq = dist * dist;
    true
}

// ---- col models ----

fn inverse(m: &Matrix) -> Matrix {
    let mut inv = Matrix::default();
    Matrix::invert_into(m, &mut inv);
    inv
}

/// Filters of the line-of-sight functions.
fn skipped(surface: u8, see: bool, shoot: bool) -> bool {
    (see && is_see_through(surface)) || (shoot && is_shoot_through(surface))
}

/// `planes(model)` makes sure the model's triangle planes exist (CCollision::CalculateTrianglePlanes).
pub fn test_line_of_sight(line: &ColLine, matrix: &Matrix, model: &mut ColModel, see: bool, shoot: bool, planes: &mut dyn FnMut(&mut ColModel)) -> bool {
    let inv = inverse(matrix);
    let nl = ColLine::new(inv.mul_point(&line.p0), inv.mul_point(&line.p1));
    if !test_line_box(&nl, &model.bounding_box) {
        return false;
    }
    if model.spheres().iter().any(|s| !skipped(s.surface, see, shoot) && test_line_sphere(&nl, &s.sphere())) {
        return true;
    }
    if model.boxes().iter().any(|b| !skipped(b.surface, see, shoot) && test_line_box(&nl, &b.bbox())) {
        return true;
    }
    planes(model);
    let v = model.verts();
    model.triangles().iter().zip(model.planes()).any(|(t, p)| !skipped(t.surface, see, shoot) && unsafe { test_line_triangle(&nl, v, t, p) })
}

#[allow(clippy::too_many_arguments)]
pub fn process_line_of_sight(
    line: &ColLine,
    matrix: &Matrix,
    model: &mut ColModel,
    point: &mut ColPoint,
    mindist: &mut f32,
    see: bool,
    shoot: bool,
    planes: &mut dyn FnMut(&mut ColModel),
) -> bool {
    let inv = inverse(matrix);
    let nl = ColLine::new(inv.mul_point(&line.p0), inv.mul_point(&line.p1));
    if !test_line_box(&nl, &model.bounding_box) {
        return false;
    }
    let mut coldist = *mindist;
    for s in model.spheres().iter().filter(|s| !skipped(s.surface, see, shoot)) {
        process_line_sphere(&nl, s, point, &mut coldist);
    }
    for b in model.boxes().iter().filter(|b| !skipped(b.surface, see, shoot)) {
        process_line_box(&nl, b, point, &mut coldist);
    }
    planes(model);
    let v = model.verts();
    for (t, p) in model.triangles().iter().zip(model.planes()) {
        if !skipped(t.surface, see, shoot) {
            unsafe { process_line_triangle(&nl, v, t, p, point, &mut coldist, None) };
        }
    }
    if coldist < *mindist {
        point.point = matrix.mul_point(&point.point);
        point.normal = matrix.mul3x3(&point.normal);
        *mindist = coldist;
        return true;
    }
    false
}

/// (Checks only see-through-vertical surfaces when `see`, never shoot-through, as the original.)
#[allow(clippy::too_many_arguments)]
pub fn process_vertical_line(
    line: &ColLine,
    matrix: &Matrix,
    model: &mut ColModel,
    point: &mut ColPoint,
    mindist: &mut f32,
    see: bool,
    poly: Option<&mut StoredCollPoly>,
    planes: &mut dyn FnMut(&mut ColModel),
) -> bool {
    let nl = ColLine::new(multiply_inverse(matrix, line.p0), multiply_inverse(matrix, line.p1));
    if !test_line_box(&nl, &model.bounding_box) {
        return false;
    }
    let mut coldist = *mindist;
    for s in model.spheres().iter().filter(|s| !(see && is_see_through_vertical(s.surface))) {
        process_line_sphere(&nl, s, point, &mut coldist);
    }
    for b in model.boxes().iter().filter(|b| !(see && is_see_through_vertical(b.surface))) {
        process_line_box(&nl, b, point, &mut coldist);
    }
    planes(model);
    // The original's static TempStoredPoly: only `valid` is reset per call.
    let mut temp = TEMP_STORED_POLY.with(|t| *t.borrow());
    temp.valid = false;
    let v = model.verts();
    for (t, p) in model.triangles().iter().zip(model.planes()) {
        if !(see && is_see_through_vertical(t.surface)) {
            unsafe { process_line_triangle(&nl, v, t, p, point, &mut coldist, Some(&mut temp)) };
        }
    }
    TEMP_STORED_POLY.with(|t| *t.borrow_mut() = temp);
    if coldist < *mindist {
        point.point = matrix.mul_point(&point.point);
        point.normal = matrix.mul3x3(&point.normal);
        if temp.valid {
            if let Some(poly) = poly {
                // Member-wise copy (the poly's padding stays as it was), then to world space.
                poly.valid = temp.valid;
                for (v, t) in poly.verts.iter_mut().zip(temp.verts) {
                    *v = matrix.mul_point(&t);
                }
            }
        }
        *mindist = coldist;
        return true;
    }
    false
}

thread_local! {
    static TEMP_STORED_POLY: std::cell::RefCell<StoredCollPoly> = std::cell::RefCell::new(StoredCollPoly::default());
}

/// Model A's spheres and lines against model B's spheres, boxes and triangles. Returns how many of
/// A's spheres collide (their points in `spherepoints`); lines' points and distances are by line
/// index in `linepoints`/`linedists`. Points come back in world space.
///
/// # Safety
/// `spherepoints` has room for A's spheres, `linepoints`/`linedists` for its lines.
pub unsafe fn process_col_models(
    matrix_a: &Matrix,
    model_a: &ColModel,
    matrix_b: &Matrix,
    model_b: &mut ColModel,
    spherepoints: *mut ColPoint,
    linepoints: *mut ColPoint,
    linedists: *mut f32,
    planes: &mut dyn FnMut(&mut ColModel),
) -> i32 {
    // A space to B space.
    let mut mat_ab = Matrix::default();
    Matrix::mul_into(&inverse(matrix_b), matrix_a, &mut mat_ab);
    let bsphere_ab = Sphere { center: mat_ab.mul_point(&model_a.bounding_sphere.center), radius: model_a.bounding_sphere.radius };
    if !test_sphere_box(&bsphere_ab, &model_b.bounding_box) {
        return 0;
    }
    let mut mat_ba = Matrix::default();
    Matrix::mul_into(&inverse(matrix_a), matrix_b, &mut mat_ba);

    let spheres_a: Vec<ColSphere> =
        model_a.spheres().iter().map(|s| ColSphere { center: mat_ab.mul_point(&s.center), radius: s.radius, surface: s.surface, piece: s.piece }).collect();
    let lines_a: Vec<ColLine> = model_a.lines().iter().map(|l| ColLine::new(mat_ab.mul_point(&l.p0), mat_ab.mul_point(&l.p1))).collect();

    let idx_spheres_a: Vec<usize> = (0..spheres_a.len()).filter(|&i| test_sphere_box(&spheres_a[i].sphere(), &model_b.bounding_box)).collect();
    // (No test for the lines.)
    let num_lines_a = lines_a.len();
    if idx_spheres_a.is_empty() && num_lines_a == 0 {
        return 0;
    }

    let idx_spheres_b: Vec<usize> = model_b
        .spheres()
        .iter()
        .enumerate()
        .filter(|(_, s)| test_sphere_box(&Sphere { center: mat_ba.mul_point(&s.center), radius: s.radius }, &model_a.bounding_box))
        .map(|(i, _)| i)
        .collect();
    let idx_boxes_b: Vec<usize> = model_b.boxes().iter().enumerate().filter(|(_, b)| test_sphere_box(&bsphere_ab, &b.bbox())).map(|(i, _)| i).collect();
    planes(model_b);
    let vb = model_b.verts();
    let (tris_b, planes_b) = (model_b.triangles(), model_b.planes());
    let idx_tris_b: Vec<usize> = (0..tris_b.len()).filter(|&i| unsafe { test_sphere_triangle(&bsphere_ab, vb, &tris_b[i], &planes_b[i]) }).collect();
    if idx_spheres_b.is_empty() && idx_boxes_b.is_empty() && idx_tris_b.is_empty() {
        return 0;
    }
    let (sph_b, box_b) = (model_b.spheres(), model_b.boxes());

    let mut n = 0usize;
    for &i in &idx_spheres_a {
        let mut coldist = 1.0e24f32;
        let mut collided = false;
        let pt = unsafe { &mut *spherepoints.add(n) };
        for &j in &idx_spheres_b {
            collided |= process_sphere_sphere(&spheres_a[i], &sph_b[j], pt, &mut coldist);
        }
        for &j in &idx_boxes_b {
            collided |= process_sphere_box(&spheres_a[i], &box_b[j], pt, &mut coldist);
        }
        for &j in &idx_tris_b {
            collided |= unsafe { process_sphere_triangle(&spheres_a[i], vb, &tris_b[j], &planes_b[j], pt, &mut coldist) };
        }
        if collided {
            n += 1;
        }
    }
    for i in 0..n {
        let pt = unsafe { &mut *spherepoints.add(i) };
        pt.point = matrix_b.mul_point(&pt.point);
        pt.normal = matrix_b.mul3x3(&pt.normal);
    }

    for (i, line) in lines_a.iter().enumerate() {
        let pt = unsafe { &mut *linepoints.add(i) };
        let dist = unsafe { &mut *linedists.add(i) };
        let mut collided = false;
        for &j in &idx_spheres_b {
            collided |= process_line_sphere(line, &sph_b[j], pt, dist);
        }
        for &j in &idx_boxes_b {
            collided |= process_line_box(line, &box_b[j], pt, dist);
        }
        for &j in &idx_tris_b {
            collided |= unsafe { process_line_triangle(line, vb, &tris_b[j], &planes_b[j], pt, dist, None) };
        }
        if collided {
            pt.point = matrix_b.mul_point(&pt.point);
            pt.normal = matrix_b.mul3x3(&pt.normal);
        }
    }
    n as i32
}

// ---- misc ----

pub fn dist_to_line(l0: V3, l1: V3, point: V3) -> f32 {
    let lensq = mag_sqr(sub(l1, l0));
    let d = dot(sub(point, l0), sub(l1, l0));
    if d <= 0.0 {
        return mag(sub(point, l0));
    }
    if d >= lensq {
        return mag(sub(point, l1));
    }
    let dist_sqr = mag_sqr(sub(point, l0)) - d * d / lensq;
    if dist_sqr <= 0.0 {
        return 0.0;
    }
    dist_sqr.sqrt()
}

/// DistToLine with the closest point: (distance, point).
pub fn dist_to_line_closest(l0: V3, l1: V3, point: V3) -> (f32, V3) {
    let lensq = mag_sqr(sub(l1, l0));
    let d = dot(sub(point, l0), sub(l1, l0));
    let closest = if d <= 0.0 {
        l0
    } else if d >= lensq {
        l1
    } else {
        add(l0, scale(sub(l1, l0), d / lensq))
    };
    (mag(sub(point, closest)), closest)
}

// ---- C ABI (cpp/re3-miami/src/collision/Collision.cpp forwards here) ----

unsafe extern "C" {
    /// CCollision::CalculateTrianglePlanes (the plane cache, C++).
    fn revc_col_calc_planes(model: *mut ColModel);
}

fn cpp_planes(m: &mut ColModel) {
    unsafe { revc_col_calc_planes(m) }
}

#[allow(clippy::missing_safety_doc)]
mod ffi {
    use super::*;

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_sphere_sphere(a: *const Sphere, b: *const Sphere) -> bool {
        unsafe { test_sphere_sphere(&*a, &*b) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_sphere_box(s: *const Sphere, b: *const BBox) -> bool {
        unsafe { test_sphere_box(&*s, &*b) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_line_box(l: *const ColLine, b: *const BBox) -> bool {
        unsafe { test_line_box(&*l, &*b) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_vertical_line_box(l: *const ColLine, b: *const BBox) -> bool {
        unsafe { test_vertical_line_box(&*l, &*b) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_line_triangle(l: *const ColLine, v: *const V3, t: *const ColTriangle, p: *const ColTrianglePlane) -> bool {
        unsafe { test_line_triangle(&*l, v, &*t, &*p) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_line_sphere(l: *const ColLine, s: *const Sphere) -> bool {
        unsafe { test_line_sphere(&*l, &*s) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_sphere_triangle(s: *const Sphere, v: *const V3, t: *const ColTriangle, p: *const ColTrianglePlane) -> bool {
        unsafe { test_sphere_triangle(&*s, v, &*t, &*p) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_test_line_of_sight(l: *const ColLine, m: *const Matrix, model: *mut ColModel, see: bool, shoot: bool) -> bool {
        unsafe { test_line_of_sight(&*l, &*m, &mut *model, see, shoot, &mut cpp_planes) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_sphere_sphere(a: *const ColSphere, b: *const ColSphere, pt: *mut ColPoint, d: *mut f32) -> bool {
        unsafe { process_sphere_sphere(&*a, &*b, &mut *pt, &mut *d) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_sphere_box(s: *const ColSphere, b: *const ColBox, pt: *mut ColPoint, d: *mut f32) -> bool {
        unsafe { process_sphere_box(&*s, &*b, &mut *pt, &mut *d) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_line_box(l: *const ColLine, b: *const ColBox, pt: *mut ColPoint, d: *mut f32) -> bool {
        unsafe { process_line_box(&*l, &*b, &mut *pt, &mut *d) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_line_sphere(l: *const ColLine, s: *const ColSphere, pt: *mut ColPoint, d: *mut f32) -> bool {
        unsafe { process_line_sphere(&*l, &*s, &mut *pt, &mut *d) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_vertical_line_triangle(
        l: *const ColLine,
        v: *const V3,
        t: *const ColTriangle,
        p: *const ColTrianglePlane,
        pt: *mut ColPoint,
        d: *mut f32,
        poly: *mut StoredCollPoly,
    ) -> bool {
        unsafe { process_vertical_line_triangle(&*l, v, &*t, &*p, &mut *pt, &mut *d, poly.as_mut()) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_line_triangle(
        l: *const ColLine,
        v: *const V3,
        t: *const ColTriangle,
        p: *const ColTrianglePlane,
        pt: *mut ColPoint,
        d: *mut f32,
        poly: *mut StoredCollPoly,
    ) -> bool {
        unsafe { process_line_triangle(&*l, v, &*t, &*p, &mut *pt, &mut *d, poly.as_mut()) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_sphere_triangle(
        s: *const ColSphere,
        v: *const V3,
        t: *const ColTriangle,
        p: *const ColTrianglePlane,
        pt: *mut ColPoint,
        d: *mut f32,
    ) -> bool {
        unsafe { process_sphere_triangle(&*s, v, &*t, &*p, &mut *pt, &mut *d) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_line_of_sight(
        l: *const ColLine,
        m: *const Matrix,
        model: *mut ColModel,
        pt: *mut ColPoint,
        d: *mut f32,
        see: bool,
        shoot: bool,
    ) -> bool {
        unsafe { process_line_of_sight(&*l, &*m, &mut *model, &mut *pt, &mut *d, see, shoot, &mut cpp_planes) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_vertical_line(
        l: *const ColLine,
        m: *const Matrix,
        model: *mut ColModel,
        pt: *mut ColPoint,
        d: *mut f32,
        see: bool,
        poly: *mut StoredCollPoly,
    ) -> bool {
        unsafe { process_vertical_line(&*l, &*m, &mut *model, &mut *pt, &mut *d, see, poly.as_mut(), &mut cpp_planes) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_process_col_models(
        ma: *const Matrix,
        a: *mut ColModel,
        mb: *const Matrix,
        b: *mut ColModel,
        sp: *mut ColPoint,
        lp: *mut ColPoint,
        ld: *mut f32,
    ) -> i32 {
        // A and B can be the same model; A is only read.
        unsafe {
            let a_ref: &ColModel = &*a;
            process_col_models(&*ma, a_ref, &*mb, &mut *b, sp, lp, ld, &mut cpp_planes)
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_stored_poly_valid(pos: *const V3, z: f32, pt: *mut ColPoint, poly: *mut StoredCollPoly) -> bool {
        unsafe { is_stored_poly_still_valid_vertical_line(*pos, z, &mut *pt, &mut *poly) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_dist_to_line(l0: *const V3, l1: *const V3, p: *const V3) -> f32 {
        unsafe { dist_to_line(*l0, *l1, *p) }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_dist_to_line_closest(l0: *const V3, l1: *const V3, p: *const V3, closest: *mut V3) -> f32 {
        unsafe {
            let (d, c) = dist_to_line_closest(*l0, *l1, *p);
            *closest = c;
            d
        }
    }
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn revc_col_plane_set(p: *mut ColTrianglePlane, va: *const V3, vb: *const V3, vc: *const V3) {
        unsafe {
            // Only normal, dist and dir: the struct's padding stays as it was.
            let n = ColTrianglePlane::set(*va, *vb, *vc);
            (*p).normal = n.normal;
            (*p).dist = n.dist;
            (*p).dir = n.dir;
        }
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/collision.txt (tests/golden/recorders/collision/recorder.cpp: the
    //! original C++ on 12400 random cases) and compares every output bit.
    use super::*;

    struct Toks<'a>(std::slice::Iter<'a, u32>);
    impl Toks<'_> {
        fn u(&mut self) -> u32 {
            *self.0.next().expect("short line")
        }
        fn f(&mut self) -> f32 {
            f32::from_bits(self.u())
        }
        fn v(&mut self) -> V3 {
            V3::new(self.f(), self.f(), self.f())
        }
        fn sphere(&mut self) -> ColSphere {
            ColSphere { center: self.v(), radius: self.f(), surface: self.u() as u8, piece: self.u() as u8 }
        }
        fn bx(&mut self) -> ColBox {
            ColBox { min: self.v(), max: self.v(), surface: self.u() as u8, piece: self.u() as u8 }
        }
        fn line(&mut self) -> ColLine {
            ColLine::new(self.v(), self.v())
        }
        /// Three vertices and a surface: (verts, triangle 0 1 2, its plane).
        fn tri(&mut self) -> ([V3; 3], ColTriangle, ColTrianglePlane) {
            let v = [self.v(), self.v(), self.v()];
            let t = ColTriangle { a: 0, b: 1, c: 2, surface: self.u() as u8 };
            (v, t, ColTrianglePlane::set(v[0], v[1], v[2]))
        }
        fn matrix(&mut self) -> Matrix {
            let mut m = Matrix::default();
            for x in m.f.iter_mut() {
                *x = self.f();
            }
            m
        }
        fn model(&mut self) -> Model {
            let mut md = Model::default();
            for _ in 0..self.u() {
                md.spheres.push(self.sphere());
            }
            for _ in 0..self.u() {
                md.boxes.push(self.bx());
            }
            for _ in 0..self.u() {
                md.verts.push(self.v());
            }
            for _ in 0..self.u() {
                md.tris.push(ColTriangle { a: self.u() as u16, b: self.u() as u16, c: self.u() as u16, surface: self.u() as u8 });
            }
            for _ in 0..self.u() {
                md.lines.push(self.line());
            }
            md.bsphere = Sphere { center: self.v(), radius: self.f() };
            md.bbox = BBox { min: self.v(), max: self.v() };
            let v = &md.verts;
            md.planes = md.tris.iter().map(|t| ColTrianglePlane::set(v[t.a as usize], v[t.b as usize], v[t.c as usize])).collect();
            md
        }
    }

    #[derive(Default)]
    struct Model {
        spheres: Vec<ColSphere>,
        boxes: Vec<ColBox>,
        verts: Vec<V3>,
        tris: Vec<ColTriangle>,
        lines: Vec<ColLine>,
        planes: Vec<ColTrianglePlane>,
        bsphere: Sphere,
        bbox: BBox,
    }
    impl Model {
        /// The C++ view (pointers into the vectors).
        fn col(&mut self) -> ColModel {
            ColModel {
                bounding_sphere: self.bsphere,
                bounding_box: self.bbox,
                num_spheres: self.spheres.len() as i16,
                num_boxes: self.boxes.len() as i16,
                num_triangles: self.tris.len() as i16,
                num_lines: self.lines.len() as i8,
                level: 0,
                owns_collision_volumes: false,
                spheres: self.spheres.as_mut_ptr(),
                lines: self.lines.as_mut_ptr(),
                boxes: self.boxes.as_mut_ptr(),
                vertices: self.verts.as_mut_ptr(),
                triangles: self.tris.as_mut_ptr(),
                triangle_planes: self.planes.as_mut_ptr(),
            }
        }
    }

    fn fresh_point() -> ColPoint {
        unsafe { std::mem::transmute::<[u8; 40], ColPoint>([0xA5; 40]) }
    }
    fn fresh_poly() -> StoredCollPoly {
        unsafe { std::mem::transmute::<[u8; 40], StoredCollPoly>([0x5A; 40]) }
    }
    fn words<T>(out: &mut Vec<u32>, x: &T) {
        assert_eq!(std::mem::size_of::<T>(), 40);
        let w: [u32; 10] = unsafe { std::mem::transmute_copy(x) };
        out.extend_from_slice(&w);
    }
    fn fv(out: &mut Vec<u32>, v: V3) {
        out.extend([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
    }
    fn no_planes(_: &mut ColModel) {}

    fn run(op: &str, t: &mut Toks) -> Vec<u32> {
        let mut o = Vec::new();
        let b = |x: bool| x as u32;
        match op {
            "tss" => o.push(b(test_sphere_sphere(&t.sphere().sphere(), &t.sphere().sphere()))),
            "tsb" => o.push(b(test_sphere_box(&t.sphere().sphere(), &t.bx().bbox()))),
            "tlb" => o.push(b(test_line_box(&t.line(), &t.bx().bbox()))),
            "tvlb" => o.push(b(test_vertical_line_box(&t.line(), &t.bx().bbox()))),
            "tlt" => {
                let l = t.line();
                let (v, tr, p) = t.tri();
                o.push(b(unsafe { test_line_triangle(&l, v.as_ptr(), &tr, &p) }));
            }
            "tls" => o.push(b(test_line_sphere(&t.line(), &t.sphere().sphere()))),
            "tst" => {
                let s = t.sphere();
                let (v, tr, p) = t.tri();
                o.push(b(unsafe { test_sphere_triangle(&s.sphere(), v.as_ptr(), &tr, &p) }));
            }
            "pss" | "psb" | "plb" | "pls" | "pst" => {
                let mut pt = fresh_point();
                let r = match op {
                    "pss" => {
                        let (a, s2, mut md) = (t.sphere(), t.sphere(), t.f());
                        let r = process_sphere_sphere(&a, &s2, &mut pt, &mut md);
                        (r, md)
                    }
                    "psb" => {
                        let (a, bx, mut md) = (t.sphere(), t.bx(), t.f());
                        (process_sphere_box(&a, &bx, &mut pt, &mut md), md)
                    }
                    "plb" => {
                        let (l, bx, mut md) = (t.line(), t.bx(), t.f());
                        (process_line_box(&l, &bx, &mut pt, &mut md), md)
                    }
                    "pls" => {
                        let (l, s, mut md) = (t.line(), t.sphere(), t.f());
                        (process_line_sphere(&l, &s, &mut pt, &mut md), md)
                    }
                    _ => {
                        let s = t.sphere();
                        let (v, tr, p) = t.tri();
                        let mut md = t.f();
                        (unsafe { process_sphere_triangle(&s, v.as_ptr(), &tr, &p, &mut pt, &mut md) }, md)
                    }
                };
                o.push(b(r.0));
                words(&mut o, &pt);
                o.push(r.1.to_bits());
            }
            "pvlt" | "plt" => {
                let l = t.line();
                let (v, tr, p) = t.tri();
                let mut md = t.f();
                let (mut pt, mut q) = (fresh_point(), fresh_poly());
                let r = unsafe {
                    if op == "pvlt" {
                        process_vertical_line_triangle(&l, v.as_ptr(), &tr, &p, &mut pt, &mut md, Some(&mut q))
                    } else {
                        process_line_triangle(&l, v.as_ptr(), &tr, &p, &mut pt, &mut md, Some(&mut q))
                    }
                };
                o.push(b(r));
                words(&mut o, &pt);
                o.push(md.to_bits());
                words(&mut o, &q);
            }
            "spv" => {
                let (pos, z) = (t.v(), t.f());
                let mut w = [0u32; 10];
                for x in w.iter_mut() {
                    *x = t.u();
                }
                // The bool's byte holds 0 or 1; the other three bytes are the 0x5A padding.
                let mut q: StoredCollPoly = unsafe { std::mem::transmute(w) };
                let mut pt = fresh_point();
                o.push(b(is_stored_poly_still_valid_vertical_line(pos, z, &mut pt, &mut q)));
                words(&mut o, &pt);
                words(&mut o, &q);
            }
            "d2l" => o.push(dist_to_line(t.v(), t.v(), t.v()).to_bits()),
            "d2lc" => {
                let (d, c) = dist_to_line_closest(t.v(), t.v(), t.v());
                o.push(d.to_bits());
                fv(&mut o, c);
            }
            "plane" => {
                let p = ColTrianglePlane::set(t.v(), t.v(), t.v());
                fv(&mut o, p.normal);
                o.push(p.dist.to_bits());
                o.push(p.dir as u32);
            }
            "tlos" => {
                let (l, m, mut md) = (t.line(), t.matrix(), t.model());
                let (see, shoot) = (t.u() != 0, t.u() != 0);
                o.push(b(test_line_of_sight(&l, &m, &mut md.col(), see, shoot, &mut no_planes)));
            }
            "plos" => {
                let (l, m, mut md) = (t.line(), t.matrix(), t.model());
                let mut d = t.f();
                let (see, shoot) = (t.u() != 0, t.u() != 0);
                let mut pt = fresh_point();
                o.push(b(process_line_of_sight(&l, &m, &mut md.col(), &mut pt, &mut d, see, shoot, &mut no_planes)));
                words(&mut o, &pt);
                o.push(d.to_bits());
            }
            "pvl" => {
                let (l, m, mut md) = (t.line(), t.matrix(), t.model());
                let mut d = t.f();
                let see = t.u() != 0;
                let (mut pt, mut q) = (fresh_point(), fresh_poly());
                o.push(b(process_vertical_line(&l, &m, &mut md.col(), &mut pt, &mut d, see, Some(&mut q), &mut no_planes)));
                words(&mut o, &pt);
                o.push(d.to_bits());
                words(&mut o, &q);
            }
            "pcm" => {
                let (ma, mut a, mb, mut bm) = (t.matrix(), t.model(), t.matrix(), t.model());
                let (mut sp, mut lp, mut ld) = ([fresh_point(); 16], [fresh_point(); 16], [1.0f32; 16]);
                let ca = a.col();
                let n = unsafe { process_col_models(&ma, &ca, &mb, &mut bm.col(), sp.as_mut_ptr(), lp.as_mut_ptr(), ld.as_mut_ptr(), &mut no_planes) };
                o.push(n as u32);
                for p in &sp[..n as usize] {
                    words(&mut o, p);
                }
                for k in 0..a.lines.len() {
                    words(&mut o, &lp[k]);
                    o.push(ld[k].to_bits());
                }
            }
            _ => panic!("unknown op {op}"),
        }
        o
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/collision.txt")).unwrap();
        let hex = |s: &str| s.split_whitespace().map(|w| u32::from_str_radix(w, 16).unwrap()).collect::<Vec<u32>>();
        let (mut cases, mut bad) = (0, Vec::new());
        for (i, line) in text.lines().enumerate() {
            let (lhs, rhs) = line.split_once(" =>").unwrap();
            let (op, ins) = lhs.split_once(' ').unwrap();
            let ins = hex(ins);
            let mut toks = Toks(ins.iter());
            let got = run(op, &mut toks);
            assert!(toks.0.next().is_none(), "line {}: {op}: inputs left over", i + 1);
            let mut want = hex(rhs);
            let mut got = got;
            // A zero-length line makes ProcessLineSphere's t 0/0 = NaN, which passes its range
            // checks and makes the distance NaN; ProcessLineBox then writes its uninitialised point
            // and normal (stack garbage in the original). The result stays "no hit" with the
            // distance unchanged; only the scratch point's position and normal can't match.
            if matches!(op, "plos" | "pvl") && want[0] == 0 {
                for w in [&mut want, &mut got] {
                    for k in [1, 2, 3, 5, 6, 7] {
                        w[k] = 0;
                    }
                }
            }
            // The same through ProcessColModels' lines: the NaN distance lets ProcessLineBox
            // "hit" at exactly 1.0 with its uninitialised point, which then goes to world space.
            // (A real line hit is always nearer than 1.0.)
            if op == "pcm" && want.len() == got.len() {
                let first_line = 1 + 10 * want[0] as usize;
                for at in (first_line..want.len()).step_by(11) {
                    if want[at + 10] == 0x3f80_0000 && want[at + 8] != 0xa5a5_a5a5 {
                        for w in [&mut want, &mut got] {
                            for k in [0, 1, 2, 4, 5, 6] {
                                w[at + k] = 0;
                            }
                        }
                    }
                }
            }
            if got != want {
                bad.push(format!("line {} {op}: expected{rhs}\n  got {}", i + 1, got.iter().map(|w| format!(" {w:08x}")).collect::<String>()));
            }
            cases += 1;
        }
        assert_eq!(cases, 12400);
        assert!(bad.is_empty(), "{} of {cases} differ:\n{}", bad.len(), bad[..bad.len().min(20)].join("\n"));
    }
}

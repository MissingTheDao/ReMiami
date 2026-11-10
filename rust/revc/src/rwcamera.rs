//! librw's camera math (vendor/librw/src/camera.cpp), stage 2 step 4a: the view matrix, frustum
//! corners and planes and their bounding box (cameraSync with buildClipPersp/buildClipParallel and
//! buildPlanes), the sphere test, the depth-range constants (calczShiftScale) and setFOV's view
//! window. camera.cpp forwards here; rw::Camera stays a C++ object and passes its fields.
//!
//! Bit-identical to librw compiled like the game (same expression order, same libm calls).
//! Checked against tests/golden/rwcamera.txt, recorded from the C++ (tests/golden/recorders/rwcamera).

use crate::math::{RwMatrix, V3};
use crate::rwmatrix;

pub const PERSPECTIVE: i32 = 1;
pub const SPHEREOUTSIDE: i32 = 0;
pub const SPHEREBOUNDARY: i32 = 1;
pub const SPHEREINSIDE: i32 = 2;

/// rw::FrustumPlane (20 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrustumPlane {
    pub normal: V3,
    pub distance: f32,
    pub closest_x: u8,
    pub closest_y: u8,
    pub closest_z: u8,
}

/// rw::BBox.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BBox {
    pub sup: V3,
    pub inf: V3,
}

/// rw::Sphere.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub center: V3,
    pub radius: f32,
}

/// rw::Camera's viewWindow .. projection (contiguous, 32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CamParams {
    pub view_window: [f32; 2],
    pub view_offset: [f32; 2],
    pub near: f32,
    pub far: f32,
    pub fog: f32,
    pub projection: i32,
}

fn v(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}
fn add(a: V3, b: V3) -> V3 {
    v(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn sub(a: V3, b: V3) -> V3 {
    v(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn scale(a: V3, r: f32) -> V3 {
    v(a.x * r, a.y * r, a.z * r)
}
fn neg(a: V3) -> V3 {
    v(-a.x, -a.y, -a.z)
}
fn dot(a: V3, b: V3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: V3, b: V3) -> V3 {
    v(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}
fn normalize(a: V3) -> V3 {
    scale(a, 1.0 / (a.x * a.x + a.y * a.y + a.z * a.z).sqrt())
}

fn plane(normal: V3, through: V3) -> FrustumPlane {
    FrustumPlane {
        normal,
        distance: dot(normal, through),
        closest_x: !(normal.x < 0.0) as u8,
        closest_y: !(normal.y < 0.0) as u8,
        closest_z: !(normal.z < 0.0) as u8,
    }
}

fn build_planes(ltm: &RwMatrix, c: &[V3; 8], p: &mut [FrustumPlane; 6]) {
    let v51 = sub(c[1], c[5]);
    let v73 = sub(c[3], c[7]);
    p[0] = plane(ltm.at, c[4]); // far
    p[1] = plane(neg(p[0].normal), c[0]); // near
    p[2] = plane(normalize(cross(v51, sub(c[6], c[5]))), c[1]); // right
    p[3] = plane(normalize(cross(sub(c[4], c[5]), v51)), c[1]); // top
    p[4] = plane(normalize(cross(v73, sub(c[4], c[7]))), c[3]); // left
    p[5] = plane(normalize(cross(sub(c[6], c[7]), v73)), c[3]); // bottom
}

fn build_clip_persp(ltm: &RwMatrix, cam: &CamParams, c: &mut [V3; 8]) {
    let up = scale(ltm.up, cam.view_window[1]);
    let left = scale(ltm.right, cam.view_window[0]);
    c[0] = add(add(ltm.at, up), left);
    c[1] = sub(add(ltm.at, up), left);
    c[2] = sub(sub(ltm.at, up), left);
    c[3] = add(sub(ltm.at, up), left);
    let off = sub(scale(ltm.up, cam.view_offset[1]), scale(ltm.right, cam.view_offset[0]));
    for i in 0..4 {
        let corner = sub(c[i], off);
        let pos = add(ltm.pos, off);
        c[i] = add(scale(corner, cam.near), pos);
        c[i + 4] = add(scale(corner, cam.far), pos);
    }
}

fn build_clip_parallel(ltm: &RwMatrix, cam: &CamParams, c: &mut [V3; 8]) {
    let (vw, vo) = (cam.view_window, cam.view_offset);
    let nearoffx = -(1.0 - cam.near) * vo[0];
    let nearoffy = (1.0 - cam.near) * vo[1];
    let faroffx = -(1.0 - cam.far) * vo[0];
    let faroffy = (1.0 - cam.far) * vo[1];
    c[0] = v(nearoffx + vw[0], nearoffy + vw[1], cam.near);
    c[1] = v(nearoffx - vw[0], nearoffy + vw[1], cam.near);
    c[2] = v(nearoffx - vw[0], nearoffy - vw[1], cam.near);
    c[3] = v(nearoffx + vw[0], nearoffy - vw[1], cam.near);
    c[4] = v(faroffx + vw[0], faroffy + vw[1], cam.far);
    c[5] = v(faroffx - vw[0], faroffy + vw[1], cam.far);
    c[6] = v(faroffx - vw[0], faroffy - vw[1], cam.far);
    c[7] = v(faroffx + vw[0], faroffy - vw[1], cam.far);
    // V3d::transformPoints
    let m = ltm;
    for p in c.iter_mut() {
        let i = *p;
        *p = v(
            i.x * m.right.x + i.y * m.up.x + i.z * m.at.x + m.pos.x,
            i.x * m.right.y + i.y * m.up.y + i.z * m.at.y + m.pos.y,
            i.x * m.right.z + i.y * m.up.z + i.z * m.at.z + m.pos.z,
        );
    }
}

/// BBox::calculate
fn bbox(points: &[V3; 8]) -> BBox {
    let mut b = BBox { sup: points[0], inf: points[0] };
    for p in &points[1..] {
        if p.x < b.inf.x {
            b.inf.x = p.x;
        }
        if p.y < b.inf.y {
            b.inf.y = p.y;
        }
        if p.z < b.inf.z {
            b.inf.z = p.z;
        }
        if p.x > b.sup.x {
            b.sup.x = p.x;
        }
        if p.y > b.sup.y {
            b.sup.y = p.y;
        }
        if p.z > b.sup.z {
            b.sup.z = p.z;
        }
    }
    b
}

/// cameraSync for a camera whose frame has this LTM.
pub fn sync(ltm: &RwMatrix, cam: &CamParams, view: &mut RwMatrix, planes: &mut [FrustumPlane; 6], corners: &mut [V3; 8], bound: &mut BBox) {
    let mut inv = RwMatrix::default();
    unsafe { rwmatrix::invert_orthonormal(&mut inv, ltm) };
    inv.right.x = -inv.right.x;
    inv.up.x = -inv.up.x;
    inv.at.x = -inv.at.x;
    inv.pos.x = -inv.pos.x;

    let xscl = 1.0 / (2.0 * cam.view_window[0]);
    let yscl = 1.0 / (2.0 * cam.view_window[1]);
    let mut proj = RwMatrix::default();
    proj.right = v(xscl, 0.0, 0.0);
    proj.up = v(0.0, -yscl, 0.0);
    if cam.projection == PERSPECTIVE {
        proj.pos = v(-cam.view_offset[0] * xscl, cam.view_offset[1] * yscl, 0.0);
        proj.at = v(-proj.pos.x + 0.5, -proj.pos.y + 0.5, 1.0);
    } else {
        proj.at = v(cam.view_offset[0] * xscl, -cam.view_offset[1] * yscl, 1.0);
        proj.pos = v(-proj.at.x + 0.5, -proj.at.y + 0.5, 0.0);
    }
    unsafe {
        rwmatrix::optimize(&mut proj, std::ptr::null());
        rwmatrix::mult(view, &inv, &proj);
    }
    if cam.projection == PERSPECTIVE {
        build_clip_persp(ltm, cam, corners);
    } else {
        build_clip_parallel(ltm, cam, corners);
    }
    build_planes(ltm, corners, planes);
    *bound = bbox(corners);
}

/// Camera::frustumTestSphere
pub fn frustum_test_sphere(planes: &[FrustumPlane; 6], s: &Sphere) -> i32 {
    let mut res = SPHEREINSIDE;
    for p in planes {
        let dist = dot(p.normal, s.center) - p.distance;
        if s.radius < dist {
            return SPHEREOUTSIDE;
        }
        if s.radius > -dist {
            res = SPHEREBOUNDARY;
        }
    }
    res
}

/// calczShiftScale: (zScale, zShift) for the device's depth range [z_near, z_far].
pub fn z_shift_scale(n: f32, f: f32, projection: i32, z_near: f32, z_far: f32) -> (f32, f32) {
    let mut nn = z_near;
    let mut ff = z_far;
    nn += (ff - nn) / 10000.0;
    ff -= (ff - nn) / 10000.0;
    if projection == PERSPECTIVE {
        ((nn - ff) * n * f / (f - n), (ff * f - nn * n) / (f - n))
    } else {
        ((ff - nn) / (f - n), (nn * f - ff * n) / (f - n))
    }
}

/// Camera::setFOV's view window (horizontal FOV meant for 4:3, adapted to the frame buffer).
pub fn fov_window(hfov: f32, ratio: f32, width: i32, height: i32) -> [f32; 2] {
    let (mut w, mut h) = (width as f32, height as f32);
    if w < 1.0 || h < 1.0 {
        w = 1.0;
        h = 1.0;
    }
    let hfov = hfov * 3.14159 / 360.0;
    let ar1 = 4.0f32 / 3.0;
    let ar2 = w / h;
    let vfov = ((hfov / 2.0).tan() / ar1).atan() * 2.0;
    let hfov = ((vfov / 2.0).tan() * ar2).atan() * 2.0;
    let a = hfov.tan();
    [a, a / ratio]
}

// ---------------------------------------------------------------- C ABI (camera.cpp forwards)

/// # Safety
/// Pointers to a camera's fields as camera.cpp passes them (planes: 6, corners: 8).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_camera_sync(ltm: *const RwMatrix, cam: *const CamParams, view: *mut RwMatrix, planes: *mut FrustumPlane, corners: *mut V3, bound: *mut BBox) {
    unsafe { sync(&*ltm, &*cam, &mut *view, &mut *(planes as *mut [FrustumPlane; 6]), &mut *(corners as *mut [V3; 8]), &mut *bound) }
}

/// # Safety
/// `planes`: 6 planes; `s`: a sphere.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_camera_frustum_test_sphere(planes: *const FrustumPlane, s: *const Sphere) -> i32 {
    unsafe { frustum_test_sphere(&*(planes as *const [FrustumPlane; 6]), &*s) }
}

/// # Safety
/// `out`: zScale, zShift.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_camera_z_shift_scale(n: f32, f: f32, projection: i32, z_near: f32, z_far: f32, out: *mut [f32; 2]) {
    let (s, t) = z_shift_scale(n, f, projection, z_near, z_far);
    unsafe { *out = [s, t] };
}

/// # Safety
/// `out`: the view window.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_camera_fov_window(hfov: f32, ratio: f32, width: i32, height: i32, out: *mut [f32; 2]) {
    unsafe { *out = fov_window(hfov, ratio, width, height) };
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwcamera.txt (recorded from librw's C++, see the recorder).
    use super::*;

    struct In<'a>(std::str::SplitWhitespace<'a>);
    impl In<'_> {
        fn word(&mut self) -> &str {
            self.0.next().expect("short line")
        }
        fn u(&mut self) -> u32 {
            u32::from_str_radix(self.word(), 16).unwrap()
        }
        fn i(&mut self) -> i32 {
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
    fn hv(a: V3) -> String {
        hex(a.x) + &hex(a.y) + &hex(a.z)
    }
    fn hm(m: &RwMatrix) -> String {
        format!("{} {:08x}{} --------{} --------{} --------", hv(m.right), m.flags, hv(m.up), hv(m.at), hv(m.pos))
    }

    #[test]
    fn matches_librw_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwcamera.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwcamera.txt");
        let mut planes = [FrustumPlane::default(); 6];
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let mut w = In(input.split_whitespace());
            let op = w.word().to_string();
            let got = match op.as_str() {
                "sync" => {
                    let ltm = w.m();
                    let cam = CamParams {
                        view_window: [w.f(), w.f()],
                        view_offset: [w.f(), w.f()],
                        near: w.f(),
                        far: w.f(),
                        fog: 0.0,
                        projection: w.i(),
                    };
                    let (mut view, mut corners, mut bound) = (RwMatrix::default(), [V3::default(); 8], BBox::default());
                    sync(&ltm, &cam, &mut view, &mut planes, &mut corners, &mut bound);
                    let mut o = hm(&view);
                    for p in &planes {
                        o += &format!("{}{} {} {} {}", hv(p.normal), hex(p.distance), p.closest_x, p.closest_y, p.closest_z);
                    }
                    for c in &corners {
                        o += &hv(*c);
                    }
                    o + &hv(bound.sup) + &hv(bound.inf)
                }
                "sphere" => {
                    let s = Sphere { center: w.v3(), radius: w.f() };
                    format!(" {}", frustum_test_sphere(&planes, &s))
                }
                "zss" => {
                    let (nr, fr, p, zn, zf) = (w.f(), w.f(), w.i(), w.f(), w.f());
                    let (s, t) = z_shift_scale(nr, fr, p, zn, zf);
                    hex(s) + &hex(t)
                }
                "fov" => {
                    let (h, r, wd, ht) = (w.f(), w.f(), w.i(), w.i());
                    let o = fov_window(h, r, wd, ht);
                    hex(o[0]) + &hex(o[1])
                }
                other => panic!("unknown op {other}"),
            };
            n += 1;
            if got != want && bad.len() < 5 {
                bad.push(format!("{input}\n  want{want}\n  got {got}"));
            }
        }
        assert!(n > 20000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

//! librw's rw::Matrix functions (vendor/librw/src/base.cpp), stage 2 step 3a: frames and matrices
//! in Rust. The C++ functions forward here (rw::Matrix keeps its layout: math::RwMatrix).
//!
//! Bit-identical to librw compiled like the game: every expression keeps the C++ evaluation order
//! and the same libm calls (sqrtf, sinf, cosf). The functions work through raw pointers and keep
//! librw's statement order, so calls whose destination aliases a source (dst == src) give the
//! same results as librw's (invertGeneral reads back what it already wrote). Checked against
//! tests/golden/rwmatrix.txt, recorded from the C++ (tests/golden/recorders/rwmatrix).

use crate::math::{Quat, RwMatrix, V3};

pub const TYPENORMAL: u32 = 1;
pub const TYPEORTHOGONAL: u32 = 2;
pub const TYPEORTHONORMAL: u32 = 3;
pub const TYPEMASK: u32 = 3;
pub const IDENTITY: u32 = 0x20000;

const COMBINEREPLACE: i32 = 0;
const COMBINEPRECONCAT: i32 = 1;
const COMBINEPOSTCONCAT: i32 = 2;

#[repr(C)]
pub struct Tolerance {
    pub normal: f32,
    pub orthogonal: f32,
    pub identity: f32,
}

const DEFAULT_TOLERANCE: Tolerance = Tolerance { normal: 0.01, orthogonal: 0.01, identity: 0.01 };

pub const IDENT: RwMatrix = RwMatrix {
    right: V3 { x: 1.0, y: 0.0, z: 0.0 },
    flags: IDENTITY | TYPEORTHONORMAL,
    up: V3 { x: 0.0, y: 1.0, z: 0.0 },
    pad1: 0,
    at: V3 { x: 0.0, y: 0.0, z: 1.0 },
    pad2: 0,
    pos: V3 { x: 0.0, y: 0.0, z: 0.0 },
    pad3: 0,
};

fn dot(a: &V3, b: &V3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn scale(a: &V3, r: f32) -> V3 {
    V3 { x: a.x * r, y: a.y * r, z: a.z * r }
}

fn normalize(v: &V3) -> V3 {
    scale(v, 1.0 / (v.x * v.x + v.y * v.y + v.z * v.z).sqrt())
}

fn cross(a: &V3, b: &V3) -> V3 {
    V3 { x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x }
}

/// # Safety (all functions below)
/// Pointers point to valid rw::Matrix / V3d / Quat values; they may alias as in librw.
pub unsafe fn normal_error(m: *const RwMatrix) -> f32 {
    let m = unsafe { &*m };
    let x = dot(&m.right, &m.right) - 1.0;
    let y = dot(&m.up, &m.up) - 1.0;
    let z = dot(&m.at, &m.at) - 1.0;
    x * x + y * y + z * z
}

pub unsafe fn orthogonal_error(m: *const RwMatrix) -> f32 {
    let m = unsafe { &*m };
    let x = dot(&m.at, &m.up);
    let y = dot(&m.at, &m.right);
    let z = dot(&m.up, &m.right);
    x * x + y * y + z * z
}

pub unsafe fn identity_error(m: *const RwMatrix) -> f32 {
    let m = unsafe { &*m };
    let r = V3 { x: m.right.x - 1.0, y: m.right.y, z: m.right.z };
    let u = V3 { x: m.up.x, y: m.up.y - 1.0, z: m.up.z };
    let a = V3 { x: m.at.x, y: m.at.y, z: m.at.z - 1.0 };
    dot(&r, &r) + dot(&u, &u) + dot(&a, &a) + dot(&m.pos, &m.pos)
}

pub unsafe fn optimize(m: *mut RwMatrix, tol: *const Tolerance) {
    let tol = if tol.is_null() { &DEFAULT_TOLERANCE } else { unsafe { &*tol } };
    unsafe {
        let isnormal = normal_error(m) <= tol.normal;
        let isorthogonal = orthogonal_error(m) <= tol.orthogonal;
        let isidentity = isnormal && isorthogonal && identity_error(m) <= tol.identity;
        let f = &mut (*m).flags;
        if isnormal { *f |= TYPENORMAL } else { *f &= !TYPENORMAL }
        if isorthogonal { *f |= TYPEORTHOGONAL } else { *f &= !TYPEORTHOGONAL }
        if isidentity { *f |= IDENTITY } else { *f &= !IDENTITY }
    }
}

/// Matrix::mult_: dst = src1 * src2 (rows), statement by statement (aliasing as in librw).
pub unsafe fn mult_(d: *mut RwMatrix, a: *const RwMatrix, b: *const RwMatrix) {
    unsafe {
        (*d).right.x = (*a).right.x * (*b).right.x + (*a).right.y * (*b).up.x + (*a).right.z * (*b).at.x;
        (*d).right.y = (*a).right.x * (*b).right.y + (*a).right.y * (*b).up.y + (*a).right.z * (*b).at.y;
        (*d).right.z = (*a).right.x * (*b).right.z + (*a).right.y * (*b).up.z + (*a).right.z * (*b).at.z;
        (*d).up.x = (*a).up.x * (*b).right.x + (*a).up.y * (*b).up.x + (*a).up.z * (*b).at.x;
        (*d).up.y = (*a).up.x * (*b).right.y + (*a).up.y * (*b).up.y + (*a).up.z * (*b).at.y;
        (*d).up.z = (*a).up.x * (*b).right.z + (*a).up.y * (*b).up.z + (*a).up.z * (*b).at.z;
        (*d).at.x = (*a).at.x * (*b).right.x + (*a).at.y * (*b).up.x + (*a).at.z * (*b).at.x;
        (*d).at.y = (*a).at.x * (*b).right.y + (*a).at.y * (*b).up.y + (*a).at.z * (*b).at.y;
        (*d).at.z = (*a).at.x * (*b).right.z + (*a).at.y * (*b).up.z + (*a).at.z * (*b).at.z;
        (*d).pos.x = (*a).pos.x * (*b).right.x + (*a).pos.y * (*b).up.x + (*a).pos.z * (*b).at.x + (*b).pos.x;
        (*d).pos.y = (*a).pos.x * (*b).right.y + (*a).pos.y * (*b).up.y + (*a).pos.z * (*b).at.y + (*b).pos.y;
        (*d).pos.z = (*a).pos.x * (*b).right.z + (*a).pos.y * (*b).up.z + (*a).pos.z * (*b).at.z + (*b).pos.z;
    }
}

pub unsafe fn mult(d: *mut RwMatrix, a: *const RwMatrix, b: *const RwMatrix) {
    unsafe {
        if (*a).flags & IDENTITY != 0 {
            *d = *b;
        } else if (*b).flags & IDENTITY != 0 {
            *d = *a;
        } else {
            mult_(d, a, b);
            (*d).flags = (*a).flags & (*b).flags;
        }
    }
}

pub unsafe fn invert_orthonormal(d: *mut RwMatrix, s: *const RwMatrix) {
    unsafe {
        (*d).right.x = (*s).right.x;
        (*d).right.y = (*s).up.x;
        (*d).right.z = (*s).at.x;
        (*d).up.x = (*s).right.y;
        (*d).up.y = (*s).up.y;
        (*d).up.z = (*s).at.y;
        (*d).at.x = (*s).right.z;
        (*d).at.y = (*s).up.z;
        (*d).at.z = (*s).at.z;
        (*d).pos.x = -((*s).pos.x * (*s).right.x + (*s).pos.y * (*s).right.y + (*s).pos.z * (*s).right.z);
        (*d).pos.y = -((*s).pos.x * (*s).up.x + (*s).pos.y * (*s).up.y + (*s).pos.z * (*s).up.z);
        (*d).pos.z = -((*s).pos.x * (*s).at.x + (*s).pos.y * (*s).at.y + (*s).pos.z * (*s).at.z);
        (*d).flags = TYPEORTHONORMAL;
    }
}

pub unsafe fn invert_general(d: *mut RwMatrix, s: *const RwMatrix) {
    unsafe {
        (*d).right.x = (*s).up.y * (*s).at.z - (*s).up.z * (*s).at.y;
        (*d).right.y = (*s).at.y * (*s).right.z - (*s).at.z * (*s).right.y;
        (*d).right.z = (*s).right.y * (*s).up.z - (*s).right.z * (*s).up.y;
        let det = (*s).up.x * (*d).right.y + (*s).at.x * (*d).right.z + (*d).right.x * (*s).right.x;
        let mut invdet = 1.0f32;
        if det != 0.0 {
            invdet = 1.0 / det;
        }
        (*d).right.x *= invdet;
        (*d).right.y *= invdet;
        (*d).right.z *= invdet;
        (*d).up.x = invdet * ((*s).up.z * (*s).at.x - (*s).up.x * (*s).at.z);
        (*d).up.y = invdet * ((*s).at.z * (*s).right.x - (*s).at.x * (*s).right.z);
        (*d).up.z = invdet * ((*s).right.z * (*s).up.x - (*s).right.x * (*s).up.z);
        (*d).at.x = invdet * ((*s).up.x * (*s).at.y - (*s).up.y * (*s).at.x);
        (*d).at.y = invdet * ((*s).at.x * (*s).right.y - (*s).at.y * (*s).right.x);
        (*d).at.z = invdet * ((*s).right.x * (*s).up.y - (*s).right.y * (*s).up.x);
        (*d).pos.x = -((*s).pos.x * (*d).right.x + (*s).pos.y * (*d).up.x + (*s).pos.z * (*d).at.x);
        (*d).pos.y = -((*s).pos.x * (*d).right.y + (*s).pos.y * (*d).up.y + (*s).pos.z * (*d).at.y);
        (*d).pos.z = -((*s).pos.x * (*d).right.z + (*s).pos.y * (*d).up.z + (*s).pos.z * (*d).at.z);
        (*d).flags &= !IDENTITY;
    }
}

pub unsafe fn invert(d: *mut RwMatrix, s: *const RwMatrix) {
    unsafe {
        if (*s).flags & IDENTITY != 0 {
            *d = *s;
        } else if (*s).flags & TYPEMASK == TYPEORTHONORMAL {
            invert_orthonormal(d, s);
        } else {
            invert_general(d, s);
        }
    }
}

pub unsafe fn transpose(d: *mut RwMatrix, s: *const RwMatrix) {
    unsafe {
        if (*s).flags & IDENTITY != 0 {
            *d = *s;
        }
        (*d).right.x = (*s).right.x;
        (*d).up.x = (*s).right.y;
        (*d).at.x = (*s).right.z;
        (*d).right.y = (*s).up.x;
        (*d).up.y = (*s).up.y;
        (*d).at.y = (*s).up.z;
        (*d).right.z = (*s).at.x;
        (*d).up.z = (*s).at.y;
        (*d).at.z = (*s).at.z;
        (*d).pos.x = 0.0;
        (*d).pos.y = 0.0;
        (*d).pos.z = 0.0;
    }
}

pub unsafe fn make_rotation_axis(d: *mut RwMatrix, axis: *const V3, angle: f32) {
    unsafe {
        let a = &*axis;
        let mut len = dot(a, a);
        if len != 0.0 {
            len = 1.0 / len.sqrt();
        }
        let v = scale(a, len);
        let angle = angle * std::f32::consts::PI / 180.0;
        let s = angle.sin();
        let c = angle.cos();
        let t = 1.0 - c;
        (*d).right.x = c + v.x * v.x * t;
        (*d).right.y = v.x * v.y * t + v.z * s;
        (*d).right.z = v.z * v.x * t - v.y * s;
        (*d).up.x = v.x * v.y * t - v.z * s;
        (*d).up.y = c + v.y * v.y * t;
        (*d).up.z = v.y * v.z * t + v.x * s;
        (*d).at.x = v.z * v.x * t + v.y * s;
        (*d).at.y = v.y * v.z * t - v.x * s;
        (*d).at.z = c + v.z * v.z * t;
        (*d).pos.x = 0.0;
        (*d).pos.y = 0.0;
        (*d).pos.z = 0.0;
        (*d).flags = TYPEORTHONORMAL;
    }
}

pub unsafe fn make_rotation_quat(d: *mut RwMatrix, q: *const Quat) {
    unsafe {
        let q = &*q;
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let yz = q.y * q.z;
        let zx = q.z * q.x;
        let xy = q.x * q.y;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;
        (*d).right.x = 1.0 - 2.0 * (yy + zz);
        (*d).right.y = 2.0 * (xy + wz);
        (*d).right.z = 2.0 * (zx - wy);
        (*d).up.x = 2.0 * (xy - wz);
        (*d).up.y = 1.0 - 2.0 * (xx + zz);
        (*d).up.z = 2.0 * (yz + wx);
        (*d).at.x = 2.0 * (zx + wy);
        (*d).at.y = 2.0 * (yz - wx);
        (*d).at.z = 1.0 - 2.0 * (xx + yy);
        (*d).pos.x = 0.0;
        (*d).pos.y = 0.0;
        (*d).pos.z = 0.0;
        (*d).flags = TYPEORTHONORMAL;
    }
}

/// The common tail of rotate/translate/scale/transform: combine `m` with `with` by `op`.
unsafe fn combine(m: *mut RwMatrix, with: *const RwMatrix, op: i32) {
    unsafe {
        let mut tmp = IDENT;
        match op {
            COMBINEREPLACE => *m = *with,
            COMBINEPRECONCAT => {
                mult(&mut tmp, with, m);
                *m = tmp;
            }
            COMBINEPOSTCONCAT => {
                mult(&mut tmp, m, with);
                *m = tmp;
            }
            _ => {}
        }
    }
}

pub unsafe fn rotate_axis(m: *mut RwMatrix, axis: *const V3, angle: f32, op: i32) {
    let mut rot = IDENT;
    unsafe {
        make_rotation_axis(&mut rot, axis, angle);
        combine(m, &rot, op);
    }
}

pub unsafe fn rotate_quat(m: *mut RwMatrix, q: *const Quat, op: i32) {
    let mut rot = IDENT;
    unsafe {
        make_rotation_quat(&mut rot, q);
        combine(m, &rot, op);
    }
}

pub unsafe fn translate(m: *mut RwMatrix, t: *const V3, op: i32) {
    let mut trans = IDENT;
    unsafe {
        trans.pos = *t;
        trans.flags &= !IDENTITY;
        combine(m, &trans, op);
    }
}

pub unsafe fn scale_by(m: *mut RwMatrix, s: *const V3, op: i32) {
    let mut scl = IDENT;
    unsafe {
        scl.right.x = (*s).x;
        scl.up.y = (*s).y;
        scl.at.z = (*s).z;
        scl.flags &= !IDENTITY;
        combine(m, &scl, op);
    }
}

pub unsafe fn transform(m: *mut RwMatrix, with: *const RwMatrix, op: i32) {
    unsafe { combine(m, with, op) }
}

pub unsafe fn get_rotation(m: *const RwMatrix) -> Quat {
    let m = unsafe { &*m };
    let mut q = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };
    let tr = m.right.x + m.up.y + m.at.z;
    if tr > 0.0 {
        let s = (1.0 + tr).sqrt() * 2.0;
        q.w = s / 4.0;
        q.x = (m.up.z - m.at.y) / s;
        q.y = (m.at.x - m.right.z) / s;
        q.z = (m.right.y - m.up.x) / s;
    } else if m.right.x > m.up.y && m.right.x > m.at.z {
        let s = (1.0 + m.right.x - m.up.y - m.at.z).sqrt() * 2.0;
        q.w = (m.up.z - m.at.y) / s;
        q.x = s / 4.0;
        q.y = (m.up.x + m.right.y) / s;
        q.z = (m.at.x + m.right.z) / s;
    } else if m.up.y > m.at.z {
        let s = (1.0 + m.up.y - m.right.x - m.at.z).sqrt() * 2.0;
        q.w = (m.at.x - m.right.z) / s;
        q.x = (m.up.x + m.right.y) / s;
        q.y = s / 4.0;
        q.z = (m.at.y + m.up.z) / s;
    } else {
        let s = (1.0 + m.at.z - m.right.x - m.up.y).sqrt() * 2.0;
        q.w = (m.right.y - m.up.x) / s;
        q.x = (m.at.x + m.right.z) / s;
        q.y = (m.at.y + m.up.z) / s;
        q.z = s / 4.0;
    }
    q
}

pub unsafe fn look_at(m: *mut RwMatrix, dir: *const V3, up: *const V3) {
    unsafe {
        (*m).at = normalize(&*dir);
        (*m).right = normalize(&cross(&*up, &(*m).at));
        (*m).up = cross(&(*m).at, &(*m).right);
        (*m).flags = TYPEORTHONORMAL;
    }
}

// ---------------------------------------------------------------- C ABI (base.cpp forwards)

macro_rules! ffi {
    ($($name:ident ( $($a:ident : $t:ty),* ) $(-> $r:ty)? => $body:expr;)*) => {
        $(
            /// # Safety
            /// As the Rust function it forwards to.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name($($a: $t),*) $(-> $r)? { unsafe { $body } }
        )*
    };
}

ffi! {
    revc_rwm_optimize(m: *mut RwMatrix, tol: *const Tolerance) => optimize(m, tol);
    revc_rwm_mult(d: *mut RwMatrix, a: *const RwMatrix, b: *const RwMatrix) => mult(d, a, b);
    revc_rwm_mult_(d: *mut RwMatrix, a: *const RwMatrix, b: *const RwMatrix) => mult_(d, a, b);
    revc_rwm_invert(d: *mut RwMatrix, s: *const RwMatrix) => invert(d, s);
    revc_rwm_invert_orthonormal(d: *mut RwMatrix, s: *const RwMatrix) => invert_orthonormal(d, s);
    revc_rwm_invert_general(d: *mut RwMatrix, s: *const RwMatrix) => invert_general(d, s);
    revc_rwm_transpose(d: *mut RwMatrix, s: *const RwMatrix) => transpose(d, s);
    revc_rwm_rotate_axis(m: *mut RwMatrix, axis: *const V3, angle: f32, op: i32) => rotate_axis(m, axis, angle, op);
    revc_rwm_rotate_quat(m: *mut RwMatrix, q: *const Quat, op: i32) => rotate_quat(m, q, op);
    revc_rwm_translate(m: *mut RwMatrix, t: *const V3, op: i32) => translate(m, t, op);
    revc_rwm_scale(m: *mut RwMatrix, s: *const V3, op: i32) => scale_by(m, s, op);
    revc_rwm_transform(m: *mut RwMatrix, with: *const RwMatrix, op: i32) => transform(m, with, op);
    revc_rwm_get_rotation(m: *const RwMatrix, out: *mut Quat) => { *out = get_rotation(m) };
    revc_rwm_look_at(m: *mut RwMatrix, dir: *const V3, up: *const V3) => look_at(m, dir, up);
    revc_rwm_make_rotation_axis(d: *mut RwMatrix, axis: *const V3, angle: f32) => make_rotation_axis(d, axis, angle);
    revc_rwm_make_rotation_quat(d: *mut RwMatrix, q: *const Quat) => make_rotation_quat(d, q);
    revc_rwm_normal_error(m: *const RwMatrix) -> f32 => normal_error(m);
    revc_rwm_orthogonal_error(m: *const RwMatrix) -> f32 => orthogonal_error(m);
    revc_rwm_identity_error(m: *const RwMatrix) -> f32 => identity_error(m);
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwmatrix.txt (recorded from librw's C++, see the recorder).
    use super::*;

    struct In<'a>(std::str::SplitWhitespace<'a>);
    impl In<'_> {
        fn word(&mut self) -> &str {
            self.0.next().expect("short line")
        }
        fn u(&mut self) -> u32 {
            u32::from_str_radix(self.word(), 16).unwrap()
        }
        fn f(&mut self) -> f32 {
            f32::from_bits(self.u())
        }
        fn v3(&mut self) -> V3 {
            V3 { x: self.f(), y: self.f(), z: self.f() }
        }
        fn q(&mut self) -> Quat {
            Quat { x: self.f(), y: self.f(), z: self.f(), w: self.f() }
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
            RwMatrix { right, flags, up, pad1: 0, at, pad2: 0, pos, pad3: 0 }
        }
    }

    fn f(o: &mut String, v: f32) {
        o.push_str(&format!(" {:08x}", v.to_bits()));
    }
    fn v3(o: &mut String, v: &V3) {
        f(o, v.x);
        f(o, v.y);
        f(o, v.z);
    }
    fn m(o: &mut String, x: &RwMatrix) {
        v3(o, &x.right);
        o.push_str(&format!(" {:08x}", x.flags));
        v3(o, &x.up);
        o.push_str(" --------");
        v3(o, &x.at);
        o.push_str(" --------");
        v3(o, &x.pos);
        o.push_str(" --------");
    }

    #[test]
    fn matches_librw_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/rwmatrix.txt");
        let text = std::fs::read_to_string(&path).expect("tests/golden/rwmatrix.txt");
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let mut i = In(input.split_whitespace());
            let op = i.word().to_string();
            let mut o = String::new();
            unsafe {
                match op.as_str() {
                    "optimize" => {
                        let mut a = i.m();
                        let w = i.word().to_string();
                        if w == "default" {
                            optimize(&mut a, std::ptr::null());
                        } else {
                            let normal = f32::from_bits(u32::from_str_radix(&w, 16).unwrap());
                            let t = Tolerance { normal, orthogonal: i.f(), identity: i.f() };
                            optimize(&mut a, &t);
                        }
                        m(&mut o, &a);
                    }
                    "mult" => {
                        let (a, b) = (i.m(), i.m());
                        let mut d = IDENT;
                        mult(&mut d, &a, &b);
                        m(&mut o, &d);
                    }
                    "mult_d=a" => {
                        let (mut a, b) = (i.m(), i.m());
                        let p: *mut RwMatrix = &mut a;
                        mult(p, p, &b);
                        m(&mut o, &a);
                    }
                    "mult_d=b" => {
                        let (a, mut b) = (i.m(), i.m());
                        let p: *mut RwMatrix = &mut b;
                        mult(p, &a, p);
                        m(&mut o, &b);
                    }
                    "invert" | "invertgeneral" | "invertortho" => {
                        let a = i.m();
                        let mut d = i.m(); // the destination's previous value (invertGeneral keeps its flags)
                        match op.as_str() {
                            "invert" => invert(&mut d, &a),
                            "invertgeneral" => invert_general(&mut d, &a),
                            _ => invert_orthonormal(&mut d, &a),
                        }
                        m(&mut o, &d);
                    }
                    "invert_d=s" => {
                        let mut a = i.m();
                        let p: *mut RwMatrix = &mut a;
                        invert(p, p);
                        m(&mut o, &a);
                    }
                    "transpose" => {
                        let (a, mut d) = (i.m(), i.m());
                        transpose(&mut d, &a);
                        m(&mut o, &d);
                    }
                    "rotateaxis" => {
                        let mut a = i.m();
                        let v = i.v3();
                        let ang = i.f();
                        let op = i.u() as i32;
                        rotate_axis(&mut a, &v, ang, op);
                        m(&mut o, &a);
                    }
                    "rotatequat" => {
                        let mut a = i.m();
                        let q = i.q();
                        let op = i.u() as i32;
                        rotate_quat(&mut a, &q, op);
                        m(&mut o, &a);
                    }
                    "translate" | "scale" => {
                        let mut a = i.m();
                        let v = i.v3();
                        let op2 = i.u() as i32;
                        if op == "translate" { translate(&mut a, &v, op2) } else { scale_by(&mut a, &v, op2) }
                        m(&mut o, &a);
                    }
                    "transform" => {
                        let (mut a, b) = (i.m(), i.m());
                        let op = i.u() as i32;
                        transform(&mut a, &b, op);
                        m(&mut o, &a);
                    }
                    "getrotation" => {
                        let a = i.m();
                        let q = get_rotation(&a);
                        for v in [q.x, q.y, q.z, q.w] {
                            f(&mut o, v);
                        }
                    }
                    "lookat" => {
                        let mut a = i.m();
                        let (d, u) = (i.v3(), i.v3());
                        look_at(&mut a, &d, &u);
                        m(&mut o, &a);
                    }
                    "makerotaxis" => {
                        let mut d = i.m();
                        let v = i.v3();
                        let ang = i.f();
                        make_rotation_axis(&mut d, &v, ang);
                        m(&mut o, &d);
                    }
                    "makerotquat" => {
                        let mut d = i.m();
                        let q = i.q();
                        make_rotation_quat(&mut d, &q);
                        m(&mut o, &d);
                    }
                    "errors" => {
                        let a = i.m();
                        f(&mut o, normal_error(&a));
                        f(&mut o, orthogonal_error(&a));
                        f(&mut o, identity_error(&a));
                    }
                    other => panic!("unknown op {other}"),
                }
            }
            n += 1;
            if o != want {
                bad.push(format!("{input}\n  want{want}\n  got {o}"));
            }
        }
        assert!(n >= 6000, "only {n} cases");
        assert!(bad.is_empty(), "{} of {n} differ, first:\n{}", bad.len(), bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n"));
    }
}

//! The game's math module (plan task 15, `src/math`): CVector, CQuaternion and CMatrix functions
//! that were out of line in C++ (Vector.cpp, Quaternion.cpp, Matrix.cpp, math.cpp, Rect.cpp),
//! called from the C++ through thin forwarders. The inline operators in the headers stay C++ until
//! their callers are ported.
//!
//! Results are bit-identical to the original: every expression keeps the C++ evaluation order
//! (float addition is not associative) and the same libm functions (sinf, cosf, acosf, atan2f).
//! tests/golden/math.txt holds 6000 cases recorded from the C++ compiled like the game
//! (clang++ -O2); the test replays them bit for bit.
//!
//! Layouts are the C++ ones (PC build): CVector = 3 floats, CQuaternion = 4, CMatrix = 4x4 floats
//! (right, forward, up, position rows with a w each) + RwMatrix pointer + owner flag,
//! RwMatrix = librw's rw::Matrix (vectors with a flags/pad word each).

use std::f32::consts::PI;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// librw's rw::Matrix (RwMatrix).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RwMatrix {
    pub right: V3,
    pub flags: u32,
    pub up: V3,
    pub pad1: u32,
    pub at: V3,
    pub pad2: u32,
    pub pos: V3,
    pub pad3: u32,
}

/// rw::Matrix::update (RwMatrixUpdate): clears IDENTITY and the type bits.
const RW_MATRIX_UPDATE_MASK: u32 = !(0x20000 | 3);

/// CMatrix (PC layout).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Matrix {
    /// Rows right (0..4), forward (4..8), up (8..12), position (12..16); index 3 of each is w.
    pub f: [f32; 16],
    pub attachment: *mut RwMatrix,
    pub has_rw_matrix: bool,
}

impl Default for Matrix {
    /// Zeros, not attached (a C++ static CMatrix).
    fn default() -> Matrix {
        Matrix { f: [0.0; 16], attachment: std::ptr::null_mut(), has_rw_matrix: false }
    }
}

// Element indices.
const RX: usize = 0;
const RY: usize = 1;
const RZ: usize = 2;
const FX: usize = 4;
const FY: usize = 5;
const FZ: usize = 6;
const UX: usize = 8;
const UY: usize = 9;
const UZ: usize = 10;
const PX: usize = 12;
const PY: usize = 13;
const PZ: usize = 14;

/// CCompressedMatrixNotAligned: position plus right and forward scaled to int8 (x127).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CompressedMatrix {
    pub pos: V3,
    pub right: [i8; 3],
    pub up: [i8; 3],
}

/// CRect.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

fn recip_sqrt(x: f32) -> f32 {
    1.0 / x.sqrt()
}

impl V3 {
    pub const fn new(x: f32, y: f32, z: f32) -> V3 {
        V3 { x, y, z }
    }
    fn mag_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    /// CVector::Normalise: a zero vector becomes (1, 0, 0) (x only, as the original).
    pub fn normalise(&mut self) {
        let sq = self.mag_sqr();
        if sq > 0.0 {
            let inv = recip_sqrt(sq);
            self.x *= inv;
            self.y *= inv;
            self.z *= inv;
        } else {
            self.x = 1.0;
        }
    }
}

pub fn cross(a: &V3, b: &V3) -> V3 {
    V3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

impl Matrix {
    fn row(&self, i: usize) -> V3 {
        V3::new(self.f[i], self.f[i + 1], self.f[i + 2])
    }
    fn set_row(&mut self, i: usize, v: V3) {
        self.f[i] = v.x;
        self.f[i + 1] = v.y;
        self.f[i + 2] = v.z;
    }

    /// Multiply3x3(mat, vec): rotation only.
    pub fn mul3x3(&self, v: &V3) -> V3 {
        let m = &self.f;
        V3::new(
            m[RX] * v.x + m[FX] * v.y + m[UX] * v.z,
            m[RY] * v.x + m[FY] * v.y + m[UY] * v.z,
            m[RZ] * v.x + m[FZ] * v.y + m[UZ] * v.z,
        )
    }
    /// Multiply3x3(vec, mat): by the transposed rotation.
    pub fn mul3x3_transposed(&self, v: &V3) -> V3 {
        let m = &self.f;
        V3::new(
            m[RX] * v.x + m[RY] * v.y + m[RZ] * v.z,
            m[FX] * v.x + m[FY] * v.y + m[FZ] * v.z,
            m[UX] * v.x + m[UY] * v.y + m[UZ] * v.z,
        )
    }
    /// operator*(CMatrix, CVector): point transform.
    pub fn mul_point(&self, v: &V3) -> V3 {
        let m = &self.f;
        V3::new(
            m[RX] * v.x + m[FX] * v.y + m[UX] * v.z + m[PX],
            m[RY] * v.x + m[FY] * v.y + m[UY] * v.z + m[PY],
            m[RZ] * v.x + m[FZ] * v.y + m[UZ] * v.z + m[PZ],
        )
    }

    fn set_orientation(&mut self, r: [f32; 3], f: [f32; 3], u: [f32; 3]) {
        self.set_row(RX, V3::new(r[0], r[1], r[2]));
        self.set_row(FX, V3::new(f[0], f[1], f[2]));
        self.set_row(UX, V3::new(u[0], u[1], u[2]));
    }
    fn zero_position(&mut self) {
        self.set_row(PX, V3::default());
    }

    pub fn set_unity(&mut self) {
        self.reset_orientation();
        self.zero_position();
    }
    pub fn reset_orientation(&mut self) {
        self.set_orientation([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    }
    pub fn set_scale(&mut self, s: f32) {
        self.set_orientation([s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, s]);
        self.zero_position();
    }
    pub fn set_translate(&mut self, x: f32, y: f32, z: f32) {
        self.reset_orientation();
        self.set_row(PX, V3::new(x, y, z));
    }
    pub fn set_rotate_x_only(&mut self, angle: f32) {
        let (c, s) = (angle.cos(), angle.sin());
        self.set_orientation([1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]);
    }
    pub fn set_rotate_y_only(&mut self, angle: f32) {
        let (c, s) = (angle.cos(), angle.sin());
        self.set_orientation([c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]);
    }
    pub fn set_rotate_z_only(&mut self, angle: f32) {
        let (c, s) = (angle.cos(), angle.sin());
        self.set_orientation([c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]);
    }
    pub fn set_rotate(&mut self, x_angle: f32, y_angle: f32, z_angle: f32) {
        let (cx, sx) = (x_angle.cos(), x_angle.sin());
        let (cy, sy) = (y_angle.cos(), y_angle.sin());
        let (cz, sz) = (z_angle.cos(), z_angle.sin());
        self.set_orientation(
            [cz * cy - (sz * sx) * sy, (cz * sx) * sy + sz * cy, -cx * sy],
            [-sz * cx, cz * cx, sx],
            [(sz * sx) * cy + cz * sy, sz * sy - (cz * sx) * cy, cx * cy],
        );
        self.zero_position();
    }

    /// RotateX: rotates every row (position included) about the x axis.
    pub fn rotate_x(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        for i in [RX, FX, UX, PX] {
            let (y, z) = (self.f[i + 1], self.f[i + 2]);
            self.f[i + 1] = c * y - s * z;
            self.f[i + 2] = c * z + s * y;
        }
    }
    pub fn rotate_y(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        for i in [RX, FX, UX, PX] {
            let (x, z) = (self.f[i], self.f[i + 2]);
            self.f[i] = c * x + s * z;
            self.f[i + 2] = c * z - s * x;
        }
    }
    pub fn rotate_z(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        for i in [RX, FX, UX, PX] {
            let (x, y) = (self.f[i], self.f[i + 1]);
            self.f[i] = c * x - s * y;
            self.f[i + 1] = c * y + s * x;
        }
    }
    pub fn rotate(&mut self, x: f32, y: f32, z: f32) {
        let (cx, sx) = (x.cos(), x.sin());
        let (cy, sy) = (y.cos(), y.sin());
        let (cz, sz) = (z.cos(), z.sin());
        let x1 = cz * cy - (sz * sx) * sy;
        let x2 = (cz * sx) * sy + sz * cy;
        let x3 = -cx * sy;
        let y1 = -sz * cx;
        let y2 = cz * cx;
        let y3 = sx;
        let z1 = (sz * sx) * cy + cz * sy;
        let z2 = sz * sy - (cz * sx) * cy;
        let z3 = cx * cy;
        for i in [RX, FX, UX, PX] {
            let (a, b, c) = (self.f[i], self.f[i + 1], self.f[i + 2]);
            self.f[i] = x1 * a + y1 * b + z1 * c;
            self.f[i + 1] = x2 * a + y2 * b + z2 * c;
            self.f[i + 2] = x3 * a + y3 * b + z3 * c;
        }
    }

    pub fn reorthogonalise(&mut self) {
        let (r, f) = (self.row(RX), self.row(FX));
        let mut u = cross(&r, &f);
        u.normalise();
        self.set_row(UX, u);
        let mut r = cross(&f, &u);
        r.normalise();
        self.set_row(RX, r);
        self.set_row(FX, cross(&u, &r));
    }

    /// operator*(CMatrix, CMatrix) into `out`; out's w column is left as it was (the original
    /// leaves it uninitialised).
    pub fn mul_into(m1: &Matrix, m2: &Matrix, out: &mut Matrix) {
        let (a, b) = (&m1.f, &m2.f);
        let mut o = out.f;
        for col in [RX, FX, UX] {
            o[col] = a[RX] * b[col] + a[FX] * b[col + 1] + a[UX] * b[col + 2];
            o[col + 1] = a[RY] * b[col] + a[FY] * b[col + 1] + a[UY] * b[col + 2];
            o[col + 2] = a[RZ] * b[col] + a[FZ] * b[col + 1] + a[UZ] * b[col + 2];
        }
        o[PX] = a[RX] * b[PX] + a[FX] * b[PY] + a[UX] * b[PZ] + a[PX];
        o[PY] = a[RY] * b[PX] + a[FY] * b[PY] + a[UY] * b[PZ] + a[PY];
        o[PZ] = a[RZ] * b[PX] + a[FZ] * b[PY] + a[UZ] * b[PZ] + a[PZ];
        out.f = o;
    }

    /// Invert(src, dst) for orthonormal matrices: transposed rotation, position -(R^T p), with the
    /// original's accumulation order. dst's w column is untouched.
    pub fn invert_into(src: &Matrix, dst: &mut Matrix) {
        let s = &src.f;
        let d = &mut dst.f;
        d[PX] = 0.0;
        d[PY] = 0.0;
        d[PZ] = 0.0;
        for r in 0..3 {
            for c in 0..3 {
                d[r * 4 + c] = s[c * 4 + r];
            }
        }
        for k in 0..3 {
            let p = s[PX + k];
            d[PX] += d[k * 4] * p;
            d[PY] += d[k * 4 + 1] * p;
            d[PZ] += d[k * 4 + 2] * p;
        }
        d[PX] = -d[PX];
        d[PY] = -d[PY];
        d[PZ] = -d[PZ];
    }

    /// operator+=: the four rows (w untouched).
    pub fn add_assign(&mut self, rhs: &Matrix) {
        for i in [RX, FX, UX, PX] {
            for k in 0..3 {
                self.f[i + k] += rhs.f[i + k];
            }
        }
    }

    /// CMatrix::Update: rows from the attached RwMatrix.
    ///
    /// # Safety
    /// `attachment` must point to a valid RwMatrix.
    pub unsafe fn update_from_rw(&mut self) {
        let a = unsafe { &*self.attachment };
        self.set_row(RX, a.right);
        self.set_row(FX, a.up);
        self.set_row(UX, a.at);
        self.set_row(PX, a.pos);
    }

    /// CopyToRwMatrix (also UpdateRW's body): rows into `rw`, then RwMatrixUpdate.
    pub fn copy_to_rw(&self, rw: &mut RwMatrix) {
        rw.right = self.row(RX);
        rw.up = self.row(FX);
        rw.at = self.row(UX);
        rw.pos = self.row(PX);
        rw.flags &= RW_MATRIX_UPDATE_MASK;
    }

    /// CMatrix::UpdateRW: writes to the attached RwMatrix, if any.
    ///
    /// # Safety
    /// `attachment` must be null or point to a valid RwMatrix.
    pub unsafe fn update_rw(&self) {
        if let Some(a) = unsafe { self.attachment.as_mut() } {
            self.copy_to_rw(a);
        }
    }
}

impl Quat {
    fn mag_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }
    /// CQuaternion::Normalise: a zero quaternion gets w = 1.
    pub fn normalise(&mut self) {
        let sq = self.mag_sqr();
        if sq == 0.0 {
            self.w = 1.0;
        } else {
            let inv = recip_sqrt(sq);
            self.x *= inv;
            self.y *= inv;
            self.z *= inv;
            self.w *= inv;
        }
    }
    fn scaled(&self, s: f32) -> Quat {
        Quat { x: s * self.x, y: s * self.y, z: s * self.z, w: s * self.w }
    }
    /// CQuaternion::Slerp (theta and 1/sin(theta) precomputed by the caller).
    pub fn slerp(q1: &Quat, q2: &Quat, mut theta: f32, inv_sin: f32, t: f32) -> Quat {
        if theta == 0.0 {
            return *q2;
        }
        let (w1, w2);
        if theta > PI / 2.0 {
            theta = PI - theta;
            w1 = ((1.0 - t) * theta).sin() * inv_sin;
            w2 = -(t * theta).sin() * inv_sin;
        } else {
            w1 = ((1.0 - t) * theta).sin() * inv_sin;
            w2 = (t * theta).sin() * inv_sin;
        }
        let (a, b) = (q1.scaled(w1), q2.scaled(w2));
        Quat { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z, w: a.w + b.w }
    }
    pub fn multiply(q1: &Quat, q2: &Quat) -> Quat {
        Quat {
            x: (q2.z * q1.y) - (q1.z * q2.y) + (q1.x * q2.w) + (q2.x * q1.w),
            y: (q2.x * q1.z) - (q1.x * q2.z) + (q1.y * q2.w) + (q2.y * q1.w),
            z: (q2.y * q1.x) - (q1.y * q2.x) + (q1.z * q2.w) + (q2.z * q1.w),
            w: (q2.w * q1.w) - (q2.x * q1.x) - (q2.y * q1.y) - (q2.z * q1.z),
        }
    }
    /// Get(axis, angle).
    pub fn axis_angle(&self) -> (V3, f32) {
        let angle = self.w.acos();
        let s = angle.sin();
        (V3::new(self.x * (1.0 / s), self.y * (1.0 / s), self.z * (1.0 / s)), angle)
    }
    pub fn from_axis_angle(axis: &V3, angle: f32) -> Quat {
        let half_cos = (angle * 0.5).cos();
        let half_sin = (angle * 0.5).sin();
        Quat { x: axis.x * half_sin, y: axis.y * half_sin, z: axis.z * half_sin, w: half_cos }
    }
    /// Get(RwMatrix*): writes right, up and at only.
    pub fn to_rw_matrix(&self, m: &mut RwMatrix) {
        let (x2, y2, z2) = (self.x + self.x, self.y + self.y, self.z + self.z);
        let (x_2x, x_2y, x_2z) = (self.x * x2, self.x * y2, self.x * z2);
        let (y_2y, y_2z, z_2z) = (self.y * y2, self.y * z2, self.z * z2);
        let (w_2x, w_2y, w_2z) = (self.w * x2, self.w * y2, self.w * z2);
        m.right.x = 1.0 - (y_2y + z_2z);
        m.up.x = x_2y - w_2z;
        m.at.x = x_2z + w_2y;
        m.right.y = x_2y + w_2z;
        m.up.y = 1.0 - (x_2x + z_2z);
        m.at.y = y_2z - w_2x;
        m.right.z = x_2z - w_2y;
        m.up.z = y_2z + w_2x;
        m.at.z = 1.0 - (x_2x + y_2y);
    }
    /// Set(const RwMatrix&).
    pub fn from_rw_matrix(m: &RwMatrix) -> Quat {
        let f = m.up.y + m.right.x + m.at.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Quat { w: 0.5 * s, x: (m.up.z - m.at.y) * k, y: (m.at.x - m.right.z) * k, z: (m.right.y - m.up.x) * k };
        }
        let f = m.right.x - m.up.y - m.at.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Quat { x: 0.5 * s, y: (m.up.x + m.right.y) * k, z: (m.at.x + m.right.z) * k, w: (m.up.z - m.at.y) * k };
        }
        let f = m.up.y - m.right.x - m.at.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Quat { y: 0.5 * s, w: (m.at.x - m.right.z) * k, x: (m.up.x - m.right.y) * k, z: (m.at.y + m.up.z) * k };
        }
        let f = m.at.z - (m.up.y + m.right.x);
        let s = (f + 1.0).sqrt();
        let k = 0.5 / s;
        Quat { z: 0.5 * s, w: (m.right.y - m.up.x) * k, x: (m.at.x + m.right.z) * k, y: (m.at.y + m.up.z) * k }
    }
    /// Get(f1, f2, f3): Euler angles in [0, 2pi).
    pub fn euler(&self) -> (f32, f32, f32) {
        const TWOPI: f32 = PI * 2.0;
        let mut m = RwMatrix::default();
        self.to_rw_matrix(&mut m);
        let mut f3 = m.right.y.atan2(m.up.y);
        if f3 < 0.0 {
            f3 += TWOPI;
        }
        let (s, c) = (f3.sin(), f3.cos());
        let mut f1 = (-m.at.y).atan2(s * m.right.y + c * m.up.y);
        if f1 < 0.0 {
            f1 += TWOPI;
        }
        let mut f2 = (-(m.right.z * c - m.up.z * s)).atan2(m.right.x * c - m.up.x * s);
        if f2 < 0.0 {
            f2 += TWOPI;
        }
        (f1, f2, f3)
    }
    /// Set(f1, f2, f3).
    pub fn from_euler(f1: f32, f2: f32, f3: f32) -> Quat {
        let (c1, c2, c3) = ((f1 * 0.5).cos(), (f2 * 0.5).cos(), (f3 * 0.5).cos());
        let (s1, s2, s3) = ((f1 * 0.5).sin(), (f2 * 0.5).sin(), (f3 * 0.5).sin());
        Quat {
            x: ((c2 * c1) * s3) - ((s2 * s1) * c3),
            y: ((s1 * c2) * c3) + ((s2 * c1) * s3),
            z: ((s2 * c1) * c3) - ((s1 * c2) * s3),
            w: ((c2 * c1) * c3) + ((s2 * s1) * s3),
        }
    }
}

impl CompressedMatrix {
    pub fn compress(m: &Matrix) -> CompressedMatrix {
        // float -> int8 as the C++ compiles it (truncating through int32); only defined for the
        // orthonormal matrices the game compresses.
        let q = |v: f32| (127.0 * v) as i32 as i8;
        CompressedMatrix {
            pos: m.row(PX),
            right: [q(m.f[RX]), q(m.f[RY]), q(m.f[RZ])],
            up: [q(m.f[FX]), q(m.f[FY]), q(m.f[FZ])],
        }
    }
    pub fn decompress(&self, m: &mut Matrix) {
        let d = |v: i8| v as f32 / 127.0;
        m.set_row(RX, V3::new(d(self.right[0]), d(self.right[1]), d(self.right[2])));
        m.set_row(FX, V3::new(d(self.up[0]), d(self.up[1]), d(self.up[2])));
        m.set_row(UX, cross(&m.row(RX), &m.row(FX)));
        m.set_row(PX, self.pos);
        m.reorthogonalise();
    }
}

// ---- C ABI for the C++ forwarders (src/math/*.cpp) ----
// Pointers come from C++ references: valid, aligned, and (where both are passed) possibly the same
// object, so inputs are copied before outputs are written.

macro_rules! ffi {
    ($($(#[$m:meta])* fn $name:ident($($arg:ident: $ty:ty),*) $body:block)*) => {$(
        $(#[$m])*
        /// # Safety
        /// Pointer arguments are valid C++ references (see the module's C ABI notes).
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg: $ty),*) { unsafe { $body } }
    )*};
}

ffi! {
    fn revc_vector_normalise(v: *mut V3) { (*v).normalise() }
    fn revc_vector_cross(out: *mut V3, a: *const V3, b: *const V3) { *out = cross(&*a, &*b) }
    fn revc_matrix_mul3x3(out: *mut V3, m: *const Matrix, v: *const V3) { *out = (*m).mul3x3(&*v) }
    fn revc_matrix_mul3x3_transposed(out: *mut V3, v: *const V3, m: *const Matrix) { *out = (*m).mul3x3_transposed(&*v) }
    fn revc_matrix_mul_point(out: *mut V3, m: *const Matrix, v: *const V3) { *out = (*m).mul_point(&*v) }
    /// TransformPoints: `n` points `stride` bytes apart into 16-byte CVuVectors (x, y, z written).
    fn revc_transform_points(out: *mut [f32; 4], n: i32, m: *const Matrix, input: *const u8, stride: i32) {
        let m = *m;
        for i in 0..n.max(0) as usize {
            let v = std::ptr::read_unaligned(input.add(i * stride as usize) as *const V3);
            let p = m.mul_point(&v);
            let o = &mut *out.add(i);
            o[0] = p.x;
            o[1] = p.y;
            o[2] = p.z;
        }
    }

    fn revc_quat_normalise(q: *mut Quat) { (*q).normalise() }
    fn revc_quat_slerp(out: *mut Quat, q1: *const Quat, q2: *const Quat, theta: f32, inv_sin: f32, t: f32) { *out = Quat::slerp(&*q1, &*q2, theta, inv_sin, t) }
    fn revc_quat_multiply(out: *mut Quat, q1: *const Quat, q2: *const Quat) { *out = Quat::multiply(&*q1, &*q2) }
    fn revc_quat_get_axis_angle(q: *const Quat, axis: *mut V3, angle: *mut f32) { let (a, an) = (*q).axis_angle(); *axis = a; *angle = an; }
    fn revc_quat_set_axis_angle(q: *mut Quat, axis: *const V3, angle: f32) { *q = Quat::from_axis_angle(&*axis, angle) }
    fn revc_quat_get_rw_matrix(q: *const Quat, m: *mut RwMatrix) { let q = *q; q.to_rw_matrix(&mut *m) }
    fn revc_quat_set_rw_matrix(q: *mut Quat, m: *const RwMatrix) { *q = Quat::from_rw_matrix(&*m) }
    fn revc_quat_get_euler(q: *const Quat, f1: *mut f32, f2: *mut f32, f3: *mut f32) { let (a, b, c) = (*q).euler(); *f1 = a; *f2 = b; *f3 = c; }
    fn revc_quat_set_euler(q: *mut Quat, f1: f32, f2: f32, f3: f32) { *q = Quat::from_euler(f1, f2, f3) }

    fn revc_matrix_set_unity(m: *mut Matrix) { (*m).set_unity() }
    fn revc_matrix_reset_orientation(m: *mut Matrix) { (*m).reset_orientation() }
    fn revc_matrix_set_scale(m: *mut Matrix, s: f32) { (*m).set_scale(s) }
    fn revc_matrix_set_translate(m: *mut Matrix, x: f32, y: f32, z: f32) { (*m).set_translate(x, y, z) }
    fn revc_matrix_set_rotate_x_only(m: *mut Matrix, a: f32) { (*m).set_rotate_x_only(a) }
    fn revc_matrix_set_rotate_y_only(m: *mut Matrix, a: f32) { (*m).set_rotate_y_only(a) }
    fn revc_matrix_set_rotate_z_only(m: *mut Matrix, a: f32) { (*m).set_rotate_z_only(a) }
    fn revc_matrix_set_rotate_x(m: *mut Matrix, a: f32) { (*m).set_rotate_x_only(a); (*m).zero_position() }
    fn revc_matrix_set_rotate_y(m: *mut Matrix, a: f32) { (*m).set_rotate_y_only(a); (*m).zero_position() }
    fn revc_matrix_set_rotate_z(m: *mut Matrix, a: f32) { (*m).set_rotate_z_only(a); (*m).zero_position() }
    fn revc_matrix_set_rotate(m: *mut Matrix, x: f32, y: f32, z: f32) { (*m).set_rotate(x, y, z) }
    fn revc_matrix_rotate_x(m: *mut Matrix, a: f32) { (*m).rotate_x(a) }
    fn revc_matrix_rotate_y(m: *mut Matrix, a: f32) { (*m).rotate_y(a) }
    fn revc_matrix_rotate_z(m: *mut Matrix, a: f32) { (*m).rotate_z(a) }
    fn revc_matrix_rotate(m: *mut Matrix, x: f32, y: f32, z: f32) { (*m).rotate(x, y, z) }
    fn revc_matrix_reorthogonalise(m: *mut Matrix) { (*m).reorthogonalise() }
    /// operator*: `out` may be m1 or m2 (operator*= passes itself).
    fn revc_matrix_mul(out: *mut Matrix, m1: *const Matrix, m2: *const Matrix) { let (a, b) = (*m1, *m2); Matrix::mul_into(&a, &b, &mut *out) }
    fn revc_matrix_invert(src: *const Matrix, dst: *mut Matrix) { let s = *src; Matrix::invert_into(&s, &mut *dst) }
    fn revc_matrix_add_assign(m: *mut Matrix, rhs: *const Matrix) { let r = *rhs; (*m).add_assign(&r) }
    /// operator=: the 16 floats, then UpdateRW if attached.
    fn revc_matrix_assign(m: *mut Matrix, rhs: *const Matrix) { (*m).f = (*rhs).f; (*m).update_rw() }
    fn revc_matrix_update(m: *mut Matrix) { (*m).update_from_rw() }
    fn revc_matrix_update_rw(m: *mut Matrix) { (*m).update_rw() }
    fn revc_matrix_copy_to_rw(m: *const Matrix, rw: *mut RwMatrix) { (*m).copy_to_rw(&mut *rw) }
    fn revc_matrix_compress(c: *mut CompressedMatrix, m: *const Matrix) { *c = CompressedMatrix::compress(&*m) }
    fn revc_matrix_decompress(c: *const CompressedMatrix, m: *mut Matrix) { let c = *c; c.decompress(&mut *m) }

    /// CRect(): an "empty" rect that any point extends.
    fn revc_rect_init(r: *mut Rect) { *r = Rect { left: 1000000.0, top: 1000000.0, right: -1000000.0, bottom: -1000000.0 } }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/math.txt (recorded from the original C++).
    use super::*;

    fn f(s: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(s, 16).unwrap())
    }
    fn bits(v: f32) -> String {
        format!("{:08x}", v.to_bits())
    }
    struct Rd<'a>(std::str::SplitWhitespace<'a>);
    impl Rd<'_> {
        fn f(&mut self) -> f32 {
            f(self.0.next().unwrap())
        }
        fn v(&mut self) -> V3 {
            V3::new(self.f(), self.f(), self.f())
        }
        fn q(&mut self) -> Quat {
            Quat { x: self.f(), y: self.f(), z: self.f(), w: self.f() }
        }
        fn m(&mut self) -> Matrix {
            let mut m = Matrix { f: [0.0; 16], attachment: std::ptr::null_mut(), has_rw_matrix: false };
            for i in 0..16 {
                m.f[i] = self.f();
            }
            m
        }
        fn rw(&mut self) -> RwMatrix {
            let flags = u32::from_str_radix(self.0.next().unwrap(), 16).unwrap();
            RwMatrix { flags, right: self.v(), up: self.v(), at: self.v(), pos: self.v(), ..Default::default() }
        }
        fn bytes(&mut self, n: usize) -> Vec<u8> {
            (0..n).map(|_| u8::from_str_radix(self.0.next().unwrap(), 16).unwrap()).collect()
        }
    }
    fn v(o: &mut Vec<String>, v: V3) {
        o.extend([v.x, v.y, v.z].map(bits));
    }
    fn q(o: &mut Vec<String>, q: Quat) {
        o.extend([q.x, q.y, q.z, q.w].map(bits));
    }
    fn m(o: &mut Vec<String>, m: &Matrix) {
        o.extend(m.f.map(bits));
    }
    fn rw(o: &mut Vec<String>, r: &RwMatrix) {
        o.push(format!("{:08x}", r.flags));
        for x in [r.right, r.up, r.at, r.pos] {
            v(o, x);
        }
    }

    #[test]
    fn matches_cpp_golden() {
        assert_eq!(std::mem::size_of::<Matrix>(), 80);
        assert_eq!(std::mem::size_of::<RwMatrix>(), 64);
        assert_eq!(std::mem::size_of::<CompressedMatrix>(), 20);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/math.txt");
        let data = std::fs::read_to_string(&path).expect("tests/golden/math.txt");
        let (mut cases, mut bad) = (0, 0);
        for line in data.lines() {
            let (input, want) = line.split_once(" => ").unwrap();
            let (op, args) = input.split_once(' ').unwrap();
            let mut r = Rd(args.split_whitespace());
            let mut o = Vec::new();
            match op {
                "vnorm" => {
                    let mut x = r.v();
                    x.normalise();
                    v(&mut o, x)
                }
                "cross" => {
                    let (a, b) = (r.v(), r.v());
                    v(&mut o, cross(&a, &b))
                }
                "m3x3" => {
                    let (mm, x) = (r.m(), r.v());
                    v(&mut o, mm.mul3x3(&x))
                }
                "v3x3" => {
                    let (mm, x) = (r.m(), r.v());
                    v(&mut o, mm.mul3x3_transposed(&x))
                }
                "mulv" | "tpoint" => {
                    let (mm, x) = (r.m(), r.v());
                    v(&mut o, mm.mul_point(&x))
                }
                "qnorm" => {
                    let mut x = r.q();
                    x.normalise();
                    q(&mut o, x)
                }
                "slerp" => {
                    let (a, b) = (r.q(), r.q());
                    let (th, inv, t) = (r.f(), r.f(), r.f());
                    q(&mut o, Quat::slerp(&a, &b, th, inv, t))
                }
                "qmul" => {
                    let (a, b) = (r.q(), r.q());
                    q(&mut o, Quat::multiply(&a, &b))
                }
                "qgetaxis" => {
                    let (ax, an) = r.q().axis_angle();
                    v(&mut o, ax);
                    o.push(bits(an))
                }
                "qsetaxis" => {
                    let (ax, an) = (r.v(), r.f());
                    q(&mut o, Quat::from_axis_angle(&ax, an))
                }
                "qgetmat" => {
                    let x = r.q();
                    let mut mm = r.rw();
                    x.to_rw_matrix(&mut mm);
                    rw(&mut o, &mm)
                }
                "qsetmat" => q(&mut o, Quat::from_rw_matrix(&r.rw())),
                "qgeteuler" => {
                    let (a, b, c) = r.q().euler();
                    o.extend([a, b, c].map(bits))
                }
                "qseteuler" => {
                    let (a, b, c) = (r.f(), r.f(), r.f());
                    q(&mut o, Quat::from_euler(a, b, c))
                }
                "mmul" | "mmuleq" => {
                    let (a, b) = (r.m(), r.m());
                    let mut out = a;
                    Matrix::mul_into(&a, &b, &mut out);
                    m(&mut o, &out);
                    for i in [3, 7, 11, 15] {
                        o[i] = "--------".into(); // uninitialised in the original
                    }
                }
                "maddeq" => {
                    let (mut a, b) = (r.m(), r.m());
                    a.add_assign(&b);
                    m(&mut o, &a)
                }
                "invert" => {
                    let (s, mut d) = (r.m(), r.m());
                    Matrix::invert_into(&s, &mut d);
                    m(&mut o, &d)
                }
                "compress" => {
                    let c = CompressedMatrix::compress(&r.m());
                    let mut raw = [0u8; 20]; // padding zero, as the recorder's memset
                    for (i, x) in [c.pos.x, c.pos.y, c.pos.z].iter().enumerate() {
                        raw[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
                    }
                    for (i, b) in c.right.iter().chain(c.up.iter()).enumerate() {
                        raw[12 + i] = *b as u8;
                    }
                    o.extend(raw.iter().map(|b| format!("{b:02x}")))
                }
                "decompress" => {
                    let raw: [u8; 20] = r.bytes(20).try_into().unwrap();
                    let c: CompressedMatrix = unsafe { std::mem::transmute(raw) };
                    let mut mm = r.m();
                    c.decompress(&mut mm);
                    m(&mut o, &mm)
                }
                "update" => {
                    let mut mm = r.m();
                    let mut a = r.rw();
                    mm.attachment = &mut a;
                    unsafe { mm.update_from_rw() };
                    m(&mut o, &mm)
                }
                "updaterw" | "copytorw" => {
                    let mm = r.m();
                    let mut a = r.rw();
                    mm.copy_to_rw(&mut a);
                    rw(&mut o, &a)
                }
                _ => {
                    // Single-matrix operations with three float arguments.
                    let mut mm = r.m();
                    let (a, b, c) = (r.f(), r.f(), r.f());
                    match op {
                        "setunity" => mm.set_unity(),
                        "resetorient" => mm.reset_orientation(),
                        "setscale" => mm.set_scale(a),
                        "settranslate" => mm.set_translate(a, b, c),
                        "setrotxonly" => mm.set_rotate_x_only(a),
                        "setrotyonly" => mm.set_rotate_y_only(a),
                        "setrotzonly" => mm.set_rotate_z_only(a),
                        "setrotx" => unsafe { revc_matrix_set_rotate_x(&mut mm, a) },
                        "setroty" => unsafe { revc_matrix_set_rotate_y(&mut mm, a) },
                        "setrotz" => unsafe { revc_matrix_set_rotate_z(&mut mm, a) },
                        "setrotate" => mm.set_rotate(a, b, c),
                        "rotx" => mm.rotate_x(a),
                        "roty" => mm.rotate_y(a),
                        "rotz" => mm.rotate_z(a),
                        "rotate" => mm.rotate(a, b, c),
                        "reortho" => mm.reorthogonalise(),
                        _ => panic!("unknown op {op}"),
                    }
                    m(&mut o, &mm)
                }
            }
            cases += 1;
            let got = o.join(" ");
            if got != want.trim() {
                bad += 1;
                if bad <= 10 {
                    eprintln!("{op}: {args}\n  C++  {want}\n  Rust {got}");
                }
            }
        }
        assert_eq!(cases, 6000);
        assert_eq!(bad, 0, "{bad} of {cases} cases differ from the C++");
    }
}

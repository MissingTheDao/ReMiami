//! librw's HAnim bone matrices and keyframe interpolation, stage 2 step 4b:
//! hanim.cpp's updateMatrices (the matrix stack over the node PUSH/POP flags) and the HAnim
//! keyframe callbacks (apply, interpolate), anim.cpp's AnimInterpolator (setCurrentAnim, addTime)
//! and base.cpp's Quat mult/lerp/slerp. The C++ objects keep their layouts (repr(C) here) and
//! their functions forward; hanimOpen registers the Rust callbacks.
//!
//! Bit-identical to librw compiled like the game (same expression order, same libm calls: acosf,
//! sinf). Checked against tests/golden/rwhanim.txt, recorded from the C++
//! (tests/golden/recorders/rwhanim).

use std::ffi::c_void;

use crate::math::{Quat, RwMatrix, V3};
use crate::rwframe::{self, Frame};
use crate::rwmatrix;

pub const POP: i32 = 1;
pub const PUSH: i32 = 2;
pub const LOCALSPACEMATRICES: i32 = 0x4000;

pub type ApplyCb = unsafe extern "C" fn(result: *mut c_void, frame: *mut c_void);
pub type BlendCb = unsafe extern "C" fn(out: *mut c_void, in1: *mut c_void, in2: *mut c_void, a: f32);
pub type InterpCb = unsafe extern "C" fn(out: *mut c_void, in1: *mut c_void, in2: *mut c_void, t: f32, custom: *mut c_void);
pub type AddCb = unsafe extern "C" fn(out: *mut c_void, in1: *mut c_void, in2: *mut c_void);

/// rw::KeyFrameHeader
#[repr(C)]
pub struct KeyFrameHeader {
    pub prev: *mut KeyFrameHeader,
    pub time: f32,
}

/// rw::InterpFrameHeader
#[repr(C)]
pub struct InterpFrameHeader {
    pub key_frame1: *mut KeyFrameHeader,
    pub key_frame2: *mut KeyFrameHeader,
}

/// rw::AnimInterpolatorInfo
#[repr(C)]
pub struct AnimInterpolatorInfo {
    pub id: i32,
    pub interp_key_frame_size: i32,
    pub anim_key_frame_size: i32,
    pub custom_data_size: i32,
    pub apply_cb: Option<ApplyCb>,
    pub blend_cb: Option<BlendCb>,
    pub interp_cb: Option<InterpCb>,
    pub add_cb: Option<AddCb>,
    pub mul_recip_cb: *mut c_void,
    pub stream_read: *mut c_void,
    pub stream_write: *mut c_void,
    pub stream_get_size: *mut c_void,
}

/// rw::Animation
#[repr(C)]
pub struct Animation {
    pub interp_info: *mut AnimInterpolatorInfo,
    pub num_frames: i32,
    pub flags: i32,
    pub duration: f32,
    pub keyframes: *mut c_void,
    pub custom_data: *mut c_void,
}

/// rw::AnimInterpolator; the interpolated frames follow it in memory.
#[repr(C)]
pub struct AnimInterpolator {
    pub current_anim: *mut Animation,
    pub current_time: f32,
    pub next_frame: *mut c_void,
    pub max_interp_key_frame_size: i32,
    pub current_interp_key_frame_size: i32,
    pub current_anim_key_frame_size: i32,
    pub num_nodes: i32,
    pub apply_cb: Option<ApplyCb>,
    pub blend_cb: Option<BlendCb>,
    pub interp_cb: Option<InterpCb>,
    pub add_cb: Option<AddCb>,
}

/// rw::HAnimKeyFrame
#[repr(C)]
pub struct HAnimKeyFrame {
    pub prev: *mut HAnimKeyFrame,
    pub time: f32,
    pub q: Quat,
    pub t: V3,
}

/// rw::HAnimInterpFrame
#[repr(C)]
pub struct HAnimInterpFrame {
    pub key_frame1: *mut HAnimKeyFrame,
    pub key_frame2: *mut HAnimKeyFrame,
    pub q: Quat,
    pub t: V3,
}

/// rw::HAnimNodeInfo
#[repr(C)]
pub struct HAnimNodeInfo {
    pub id: i32,
    pub index: i32,
    pub flags: i32,
    pub frame: *mut Frame,
}

/// rw::HAnimHierarchy
#[repr(C)]
pub struct HAnimHierarchy {
    pub flags: i32,
    pub num_nodes: i32,
    pub matrices: *mut RwMatrix,
    pub matrices_unaligned: *mut c_void,
    pub node_info: *mut HAnimNodeInfo,
    pub parent_frame: *mut Frame,
    pub parent_hierarchy: *mut HAnimHierarchy,
    pub interpolator: *mut AnimInterpolator,
}

// ---------------------------------------------------------------- Quat (base.cpp)

fn qdot(q: &Quat, p: &Quat) -> f32 {
    q.w * p.w + q.x * p.x + q.y * p.y + q.z * p.z
}
fn qneg(q: &Quat) -> Quat {
    Quat { x: -q.x, y: -q.y, z: -q.z, w: -q.w }
}
fn qscale(q: &Quat, r: f32) -> Quat {
    Quat { x: q.x * r, y: q.y * r, z: q.z * r, w: q.w * r }
}

/// mult(Quat, Quat)
pub fn qmult(q: &Quat, p: &Quat) -> Quat {
    Quat {
        w: q.w * p.w - q.x * p.x - q.y * p.y - q.z * p.z,
        x: q.w * p.x + q.x * p.w + q.y * p.z - q.z * p.y,
        y: q.w * p.y + q.y * p.w + q.z * p.x - q.x * p.z,
        z: q.w * p.z + q.z * p.w + q.x * p.y - q.y * p.x,
    }
}

/// lerp(Quat, Quat, r): the short way round, not normalized.
pub fn qlerp(q: &Quat, p: &Quat, r: f32) -> Quat {
    let mut q1 = *q;
    if qdot(&q1, p) < 0.0 {
        q1 = qneg(&q1);
    }
    Quat {
        w: q1.w + r * (p.w - q1.w),
        x: q1.x + r * (p.x - q1.x),
        y: q1.y + r * (p.y - q1.y),
        z: q1.z + r * (p.z - q1.z),
    }
}

/// slerp(Quat, Quat, a): the short way round; q (flipped) when the angle is tiny.
pub fn qslerp(q: &Quat, p: &Quat, a: f32) -> Quat {
    let mut q1 = *q;
    let mut c = qdot(&q1, p);
    if c < 0.0 {
        c = -c;
        q1 = qneg(&q1);
    }
    let phi = c.acos();
    if phi > 0.00001 {
        let s = phi.sin();
        let a1 = qscale(&q1, ((1.0 - a) * phi).sin() / s);
        let b1 = qscale(p, (a * phi).sin() / s);
        return Quat { w: a1.w + b1.w, x: a1.x + b1.x, y: a1.y + b1.y, z: a1.z + b1.z };
    }
    q1
}

// ---------------------------------------------------------------- HAnim keyframe callbacks

/// hanimApplyCB: the node's local matrix from its interpolated frame.
///
/// # Safety
/// `result`: an rw::Matrix; `frame`: an HAnimInterpFrame.
pub unsafe extern "C" fn hanim_apply_cb(result: *mut c_void, frame: *mut c_void) {
    unsafe {
        let m = result as *mut RwMatrix;
        let f = &*(frame as *const HAnimInterpFrame);
        rwmatrix::rotate_quat(m, &f.q, 0);
        (*m).pos = f.t;
    }
}

/// hanimInterpCB: the interpolated frame at time t between two keyframes.
///
/// # Safety
/// `vout`: an HAnimInterpFrame; `vin1`, `vin2`: HAnimKeyFrames.
pub unsafe extern "C" fn hanim_interp_cb(vout: *mut c_void, vin1: *mut c_void, vin2: *mut c_void, t: f32, _custom: *mut c_void) {
    unsafe {
        let out = &mut *(vout as *mut HAnimInterpFrame);
        let in1 = &*(vin1 as *const HAnimKeyFrame);
        let in2 = &*(vin2 as *const HAnimKeyFrame);
        let a = (t - in1.time) / (in2.time - in1.time);
        out.t = V3 {
            x: in1.t.x + a * (in2.t.x - in1.t.x),
            y: in1.t.y + a * (in2.t.y - in1.t.y),
            z: in1.t.z + a * (in2.t.z - in1.t.z),
        };
        out.q = qslerp(&in1.q, &in2.q, a);
    }
}

// ---------------------------------------------------------------- AnimInterpolator (anim.cpp)

unsafe fn interp_frame(it: *mut AnimInterpolator, n: i32) -> *mut InterpFrameHeader {
    unsafe { (it.add(1) as *mut u8).offset((n * (*it).current_interp_key_frame_size) as isize) as *mut InterpFrameHeader }
}

unsafe fn anim_frame(it: *mut AnimInterpolator, n: i32) -> *mut KeyFrameHeader {
    unsafe { ((*(*it).current_anim).keyframes as *mut u8).offset((n * (*it).current_anim_key_frame_size) as isize) as *mut KeyFrameHeader }
}

/// AnimInterpolator::setCurrentAnim; false (after taking the animation and resetting the
/// time, as librw does) when the animation's interpolated frames don't fit.
///
/// # Safety
/// A valid interpolator (with room for its nodes' frames) and animation.
pub unsafe fn set_current_anim(it: *mut AnimInterpolator, anim: *mut Animation) -> bool {
    unsafe {
        let info = (*anim).interp_info;
        (*it).current_anim = anim;
        (*it).current_time = 0.0;
        let mut maxkf = (*it).max_interp_key_frame_size;
        if size_of::<*mut c_void>() > 4 {
            maxkf += 16;
        }
        if (*info).interp_key_frame_size > maxkf {
            return false;
        }
        (*it).current_interp_key_frame_size = (*info).interp_key_frame_size;
        (*it).current_anim_key_frame_size = (*info).anim_key_frame_size;
        (*it).apply_cb = (*info).apply_cb;
        (*it).blend_cb = (*info).blend_cb;
        (*it).interp_cb = (*info).interp_cb;
        (*it).add_cb = (*info).add_cb;
        let n = (*it).num_nodes;
        for i in 0..n {
            let intf = interp_frame(it, i);
            let kf1 = anim_frame(it, i);
            let kf2 = anim_frame(it, i + n);
            (*intf).key_frame1 = kf1;
            (*intf).key_frame2 = kf2;
            if let Some(cb) = (*it).interp_cb {
                cb(intf as _, kf1 as _, kf2 as _, 0.0, (*anim).custom_data);
            }
        }
        (*it).next_frame = anim_frame(it, n * 2) as _;
        true
    }
}

/// AnimInterpolator::addTime: advances, moving each node to the keyframes that cover the new
/// time; past the end it restarts the animation.
///
/// # Safety
/// An interpolator with a current animation.
pub unsafe fn add_time(it: *mut AnimInterpolator, t: f32) {
    unsafe {
        if t <= 0.0 {
            return;
        }
        (*it).current_time += t;
        if (*it).current_time > (*(*it).current_anim).duration {
            set_current_anim(it, (*it).current_anim);
            return;
        }
        let n = (*it).num_nodes;
        let last = anim_frame(it, (*(*it).current_anim).num_frames);
        let mut next = (*it).next_frame as *mut KeyFrameHeader;
        let mut ifrm: *mut InterpFrameHeader = std::ptr::null_mut();
        while next < last && (*(*next).prev).time <= (*it).current_time {
            for i in 0..n {
                ifrm = interp_frame(it, i);
                if (*ifrm).key_frame2 == (*next).prev {
                    break;
                }
            }
            (*ifrm).key_frame1 = (*ifrm).key_frame2;
            (*ifrm).key_frame2 = next;
            next = ((*it).next_frame as *mut u8).offset((*it).current_anim_key_frame_size as isize) as *mut KeyFrameHeader;
            (*it).next_frame = next as _;
        }
        let cb = (*it).interp_cb.expect("interpolator without interpCB");
        for i in 0..n {
            ifrm = interp_frame(it, i);
            cb(ifrm as _, (*ifrm).key_frame1 as _, (*ifrm).key_frame2 as _, (*it).current_time, (*(*it).current_anim).custom_data);
        }
    }
}

// ---------------------------------------------------------------- HAnimHierarchy (hanim.cpp)

/// HAnimHierarchy::updateMatrices: every node's matrix from its interpolated frame times its
/// parent's (the PUSH/POP flags walk the tree depth first); the root is the parent frame's LTM
/// unless the hierarchy wants local space.
///
/// # Safety
/// A valid hierarchy with matrices and an interpolator with a current animation.
pub unsafe fn update_matrices(h: *mut HAnimHierarchy) {
    unsafe {
        let anim = (*h).interpolator;
        let apply = (*anim).apply_cb.expect("interpolator without applyCB");
        let mut root_mat = rwmatrix::IDENT;
        let frm = (*h).parent_frame;
        if !frm.is_null() {
            let parfrm = (*frm).object.parent as *mut Frame;
            if !parfrm.is_null() && (*h).flags & LOCALSPACEMATRICES == 0 {
                root_mat = *rwframe::get_ltm(parfrm);
            }
        }
        let mut stack: [*mut RwMatrix; 64] = [std::ptr::null_mut(); 64];
        let mut sp = 0usize;
        let mut parent_mat: *mut RwMatrix = &mut root_mat;
        stack[sp] = parent_mat;
        sp += 1;
        let mut anim_mat = RwMatrix::default();
        for i in 0..(*h).num_nodes {
            let node = &*(*h).node_info.offset(i as isize);
            let cur_mat = (*h).matrices.offset(i as isize);
            apply(&mut anim_mat as *mut RwMatrix as _, interp_frame(anim, i) as _);
            rwmatrix::mult(cur_mat, &anim_mat, parent_mat);
            if node.flags & PUSH != 0 {
                stack[sp] = parent_mat;
                sp += 1;
            }
            parent_mat = cur_mat;
            if node.flags & POP != 0 {
                sp -= 1;
                parent_mat = stack[sp];
            }
        }
    }
}

// ---------------------------------------------------------------- C ABI

/// # Safety
/// As update_matrices.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_hanim_update_matrices(h: *mut HAnimHierarchy) {
    unsafe { update_matrices(h) }
}

/// # Safety
/// As set_current_anim.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_anim_set_current_anim(it: *mut AnimInterpolator, anim: *mut Animation) -> i32 {
    unsafe { set_current_anim(it, anim) as i32 }
}

/// # Safety
/// As add_time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_anim_add_time(it: *mut AnimInterpolator, t: f32) {
    unsafe { add_time(it, t) }
}

/// hanimOpen registers these as the HAnim interpolator's callbacks.
#[unsafe(no_mangle)]
pub extern "C" fn revc_hanim_callbacks(apply: *mut Option<ApplyCb>, interp: *mut Option<InterpCb>) {
    unsafe {
        *apply = Some(hanim_apply_cb);
        *interp = Some(hanim_interp_cb);
    }
}

/// # Safety
/// Valid quaternions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rwquat_mult(q: *const Quat, p: *const Quat, out: *mut Quat) {
    unsafe { *out = qmult(&*q, &*p) }
}

/// # Safety
/// Valid quaternions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rwquat_lerp(q: *const Quat, p: *const Quat, r: f32, out: *mut Quat) {
    unsafe { *out = qlerp(&*q, &*p, r) }
}

/// # Safety
/// Valid quaternions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_rwquat_slerp(q: *const Quat, p: *const Quat, a: f32, out: *mut Quat) {
    unsafe { *out = qslerp(&*q, &*p, a) }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwhanim.txt (recorded from librw's C++, see the recorder).
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
            RwMatrix { right, flags, up, at, pos, ..Default::default() }
        }
    }

    fn hex(v: f32) -> String {
        format!(" {:08x}", v.to_bits())
    }
    fn hv(a: &V3) -> String {
        hex(a.x) + &hex(a.y) + &hex(a.z)
    }
    fn hq(q: &Quat) -> String {
        hex(q.x) + &hex(q.y) + &hex(q.z) + &hex(q.w)
    }
    fn hm(m: &RwMatrix) -> String {
        format!("{} {:08x}{} --------{} --------{} --------", hv(&m.right), m.flags, hv(&m.up), hv(&m.at), hv(&m.pos))
    }
    fn fnv(s: &str) -> String {
        format!(" {:016x}", s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)))
    }

    /// One hierarchy with its interpolator, animation and parent frames, built like the recorder.
    struct Scene {
        hier: Box<HAnimHierarchy>,
        _nodes: Vec<HAnimNodeInfo>,
        mats: Vec<RwMatrix>,
        interp: Vec<u64>, // AnimInterpolator + frames, 8-aligned
        keys: Vec<HAnimKeyFrame>,
        anim: Box<Animation>,
        frames: Vec<Box<Frame>>,
    }

    unsafe fn state(sc: &mut Scene) -> String {
        unsafe {
            let it = sc.interp.as_mut_ptr() as *mut AnimInterpolator;
            let base = sc.keys.as_mut_ptr();
            let k = |p: *mut c_void| if p.is_null() { -1 } else { (p as *mut HAnimKeyFrame).offset_from(base) as i32 };
            let mut o = hex((*it).current_time) + &format!(" {}", k((*it).next_frame));
            for i in 0..sc.hier.num_nodes {
                let f = &*(interp_frame(it, i) as *const HAnimInterpFrame);
                o += &format!(" {} {}", k(f.key_frame1 as _), k(f.key_frame2 as _));
                o += &hq(&f.q);
                o += &hv(&f.t);
                o += &hm(&sc.mats[i as usize]);
            }
            fnv(&o)
        }
    }

    #[test]
    fn matches_librw_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden/rwhanim.txt");
        let text = std::fs::read_to_string(path).expect("tests/golden/rwhanim.txt");
        let mut info = AnimInterpolatorInfo {
            id: 1,
            interp_key_frame_size: size_of::<HAnimInterpFrame>() as i32,
            anim_key_frame_size: size_of::<HAnimKeyFrame>() as i32,
            custom_data_size: 0,
            apply_cb: Some(hanim_apply_cb),
            blend_cb: None,
            interp_cb: Some(hanim_interp_cb),
            add_cb: None,
            mul_recip_cb: std::ptr::null_mut(),
            stream_read: std::ptr::null_mut(),
            stream_write: std::ptr::null_mut(),
            stream_get_size: std::ptr::null_mut(),
        };
        let mut sc: Option<Scene> = None;
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap_or((line, ""));
            let mut w = In(input.split_whitespace());
            let op = w.word().to_string();
            let got = unsafe {
                match op.as_str() {
                    "qmult" => hq(&qmult(&w.q(), &w.q())),
                    "qlerp" => {
                        let (q, p, a) = (w.q(), w.q(), w.f());
                        hq(&qlerp(&q, &p, a))
                    }
                    "qslerp" => {
                        let (q, p, a) = (w.q(), w.q(), w.f());
                        hq(&qslerp(&q, &p, a))
                    }
                    "hier" => {
                        let (nn, flags, mode) = (w.i(), w.i(), w.i());
                        // parent frame under a frame whose LTM is the recorded one
                        let mut frames = Vec::new();
                        let mut parent_frame = std::ptr::null_mut();
                        if mode != 0 {
                            let ltm = w.m();
                            let mut pp: Box<Frame> = Box::new(std::mem::zeroed());
                            let mut pf: Box<Frame> = Box::new(std::mem::zeroed());
                            pp.ltm = ltm;
                            pp.matrix = ltm;
                            pp.root = &mut *pp;
                            pf.object.parent = &mut *pp as *mut Frame as _;
                            pf.root = &mut *pp;
                            parent_frame = &mut *pf as *mut Frame;
                            frames.push(pp);
                            frames.push(pf);
                        }
                        assert_eq!(w.word(), "|");
                        let nodes: Vec<HAnimNodeInfo> = (0..nn).map(|i| HAnimNodeInfo { id: 0, index: i, flags: w.i(), frame: std::ptr::null_mut() }).collect();
                        let mut s = Scene {
                            hier: Box::new(std::mem::zeroed()),
                            _nodes: nodes,
                            mats: vec![RwMatrix::default(); nn as usize],
                            interp: vec![0u64; (size_of::<AnimInterpolator>() + nn as usize * (size_of::<HAnimInterpFrame>() + 16)) / 8 + 1],
                            keys: Vec::new(),
                            anim: Box::new(std::mem::zeroed()),
                            frames,
                        };
                        let it = s.interp.as_mut_ptr() as *mut AnimInterpolator;
                        (*it).max_interp_key_frame_size = size_of::<HAnimInterpFrame>() as i32;
                        (*it).current_interp_key_frame_size = size_of::<HAnimInterpFrame>() as i32;
                        (*it).current_anim_key_frame_size = -1;
                        (*it).num_nodes = nn;
                        s.hier.flags = flags;
                        s.hier.num_nodes = nn;
                        s.hier.matrices = s.mats.as_mut_ptr();
                        s.hier.node_info = s._nodes.as_mut_ptr();
                        s.hier.parent_frame = parent_frame;
                        s.hier.interpolator = it;
                        let _ = &s.frames;
                        sc = Some(s);
                        continue;
                    }
                    "anim" => {
                        let s = sc.as_mut().unwrap();
                        let (total, duration) = (w.i(), w.f());
                        assert_eq!(w.word(), "|");
                        s.keys = (0..total).map(|_| HAnimKeyFrame { prev: std::ptr::null_mut(), time: 0.0, q: Quat::default(), t: V3::default() }).collect();
                        let base = s.keys.as_mut_ptr();
                        for j in 0..total as usize {
                            let prev = w.i();
                            s.keys[j].prev = if prev < 0 { std::ptr::null_mut() } else { base.add(prev as usize) };
                            s.keys[j].time = w.f();
                            s.keys[j].q = w.q();
                            s.keys[j].t = w.v3();
                        }
                        *s.anim = Animation { interp_info: &mut info, num_frames: total, flags: 0, duration, keyframes: base as _, custom_data: std::ptr::null_mut() };
                        continue;
                    }
                    "set" => {
                        let s = sc.as_mut().unwrap();
                        set_current_anim(s.hier.interpolator, &mut *s.anim);
                        update_matrices(&mut *s.hier);
                        state(s)
                    }
                    "addtime" => {
                        let s = sc.as_mut().unwrap();
                        add_time(s.hier.interpolator, w.f());
                        update_matrices(&mut *s.hier);
                        state(s)
                    }
                    other => panic!("unknown op {other}"),
                }
            };
            n += 1;
            if got != want && bad.len() < 5 {
                bad.push(format!("{}\n  want{want}\n  got {got}", &input[..input.len().min(200)]));
            }
        }
        assert!(n > 19000, "only {n} cases");
        assert!(bad.is_empty(), "differences (first {}):\n{}", bad.len(), bad.join("\n"));
    }
}

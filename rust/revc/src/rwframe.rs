//! librw's rw::Frame hierarchy logic (vendor/librw/src/frame.cpp), stage 2 step 3b: child links,
//! hierarchy roots, the dirty list and LTM synchronisation. The C++ Frame keeps its layout (it
//! still owns allocation, plugins and cloning until step 3c); frame.cpp forwards here.
//!
//! Same behaviour as librw, step by step: the same traversal orders (children are synced in list
//! order, depth first), the same flag updates, LTMs through rwmatrix::mult (bit-identical), and
//! attached objects synced through their C++ callbacks in the same order. Checked against
//! tests/golden/rwframe.txt, recorded from the C++ (tests/golden/recorders/rwframe).

use crate::math::RwMatrix;
use crate::rwmatrix;
use std::ffi::c_void;

/// rw::Object
#[repr(C)]
pub struct Object {
    pub kind: u8,
    pub sub_type: u8,
    pub flags: u8,
    pub private_flags: u8,
    pub parent: *mut c_void,
}

/// rw::LLLink
#[repr(C)]
pub struct LLLink {
    pub next: *mut LLLink,
    pub prev: *mut LLLink,
}

/// rw::LinkList
#[repr(C)]
pub struct LinkList {
    pub link: LLLink,
}

/// rw::Frame (plugin data follows it in memory).
#[repr(C)]
pub struct Frame {
    pub object: Object,
    pub in_dirty_list: LLLink,
    pub object_list: LinkList,
    pub matrix: RwMatrix,
    pub ltm: RwMatrix,
    pub child: *mut Frame,
    pub next: *mut Frame,
    pub root: *mut Frame,
}

/// rw::ObjectWithFrame
#[repr(C)]
pub struct ObjectWithFrame {
    pub object: Object,
    pub in_frame: LLLink,
    pub sync_cb: unsafe extern "C" fn(*mut ObjectWithFrame),
}

// Frame private flags
pub const HIERARCHYSYNCLTM: u8 = 0x01;
pub const HIERARCHYSYNCOBJ: u8 = 0x02;
pub const HIERARCHYSYNC: u8 = HIERARCHYSYNCLTM | HIERARCHYSYNCOBJ;
pub const SUBTREESYNCLTM: u8 = 0x04;
pub const SUBTREESYNCOBJ: u8 = 0x08;
pub const SUBTREESYNC: u8 = SUBTREESYNCLTM | SUBTREESYNCOBJ;
pub const SYNCLTM: u8 = HIERARCHYSYNCLTM | SUBTREESYNCLTM;
pub const SYNCOBJ: u8 = HIERARCHYSYNCOBJ | SUBTREESYNCOBJ;

// ---------------------------------------------------------------- intrusive lists (rwbase.h)

pub(crate) unsafe fn list_add(list: *mut LinkList, link: *mut LLLink) {
    unsafe {
        (*link).next = (*list).link.next;
        (*link).prev = &mut (*list).link;
        (*(*list).link.next).prev = link;
        (*list).link.next = link;
    }
}

pub(crate) unsafe fn link_remove(link: *mut LLLink) {
    unsafe {
        (*(*link).prev).next = (*link).next;
        (*(*link).next).prev = (*link).prev;
    }
}

pub(crate) unsafe fn list_init(list: *mut LinkList) {
    unsafe {
        (*list).link.next = &mut (*list).link;
        (*list).link.prev = &mut (*list).link;
    }
}

/// The object owning `link` (offset of `field` in `T`), as LLLinkGetData.
macro_rules! container_of {
    ($link:expr, $t:ty, $field:ident) => {
        ($link as *mut u8).sub(std::mem::offset_of!($t, $field)) as *mut $t
    };
}

/// Calls the sync callback of every object attached to `f`, in list order (FORLIST).
unsafe fn sync_objects(f: *mut Frame) {
    unsafe {
        let end: *mut LLLink = &mut (*f).object_list.link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            let o = container_of!(lnk, ObjectWithFrame, in_frame);
            ((*o).sync_cb)(o);
            lnk = next;
        }
    }
}

fn parent(f: *mut Frame) -> *mut Frame {
    unsafe { (*f).object.parent as *mut Frame }
}

// ---------------------------------------------------------------- Frame

/// # Safety (all functions below)
/// `f` and the frames linked from it are valid rw::Frames; `dirty` is the engine's dirty list.
pub unsafe fn update_objects(f: *mut Frame, dirty: *mut LinkList) {
    unsafe {
        let root = (*f).root;
        if (*root).object.private_flags & HIERARCHYSYNC == 0 {
            list_add(dirty, &mut (*root).in_dirty_list);
        }
        (*root).object.private_flags |= HIERARCHYSYNC;
        (*f).object.private_flags |= SUBTREESYNC;
    }
}

pub unsafe fn set_hierarchy_root(f: *mut Frame, root: *mut Frame) {
    unsafe {
        (*f).root = root;
        let mut c = (*f).child;
        while !c.is_null() {
            set_hierarchy_root(c, root);
            c = (*c).next;
        }
    }
}

pub unsafe fn remove_child(f: *mut Frame, dirty: *mut LinkList) {
    unsafe {
        let p = parent(f);
        let mut child = (*p).child;
        if child == f {
            (*p).child = (*f).next;
        } else {
            while (*child).next != f {
                child = (*child).next;
            }
            (*child).next = (*f).next;
        }
        (*f).next = std::ptr::null_mut();
        (*f).object.parent = std::ptr::null_mut();
        set_hierarchy_root(f, f);
        update_objects(f, dirty);
    }
}

pub unsafe fn add_child(f: *mut Frame, child: *mut Frame, append: bool, dirty: *mut LinkList) {
    unsafe {
        if !parent(child).is_null() {
            remove_child(child, dirty);
        }
        if append {
            if (*f).child.is_null() {
                (*f).child = child;
            } else {
                let mut c = (*f).child;
                while !(*c).next.is_null() {
                    c = (*c).next;
                }
                (*c).next = child;
            }
            (*child).next = std::ptr::null_mut();
        } else {
            (*child).next = (*f).child;
            (*f).child = child;
        }
        (*child).object.parent = f as *mut c_void;
        (*child).root = (*f).root;
        let mut c = (*child).child;
        while !c.is_null() {
            set_hierarchy_root(c, f);
            c = (*c).next;
        }
        if (*child).object.private_flags & HIERARCHYSYNC != 0 {
            link_remove(&mut (*child).in_dirty_list);
            (*child).object.private_flags &= !HIERARCHYSYNC;
        }
        update_objects(f, dirty);
    }
}

/// Frame::count: this frame and every frame below it.
pub unsafe fn count(f: *mut Frame) -> i32 {
    unsafe {
        let mut n = 1;
        let mut c = (*f).child;
        while !c.is_null() {
            let next = (*c).next;
            n += count(c);
            c = next;
        }
        n
    }
}

unsafe fn sync_ltm_recurse(mut frame: *mut Frame, mut hierarchy_flags: u8) {
    unsafe {
        while !frame.is_null() {
            hierarchy_flags |= (*frame).object.private_flags;
            if hierarchy_flags & SUBTREESYNCLTM != 0 {
                rwmatrix::mult(&mut (*frame).ltm, &(*frame).matrix, &(*parent(frame)).ltm);
                (*frame).object.private_flags &= !SUBTREESYNCLTM;
            }
            sync_ltm_recurse((*frame).child, hierarchy_flags);
            frame = (*frame).next;
        }
    }
}

unsafe fn sync_obj_recurse(mut frame: *mut Frame) {
    unsafe {
        while !frame.is_null() {
            sync_objects(frame);
            (*frame).object.private_flags &= !SUBTREESYNCOBJ;
            sync_obj_recurse((*frame).child);
            frame = (*frame).next;
        }
    }
}

unsafe fn sync_recurse(mut frame: *mut Frame, mut hierarchy_flags: u8) {
    unsafe {
        while !frame.is_null() {
            hierarchy_flags |= (*frame).object.private_flags;
            if hierarchy_flags & SUBTREESYNCLTM != 0 {
                rwmatrix::mult(&mut (*frame).ltm, &(*frame).matrix, &(*parent(frame)).ltm);
            }
            sync_objects(frame);
            (*frame).object.private_flags &= !SUBTREESYNC;
            sync_recurse((*frame).child, hierarchy_flags);
            frame = (*frame).next;
        }
    }
}

pub unsafe fn sync_hierarchy_ltm(f: *mut Frame) {
    unsafe {
        if (*f).object.private_flags & SUBTREESYNCLTM != 0 {
            (*f).ltm = (*f).matrix;
        }
        sync_ltm_recurse((*f).child, (*f).object.private_flags);
        (*f).object.private_flags &= !SYNCLTM;
    }
}

pub unsafe fn get_ltm(f: *mut Frame) -> *mut RwMatrix {
    unsafe {
        let root = (*f).root;
        if (*root).object.private_flags & HIERARCHYSYNCLTM != 0 {
            sync_hierarchy_ltm(root);
        }
        &mut (*f).ltm
    }
}

pub unsafe fn sync_dirty(dirty: *mut LinkList) {
    unsafe {
        let end: *mut LLLink = &mut (*dirty).link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            let frame = container_of!(lnk, Frame, in_dirty_list);
            if (*frame).object.private_flags & HIERARCHYSYNCLTM != 0 {
                if (*frame).object.private_flags & SUBTREESYNCLTM != 0 {
                    (*frame).ltm = (*frame).matrix;
                }
                sync_objects(frame);
                sync_recurse((*frame).child, (*frame).object.private_flags);
            } else {
                sync_objects(frame);
                sync_obj_recurse((*frame).child);
            }
            (*frame).object.private_flags &= !(SYNCLTM | SYNCOBJ);
            lnk = next;
        }
        list_init(dirty);
    }
}

// ---------------------------------------------------------------- lifecycle (step 3c)

/// What stays C++ for a frame's life: its size with plugin data, the plugin constructors,
/// destructors and copy callbacks (NodeName, HAnim, the game's plugins) and the allocation
/// counter. frame.cpp passes them in (revc_frame_set_hooks).
#[repr(C)]
pub struct Hooks {
    pub size: unsafe extern "C" fn() -> usize,
    pub construct: unsafe extern "C" fn(*mut Frame),
    pub destruct: unsafe extern "C" fn(*mut Frame),
    pub copy: unsafe extern "C" fn(*mut Frame, *mut Frame),
    pub allocated: unsafe extern "C" fn(i32),
}

static mut HOOKS: Option<Hooks> = None;

fn hooks() -> &'static Hooks {
    // SAFETY: set once at startup (before any frame exists), read-only afterwards.
    unsafe { (*std::ptr::addr_of!(HOOKS)).as_ref().expect("revc_frame_set_hooks not called") }
}

/// Frames (with their plugin data) are allocated here; the size is fixed once the plugins are
/// registered, which happens before the first frame is created.
fn layout(size: usize) -> std::alloc::Layout {
    std::alloc::Layout::from_size_align(size.max(std::mem::size_of::<Frame>()), 16).unwrap()
}

/// Frame::create
pub unsafe fn create() -> *mut Frame {
    unsafe {
        let h = hooks();
        let f = std::alloc::alloc(layout((h.size)())) as *mut Frame;
        if f.is_null() {
            return f;
        }
        (h.allocated)(1);
        (*f).object = Object { kind: 0, sub_type: 0, flags: 0, private_flags: 0, parent: std::ptr::null_mut() };
        list_init(&mut (*f).object_list);
        (*f).child = std::ptr::null_mut();
        (*f).next = std::ptr::null_mut();
        (*f).root = f;
        (*f).matrix = rwmatrix::IDENT;
        (*f).ltm = rwmatrix::IDENT;
        (h.construct)(f);
        f
    }
}

unsafe fn free(f: *mut Frame) {
    unsafe { std::alloc::dealloc(f as *mut u8, layout((hooks().size)())) }
}

/// Frame::destroy
pub unsafe fn destroy(f: *mut Frame, dirty: *mut LinkList) {
    unsafe {
        let h = hooks();
        // ObjectWithFrame::setFrame(nil) for every attached object
        let end: *mut LLLink = &mut (*f).object_list.link;
        let mut lnk = (*end).next;
        while lnk != end {
            let next = (*lnk).next;
            let o = container_of!(lnk, ObjectWithFrame, in_frame);
            if !(*o).object.parent.is_null() {
                link_remove(&mut (*o).in_frame);
            }
            (*o).object.parent = std::ptr::null_mut();
            lnk = next;
        }
        (h.destruct)(f);
        if !parent(f).is_null() {
            remove_child(f, dirty);
        }
        if (*f).object.private_flags & HIERARCHYSYNC != 0 {
            link_remove(&mut (*f).in_dirty_list);
        }
        let mut c = (*f).child;
        while !c.is_null() {
            (*c).object.parent = std::ptr::null_mut();
            c = (*c).next;
        }
        free(f);
        (h.allocated)(-1);
    }
}

/// Frame::destroyHierarchy (librw doesn't count these frames down; kept as it was)
pub unsafe fn destroy_hierarchy(f: *mut Frame) {
    unsafe {
        let mut child = (*f).child;
        while !child.is_null() {
            let next = (*child).next;
            destroy_hierarchy(child);
            child = next;
        }
        (hooks().destruct)(f);
        if (*f).object.private_flags & HIERARCHYSYNC != 0 {
            link_remove(&mut (*f).in_dirty_list);
        }
        free(f);
    }
}

unsafe fn clone_recurse(old: *mut Frame, mut newroot: *mut Frame) -> *mut Frame {
    unsafe {
        let frame = create();
        if newroot.is_null() {
            newroot = frame;
        }
        // Object::copy
        (*frame).object.kind = (*old).object.kind;
        (*frame).object.sub_type = (*old).object.sub_type;
        (*frame).object.flags = (*old).object.flags;
        (*frame).object.private_flags = (*old).object.private_flags;
        (*frame).object.parent = std::ptr::null_mut();
        (*frame).matrix = (*old).matrix;
        (*frame).root = newroot;
        (*old).root = frame; // remember the clone
        let mut child = (*old).child;
        while !child.is_null() {
            let cloned = clone_recurse(child, newroot);
            (*cloned).next = (*frame).child;
            (*frame).child = cloned;
            (*cloned).object.parent = frame as *mut c_void;
            child = (*child).next;
        }
        (hooks().copy)(frame, old);
        frame
    }
}

/// Frame::cloneAndLink: clones the hierarchy; each original's root points at its clone.
pub unsafe fn clone_and_link(f: *mut Frame, dirty: *mut LinkList) -> *mut Frame {
    unsafe {
        let h = clone_recurse(f, std::ptr::null_mut());
        if !h.is_null() {
            (*h).object.private_flags &= !HIERARCHYSYNC;
            update_objects(h, dirty);
        }
        h
    }
}

/// Frame::purgeClone: removes the links to the clones again.
pub unsafe fn purge_clone(f: *mut Frame) {
    unsafe {
        let p = parent(f);
        set_hierarchy_root(f, if p.is_null() { f } else { (*p).root });
    }
}

// ---------------------------------------------------------------- C ABI (frame.cpp forwards)

/// # Safety
/// Called once at startup, before any frame is created.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_set_hooks(h: Hooks) {
    unsafe { *std::ptr::addr_of_mut!(HOOKS) = Some(h) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_create() -> *mut Frame {
    unsafe { create() }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_destroy(f: *mut Frame, dirty: *mut LinkList) {
    unsafe { destroy(f, dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_destroy_hierarchy(f: *mut Frame) {
    unsafe { destroy_hierarchy(f) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_clone_and_link(f: *mut Frame, dirty: *mut LinkList) -> *mut Frame {
    unsafe { clone_and_link(f, dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_purge_clone(f: *mut Frame) {
    unsafe { purge_clone(f) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_add_child(f: *mut Frame, child: *mut Frame, append: i32, dirty: *mut LinkList) {
    unsafe { add_child(f, child, append != 0, dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_remove_child(f: *mut Frame, dirty: *mut LinkList) {
    unsafe { remove_child(f, dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_count(f: *mut Frame) -> i32 {
    unsafe { count(f) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_sync_hierarchy_ltm(f: *mut Frame) {
    unsafe { sync_hierarchy_ltm(f) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_get_ltm(f: *mut Frame) -> *mut RwMatrix {
    unsafe { get_ltm(f) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_sync_dirty(dirty: *mut LinkList) {
    unsafe { sync_dirty(dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_update_objects(f: *mut Frame, dirty: *mut LinkList) {
    unsafe { update_objects(f, dirty) }
}

/// # Safety
/// As the functions above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_set_hierarchy_root(f: *mut Frame, root: *mut Frame) {
    unsafe { set_hierarchy_root(f, root) }
}

/// `op`: 0 rotate (axis, angle), 1 translate, 2 scale (vector), 3 transform (matrix); then the
/// object update as Frame::rotate/translate/scale/transform do.
///
/// # Safety
/// As the functions above; `v` points to a V3d, `m` to an rw::Matrix as `op` needs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_frame_modify(
    f: *mut Frame,
    op: i32,
    v: *const crate::math::V3,
    angle: f32,
    m: *const RwMatrix,
    combine: i32,
    dirty: *mut LinkList,
) {
    unsafe {
        let mat: *mut RwMatrix = &mut (*f).matrix;
        match op {
            0 => rwmatrix::rotate_axis(mat, v, angle, combine),
            1 => rwmatrix::translate(mat, v, combine),
            2 => rwmatrix::scale_by(mat, v, combine),
            _ => rwmatrix::transform(mat, m, combine),
        }
        update_objects(f, dirty);
    }
}

#[cfg(test)]
mod golden {
    //! Replays tests/golden/rwframe.txt (recorded from librw's frame.cpp, see the recorder).
    use super::*;
    use crate::math::V3;
    use std::cell::RefCell;

    thread_local! {
        static SYNCED: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    }

    #[repr(C)]
    struct Obj {
        owf: ObjectWithFrame,
        id: i32,
    }

    unsafe extern "C" fn sync_cb(o: *mut ObjectWithFrame) {
        let id = unsafe { (*(o as *mut Obj)).id };
        SYNCED.with(|s| s.borrow_mut().push(id));
    }

    fn new_frame() -> Box<Frame> {
        let mut f = Box::new(Frame {
            object: Object { kind: 0, sub_type: 0, flags: 0, private_flags: 0, parent: std::ptr::null_mut() },
            in_dirty_list: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() },
            object_list: LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } },
            matrix: rwmatrix::IDENT,
            ltm: rwmatrix::IDENT,
            child: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
            root: std::ptr::null_mut(),
        });
        let p: *mut Frame = &mut *f;
        unsafe {
            list_init(&mut (*p).object_list);
            (*p).root = p;
        }
        f
    }

    /// ObjectWithFrame::setFrame (inline in rwobjects.h)
    unsafe fn set_frame(o: *mut ObjectWithFrame, f: *mut Frame, dirty: *mut LinkList) {
        unsafe {
            if !(*o).object.parent.is_null() {
                link_remove(&mut (*o).in_frame);
            }
            (*o).object.parent = f as *mut c_void;
            if !f.is_null() {
                list_add(&mut (*f).object_list, &mut (*o).in_frame);
                update_objects(f, dirty);
            }
        }
    }

    fn hex(o: &mut String, v: f32) {
        o.push_str(&format!(" {:08x}", v.to_bits()));
    }
    fn mat(o: &mut String, x: &RwMatrix) {
        for v in [x.right.x, x.right.y, x.right.z] {
            hex(o, v);
        }
        o.push_str(&format!(" {:08x}", x.flags));
        for (vs, last) in [([x.up.x, x.up.y, x.up.z], ""), ([x.at.x, x.at.y, x.at.z], ""), ([x.pos.x, x.pos.y, x.pos.z], "")] {
            for v in vs {
                hex(o, v);
            }
            o.push_str(" --------");
            let _ = last;
        }
    }

    struct Words<'a>(std::str::SplitWhitespace<'a>);
    impl Words<'_> {
        fn s(&mut self) -> &str {
            self.0.next().unwrap()
        }
        fn i(&mut self) -> usize {
            self.s().parse().unwrap()
        }
        fn f(&mut self) -> f32 {
            f32::from_bits(u32::from_str_radix(self.s(), 16).unwrap())
        }
        fn v3(&mut self) -> V3 {
            V3 { x: self.f(), y: self.f(), z: self.f() }
        }
        fn m(&mut self) -> RwMatrix {
            let right = self.v3();
            let flags = u32::from_str_radix(self.s(), 16).unwrap();
            let up = self.v3();
            self.s();
            let at = self.v3();
            self.s();
            let pos = self.v3();
            self.s();
            RwMatrix { right, flags, up, pad1: 0, at, pad2: 0, pos, pad3: 0 }
        }
    }

    #[test]
    fn matches_librw_golden() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/rwframe.txt");
        let text = std::fs::read_to_string(&path).expect("tests/golden/rwframe.txt");
        let mut dirty = Box::new(LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } });
        let dp: *mut LinkList = &mut *dirty;
        let mut frames: Vec<Box<Frame>> = Vec::new();
        let mut objs: Vec<Box<Obj>> = Vec::new();
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = match line.split_once(" =>") {
                Some((a, b)) => (a, Some(b)),
                None => (line, None),
            };
            let mut w = Words(input.split_whitespace());
            let op = w.s().to_string();
            let fp = |frames: &mut Vec<Box<Frame>>, i: usize| -> *mut Frame { &mut *frames[i] };
            let mut out = String::new();
            unsafe {
                match op.as_str() {
                    "new" => {
                        let k = w.i();
                        list_init(dp);
                        frames = (0..k).map(|_| new_frame()).collect();
                        objs.clear();
                        continue;
                    }
                    "attach" => {
                        let (o, at) = (w.i(), w.i());
                        assert_eq!(o, objs.len());
                        let mut ob = Box::new(Obj {
                            owf: ObjectWithFrame {
                                object: Object { kind: 1, sub_type: 0, flags: 0, private_flags: 0, parent: std::ptr::null_mut() },
                                in_frame: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() },
                                sync_cb,
                            },
                            id: o as i32,
                        });
                        let f = fp(&mut frames, at);
                        set_frame(&mut ob.owf, f, dp);
                        objs.push(ob);
                    }
                    "addchild" => {
                        let (a, b, app) = (w.i(), w.i(), w.i());
                        let (fa, fb) = (fp(&mut frames, a), fp(&mut frames, b));
                        add_child(fa, fb, app != 0, dp);
                    }
                    "removechild" => {
                        let a = w.i();
                        remove_child(fp(&mut frames, a), dp);
                    }
                    "count" => {
                        let a = w.i();
                        out.push_str(&format!(" {}", count(fp(&mut frames, a))));
                    }
                    "rotate" => {
                        let a = w.i();
                        let v = w.v3();
                        let ang = w.f();
                        let opn = w.i() as i32;
                        revc_frame_modify(fp(&mut frames, a), 0, &v, ang, std::ptr::null(), opn, dp);
                    }
                    "translate" | "scale" => {
                        let a = w.i();
                        let v = w.v3();
                        let opn = w.i() as i32;
                        revc_frame_modify(fp(&mut frames, a), if op == "translate" { 1 } else { 2 }, &v, 0.0, std::ptr::null(), opn, dp);
                    }
                    "transform" => {
                        let a = w.i();
                        let t = w.m();
                        let opn = w.i() as i32;
                        revc_frame_modify(fp(&mut frames, a), 3, std::ptr::null(), 0.0, &t, opn, dp);
                    }
                    "getltm" => {
                        let a = w.i();
                        let l = get_ltm(fp(&mut frames, a));
                        mat(&mut out, &*l);
                    }
                    "syncdirty" => sync_dirty(dp),
                    other => panic!("unknown op {other}"),
                }
                // state, as the recorder prints it (after the op's own output)
                let ptrs: Vec<*mut Frame> = frames.iter_mut().map(|f| &mut **f as *mut Frame).collect();
                let idx = |p: *mut Frame| ptrs.iter().position(|&q| q == p).map(|i| i as i32).unwrap_or(-1);
                out.push_str(" frames:");
                for &p in &ptrs {
                    let f = &*p;
                    out.push_str(&format!(" [{} {} {} {} {:02x}", idx(parent(p)), idx(f.child), idx(f.next), idx(f.root), f.object.private_flags));
                    mat(&mut out, &f.matrix);
                    mat(&mut out, &f.ltm);
                    out.push(']');
                }
                out.push_str(" | dirty:");
                let end: *mut LLLink = &mut (*dp).link;
                let mut lnk = (*end).next;
                while lnk != end {
                    out.push_str(&format!(" {}", idx(container_of!(lnk, Frame, in_dirty_list))));
                    lnk = (*lnk).next;
                }
                out.push_str(" | synced:");
                SYNCED.with(|s| {
                    for id in s.borrow_mut().drain(..) {
                        out.push_str(&format!(" {id}"));
                    }
                });
            }
            n += 1;
            // the op's own output (count, getltm) is printed before "=>" in the recorder
            let (own, state) = match op.as_str() {
                "count" | "getltm" => {
                    let i = out.find(" frames:").unwrap();
                    (out[..i].to_string(), out[i..].to_string())
                }
                _ => (String::new(), out.clone()),
            };
            let got_input_tail = own;
            let want = want.unwrap();
            let want_input_tail: String = if got_input_tail.is_empty() { String::new() } else { input[input.len() - got_input_tail.len()..].to_string() };
            if got_input_tail != want_input_tail || format!(" {}", state.trim_start()) != want {
                bad.push(format!("{input}\n  want{want}\n  got {got_input_tail} //{state}"));
            }
        }
        assert!(n > 2500, "only {n} cases");
        assert!(bad.is_empty(), "{} of {n} differ, first:\n{}", bad.len(), bad.iter().take(2).cloned().collect::<Vec<_>>().join("\n"));
    }
}

#[cfg(test)]
mod lifecycle_golden {
    //! Replays tests/golden/rwframe-life.txt (create/destroy/clone, recorded from librw).
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static IDS: RefCell<Vec<*mut Frame>> = const { RefCell::new(Vec::new()) };
        static PLOG: RefCell<String> = const { RefCell::new(String::new()) };
        static ALLOCATED: RefCell<i32> = const { RefCell::new(0) };
    }

    fn id_of(f: *mut Frame) -> i32 {
        if f.is_null() {
            return -1;
        }
        IDS.with(|ids| ids.borrow().iter().position(|&p| p == f).map(|i| i as i32).unwrap_or(-2))
    }

    unsafe extern "C" fn h_size() -> usize {
        std::mem::size_of::<Frame>()
    }
    unsafe extern "C" fn h_construct(f: *mut Frame) {
        let id = IDS.with(|ids| {
            let mut ids = ids.borrow_mut();
            ids.push(f);
            ids.len() - 1
        });
        PLOG.with(|p| p.borrow_mut().push_str(&format!(" c{id}")));
    }
    unsafe extern "C" fn h_destruct(f: *mut Frame) {
        let id = id_of(f);
        PLOG.with(|p| p.borrow_mut().push_str(&format!(" d{id}")));
        if id >= 0 {
            IDS.with(|ids| ids.borrow_mut()[id as usize] = std::ptr::null_mut());
        }
    }
    unsafe extern "C" fn h_copy(d: *mut Frame, s: *mut Frame) {
        let (a, b) = (id_of(d), id_of(s));
        PLOG.with(|p| p.borrow_mut().push_str(&format!(" y{a},{b}")));
    }
    unsafe extern "C" fn h_allocated(n: i32) {
        ALLOCATED.with(|a| *a.borrow_mut() += n);
    }

    fn mat(o: &mut String, x: &RwMatrix) {
        let h = |o: &mut String, v: f32| o.push_str(&format!(" {:08x}", v.to_bits()));
        for v in [x.right.x, x.right.y, x.right.z] {
            h(o, v);
        }
        o.push_str(&format!(" {:08x}", x.flags));
        for vs in [[x.up.x, x.up.y, x.up.z], [x.at.x, x.at.y, x.at.z], [x.pos.x, x.pos.y, x.pos.z]] {
            for v in vs {
                h(o, v);
            }
            o.push_str(" --------");
        }
    }

    #[test]
    fn matches_librw_golden() {
        let _guard = crate::testlock();
        unsafe {
            revc_frame_set_hooks(Hooks { size: h_size, construct: h_construct, destruct: h_destruct, copy: h_copy, allocated: h_allocated });
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/rwframe-life.txt");
        let text = std::fs::read_to_string(&path).expect("tests/golden/rwframe-life.txt");
        let mut dirty = Box::new(LinkList { link: LLLink { next: std::ptr::null_mut(), prev: std::ptr::null_mut() } });
        let dp: *mut LinkList = &mut *dirty;
        unsafe { list_init(dp) };
        let (mut n, mut bad) = (0, Vec::new());
        for line in text.lines() {
            let (input, want) = line.split_once(" =>").unwrap();
            let mut w = input.split_whitespace();
            let op = w.next().unwrap();
            let mut arg = || -> usize { w.next().unwrap().parse().unwrap() };
            let get = |i: usize| IDS.with(|ids| ids.borrow()[i]);
            unsafe {
                match op {
                    "create" => {
                        let f = create();
                        // the recorder places it (translate, replace) and logs the matrix it got
                        let fields: Vec<&str> = input.split_whitespace().skip(1).collect();
                        let u = |i: usize| u32::from_str_radix(fields[i], 16).unwrap();
                        let fl = |i: usize| f32::from_bits(u(i));
                        let m = &mut (*f).matrix;
                        m.right = crate::math::V3 { x: fl(0), y: fl(1), z: fl(2) };
                        m.flags = u(3);
                        m.up = crate::math::V3 { x: fl(4), y: fl(5), z: fl(6) };
                        m.at = crate::math::V3 { x: fl(8), y: fl(9), z: fl(10) };
                        m.pos = crate::math::V3 { x: fl(12), y: fl(13), z: fl(14) };
                        // Matrix::translate doesn't touch the frame's flags or the dirty list
                    }
                    "addchild" => {
                        let (a, b, app) = (arg(), arg(), arg());
                        add_child(get(a), get(b), app != 0, dp);
                    }
                    "destroy" => destroy(get(arg()), dp),
                    "destroyhierarchy" => destroy_hierarchy(get(arg())),
                    "clonehierarchy" => {
                        let f = get(arg());
                        clone_and_link(f, dp);
                        purge_clone(f);
                    }
                    "cloneandlink" => {
                        clone_and_link(get(arg()), dp);
                    }
                    "purgeclone" => purge_clone(get(arg())),
                    other => panic!("unknown op {other}"),
                }
                let mut out = String::new();
                let ids: Vec<*mut Frame> = IDS.with(|ids| ids.borrow().clone());
                for (i, &p) in ids.iter().enumerate() {
                    if p.is_null() {
                        continue;
                    }
                    let f = &*p;
                    out.push_str(&format!(" [{i} {} {} {} {} {:02x}", id_of(parent(p)), id_of(f.child), id_of(f.next), id_of(f.root), f.object.private_flags));
                    mat(&mut out, &f.matrix);
                    out.push(']');
                }
                let hash = out.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
                let mut out = format!(" frames: {hash:016x} | dirty:");
                let end: *mut LLLink = &mut (*dp).link;
                let mut lnk = (*end).next;
                while lnk != end {
                    out.push_str(&format!(" {}", id_of(container_of!(lnk, Frame, in_dirty_list))));
                    lnk = (*lnk).next;
                }
                let plog = PLOG.with(|p| std::mem::take(&mut *p.borrow_mut()));
                let alloc = ALLOCATED.with(|a| *a.borrow());
                out.push_str(&format!(" | plugins:{plog} | allocated: {alloc}"));
                n += 1;
                if out != want {
                    let at = out.bytes().zip(want.bytes()).position(|(x, y)| x != y).unwrap_or(out.len().min(want.len()));
                    let from = at.saturating_sub(120);
                    bad.push(format!("{input}: first difference at byte {at}\n  want …{}\n  got  …{}", &want[from..(at + 80).min(want.len())], &out[from..(at + 80).min(out.len())]));
                    break;
                }
            }
        }
        assert!(n > 2000, "only {n} cases");
        assert!(bad.is_empty(), "{} differ after {n} cases:\n{}", bad.len(), bad.join("\n").chars().take(3000).collect::<String>());
    }
}

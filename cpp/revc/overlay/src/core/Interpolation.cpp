#include "common.h"
#include "Interpolation.h"
#include "Pools.h"
#include "Camera.h"
#include "World.h"
#include "Ped.h"
#include "Vehicle.h"
#include "Object.h"
#include "RwHelper.h"
#include <vector>

// Render interpolation (task 8). Only what is drawn changes: the simulation always sees its real
// transforms (ApplyForRender is undone by RestoreAfterRender right after the frame is drawn).
// Effects registered during a step (shadows, lights, particles) stay where the step put them.

namespace Interpolation
{
bool bEnabled = true;

// A transform as plain values (no RwMatrix attachment side effects).
struct Xform
{
	CVector right, forward, up, pos;

	void Get(const CMatrix &m) { right = m.GetRight(); forward = m.GetForward(); up = m.GetUp(); pos = m.GetPosition(); }
	void Put(CMatrix &m) const { m.GetRight() = right; m.GetForward() = forward; m.GetUp() = up; m.GetPosition() = pos; }
};

// Position: linear. Rotation: blend forward/up, then rebuild an orthonormal frame with the same
// handedness as the input (entities use right = forward x up, the camera up x forward). Fine for
// the small change in one 33 ms step.
static Xform
Blend(const Xform &a, const Xform &b, float t)
{
	Xform r;
	r.pos = a.pos + (b.pos - a.pos) * t;
	CVector f = a.forward + (b.forward - a.forward) * t;
	CVector u = a.up + (b.up - a.up) * t;
	f.Normalise();
	u = u - f * DotProduct(u, f);
	u.Normalise();
	CVector rt = CrossProduct(f, u);
	if (DotProduct(rt, b.right) < 0.0f)
		rt = -rt;
	// Keep each axis' scale (pickups pulse by scaling their matrix), lerped between the steps.
	float ls = a.right.Magnitude() + (b.right.Magnitude() - a.right.Magnitude()) * t;
	float lf = a.forward.Magnitude() + (b.forward.Magnitude() - a.forward.Magnitude()) * t;
	float lu = a.up.Magnitude() + (b.up.Magnitude() - a.up.Magnitude()) * t;
	r.forward = f * lf;
	r.up = u * lu;
	r.right = rt * ls;
	return r;
}

// Don't blend across teleports or sudden turns (respawns, cutscene cuts, pool slot reuse).
static bool
Continuous(const Xform &a, const Xform &b, float maxDist, float minDot)
{
	return (b.pos - a.pos).MagnitudeSqr() < maxDist * maxDist &&
	       DotProduct(a.forward, b.forward) > minDot && DotProduct(a.up, b.up) > minDot;
}

struct Slot
{
	CEntity *entity;   // what was in this pool slot at BeginStep (nil if empty)
	Xform prev;        // its transform then
	Xform current;     // saved while an interpolated transform is applied
	bool applied;
};

struct PoolSnap
{
	Slot *slots;
	int size;
};

static PoolSnap gPeds, gVehicles, gObjects;

// Skinned peds: their bone matrices come from the step's animation update; drawn with the
// interpolated body frame they would lag or lead it (the body jitters against its skeleton)
// and limbs would move at 30 Hz. So the bones are blended between the steps like the body.
struct BoneSnap
{
	RpHAnimHierarchy *hier;      // the ped's hierarchy at BeginStep (nil: none)
	std::vector<Xform> prev, current;
	bool applied;
};
static std::vector<BoneSnap> gPedBones;

static void
GetRw(Xform &x, const RwMatrix *m)
{
	x.right = CVector(m->right.x, m->right.y, m->right.z);
	x.forward = CVector(m->up.x, m->up.y, m->up.z);
	x.up = CVector(m->at.x, m->at.y, m->at.z);
	x.pos = CVector(m->pos.x, m->pos.y, m->pos.z);
}

static void
PutRw(const Xform &x, RwMatrix *m)
{
	m->right.x = x.right.x; m->right.y = x.right.y; m->right.z = x.right.z;
	m->up.x = x.forward.x; m->up.y = x.forward.y; m->up.z = x.forward.z;
	m->at.x = x.up.x; m->at.y = x.up.y; m->at.z = x.up.z;
	m->pos.x = x.pos.x; m->pos.y = x.pos.y; m->pos.z = x.pos.z;
}

static RpHAnimHierarchy*
PedHierarchy(CEntity *e)
{
	if (e == nil || e->m_rwObject == nil || RwObjectGetType(e->m_rwObject) != rpCLUMP)
		return nil;
	RpHAnimHierarchy *hier = GetAnimHierarchyFromSkinClump((RpClump*)e->m_rwObject);
	return hier && hier->matrices && hier->numNodes > 0 ? hier : nil;
}

static void
SnapshotBones(void)
{
	CPedPool *pool = CPools::GetPedPool();
	if (pool == nil)
		return;
	gPedBones.resize(pool->GetSize());
	for (int i = 0; i < pool->GetSize(); i++) {
		BoneSnap &b = gPedBones[i];
		b.applied = false;
		b.hier = PedHierarchy(pool->GetSlot(i));
		if (b.hier == nil)
			continue;
		b.prev.resize(b.hier->numNodes);
		for (int n = 0; n < b.hier->numNodes; n++)
			GetRw(b.prev[n], &b.hier->matrices[n]);
	}
}

// After Apply(peds): only for peds whose body was blended (same slot, continuous).
static void
ApplyBones(float alpha)
{
	static int off = -1;
	if (off < 0)
		off = getenv("REVC_NO_BONE_INTERP") != nil;
	if (off)
		return;
	CPedPool *pool = CPools::GetPedPool();
	if (pool == nil || (int)gPedBones.size() != pool->GetSize() || gPeds.slots == nil)
		return;
	for (int i = 0; i < pool->GetSize(); i++) {
		BoneSnap &b = gPedBones[i];
		if (!gPeds.slots[i].applied || b.hier == nil || PedHierarchy(pool->GetSlot(i)) != b.hier ||
		    (int)b.prev.size() != b.hier->numNodes)
			continue;
		b.current.resize(b.hier->numNodes);
		// Like bodies: no blending across a jump. When a ped is knocked over its skeleton can
		// change pose abruptly between two steps; blending across that drew peds thrown metres
		// into the air (pedhit test: heads 12 m up at 320 fps). Then the ped's bones are drawn
		// as the step left them.
		bool continuous = true;
		for (int n = 0; n < b.hier->numNodes; n++) {
			GetRw(b.current[n], &b.hier->matrices[n]);
			if (!Continuous(b.prev[n], b.current[n], 1.0f, 0.0f))
				continuous = false;
		}
		if (!continuous)
			continue;
		for (int n = 0; n < b.hier->numNodes; n++)
			PutRw(Blend(b.prev[n], b.current[n], alpha), &b.hier->matrices[n]);
		b.applied = true;
	}
}

static void
RestoreBones(void)
{
	CPedPool *pool = CPools::GetPedPool();
	for (size_t i = 0; i < gPedBones.size(); i++) {
		BoneSnap &b = gPedBones[i];
		if (!b.applied)
			continue;
		b.applied = false;
		if (pool == nil || (int)i >= pool->GetSize() || PedHierarchy(pool->GetSlot(i)) != b.hier)
			continue;
		for (int n = 0; n < b.hier->numNodes; n++)
			PutRw(b.current[n], &b.hier->matrices[n]);
	}
}
static bool gHavePrev;
static Xform gCamPrev, gCamCurrent;
static CVector gCamPosCurrent;
static bool gCamApplied;

template <class P>
static void
Snapshot(P *pool, PoolSnap &snap)
{
	if (pool == nil)
		return;
	if (snap.slots == nil || snap.size != pool->GetSize()) {
		delete[] snap.slots;
		snap.size = pool->GetSize();
		snap.slots = new Slot[snap.size];
	}
	for (int i = 0; i < snap.size; i++) {
		CEntity *e = pool->GetSlot(i);
		snap.slots[i].entity = e;
		snap.slots[i].applied = false;
		if (e)
			snap.slots[i].prev.Get(e->GetMatrix());
	}
}

static void
PushEntity(CEntity *e)
{
	e->GetMatrix().UpdateRW();
	e->UpdateRwFrame();
}

template <class P>
static void
Apply(P *pool, PoolSnap &snap, float alpha)
{
	if (pool == nil || snap.slots == nil || snap.size != pool->GetSize())
		return;
	for (int i = 0; i < snap.size; i++) {
		Slot &s = snap.slots[i];
		CEntity *e = pool->GetSlot(i);
		if (e == nil || e != s.entity || e->m_rwObject == nil)
			continue;
		s.current.Get(e->GetMatrix());
		if (!Continuous(s.prev, s.current, 10.0f, 0.0f))
			continue;
		Blend(s.prev, s.current, alpha).Put(e->GetMatrix());
		PushEntity(e);
		s.applied = true;
	}
}

template <class P>
static void
Restore(P *pool, PoolSnap &snap)
{
	if (pool == nil || snap.slots == nil)
		return;
	for (int i = 0; i < snap.size; i++) {
		Slot &s = snap.slots[i];
		if (!s.applied)
			continue;
		s.applied = false;
		CEntity *e = pool->GetSlot(i);
		if (e == nil || e != s.entity)
			continue;
		s.current.Put(e->GetMatrix());
		PushEntity(e);
	}
}

// The camera: its game matrix, derived values (culling planes) and the RenderWare frame.
static void
PushCamera(void)
{
	TheCamera.CalculateDerivedValues();
	RwFrame *frame = RwCameraGetFrame(TheCamera.m_pRwCamera);
	*RwMatrixGetPos(RwFrameGetMatrix(frame)) = TheCamera.GetPosition();
	*RwMatrixGetAt(RwFrameGetMatrix(frame)) = TheCamera.GetForward();
	*RwMatrixGetUp(RwFrameGetMatrix(frame)) = TheCamera.GetUp();
	*RwMatrixGetRight(RwFrameGetMatrix(frame)) = TheCamera.GetRight();
	RwMatrixUpdate(RwFrameGetMatrix(frame));
	RwFrameUpdateObjects(frame);
}

void
BeginStep(void)
{
	Snapshot(CPools::GetPedPool(), gPeds);
	SnapshotBones();
	Snapshot(CPools::GetVehiclePool(), gVehicles);
	Snapshot(CPools::GetObjectPool(), gObjects);
	gCamPrev.Get(TheCamera.GetMatrix());
	gHavePrev = true;
}

// Regression harness (REVC_TEST=motion): the player's body and one bone as this frame draws
// them, in world space, so the test can measure how smoothly they move from frame to frame.
static float gMotionSample[6];
static void RecordMotion(void);

void
ApplyForRender(float alpha)
{
	gCamApplied = false;
	if (!bEnabled || !gHavePrev || alpha <= 0.0f || alpha >= 1.0f) {
		RecordMotion();
		return;
	}
	if (TheCamera.m_pRwCamera) {
		gCamCurrent.Get(TheCamera.GetMatrix());
		gCamPosCurrent = TheCamera.m_vecGameCamPos;
		if (Continuous(gCamPrev, gCamCurrent, 5.0f, 0.7f)) {
			Xform b = Blend(gCamPrev, gCamCurrent, alpha);
			b.Put(TheCamera.GetMatrix());
			TheCamera.m_vecGameCamPos = b.pos;
			PushCamera();
			gCamApplied = true;
		}
	}
	Apply(CPools::GetPedPool(), gPeds, alpha);
	ApplyBones(alpha);
	Apply(CPools::GetVehiclePool(), gVehicles, alpha);
	Apply(CPools::GetObjectPool(), gObjects, alpha);
	RecordMotion();
}

static void
RecordMotion(void)
{
	CPed *ped = FindPlayerPed();
	if (ped == nil)
		return;
	CVector body = ped->GetPosition();
	CVector bone = body;
	RpHAnimHierarchy *hier = PedHierarchy(ped);
	if (hier) {
		// A bone away from the root (the skeleton's middle node), as the skin shader places it.
		RwMatrix *m = &hier->matrices[hier->numNodes / 2];
		bone = CVector(m->pos.x, m->pos.y, m->pos.z);
		if (hier->flags & rpHANIMHIERARCHYLOCALSPACEMATRICES) {
			RwMatrix *ltm = RwFrameGetLTM(RpClumpGetFrame((RpClump*)ped->m_rwObject));
			bone = CVector(ltm->right.x * m->pos.x + ltm->up.x * m->pos.y + ltm->at.x * m->pos.z + ltm->pos.x,
			               ltm->right.y * m->pos.x + ltm->up.y * m->pos.y + ltm->at.y * m->pos.z + ltm->pos.y,
			               ltm->right.z * m->pos.x + ltm->up.z * m->pos.y + ltm->at.z * m->pos.z + ltm->pos.z);
		}
	}
	gMotionSample[0] = body.x; gMotionSample[1] = body.y; gMotionSample[2] = body.z;
	gMotionSample[3] = bone.x; gMotionSample[4] = bone.y; gMotionSample[5] = bone.z;
}

void
RestoreAfterRender(void)
{
	RestoreBones();
	Restore(CPools::GetPedPool(), gPeds);
	Restore(CPools::GetVehiclePool(), gVehicles);
	Restore(CPools::GetObjectPool(), gObjects);
	if (gCamApplied) {
		gCamCurrent.Put(TheCamera.GetMatrix());
		TheCamera.m_vecGameCamPos = gCamPosCurrent;
		PushCamera();
		gCamApplied = false;
	}
}

void
Reset(void)
{
	gHavePrev = false;
}
}

// REVC_NO_BONE_INTERP=1 turns the bone blending off (to check that the motion test notices).
extern "C" void
revc_test_motion_sample(float *out)
{
	for (int i = 0; i < 6; i++)
		out[i] = Interpolation::gMotionSample[i];
}

#include "common.h"
#include "Interpolation.h"
#include "Pools.h"
#include "Camera.h"
#include "World.h"
#include "Ped.h"
#include "Vehicle.h"
#include "Object.h"

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
	Snapshot(CPools::GetVehiclePool(), gVehicles);
	Snapshot(CPools::GetObjectPool(), gObjects);
	gCamPrev.Get(TheCamera.GetMatrix());
	gHavePrev = true;
}

void
ApplyForRender(float alpha)
{
	gCamApplied = false;
	if (!bEnabled || !gHavePrev || alpha <= 0.0f || alpha >= 1.0f)
		return;
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
	Apply(CPools::GetVehiclePool(), gVehicles, alpha);
	Apply(CPools::GetObjectPool(), gObjects, alpha);
}

void
RestoreAfterRender(void)
{
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

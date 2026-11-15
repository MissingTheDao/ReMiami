#pragma once

// Render interpolation for the fixed-timestep loop (task 8). The game simulates at 30 Hz; frames
// are drawn at the display rate with the camera and every ped, vehicle and object placed between
// their previous and current simulation transforms. See Interpolation.cpp.
namespace Interpolation
{
	// Before each simulation step: remember the current transforms as "previous".
	void BeginStep(void);
	// Before drawing: blend previous -> current by alpha (0..1) and push to RenderWare.
	void ApplyForRender(float alpha);
	// After drawing: put the real (current) simulation transforms back.
	void RestoreAfterRender(void);
	// Forget the previous transforms (new session, loading).
	void Reset(void);
	// The blend for things drawn between the last two game steps besides the transforms (trail
	// copies, particles, HUD fades): the render alpha on a frame drawn between two steps, 1 (the
	// step's own value) otherwise. A step's first draw that follows another step's first draw (30 fps,
	// one frame per step) counts as not interpolated even if float error leaves the alpha above 0,
	// so 30 fps draws exactly what it drew before (R6).
	float DisplayBlend(void);
	// Runtime switch (also for testing); on by default.
	extern bool bEnabled;
}

// The entity whose PreRender is running (renderer/Renderer.cpp), and the owner of each corona and
// bright light it registers: drawn where the owner is drawn (Interpolation.cpp MoveOwnedLights).
extern void *gLightOwner;
extern void *gCoronaOwner[];
extern void *gBrightLightOwner[];

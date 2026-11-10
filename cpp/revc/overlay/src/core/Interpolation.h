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
	// Runtime switch (also for testing); on by default.
	extern bool bEnabled;
}

// The entity whose PreRender is running (renderer/Renderer.cpp), and the owner of each corona and
// bright light it registers: drawn where the owner is drawn (Interpolation.cpp MoveOwnedLights).
extern void *gLightOwner;
extern void *gCoronaOwner[];
extern void *gBrightLightOwner[];

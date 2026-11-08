// Automation hooks for the regression harness (rust/revc/src/testdrive.rs, scripts/test.sh).
// Plain C entry points over game state; they do nothing unless the Rust host is in test mode.
#if defined RW_GL3 && defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "skeleton.h"
#include "platform.h"
#include "crossplatform.h"
#include "Frontend.h"
#include "Timer.h"
#include "World.h"
#include "Camera.h"
#include "PlayerInfo.h"
#include "GenericGameStorage.h"

extern "C" {

// 0 = other (loading, init), 1 = front-end menu, 2 = playing.
int
revc_test_state(void)
{
	switch (gGameState) {
	case GS_FRONTEND: return 1;
	case GS_PLAYING_GAME: return 2;
	default: return 0;
	}
}

unsigned int
revc_test_frame(void)
{
	return CTimer::GetFrameCounter();
}

// Game time in ms (pauses with the game, unlike the frame counter it isn't tied to fps).
unsigned int
revc_test_game_ms(void)
{
	return CTimer::GetTimeInMilliseconds();
}

// Non-zero while the screen is fading or faded (e.g. the fade-in after loading a save).
int
revc_test_fading(void)
{
	return TheCamera.GetFading() || TheCamera.GetScreenFadeStatus() != FADE_0;
}

void
revc_test_quit(void)
{
	RsGlobal.quit = TRUE;
}

// Same steps as choosing a slot in the Load Game menu (Frontend.cpp, MENUPAGE_LOADING_IN_PROGRESS).
// slot is 0-based (GTAVCsf1.b = 0). Returns 0 if not in the front end or the slot is invalid.
int
revc_test_load_slot(int slot)
{
	if (gGameState != GS_FRONTEND)
		return 0;
	FrontEndMenuManager.m_nCurrSaveSlot = slot;
	if (!CheckSlotDataValid(slot))
		return 0;
	FrontEndMenuManager.DoSettingsBeforeStartingAGame();
	FrontEndMenuManager.m_bWantToLoad = true;
	return 1;
}

void
revc_test_player_pos(float *x, float *y, float *z)
{
	CVector v = FindPlayerCoors();
	*x = v.x;
	*y = v.y;
	*z = v.z;
}

}

#endif

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
#include "ModelInfo.h"
#include "SimpleModelInfo.h"
#include "TimeModelInfo.h"
#include "ClumpModelInfo.h"
#include "TxdStore.h"
#include "Text.h"
#include "ColModel.h"
#include "Pad.h"
#include "Game.h"
#include "Streaming.h"
#include "Clock.h"
#include "Weather.h"
#include "Population.h"
#include "CarCtrl.h"
#include "PlayerPed.h"
#include "ColStore.h"
#include "Hud.h"
#include "Font.h"

#ifdef AUDIO_OAL
// Audio effects state (sampman_oal.cpp globals).
extern bool _effectsSupported;
extern bool _usingEFX;
extern int usingEAX, usingEAX3;
extern float _fEffectsLevel;
#endif

extern "C" {

// Reverb state: whether effects are available/enabled and the current effects (reverb send) level.
void
revc_test_audio_fx(int *supported, int *mode, float *level)
{
#ifdef AUDIO_OAL
	*supported = _effectsSupported;
	*mode = usingEAX3 ? 3 : usingEAX ? 1 : _usingEFX ? 2 : 0; // 0 off, 1 EAX, 2 EFX, 3 EAX3
	*level = _fEffectsLevel;
#else
	*supported = 0; *mode = 0; *level = 0.0f;
#endif
}

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

// Pads the game uses as player 1 / player 2 (host joystick slots, -1 = none).
void
revc_test_joy_ids(int *joy1, int *joy2)
{
	*joy1 = PSGLOBAL(joy1id);
	*joy2 = PSGLOBAL(joy2id);
}

// Shakes pad 0 like the game does (CPad::StartShake), with vibration enabled in the settings.
void
revc_test_shake(int durationMs, int freq)
{
	FrontEndMenuManager.m_PrefsUseVibration = true;
	CPad::GetPad(0)->StartShake(durationMs, freq);
}

// Fixed-spot screenshots (plan task 3): puts the player at (x, y) on the main map (on the ground
// when z <= -100) with nothing random around (no peds or traffic, fixed time and weather) and
// points a fixed camera from player + (dx, dy, dz) at the player, with the HUD hidden, like the mission scripts do
// (SET_AREA_VISIBLE, LOAD_SCENE, SET_PLAYER_COORDINATES, SET_FIXED_CAMERA_POSITION,
// POINT_CAMERA_AT_POINT). Returns the ground z used.
float
revc_test_goto_spot(float x, float y, float z, float dx, float dy, float dz, int hour, int minute)
{
	CPopulation::PedDensityMultiplier = 0.0f;
	CCarCtrl::CarDensityMultiplier = 0.0f;
	CGame::currArea = AREA_MAIN_MAP;
	CStreaming::RemoveBuildingsNotInArea(AREA_MAIN_MAP);
	CClock::SetGameClock(hour, minute);
	CWeather::ForceWeatherNow(WEATHER_SUNNY);

	CVector pos(x, y, z <= -100.0f ? 20.0f : z);
	CTimer::Suspend();
	CStreaming::LoadScene(pos);
	CTimer::Resume();
	if (z <= -100.0f) {
		CColStore::EnsureCollisionIsInMemory(CVector2D(x, y));
		pos.z = CWorld::FindGroundZForCoord(x, y);
	}
	// World only: no HUD, radar, help boxes or zone names (they depend on timing).
	CHud::m_Wants_To_Draw_Hud = false;
	CHud::m_HideRadar = true;
	CHud::SetHelpMessage(nil, true);

	CPlayerPed *ped = FindPlayerPed();
	CVector pedPos = pos;
	pedPos.z += ped->GetDistanceFromCentreOfMassToBaseOfModel();
	ped->Teleport(pedPos);
	CWorld::ClearExcitingStuffFromArea(pedPos, 120.0f, true);

	TheCamera.SetCamPositionForFixedMode(pos + CVector(dx, dy, dz), CVector(0.0f, 0.0f, 0.0f));
	TheCamera.TakeControlNoEntity(pedPos, JUMP_CUT, CAMCONTROL_SCRIPT);
	return pos.z;
}

// Shows a help box with every controller button token (CFont BUTTON_ICONS order), to check the
// glyphs of the pad in use (gamepad scenario).
void
revc_test_show_buttons(void)
{
	static wchar text[128];
	AsciiToUnicode("~X~ ~O~ ~Q~ ~T~  ~K~ ~M~ ~A~  ~J~ ~V~ ~C~  ~U~ ~D~ ~<~ ~>~  ~H~ ~L~ ~(~ ~)~", text);
	CHud::SetHelpMessage(text, false, true);	// not "quick": those fade out after 1.5 s
}

// Controls menu "BUTTON PROMPTS" setting (skel/host/glyphs.cpp), as if chosen in the menu.
extern int8 gButtonPromptsOverride;
void
revc_test_set_prompt_style(int style)
{
	gButtonPromptsOverride = style;
}

// Opens the Controls page of the front-end menu (boot scenario screenshot).
void
revc_test_show_controls_menu(void)
{
	FrontEndMenuManager.m_nCurrScreen = MENUPAGE_CONTROLLER_PC;
	FrontEndMenuManager.m_nCurrOption = 0;
}

// 1 if the Controls page has the BUTTON PROMPTS entry and its label text resolves (boot scenario).
int
revc_test_check_prompt_menu(void)
{
	bool found = false;
	for (int i = 0; i < NUM_MENUROWS; i++)
		if (strcmp(aScreens[MENUPAGE_CONTROLLER_PC].m_aEntries[i].m_EntryName, "RVC_GLY") == 0)
			found = true;
	char text[64];
	strncpy(text, UnicodeToAscii(TheText.Get("RVC_GLY")), sizeof(text) - 1);
	text[sizeof(text) - 1] = '\0';
	return found && strcmp(text, "BUTTON PROMPTS") == 0;
}

// Loads mission text `table` like the LOAD_MISSION_TEXT script command and returns the string for
// `key` (formats scenario); `restore` reloads the mission text that was loaded before.
const wchar *
revc_test_mission_text(const char *table, const char *key)
{
	char name[8];
	strncpy(name, table, sizeof(name));
	TheText.LoadMissionText(name);
	return TheText.Get(key);
}

void
revc_test_restore_mission_text(const char *previous)
{
	if (previous[0]) {
		char name[8];
		strncpy(name, previous, sizeof(name));
		TheText.LoadMissionText(name);
	}
}

void
revc_test_current_mission_text(char *out8)
{
	TheText.GetNameOfLoadedMissionText(out8);
}

// Removes help text (the save's tutorial script keeps posting tips); called every frame at a spot.
void
revc_test_clear_help(void)
{
	CHud::SetHelpMessage(nil, true);
}

// Writes what the C++ loaders hold after loading (model info from IDE, collision, GXT, geometry of
// loaded models, texture sizes of loaded TXDs) as tab-separated lines (names may contain spaces), for the Rust formats check
// (rust/revc/src/formatcheck.rs) to compare against revc-formats. Returns 0 if the file can't be
// written.
static void
DumpGeometry(FILE *f, const char *model, RpAtomic *atomic)
{
	RpGeometry *geo = RpAtomicGetGeometry(atomic);
	if (geo == nil)
		return;
	int numMats = geo->matList.numMaterials; // fakerw declares RpGeometryGetNumMaterials but has no body
	fprintf(f, "GEO\t%s\t%d\t%d\t%d\t", model, RpGeometryGetNumVertices(geo), RpGeometryGetNumTriangles(geo), numMats);
	bool any = false;
	for (int i = 0; i < numMats; i++) {
		RwTexture *tex = RpMaterialGetTexture(RpGeometryGetMaterial(geo, i));
		if (tex) {
			fprintf(f, "%s%s", any ? "|" : "", RwTextureGetName(tex));
			any = true;
		}
	}
	fprintf(f, "%s\n", any ? "" : "-");
}

struct DumpCtx { FILE *f; const char *name; };

static RpAtomic *
DumpClumpAtomic(RpAtomic *atomic, void *data)
{
	DumpCtx *ctx = (DumpCtx*)data;
	DumpGeometry(ctx->f, ctx->name, atomic);
	return atomic;
}

static RwTexture *
DumpTexture(RwTexture *tex, void *data)
{
	DumpCtx *ctx = (DumpCtx*)data;
	RwRaster *ras = RwTextureGetRaster(tex);
	fprintf(ctx->f, "TEX\t%s\t%s\t%d\t%d\n", ctx->name, RwTextureGetName(tex),
	        ras ? RwRasterGetWidth(ras) : 0, ras ? RwRasterGetHeight(ras) : 0);
	return tex;
}

int
revc_test_dump_formats(const char *path)
{
	FILE *f = fopen(path, "w");
	if (f == nil)
		return 0;
	for (int id = 0; id < MODELINFOSIZE; id++) {
		CBaseModelInfo *mi = CModelInfo::GetModelInfo(id);
		if (mi == nil)
			continue;
		int type = mi->GetModelType();
		int slot = mi->GetTxdSlot();
		const char *txd = slot >= 0 && CTxdStore::GetSlot(slot) ? CTxdStore::GetTxdName(slot) : "-";
		fprintf(f, "MI\t%d\t%d\t%s\t%s", id, type, mi->GetModelName(), txd);
		if (type == MITYPE_SIMPLE || type == MITYPE_TIME) {
			CSimpleModelInfo *smi = (CSimpleModelInfo*)mi;
			fprintf(f, "\t%d\t%.9g\t%.9g\t%.9g", smi->m_numAtomics, smi->m_lodDistances[0],
			        smi->m_lodDistances[1], smi->m_lodDistances[2]);
			if (type == MITYPE_TIME)
				fprintf(f, "\t%d\t%d", ((CTimeModelInfo*)mi)->GetTimeOn(), ((CTimeModelInfo*)mi)->GetTimeOff());
		}
		fprintf(f, "\n");

		CColModel *col = mi->GetColModel();
		if (col && mi->DoesOwnColModel())
			fprintf(f, "COL\t%s\t%d\t%d\t%d\t%d\t%.9g\t%d\n", mi->GetModelName(), col->level, col->numSpheres,
			        col->numBoxes, col->numTriangles, col->boundingSphere.radius, type);

		if (mi->IsSimple()) {
			CSimpleModelInfo *smi = (CSimpleModelInfo*)mi;
			// m_atomics[2] doubles as the related-LOD model / weapon id, so only 0 and 1 are dumped.
			for (int i = 0; i < smi->m_numAtomics && i < 2; i++)
				if (smi->m_atomics[i])
					DumpGeometry(f, mi->GetModelName(), smi->m_atomics[i]);
		} else if (mi->IsClump() && mi->GetRwObject()) {
			DumpCtx ctx = { f, mi->GetModelName() };
			RpClumpForAllAtomics((RpClump*)mi->GetRwObject(), DumpClumpAtomic, &ctx);
		}
	}
	for (int slot = 0; slot < CTxdStore::GetNumSlots(); slot++) {
		TxdDef *def = CTxdStore::GetSlot(slot);
		if (def == nil || def->texDict == nil)
			continue;
		DumpCtx ctx = { f, def->name };
		RwTexDictionaryForAllTextures(def->texDict, DumpTexture, &ctx);
	}
	for (int i = 0; i < TheText.GetNumKeys(); i++) {
		const char *key = TheText.GetKeyName(i);
		fprintf(f, "GXT\t%.8s", key);
		for (wchar *c = TheText.Get(key); *c; c++)
			fprintf(f, "\t%x", *c);
		fprintf(f, "\n");
	}
	fclose(f);
	return 1;
}

}

#endif

// Automation hooks for the regression harness (rust/revc/src/testdrive.rs, scripts/test.sh).
// Plain C entry points over game state; they do nothing unless the Rust host is in test mode.
#if defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "skeleton.h"
#include "platform.h"
#include "crossplatform.h"
#include "Frontend.h"
#include "ControllerConfig.h"
extern "C" int PadLayoutCurrent(void);
void PadPictureContextPress(int8 action);	// core/PadPicture.cpp
extern "C" void PadLayoutApply(int layout);
#include "Timer.h"
#include "World.h"
#include "Camera.h"
#include "PlayerInfo.h"
#include "GenericGameStorage.h"
#include "Script.h"
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
#include "Timecycle.h"
#include "Population.h"
#include "CarCtrl.h"
#include "PlayerPed.h"
#include "ColStore.h"
#include "Hud.h"
#include "Font.h"
#include "GenericGameStorage.h"

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

// Counts as player 1 input now: the player's idle animations (CPlayerPed::PlayIdleAnimations,
// random, after 30 s without input) stay off, so screenshots don't depend on them.
void
revc_test_touch_input(void)
{
	CPad::GetPad(0)->LastTimeTouched = CTimer::GetTimeInMilliseconds();
}

// Saves to slot (0-based) like the pause menu's Save Game, then rebuilds the slot list. Returns
// SaveSlot's result (0 = saved); state = Slots[slot], name/date = the slot's lines (260/70 units).
int
revc_test_save_slot(int slot, int *state, wchar *name, wchar *date)
{
	int r = PcSaveHelper.SaveSlot(slot);
	PcSaveHelper.PopulateSlotInfo();
	*state = Slots[slot];
	memcpy(name, SlotFileName[slot], sizeof(SlotFileName[slot]));
	memcpy(date, SlotSaveDate[slot], sizeof(SlotSaveDate[slot]));
	return r;
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

// Console reference scenes (plan task 13, Claude outputs/world-captures/SCENES.md): the player stands
// at (x, y) on the ground facing `heading` degrees (0 = north, counter-clockwise like the game), the
// normal follow camera directly behind him, HUD and radar on, the clock at hour:minute and the
// weather forced (0 sunny, 1 cloudy, 2 rainy, 3 foggy). No peds or traffic, so runs repeat.
float
revc_test_scene(float x, float y, float heading, int hour, int minute, int weather)
{
	CPopulation::PedDensityMultiplier = 0.0f;
	CCarCtrl::CarDensityMultiplier = 0.0f;
	CGame::currArea = AREA_MAIN_MAP;
	CStreaming::RemoveBuildingsNotInArea(AREA_MAIN_MAP);
	// The save starts inside the hotel: drop the interior's time-cycle colours (as leaving
	// through a door does), or the outside is lit like a room.
	CTimeCycle::StopExtraColour(false);
	CClock::SetGameClock(hour, minute);
	CWeather::ForceWeatherNow(weather);

	CVector pos(x, y, 20.0f);
	CTimer::Suspend();
	CStreaming::LoadScene(pos);
	CTimer::Resume();
	CColStore::EnsureCollisionIsInMemory(CVector2D(x, y));
	pos.z = CWorld::FindGroundZForCoord(x, y);

	CHud::m_Wants_To_Draw_Hud = true;
	CHud::m_HideRadar = false;
	CHud::SetHelpMessage(nil, true);

	CPlayerPed *ped = FindPlayerPed();
	CVector pedPos = pos;
	pedPos.z += ped->GetDistanceFromCentreOfMassToBaseOfModel();
	ped->Teleport(pedPos);
	float h = DEGTORAD(heading);
	ped->m_fRotationCur = ped->m_fRotationDest = h;
	ped->SetHeading(h);
	CWorld::ClearExcitingStuffFromArea(pedPos, 120.0f, true);

	TheCamera.RestoreWithJumpCut();
	TheCamera.SetCameraDirectlyBehindForFollowPed_CamOnAString();
	return pos.z;
}

// Menu comparison shots (plan task 13, REVC_TEST=menus): with the pause menu open over a loaded
// game, sets one state: the page (switched as the menu does), the selected row, the controller
// picture's context (0 foot, 1 car, 2 flying; -1 leave), the pad layout (-1 leave), the map legend
// (-1 leave). Returns 0 if the menu isn't open yet.
extern "C" void PadPictureSetContext(int context);
// Menu walk-through clips: one menu input as the player gives it (down: next row; change: left
// or right on the row, e.g. the radio picker).
void
revc_test_menu_input(int down, int change)
{
	if (FrontEndMenuManager.m_bMenuActive)
		FrontEndMenuManager.ProcessUserInput(down, 0, 0, 0, change);
}

int
revc_test_menu_page_id(const char *name)
{
	static const struct { const char *n; int p; } pages[] = {
		{ "stats", MENUPAGE_STATS }, { "brief", MENUPAGE_BRIEFS }, { "audio", MENUPAGE_SOUND_SETTINGS },
		{ "display", MENUPAGE_DISPLAY_SETTINGS }, { "graphics", MENUPAGE_GRAPHICS_SETTINGS }, { "map", MENUPAGE_MAP },
		{ "new_game_confirm", MENUPAGE_NEW_GAME_RELOAD }, { "load", MENUPAGE_CHOOSE_LOAD_SLOT },
		{ "load_confirm", MENUPAGE_LOAD_SLOT_CONFIRM }, { "save", MENUPAGE_CHOOSE_SAVE_SLOT },
		{ "controls", MENUPAGE_CONTROLLER_PC }, { "pad", MENUPAGE_PAD_PICTURE }, { "pause", MENUPAGE_PAUSE_MENU },
		{ "console_display", MENUPAGE_CONSOLE_DISPLAY }, { "console_controls", MENUPAGE_CONSOLE_CONTROLS },
		{ "new_game", MENUPAGE_NEW_GAME },
		{ "console_options", MENUPAGE_CONSOLE_OPTIONS },
	};
	for (auto &e : pages)
		if (strcmp(e.n, name) == 0)
			return e.p;
	return -1;
}
int
revc_test_menu_state(int page, int option, int context, int layout, int legend)
{
	if (!FrontEndMenuManager.m_bMenuActive) {
		if (!FrontEndMenuManager.m_bGameNotLoaded)
			FrontEndMenuManager.m_bStartUpFrontEndRequested = true;
		return 0;
	}
	{	// REVC_TEST_BRIEF_LISTS: Xbox list entries for the shots (core/BriefLists.cpp), once
		static bool filled = false;
		void BriefListsTestFill(void);
		if (!filled) { filled = true; BriefListsTestFill(); }
	}
	if (layout >= 0)
		PadLayoutApply(layout);
	// context: the controller picture's context, or on Stats/Brief the Xbox sub-tab
	void MenuSkinXboxSetSubTab(int page, int tab);
	void MenuSkinXboxOptionsFor(int section);
	if (context >= 0 && (page == MENUPAGE_STATS || page == MENUPAGE_BRIEFS))
		MenuSkinXboxSetSubTab(page, context);
	else if (context >= 0 && page == MENUPAGE_CONSOLE_OPTIONS)	// the Xbox's "advanced" list of a section
		MenuSkinXboxOptionsFor(context);
	else if (context >= 0)
		PadPictureSetContext(context);
	if (legend >= 0)
		FrontEndMenuManager.m_PrefsShowLegends = legend;
	if (FrontEndMenuManager.m_nCurrScreen != page)
		FrontEndMenuManager.SwitchToNewScreen(page);
	if (FrontEndMenuManager.m_nCurrOption != option) {
		// from the top, moving down as the player does (so long pages scroll the row into view)
		FrontEndMenuManager.m_nCurrOption = 0;
		FrontEndMenuManager.m_nFirstVisibleRowOnList = 0;
		for (int k = 0; k < 16 && FrontEndMenuManager.m_nCurrOption < option; k++)
			FrontEndMenuManager.ProcessUserInput(1, 0, 0, 0, 0);
		FrontEndMenuManager.m_nCurrOption = option;
	}
	return 1;
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
	// The save starts inside the hotel: drop the interior's time-cycle colours (as leaving
	// through a door does), or the outside is lit like a room.
	CTimeCycle::StopExtraColour(false);
	CClock::SetGameClock(hour, minute);
	// REVC_TEST_RAIN=1 (not in the regression set): rain at the spots, for the screen droplet check
	CWeather::ForceWeatherNow(getenv("REVC_TEST_RAIN") ? WEATHER_RAINY : WEATHER_SUNNY);

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
	// With REVC_TEST_RAIN the camera looks up into the rain (the game only sprays screen drops
	// when the camera looks level or up), at a point high above the player.
	TheCamera.TakeControlNoEntity(getenv("REVC_TEST_RAIN") ? pedPos + CVector(0.0f, 0.0f, 80.0f) : pedPos, JUMP_CUT, CAMCONTROL_SCRIPT);
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
	// REVC_TEST_MENU_PAGE=n / REVC_TEST_MENU_OPTION=k: another page/entry (checking menu layouts).
	const char *page = getenv("REVC_TEST_MENU_PAGE"), *option = getenv("REVC_TEST_MENU_OPTION");
	// REVC_TEST_PAD_LAYOUT=0/1: Standard/Classic pad bindings first.
	if (const char *layout = getenv("REVC_TEST_PAD_LAYOUT")) {
		PadLayoutApply(atoi(layout));
	}
	// REVC_TEST_PAD_CONTEXT=1: the pad picture's In Car side, 2 Flying.
	// (2: the Flying side; pressed once per call, the in-game capture calls this twice)
	static bool contextDone;
	if (getenv("REVC_TEST_PAD_CONTEXT") && !contextDone) {
		contextDone = true;
		for (int n = atoi(getenv("REVC_TEST_PAD_CONTEXT")); n > 0; n--)
			PadPictureContextPress(FEOPTION_ACTION_SELECT);
	}
	int revc_test_menu_page_id(const char *name);
	int named = page ? revc_test_menu_page_id(page) : -1;
	int target = !page ? MENUPAGE_CONTROLLER_PC : named >= 0 ? named : atoi(page);
	// In a game with the menu open: switch as the menu does (its page transition sets the panel)
	if (FrontEndMenuManager.m_bMenuActive && !FrontEndMenuManager.m_bGameNotLoaded && FrontEndMenuManager.m_nCurrScreen != target)
		FrontEndMenuManager.SwitchToNewScreen(target);
	else
		FrontEndMenuManager.m_nCurrScreen = target;
	FrontEndMenuManager.m_nCurrOption = option ? atoi(option) : 0;
	// In a game (REVC_TEST_MENU_INGAME): open the pause menu first; the driver calls this again
	// once it is open to pick the page.
	if (!FrontEndMenuManager.m_bMenuActive && !FrontEndMenuManager.m_bGameNotLoaded)
		FrontEndMenuManager.m_bStartUpFrontEndRequested = true;
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

// Controller layouts (boot scenario): the test config is recognised as ORIGINAL PC, and each preset
// applies and is recognised again (Fire on B for original PC and Xbox Classic, on the right trigger
// for Xbox Standard). Returns 1 if so; leaves Standard applied.
extern "C" int
revc_test_check_pad_layouts(void)
{
	// The test config holds re3's old pad bindings: ORIGINAL PC.
	bool original = PadLayoutCurrent() == 2;
	PadLayoutApply(2);
	original = original && PadLayoutCurrent() == 2 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 1 &&
		ControlsManager.GetControllerKeyAssociatedWithAction(GO_FORWARD, JOYSTICK) == 13;
	PadLayoutApply(1);
	bool classic = PadLayoutCurrent() == 1 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 1;
	PadLayoutApply(0);
	bool standard = PadLayoutCurrent() == 0 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 6 &&
		ControlsManager.GetControllerKeyAssociatedWithAction(VEHICLE_ACCELERATE, JOYSTICK) == 6;
	return original && classic && standard;
}

// Loads and frees every texture dictionary in the CD image that isn't loaded yet (as the PC
// game's CreateTxdImageForVideoCard walks them), so REVC_TXD=check compares every game texture.
// Returns how many were loaded.
extern "C" int
revc_test_load_all_txds(void)
{
	int n = 0;
	CStreaming::FlushRequestList();
	for (int i = 0; i < TXDSTORESIZE; i++) {
		if (CTxdStore::GetSlot(i) == nil || !CStreaming::IsObjectInCdImage(i + STREAM_OFFSET_TXD) ||
		    CStreaming::HasTxdLoaded(i))
			continue;
		CStreaming::RequestTxd(i, STREAMFLAGS_KEEP_IN_MEMORY);
		CStreaming::RequestModelStream(0);
		CStreaming::FlushChannels();
		if (CTxdStore::GetSlot(i)->texDict)
			n++;
		CStreaming::RemoveTxd(i);
		CStreaming::FlushRequestList();
	}
	return n;
}

static RwTexture *
DestroyTex(RwTexture *tex, void *count)
{
	(*(int*)count)++;
	RwTextureDestroy(tex);
	return tex;
}

// Reads one texture dictionary from memory through the game's reader (RwTexDictionaryGtaStreamRead,
// so REVC_TXD=check compares it) and frees it. Returns its texture count, -1 if it failed.
extern "C" int
revc_test_read_txd(const unsigned char *data, size_t len)
{
	RwMemory mem;
	mem.start = (RwUInt8*)data;
	mem.length = len;
	RwStream *s = RwStreamOpen(rwSTREAMMEMORY, rwSTREAMREAD, &mem);
	if (s == nil)
		return -1;
	RwTexDictionary *dict = nil;
	if (RwStreamFindChunk(s, rwID_TEXDICTIONARY, nil, nil))
		dict = RwTexDictionaryGtaStreamRead(s);
	RwStreamClose(s, nil);
	if (dict == nil)
		return -1;
	int n = 0;
	RwTexDictionaryForAllTextures(dict, DestroyTex, &n);
	RwTexDictionaryDestroy(dict);
	return n;
}

// Ped-hit check (REVC_TEST=pedhit): a car rolls at a fixed speed into a row of standing peds on
// the airport tarmac; for each ped the highest point it reaches above where it stood is recorded.
// The physics runs in fixed 30 Hz steps, so the result must not depend on the display rate
// (REVC_FIXED_FPS) or on render interpolation (REVC_NO_BONE_INTERP).
#include "Automobile.h"
#include "CivilianPed.h"
#include "Script.h"

#define PEDHIT_NUM 5
static CVehicle *phCar;
static CPed *phPeds[PEDHIT_NUM];
static float phBaseZ[PEDHIT_NUM], phMaxRise[PEDHIT_NUM];
// As drawn (render interpolation applied): highest body position and head bone above their start.
static float phHeadBase[PEDHIT_NUM], phDrawnRise[PEDHIT_NUM], phDrawnHeadRise[PEDHIT_NUM];
static bool phHit[PEDHIT_NUM];
static CVector phCarSpeed, phCarStart;
static uint32 phStartMs;
static void PedHitStep(void);
static void PedHitRender(void);
extern void (*gTestStepHook)(void);
extern void (*gTestRenderHook)(void);

extern "C" int
revc_test_pedhit_setup(float kmh)
{
	// The first candidate whose ground is flat (within 0.3 m) and clear for 25 m towards +y:
	// beach sand, the airport runway, roads.
	static const float cand[][2] = {
		{ 520.0f, -1100.0f }, { 560.0f, -700.0f }, { 480.0f, -1400.0f }, { -1600.0f, -1250.0f },
		{ -1300.0f, -1250.0f }, { 290.0f, -1000.0f },
	};
	float x = 0.0f, y = 0.0f;
	bool found = false;
	for (int c = 0; c < (int)(sizeof(cand)/sizeof(cand[0])) && !found; c++) {
		x = cand[c][0]; y = cand[c][1];
		revc_test_goto_spot(x - 15.0f, y, -100.0f, -10.0f, -12.0f, 8.0f, 12, 0);
		float z0 = CWorld::FindGroundZForCoord(x, y), lo = z0, hi = z0;
		for (float d = 0.0f; d <= 25.0f; d += 1.0f)
			for (float s = -1.5f; s <= 1.5f; s += 1.5f) {
				float z = CWorld::FindGroundZForCoord(x + s, y + d);
				lo = Min(lo, z); hi = Max(hi, z);
			}
		printf("[revc-test] pedhit: candidate (%.0f, %.0f) ground %.2f..%.2f\n", x, y, lo, hi);
		found = hi - lo < 0.3f && z0 > 0.0f;
	}
	if (!found)
		return 0;
	CStreaming::RequestModel(MI_ADMIRAL, STREAMFLAGS_DEPENDENCY);
	CStreaming::RequestModel(MI_MALE01, STREAMFLAGS_DEPENDENCY);
	CStreaming::LoadAllRequestedModels(false);
	if (!CStreaming::HasModelLoaded(MI_ADMIRAL) || !CStreaming::HasModelLoaded(MI_MALE01))
		return 0;

	CAutomobile *car = new CAutomobile(MI_ADMIRAL, MISSION_VEHICLE);
	CVector pos(x, y, CWorld::FindGroundZForCoord(x, y));
	pos.z += car->GetDistanceFromCentreOfMassToBaseOfModel();
	car->SetPosition(pos);
	car->SetHeading(0.0f);	// facing +y
	CTheScripts::ClearSpaceForMissionEntity(pos, car);
	car->SetStatus(STATUS_ABANDONED);
	car->bIsLocked = true;
	car->AutoPilot.m_nCarMission = MISSION_NONE;
	car->bEngineOn = false;
	CWorld::Add(car);
	phCar = car;
	phCarStart = car->GetPosition();
	phCarSpeed = car->GetForward() * (kmh / 180.0f);	// game speed unit: 50 m/s

	static const float ahead[PEDHIT_NUM] = { 8.0f, 11.0f, 14.0f, 17.0f, 20.0f };
	static const float side[PEDHIT_NUM] = { 0.0f, 0.5f, -0.5f, 0.8f, -0.8f };
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = new CCivilianPed(PEDTYPE_CIVMALE, MI_MALE01);
		ped->CharCreatedBy = MISSION_CHAR;
		ped->bRespondsToThreats = false;
		CVector p(x + side[i], y + ahead[i], 0.0f);
		p.z = CWorld::FindGroundZForCoord(p.x, p.y) + 1.0f;
		ped->SetPosition(p);
		ped->SetOrientation(0.0f, 0.0f, 0.0f);
		CWorld::Add(ped);
		CPopulation::ms_nTotalMissionPeds++;
		phPeds[i] = ped;
		phBaseZ[i] = p.z;
		phMaxRise[i] = phDrawnRise[i] = phDrawnHeadRise[i] = -100.0f;
		phHeadBase[i] = 0.0f;	// set on the first drawn frame
		phHit[i] = false;
	}
	phStartMs = CTimer::GetTimeInMilliseconds();
	gTestStepHook = PedHitStep;
	gTestRenderHook = PedHitRender;
	return 1;
}

// Once per game step (gTestStepHook, before CGame::Process): keeps the car at its speed for the
// first 2 s, then records the peds' heights.
static void
PedHitStep(void)
{
	uint32 t = CTimer::GetTimeInMilliseconds() - phStartMs;
	if (phCar && t < 4000) {
		phCar->SetIsStatic(false);	// resting physics objects sleep until disturbed
		phCar->SetMoveSpeed(phCarSpeed);
	}
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = phPeds[i];
		if (ped == nil)	// mission peds stay for the few seconds the check runs
			continue;
		float rise = ped->GetPosition().z - phBaseZ[i];
		if (rise > phMaxRise[i])
			phMaxRise[i] = rise;
		if (ped->m_fHealth < 100.0f || ped->GetPedState() == PED_FALL || ped->GetPedState() == PED_DIE ||
		    ped->GetPedState() == PED_DEAD)
			phHit[i] = true;
	}
}

static void
PedHitRender(void)
{
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = phPeds[i];
		if (ped == nil || ped->GetClump() == nil)
			continue;
		float body = ped->GetPosition().z - phBaseZ[i];
		if (body > phDrawnRise[i])
			phDrawnRise[i] = body;
		float head = ped->GetNodePosition(PED_HEAD).z;
		if (phHeadBase[i] == 0.0f)
			phHeadBase[i] = head;
		if (head - phHeadBase[i] > phDrawnHeadRise[i])
			phDrawnHeadRise[i] = head - phHeadBase[i];
	}
}

// Called every drawn frame by the test driver; returns the game ms since setup.
extern "C" int
revc_test_pedhit_sample(void)
{
	return (int)(CTimer::GetTimeInMilliseconds() - phStartMs);
}

extern "C" void
revc_test_pedhit_drawn(float *bodyRise, float *headRise, int *hit)
{
	for (int i = 0; i < PEDHIT_NUM; i++) {
		bodyRise[i] = phDrawnRise[i];
		headRise[i] = phDrawnHeadRise[i];
		hit[i] = phHit[i];
	}
}

extern "C" void
revc_test_pedhit_result(float *maxRise, float *carDist)
{
	*carDist = phCar ? (phCar->GetPosition() - phCarStart).Magnitude() : -1.0f;
	for (int i = 0; i < PEDHIT_NUM; i++)
		maxRise[i] = phMaxRise[i];
}

// Scanner check (REVC_TEST=scanner): the player in a police car with a wanted level, so the
// police scanner talks (with its radio crackle); the mixer output is recorded (REVC_TEST_WAV).
extern "C" int
revc_test_scanner_setup(void)
{
	const float x = 480.0f, y = -1400.0f;	// beach (flat, see revc_test_pedhit_setup)
	revc_test_goto_spot(x - 10.0f, y, -100.0f, -10.0f, -12.0f, 8.0f, 22, 0);
	CStreaming::RequestModel(MI_POLICE, STREAMFLAGS_DEPENDENCY);
	CStreaming::LoadAllRequestedModels(false);
	if (!CStreaming::HasModelLoaded(MI_POLICE))
		return 0;
	CAutomobile *car = new CAutomobile(MI_POLICE, MISSION_VEHICLE);
	CVector pos(x, y, CWorld::FindGroundZForCoord(x, y));
	pos.z += car->GetDistanceFromCentreOfMassToBaseOfModel();
	car->SetPosition(pos);
	CTheScripts::ClearSpaceForMissionEntity(pos, car);
	car->SetStatus(STATUS_ABANDONED);
	CWorld::Add(car);
	CPlayerPed *player = FindPlayerPed();
	player->SetObjective(OBJECTIVE_ENTER_CAR_AS_DRIVER, car);
	player->WarpPedIntoCar(car);
	player->SetWantedLevel(2);
	// the normal follow camera behind the car (as in play), not goto_spot's fixed one: sounds
	// attached to the car are then centred, as the player hears them
	TheCamera.RestoreWithJumpCut();
	CHud::m_Wants_To_Draw_Hud = true;
	return 1;
}

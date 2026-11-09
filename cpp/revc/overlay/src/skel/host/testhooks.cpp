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

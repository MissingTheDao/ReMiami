// Runs the original CTimeCycle::Initialise on synthetic timecyc.dat files (tests/golden/timecyc/)
// and the game's data/timecyc.dat if given: one line per file with FNV-1a 64 of each of the 52
// hour x weather tables.
#include "common.h"
#include <stdio.h>
#include <string>
#include "main.h"
#include "FileMgr.h"
#define class struct
#include "Timecycle.h"
#undef class
uint8 work_buff[55000];
const char *g_dataFile;
#include "timecyc.inc"

static uint32 rng = 0x7ec7c1e0u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string f(float lo, float hi) { char b[32]; snprintf(b, sizeof b, next() % 3 ? "%.1f" : "%.4f", uni(lo, hi)); return b; }
static std::string i(int lo, int hi) { return std::to_string(lo + (int)(next() % (uint32)(hi - lo + 1))); }
static std::string sp() { return next() % 4 ? " " : (next() % 2 ? "\t" : "   "); }
static void synthetic(const char *path) {
	std::string s;
	bool first = true;
	for (int w = 0; w < 7; w++) {
		s += "//////////// WEATHER " + std::to_string(w) + "\r\n";
		for (int h = 0; h < 24; h++) {
			if (next() % 5 == 0) s += "// comment\r\n";
			if (next() % 9 == 0) s += "\r\n";
			std::string l;
			for (int k = 0; k < 27; k++) l += (k ? sp() : "") + i(-10, 300);
			l += sp() + f(-2, 30) + sp() + f(-1, 20) + sp() + f(-1, 15);
			for (int k = 0; k < 3; k++) l += sp() + i(0, 300);
			l += sp() + f(50, 3000) + sp() + f(-100, 900) + sp() + f(0, 30);
			for (int k = 0; k < 9; k++) l += sp() + i(-5, 290);
			for (int k = 0; k < 7; k++) l += sp() + f(-20, 300);
			// A later line may stop early (its values keep the previous line's); the first is
			// complete, as before it the original's locals are uninitialised.
			if (!first && next() % 8 == 0) l = l.substr(0, l.size() / 4 + next() % (l.size() * 3 / 4));
			first = false;
			s += l + (next() % 3 ? "\r\n" : "\n");
		}
	}
	FILE *fp = fopen(path, "wb"); fwrite(s.data(), 1, s.size(), fp); fclose(fp);
}
static uint64_t fnv(const void *p, size_t n) {
	uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)p;
	for (size_t k = 0; k < n; k++) h = (h ^ b[k]) * 0x100000001b3ull;
	return h;
}
#define T(x) printf(" %016llx", (unsigned long long)fnv(CTimeCycle::x, sizeof(CTimeCycle::x)))
static void run(const char *name, const char *path) {
	g_dataFile = path;
	memset(work_buff, 0, sizeof(work_buff));
	CTimeCycle::Initialise();
	if (getenv("TIMECYC_DEBUG")) { for (int k = 0; k < 7; k++) fprintf(stderr, "%d ", CTimeCycle::m_nAmbientRed[0][k]); fprintf(stderr, "|"); for (int k = 0; k < 24; k++) fprintf(stderr, " %d", CTimeCycle::m_nAmbientRed[k][0]); fprintf(stderr, "\n"); }
	printf("%s =>", name);
	T(m_nAmbientRed); T(m_nAmbientGreen); T(m_nAmbientBlue); T(m_nAmbientRed_Obj); T(m_nAmbientGreen_Obj); T(m_nAmbientBlue_Obj);
	T(m_nAmbientRed_Bl); T(m_nAmbientGreen_Bl); T(m_nAmbientBlue_Bl); T(m_nAmbientRed_Obj_Bl); T(m_nAmbientGreen_Obj_Bl); T(m_nAmbientBlue_Obj_Bl);
	T(m_nDirectionalRed); T(m_nDirectionalGreen); T(m_nDirectionalBlue); T(m_nSkyTopRed); T(m_nSkyTopGreen); T(m_nSkyTopBlue);
	T(m_nSkyBottomRed); T(m_nSkyBottomGreen); T(m_nSkyBottomBlue); T(m_nSunCoreRed); T(m_nSunCoreGreen); T(m_nSunCoreBlue);
	T(m_nSunCoronaRed); T(m_nSunCoronaGreen); T(m_nSunCoronaBlue); T(m_fSunSize); T(m_fSpriteSize); T(m_fSpriteBrightness);
	T(m_nShadowStrength); T(m_nLightShadowStrength); T(m_nPoleShadowStrength); T(m_fFarClip); T(m_fFogStart); T(m_fLightsOnGroundBrightness);
	T(m_nLowCloudsRed); T(m_nLowCloudsGreen); T(m_nLowCloudsBlue); T(m_nFluffyCloudsTopRed); T(m_nFluffyCloudsTopGreen); T(m_nFluffyCloudsTopBlue);
	T(m_nFluffyCloudsBottomRed); T(m_nFluffyCloudsBottomGreen); T(m_nFluffyCloudsBottomBlue); T(m_fBlurRed); T(m_fBlurGreen); T(m_fBlurBlue);
	T(m_fWaterRed); T(m_fWaterGreen); T(m_fWaterBlue); T(m_fWaterAlpha);
	printf("\n");
}
int main(int argc, char **argv) {
	for (int k = 0; k < 12; k++) {
		char path[512], name[32];
		snprintf(name, sizeof name, "%02d.dat", k);
		snprintf(path, sizeof path, "%s/%s", argv[1], name);
		synthetic(path);
		run(name, path);
	}
	if (argc > 2) run("game", argv[2]);
}

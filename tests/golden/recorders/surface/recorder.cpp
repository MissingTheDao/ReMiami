// Runs the original CSurfaceTable (Initialise and the lookups) on synthetic surface.dat files
// (written to tests/golden/surface/) and the game's data/surface.dat if given. One line per file:
// "name => <the 6x6 table> | per surface byte: group, wet multiplier at 0/0.37/1, soft landing,
// adhesive limit against surface*7".
#include "common.h"
#include <stdio.h>
#include <string>
#include "main.h"
#include "FileMgr.h"
#include "Weather.h"
#include "Collision.h"
#define class struct
#include "SurfaceTable.h"
#undef class
uint8 work_buff[55000];
const char *g_dataFile;
#include "surfacetable.inc"

static uint32 rng = 0x51f7ac3bu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string num() {
	char b[64]; float v = uni(0, 9);
	switch (next() % 6) {
	case 0: snprintf(b, sizeof b, "%.1f", v); break;
	case 1: snprintf(b, sizeof b, "%d", (int)v); break;
	case 2: snprintf(b, sizeof b, "%.7f", v); break;
	case 3: return "-";
	case 4: snprintf(b, sizeof b, "%.2fe0", v); break;
	default: snprintf(b, sizeof b, "%.3f", v); break;
	}
	return b;
}
static std::string sep() { static const char *s[] = { " ", "\t", "  ", ",", ", ", "\t\t" }; return s[next() % 6]; }
static void synthetic(const char *path) {
	static const char *names[] = { "Rubber", "Hard", "Road", "Loose", "Sand", "Wet" };
	std::string s = "; synthetic surface.dat\r\n;\r\n";
	for (int l = 0; l < 6; l++) {
		if (next() % 3 == 0) s += "; comment line\r\n";
		if (next() % 4 == 0) s += "\r\n";
		s += std::string(next() % 4 == 0 ? "  " : "") + names[l];
		// Every line has its fields (a short line makes the original read the next line's name).
		for (int f = 0; f <= l; f++) s += sep() + num();
		if (next() % 3 == 0) s += sep() + num(); // extra field: ignored
		s += next() % 3 ? "\r\n" : "\n";
	}
	FILE *f = fopen(path, "wb"); fwrite(s.data(), 1, s.size(), f); fclose(f);
}
static void p(float v) { unsigned u; memcpy(&u, &v, 4); printf(" %08x", u); }
static void run(const char *name, const char *path) {
	g_dataFile = path;
	memset(work_buff, 0, sizeof(work_buff));
	memset(CSurfaceTable::ms_aAdhesiveLimitTable, 0, sizeof(CSurfaceTable::ms_aAdhesiveLimitTable));
	CSurfaceTable::Initialise("surface.dat");
	printf("%s =>", name);
	for (int i = 0; i < 6; i++) for (int j = 0; j < 6; j++) p(CSurfaceTable::ms_aAdhesiveLimitTable[i][j]);
	printf(" |");
	for (int s = 0; s < 256; s++) {
		printf(" %d", CSurfaceTable::GetAdhesionGroup(s));
		for (float w : { 0.0f, 0.37f, 1.0f }) { CWeather::WetRoads = w; p(CSurfaceTable::GetWetMultiplier(s)); }
		printf(" %d", CSurfaceTable::IsSoftLanding(s));
		CColPoint cp; cp.surfaceA = s; cp.surfaceB = (uint8)(s * 7); p(CSurfaceTable::GetAdhesiveLimit(cp));
	}
	printf("\n");
}
int main(int argc, char **argv) {
	for (int i = 0; i < 16; i++) {
		char path[512], name[32];
		snprintf(name, sizeof name, "%02d.dat", i);
		snprintf(path, sizeof path, "%s/%s", argv[1], name);
		synthetic(path);
		run(name, path);
	}
	if (argc > 2) run("game", argv[2]);
}

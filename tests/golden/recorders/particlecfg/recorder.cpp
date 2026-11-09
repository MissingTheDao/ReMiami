// Runs the original cParticleSystemMgr::LoadParticleData on synthetic particle.cfg files
// (tests/golden/particlecfg/) and the game's data/particle.cfg if given: one line per file with
// FNV-1a 64 of each of the 83 entries' bytes up to (not including) the two pointers.
#include "common.h"
#include <stdio.h>
#include <string>
#include "main.h"
#include "FileMgr.h"
#include "extra.h"
#include "ParticleType.h"
#define class struct
#include "ParticleMgr.h"
#undef class
uint8 work_buff[55000];
const char *g_dataFile;
cParticleSystemMgr::cParticleSystemMgr() {}
cParticleSystemMgr mgr;
const char *ParticleFilename = "PARTICLE.CFG";
#include "particlemgr.inc"

static uint32 rng = 0x9a77c1e5u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string fnum() {
	char b[64]; float v = uni(-50, 300);
	switch (next() % 5) {
	case 0: snprintf(b, sizeof b, "%.1f", v); break;
	case 1: snprintf(b, sizeof b, "%d", (int)v); break;
	case 2: snprintf(b, sizeof b, "%.7f", v / 100); break;
	case 3: snprintf(b, sizeof b, "%.3fe-1", v); break;
	default: snprintf(b, sizeof b, "%.9g", v); break;
	}
	return b;
}
static std::string inum() { return std::to_string((int)(next() % 70000) - 200); }
static std::string sep() { static const char *s[] = { " ", "\t", "  ", "\t\t", " \t" }; return s[next() % 5]; }
static void synthetic(const char *path) {
	std::string s = ";synthetic particle.cfg\r\n;\r\n";
	int entries = 0;
	int n = 10 + next() % 70;
	for (int i = 0; i < n && entries < 80; i++) {
		if (next() % 6 == 0) { s += ";comment " + fnum() + "\r\n"; continue; }
		char name[40]; snprintf(name, sizeof name, next() % 10 == 0 ? "LONGPARTICLE%08X" : "PART%u", next() % 1000);
		std::string l = std::string(next() % 6 == 0 ? " " : "") + (name[0] == 'L' ? std::string(name).substr(0, 19) : std::string(name));
		int fields = next() % 5 == 0 ? next() % 40 : 40;
		static const bool isFloat[41] = { 0,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,0,1,0,1,0,0,0,0,1,0,0,1,1,1,0,0,1,1,1,1,1,0 };
		for (int f = 1; f <= fields; f++) l += sep() + (isFloat[f] ? fnum() : inum());
		entries++;
		// Sometimes a second entry on the same line (params wrap after FLAGS).
		if (fields == 40 && next() % 8 == 0 && entries < 80) { l += sep() + "WRAP" + std::to_string(i) + sep() + inum(); entries++; }
		s += l + (next() % 3 ? "\r\n" : "\n");
	}
	s += ";the end\r\n";
	FILE *f = fopen(path, "wb"); fwrite(s.data(), 1, s.size(), f); fclose(f);
}
static void run(const char *name, const char *path) {
	g_dataFile = path;
	memset(work_buff, 0, sizeof(work_buff));
	memset(&mgr, 0, sizeof(mgr));
	mgr.LoadParticleData();
	printf("%s =>", name);
	for (int i = 0; i < MAX_PARTICLES; i++) {
		uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)&mgr.m_aParticles[i];
		for (size_t k = 0; k < offsetof(tParticleSystemData, m_ppRaster); k++) h = (h ^ b[k]) * 0x100000001b3ull;
		printf(" %016llx", (unsigned long long)h);
	}
	printf("\n");
}
int main(int argc, char **argv) {
	static_assert(sizeof(tParticleSystemData) == 160 && offsetof(tParticleSystemData, m_ColorFadeTime) == 124 && MAX_PARTICLES == 83, "layout");
	for (int i = 0; i < 16; i++) {
		char path[512], name[32];
		snprintf(name, sizeof name, "%02d.cfg", i);
		snprintf(path, sizeof path, "%s/%s", argv[1], name);
		synthetic(path);
		run(name, path);
	}
	if (argc > 2) run("game", argv[2]);
}

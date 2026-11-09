// Runs the original CWeaponInfo::Initialise + LoadWeaponData on synthetic weapon.dat files (written
// to tests/golden/weaponinfo/) and the game's data/weapon.dat if given. Per file one line:
// "file <the SetWeaponInfo calls> => <FNV-1a 64 of each of the 37 CWeaponInfo>".
#include "common.h"
#include <stdio.h>
#include <string>
#include "main.h"
#include "FileMgr.h"
#define class struct
#include "WeaponInfo.h"
#undef class
#include "Weapon.h"
#include "ModelInfo.h"
#include "ModelIndices.h"
uint8 work_buff[55000];
const char *g_dataFile;
#include "weaponinfo.inc"

static uint32 rng = 0x13579bdfu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string num(float lo, float hi) {
	char b[64]; float v = uni(lo, hi);
	switch (next() % 7) {
	case 0: snprintf(b, sizeof b, "%d", (int)v); break;
	case 1: snprintf(b, sizeof b, "%.1f", v); break;
	case 2: snprintf(b, sizeof b, "%.6f", v); break;
	case 3: snprintf(b, sizeof b, "%ge1", v / 10); break;
	case 4: snprintf(b, sizeof b, "%.9g", v); break;
	case 5: snprintf(b, sizeof b, "%.3f", v); break;
	default: snprintf(b, sizeof b, "%.2f", v); break;
	}
	return b;
}
static std::string inum(int lo, int hi) { return std::to_string(lo + (int)(next() % (uint32)(hi - lo + 1))); }
static std::string ws() { static const char *s[] = { " ", "\t", "  ", " \t" }; return s[next() % 4]; }
static const char *FIRE[] = { "MELEE", "INSTANT_HIT", "PROJECTILE", "AREA_EFFECT", "CAMERA", "BOGUS" };
static const char *EXTRA_ANIMS[] = { "nosuchgroup", "Colt45", "" };

static void write_synthetic(const char *path) {
	std::string out = "# synthetic weapon.dat for the golden test\n\n";
	int lines = 20 + next() % 60;
	for (int l = 0; l < lines; l++) {
		std::string s;
		switch (next() % 10) {
		case 0: s = "# comment " + num(0, 9); break;
		case 1: s = std::string(next() % 2 ? "   " : "\t"); break;
		default: {
			int w = next() % 40; // 37..39: unknown names
			s = (next() % 5 == 0 ? std::string("  ") : std::string()) + (w < 37 ? CWeaponInfo::ms_aWeaponNames[w] : "NoSuchWeapon");
			s += ws() + FIRE[next() % 6] + ws() + num(0, 200);
			for (int i = 0; i < 4; i++) s += ws() + inum(0, 5000);
			for (int i = 0; i < 7; i++) s += ws() + num(-2, 50);
			int g = next() % (NUM_ANIM_ASSOC_GROUPS + 3);
			s += ws() + std::string(g < NUM_ANIM_ASSOC_GROUPS ? kAnimGroupNames[g] : EXTRA_ANIMS[g - NUM_ANIM_ASSOC_GROUPS]);
			for (int i = 0; i < 7; i++) s += ws() + num(0, 120);
			s += ws() + inum(-1, 400) + ws() + inum(-1, 400);
			char f[32]; snprintf(f, sizeof f, next() % 2 ? "%x" : "0x%X", next() & 0x3fffff); s += ws() + f + ws() + inum(0, 9);
			if (next() % 5 == 0) s = s.substr(0, s.size() / 3 + next() % (s.size() * 2 / 3)); // stops early: later values persist
		}
		}
		if (s.size() > 250) s.resize(250);
		out += s + (next() % 3 ? "\r\n" : "\n");
	}
	out += "ENDWEAPONDATA\r\n";
	if (next() % 2) out += "Colt45 MELEE 1 2 3 4 5 6 7 8 9 1 2 3 colt45 1 2 3 4 5 6 7 8 9 1 2\n";
	FILE *f = fopen(path, "wb"); fwrite(out.data(), 1, out.size(), f); fclose(f);
}

static uint64_t fnv(const void *p, size_t n) {
	uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)p;
	for (size_t i = 0; i < n; i++) h = (h ^ b[i]) * 0x100000001b3ull;
	return h;
}
static void run(const char *name, const char *path) {
	g_dataFile = path;
	memset(aWeaponInfo, 0, sizeof(aWeaponInfo));
	printf("%s", name);
	CWeaponInfo::Initialise();
	printf(" =>");
	for (int i = 0; i < WEAPONTYPE_TOTALWEAPONS; i++) printf(" %016llx", (unsigned long long)fnv(&aWeaponInfo[i], sizeof(CWeaponInfo)));
	printf("\n");
}
int main(int argc, char **argv) {
	for (int i = 0; i < 24; i++) {
		char path[512], name[32];
		snprintf(name, sizeof name, "%02d.dat", i);
		snprintf(path, sizeof path, "%s/%s", argv[1], name);
		write_synthetic(path);
		run(name, path);
	}
	if (argc > 2) run("game", argv[2]);
}

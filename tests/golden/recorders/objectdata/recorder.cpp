// Runs the original CObjectData::Initialise on synthetic object.dat files (tests/golden/objectdata/)
// and the game's data/object.dat if given (there no model exists: only the table is compared).
// One line per file: "name <SetObjectID calls> => <FNV-1a 64 of each of the 210 CObjectInfo>".
#include "common.h"
#include <stdio.h>
#include <string>
#include "main.h"
#include "FileMgr.h"
#include "ModelInfo.h"
#define class struct
#include "ObjectData.h"
#undef class
uint8 work_buff[55000];
const char *g_dataFile;
#include "objectdata.inc"

static uint32 rng = 0x0b1ec7d4u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string num(float lo, float hi) {
	char b[64]; float v = uni(lo, hi);
	switch (next() % 5) {
	case 0: snprintf(b, sizeof b, "%.1f", v); break;
	case 1: snprintf(b, sizeof b, "%d", (int)v); break;
	case 2: snprintf(b, sizeof b, "%.6f", v); break;
	case 3: snprintf(b, sizeof b, "%.9g", v); break;
	default: snprintf(b, sizeof b, "%.3f", v); break;
	}
	return b;
}
static std::string sep() { static const char *s[] = { " ", ", ", ",", "  ", "\t" }; return s[next() % 5]; }
static void synthetic(const char *path) {
	std::string s = "; synthetic object.dat\r\n";
	int n = 20 + next() % 150;
	bool first = true;
	for (int i = 0; i < n; i++) {
		if (next() % 8 == 0) { s += ";" + num(0, 9) + "\r\n"; continue; }
		if (next() % 10 == 0) { s += "\r\n"; continue; }
		char name[32]; snprintf(name, sizeof name, next() % 7 == 0 ? "nomodel%u" : "mdl%u", next() % 420);
		// Mostly default-like values, so the sharing of entries 0..3 happens too.
		bool heavy = next() % 3 == 0;
		std::string l = std::string(next() % 6 == 0 ? "  " : "") + name;
		l += sep() + (heavy ? std::string("99999.0") : num(1, 5000)) + sep() + (heavy ? std::string("99999") : num(1, 9000));
		l += sep() + num(0.9f, 1) + sep() + num(0, 1) + sep() + num(10, 150) + sep() + num(0, 500);
		l += sep() + (heavy ? std::string("1.0") : num(0, 5)) + sep() + std::to_string(heavy ? 0 : next() % 4);
		l += sep() + std::to_string(next() % 2 ? 0 : 4 * (next() % 3)) + sep() + std::to_string(next() % 2);
		// A later line may stop early (its fields keep the entry's values); the first is complete,
		// as before it the original's locals are uninitialised.
		if (!first && next() % 6 == 0) l = l.substr(0, l.size() / 3 + next() % (l.size() * 2 / 3));
		first = false;
		s += l + (next() % 3 ? "\r\n" : "\n");
	}
	s += "*\r\n";
	FILE *f = fopen(path, "wb"); fwrite(s.data(), 1, s.size(), f); fclose(f);
}
static uint64_t fnv(const void *p, size_t n) {
	uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)p;
	for (size_t i = 0; i < n; i++) h = (h ^ b[i]) * 0x100000001b3ull;
	return h;
}
static void run(const char *name, const char *path) {
	g_dataFile = path;
	memset(work_buff, 0, sizeof(work_buff));
	memset(CObjectData::ms_aObjectInfo, 0, sizeof(CObjectData::ms_aObjectInfo));
	printf("%s", name);
	CObjectData::Initialise("object.dat");
	printf(" =>");
	for (int i = 0; i < NUMOBJECTINFO; i++) printf(" %016llx", (unsigned long long)fnv(&CObjectData::ms_aObjectInfo[i], sizeof(CObjectInfo)));
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

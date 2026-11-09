// Runs the original cHandlingDataMgr::LoadHandlingData (and cTransmission::InitGearRatios) on the
// synthetic handling files in tests/golden/handling/ (written here first) and on the game's
// data/handling.cfg if given, and prints a FNV-1a 64 hash of every loaded entry's bytes:
// "file kind index hash" (kind: car bike flying boat). tests/golden/handling.txt.
#include "common.h"
#include <stdio.h>
#include <string>
#include "HandlingMgr.h"
#include "main.h"
#include "FileMgr.h"
#include "Physical.h"
uint8 work_buff[55000];
const char *g_handlingFile;
#include "transmission.inc"
#include "handlingmgr.inc"

static uint32 rng = 0x2468ace1u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }

// A number in one of the forms the parser has to cope with.
static std::string num(float lo, float hi) {
	char b[64]; float v = uni(lo, hi);
	switch (next() % 8) {
	case 0: snprintf(b, sizeof b, "%d", (int)v); break;
	case 1: snprintf(b, sizeof b, "%.1f", v); break;
	case 2: snprintf(b, sizeof b, "%.6f", v); break;
	case 3: snprintf(b, sizeof b, "%ge0", v); break;
	case 4: snprintf(b, sizeof b, "%.3fe%d", v / 10, 1); break;
	case 5: snprintf(b, sizeof b, "%.2fx", v); break; // trailing junk
	case 6: snprintf(b, sizeof b, "%.9g", v); break;
	default: snprintf(b, sizeof b, "%.4f", v); break;
	}
	return b;
}
static std::string sep() { static const char *s[] = { " ", "\t", "  ", " \t ", "\t\t" }; return s[next() % 5]; }
static std::string fields(int n, float lo, float hi) { std::string s; for (int i = 0; i < n; i++) s += sep() + num(lo, hi); return s; }

static void write_synthetic(const char *path) {
	std::string out = "; synthetic handling file for the golden test\n";
	int lines = 40 + next() % 80;
	for (int l = 0; l < lines; l++) {
		int kind = next() % 6;
		int id;
		std::string line;
		switch (kind) {
		case 0: id = 81 + next() % 5; line = "!" + sep() + VehicleNames[id] + fields(next() % 4 == 0 ? next() % 15 : 15, -60, 60); break;
		case 1: id = 96 + next() % 10; line = "$" + sep() + VehicleNames[id] + fields(next() % 4 == 0 ? next() % 18 : 18, -5, 5); break;
		case 2: id = 86 + next() % 11; line = "%" + sep() + VehicleNames[id] + fields(next() % 4 == 0 ? next() % 14 : 14, -3, 3); break;
		case 3: line = "; comment " + num(0, 9); break;
		default: {
			id = next() % 106;
			char flags[32]; snprintf(flags, sizeof flags, next() % 2 ? "%x" : "0x%X", next() & 0x0fffffff);
			line = std::string(VehicleNames[id]);
			line += sep() + num(100, 20000) + fields(6, -4, 6) + sep() + std::to_string(10 + next() % 140) + fields(3, 0, 2);
			line += sep() + std::to_string(1 + next() % 5) + sep() + num(10, 300) + sep() + num(5, 40);
			static const char *dt = "FR4"; line += sep() + std::string(1, dt[next() % 3]);
			static const char *et = "PDE"; line += sep() + std::string(1, et[next() % 3]);
			line += fields(2, 0, 20) + sep() + std::to_string(next() % 2) + fields(5, 0, 50) + sep() + std::to_string(next() % 90000);
			line += fields(4, -1, 1) + sep() + flags + sep() + std::to_string(next() % 4) + sep() + std::to_string(next() % 4);
			if (next() % 6 == 0) { size_t cut = line.size() / 2 + next() % (line.size() / 2); line = line.substr(0, cut); }
		}
		}
		if (line.size() > 200) line.resize(200); // the original copies lines into char[201]
		out += line + (next() % 3 ? "\r\n" : "\n");
	}
	out += ";the end\r\n";
	FILE *f = fopen(path, "wb"); fwrite(out.data(), 1, out.size(), f); fclose(f);
}

static uint64_t fnv(const void *p, size_t n) {
	uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)p;
	for (size_t i = 0; i < n; i++) h = (h ^ b[i]) * 0x100000001b3ull;
	return h;
}

// The manager's private arrays, by layout (5 floats, then the arrays).
struct MgrView {
	float f[5];
	tHandlingData cars[NUMHANDLINGS];
	tBikeHandlingData bikes[NUMBIKEHANDLINGS];
	tFlyingHandlingData flying[NUMFLYINGHANDLINGS];
	tBoatHandlingData boats[NUMBOATHANDLINGS];
};

static void run(const char *name, const char *path) {
	g_handlingFile = path;
	cHandlingDataMgr *mgr = new cHandlingDataMgr;
	mgr->LoadHandlingData();
	MgrView *v = (MgrView *)mgr;
	for (int i = 0; i < NUMHANDLINGS; i++) printf("%s car %d %016llx\n", name, i, (unsigned long long)fnv(&v->cars[i], sizeof(tHandlingData)));
	for (int i = 0; i < NUMBIKEHANDLINGS; i++) printf("%s bike %d %016llx\n", name, i, (unsigned long long)fnv(&v->bikes[i], sizeof(tBikeHandlingData)));
	for (int i = 0; i < NUMFLYINGHANDLINGS; i++) printf("%s flying %d %016llx\n", name, i, (unsigned long long)fnv(&v->flying[i], sizeof(tFlyingHandlingData)));
	for (int i = 0; i < NUMBOATHANDLINGS; i++) printf("%s boat %d %016llx\n", name, i, (unsigned long long)fnv(&v->boats[i], sizeof(tBoatHandlingData)));
	delete mgr;
}

int main(int argc, char **argv) {
	// argv[1]: directory for the synthetic files; argv[2]: the game's handling.cfg (optional).
	static_assert(sizeof(tHandlingData) == 220 && sizeof(tBikeHandlingData) == 64 && sizeof(tFlyingHandlingData) == 76 && sizeof(tBoatHandlingData) == 60, "layout");
	for (int i = 0; i < 16; i++) {
		char path[512], name[32];
		snprintf(name, sizeof name, "%02d.cfg", i);
		snprintf(path, sizeof path, "%s/%s", argv[1], name);
		write_synthetic(path);
		run(name, path);
	}
	if (argc > 2) run("game", argv[2]);
}

// Runs the original CPedType::LoadPedData and CPedStats::LoadPedStats on synthetic ped.dat /
// pedstats.dat files (written to tests/golden/pedtype/) and the game's files if given. The tables
// start zeroed with Initialise's defaults (the original leaves some bytes uninitialised). Per file
// pair one line: "name => <FNV-1a 64 of each CPedType> | <of each CPedStats>".
#include "common.h"
#include <stdio.h>
#include <string>
#include "General.h"
#include "FileMgr.h"
#define class struct
#include "PedType.h"
#undef class
#include "SaveBuf.h"
const char *g_pedFile, *g_statsFile;
#include "pedtype.inc"

static uint32 rng = 0x5eed1234u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static std::string num(float lo, float hi) {
	char b[64]; float v = uni(lo, hi);
	switch (next() % 5) {
	case 0: snprintf(b, sizeof b, "%d", (int)v); break;
	case 1: snprintf(b, sizeof b, "%.1f", v); break;
	case 2: snprintf(b, sizeof b, "%.6f", v); break;
	case 3: snprintf(b, sizeof b, "%.9g", v); break;
	default: snprintf(b, sizeof b, "%.2fe0", v); break;
	}
	return b;
}
static std::string sep() { static const char *s[] = { " ", "\t", ", ", ",", "  " }; return s[next() % 5]; }
static const char *TYPES[] = { "PLAYER1","PLAYER2","PLAYER3","PLAYER4","CIVMALE","CIVFEMALE","COP","GANG1","GANG2","GANG3","GANG4","GANG5","GANG6","GANG7","GANG8","GANG9","EMERGENCY","FIREMAN","CRIMINAL","SPECIAL","PROSTITUTE" };
static const char *FLAGS[] = { "PLAYER1","CIVMALE","COP","GANG1","GANG9","EMERGENCY","FIREMAN","CRIMINAL","SPECIAL","GUN","COP_CAR","FAST_CAR","EXPLOSION","PROSTITUTE","DEADPEDS","NOTAFLAG" };
static void put(const char *path, const std::string &s) { FILE *f = fopen(path, "wb"); fwrite(s.data(), 1, s.size(), f); fclose(f); }
static std::string eol() { return next() % 3 ? "\r\n" : "\n"; }

static void synthetic(const char *ped, const char *stats) {
	std::string p = "# synthetic ped.dat" + eol();
	int n = 10 + next() % 40;
	bool haveType = false;
	for (int i = 0; i < n; i++) {
		int k = next() % 6;
		if (!haveType) k = 0;
		std::string s;
		if (k <= 1) {
			s = std::string(next() % 4 == 0 ? "\t" : "") + TYPES[next() % 21];
			// The first type line is complete: before it the original's numbers are uninitialised.
			int nf = haveType && next() % 5 == 0 ? next() % 5 : 5;
			for (int j = 0; j < nf; j++) s += sep() + num(-5, 200);
			haveType = true;
		} else if (k <= 3) {
			s = std::string(next() % 2 ? "\tThreat" : "Threat");
			if (s[0] == '\t') s = "Threat"; // the original adds 7 to the first non-space column
			int nf = next() % 6; for (int j = 0; j < nf; j++) s += sep() + FLAGS[next() % 16];
		} else if (k == 4) {
			s = "Avoid"; int nf = next() % 6; for (int j = 0; j < nf; j++) s += sep() + FLAGS[next() % 16];
		} else {
			s = next() % 2 ? "# comment" : "   ";
		}
		p += s + eol();
	}
	put(ped, p);
	std::string q = "# synthetic pedstats.dat" + eol();
	int m = 5 + next() % 35; // at most 40 stat lines
	for (int i = 0; i < m; i++) {
		if (next() % 6 == 0) { q += "#" + num(0, 9) + eol(); continue; }
		char name[64]; snprintf(name, sizeof name, next() % 9 == 0 ? "AVERYLONGPEDSTATNAME_%08X" : "STAT_%X", next());
		std::string s = name + sep() + num(0, 60) + sep() + num(0, 30);
		for (int j = 0; j < 4; j++) s += sep() + std::to_string((int)(next() % 300) - 20);
		s += sep() + num(0, 3) + sep() + num(0, 3) + sep() + std::to_string(next() % 70000);
		// Lines may stop early, but not the first: before it the original's numbers are uninitialised.
		if (i > 0 && q.find("STAT") != std::string::npos && next() % 6 == 0) s = s.substr(0, s.size() / 3 + next() % (s.size() * 2 / 3));
		q += s + eol();
	}
	put(stats, q);
}

static uint64_t fnv(const void *p, size_t n) {
	uint64_t h = 0xcbf29ce484222325ull; const uint8 *b = (const uint8 *)p;
	for (size_t i = 0; i < n; i++) h = (h ^ b[i]) * 0x100000001b3ull;
	return h;
}
static void run(const char *name, const char *ped, const char *stats) {
	g_pedFile = ped; g_statsFile = stats;
	static CPedType types[NUM_PEDTYPES];
	static CPedStats st[NUM_PEDSTATS];
	memset(types, 0, sizeof types); memset(st, 0, sizeof st);
	for (int i = 0; i < NUM_PEDTYPES; i++) { CPedType::ms_apPedType[i] = &types[i]; types[i].m_flag = PED_FLAG_PLAYER1; }
	for (int i = 0; i < NUM_PEDSTATS; i++) {
		CPedStats *s = CPedStats::ms_apPedStats[i] = &st[i];
		s->m_name[8] = 'R'; s->m_fleeDistance = 20.0f; s->m_headingChangeRate = 15.0f;
		s->m_fear = s->m_temper = s->m_lawfulness = s->m_sexiness = 50; s->m_attackStrength = s->m_defendWeakness = 1.0f;
	}
	CPedType::LoadPedData();
	CPedStats::LoadPedStats();
	printf("%s =>", name);
	for (int i = 0; i < NUM_PEDTYPES; i++) printf(" %016llx", (unsigned long long)fnv(&types[i], sizeof(CPedType)));
	printf(" |");
	for (int i = 0; i < NUM_PEDSTATS; i++) printf(" %016llx", (unsigned long long)fnv(&st[i], sizeof(CPedStats)));
	printf("\n");
}
int main(int argc, char **argv) {
	static_assert(sizeof(CPedType) == 32 && sizeof(CPedStats) == 52 && NUM_PEDTYPES == 23 && NUM_PEDSTATS == 40, "layout");
	for (int i = 0; i < 24; i++) {
		char ped[512], stats[512], name[16];
		snprintf(name, sizeof name, "%02d", i);
		snprintf(ped, sizeof ped, "%s/%02d.ped", argv[1], i);
		snprintf(stats, sizeof stats, "%s/%02d.stats", argv[1], i);
		synthetic(ped, stats);
		run(name, ped, stats);
	}
	if (argc > 3) run("game", argv[2], argv[3]);
}

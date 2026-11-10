// Runs the original CRestart through random op sequences: tests/golden/restart.txt, one op per
// line "op args => result | every static". Zone levels and the path node fallback are stubs
// (the Rust test uses the same ones).
#include "common.h"
#include <stdio.h>
#include <stdint.h>
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define SQR(x) ((x) * (x))
enum eLevelName { LEVEL_IGNORE = -1, LEVEL_GENERIC = 0, LEVEL_BEACH, LEVEL_MAINLAND };
static int g_zonecalls;
struct CTheZones {
	static eLevelName GetLevelFromPosition(const CVector *v) { g_zonecalls++; return (eLevelName)(v->x < -300.0f ? 1 : v->x > 300.0f ? 2 : 0); }
};
enum { PATH_PED = 1 };
struct CPathNode { CVector p; CVector GetPosition() { return p; } };
struct CPathFind {
	CPathNode m_pathNodes[1];
	int FindNodeClosestToCoors(CVector pos, int, float) { m_pathNodes[0].p = CVector((float)(int)pos.x, (float)(int)pos.y, 7.0f); return 0; }
} ThePaths;
#include "savebuf.inc"
#define printf(...) printf(" msg")
#include "restart.inc"
#undef printf

static uint32 rng = 0x2e57a17bu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static unsigned fb(float f) { unsigned u; memcpy(&u, &f, 4); return u; }
static void v3(const CVector &v) { printf(" %08x %08x %08x", fb(v.x), fb(v.y), fb(v.z)); }
static void dump() {
	printf(" | %d %d %d %d %d", CRestart::OverrideHospitalLevel, CRestart::OverridePoliceStationLevel, CRestart::bFadeInAfterNextArrest,
	       CRestart::bFadeInAfterNextDeath, CRestart::bOverrideRestart);
	v3(CRestart::OverridePosition); printf(" %08x", fb(CRestart::OverrideHeading));
	for (int i = 0; i < NUM_RESTART_POINTS; i++) { v3(CRestart::HospitalRestartPoints[i]); printf(" %08x", fb(CRestart::HospitalRestartHeadings[i])); }
	printf(" %d", CRestart::NumberOfHospitalRestarts);
	for (int i = 0; i < NUM_RESTART_POINTS; i++) { v3(CRestart::PoliceRestartPoints[i]); printf(" %08x", fb(CRestart::PoliceRestartHeadings[i])); }
	printf(" %d\n", CRestart::NumberOfPoliceRestarts);
}
static CVector rpos() {
	if (next() % 3 == 0) return CVector(uni(-6000, 6000), uni(-6000, 6000), uni(-50, 200));
	return CVector(uni(-1500, 1500), uni(-1500, 1500), uni(0, 60));
}
static uint8 saved[512]; static uint32 savedSize;
int main() {
	for (int s = 0; s < 8; s++) {
		printf("init =>"); CRestart::Initialise(); dump();
		for (int k = 0; k < 300; k++) {
			int op = next() % 10;
			printf("%d", op);
			switch (op) {
			case 0: case 1: {
				bool police = next() % 2;
				if ((police ? CRestart::NumberOfPoliceRestarts : CRestart::NumberOfHospitalRestarts) >= NUM_RESTART_POINTS) { printf(" full =>"); break; }
				CVector p = rpos(); float h = uni(0, 360); printf(" %d", police); v3(p); printf(" %08x =>", fb(h));
				if (police) CRestart::AddPoliceRestartPoint(p, h); else CRestart::AddHospitalRestartPoint(p, h);
				break; }
			case 2: { CVector p = rpos(); float h = uni(0, 360); v3(p); printf(" %08x =>", fb(h)); CRestart::OverrideNextRestart(p, h); break; }
			case 3: printf(" =>"); CRestart::CancelOverrideRestart(); break;
			case 4: case 5: case 6: {
				bool police = next() % 2; CVector p = rpos(), out; float h;
				printf(" %d", police); v3(p); printf(" =>");
				if (police) CRestart::FindClosestPoliceRestartPoint(p, &out, &h); else CRestart::FindClosestHospitalRestartPoint(p, &out, &h);
				v3(out); printf(" %08x", fb(h));
				break; }
			case 7: {
				int a = next() % 3, b = next() % 3, c = next() % 2, d = next() % 2; printf(" %d %d %d %d =>", a, b, c, d);
				CRestart::OverrideHospitalLevel = a; CRestart::OverridePoliceStationLevel = b; CRestart::bFadeInAfterNextArrest = c; CRestart::bFadeInAfterNextDeath = d;
				break; }
			case 8: printf(" =>"); CRestart::SaveAllRestartPoints(saved, &savedSize); printf(" %u ", savedSize); for (uint32 i = 0; i < savedSize; i++) printf("%02x", saved[i]); break;
			default:
				if (!savedSize) { printf(" =>"); break; }
				printf(" =>"); CRestart::LoadAllRestartPoints(saved, savedSize); break;
			}
			dump();
		}
	}
}

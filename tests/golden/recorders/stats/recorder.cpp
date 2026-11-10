// Runs the original CStats (all but the stats screen lines) through random op sequences:
// tests/golden/stats.txt, one op per line "op args => result | every static" (gen.inc, made from
// the definition list, prints them in Stats.cpp's order and sets a scalar by index).
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <climits>
typedef int32_t int32; typedef uint32_t uint32; typedef uint8_t uint8; typedef uint16_t wchar;
#define assert(x)
#define Const const
#define nil nullptr
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#define Min(a,b) ((a) < (b) ? (a) : (b))
#define Max(a,b) ((a) > (b) ? (a) : (b))
#define FIX_BUGS
enum { NUM_PEDTYPES = 23 };
enum { NUM_RADIOS = 10 };
struct CText { wchar *Get(const char *k) { return (wchar *)k; } } TheText;
struct CPlayerInfo { int32 m_nMoney, m_nVisibleMoney; };
struct CWorld { static CPlayerInfo Players[1]; static int PlayerInFocus; };
CPlayerInfo CWorld::Players[1]; int CWorld::PlayerInFocus;
struct CPad { static bool bHasPlayerCheated; }; bool CPad::bHasPlayerCheated;
struct CGame { static bool nastyGame; }; bool CGame::nastyGame;
static float g_listen[NUM_RADIOS];
struct cDMAudio { float *GetListenTimeArray() { return g_listen; } } DMAudio;
#include "stats.h.inc"
#include "stats.inc"

static uint32 rng = 0x5a7c0de1u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static unsigned fbits(float f) { unsigned u; memcpy(&u, &f, 4); return u; }
static float bitsf(unsigned u) { float f; memcpy(&f, &u, 4); return f; }
#include "gen.inc"

static int rint_(void) {
	switch (next() % 6) {
	case 0: return 0;
	case 1: return (int)(next() % 2000000) - 1000000;
	case 2: return (int)next();
	default: return (int)(next() % 401) - 100;
	}
}
static void radio(void) {
	for (int i = 0; i < NUM_RADIOS; i++) { g_listen[i] = uni(0, 5000); printf(" %08x", fbits(g_listen[i])); }
}
static uint8 saved[2048];
static uint32 savedSize;

int main() {
	for (int s = 0; s < 6; s++) {
		printf("init"); radio(); printf(" =>"); CStats::Init(); printf(" |"); dump(); printf("\n");
		for (int k = 0; k < 400; k++) {
			int op = next() % 22;
			printf("%d", op);
			switch (op) {
			case 0: radio(); printf(" =>"); CStats::Init(); break;
			case 1: { int i = next() % CStats::TOTAL_FASTEST_TIMES, t = rint_(); printf(" %d %d =>", i, t); CStats::RegisterFastestTime(i, t); break; }
			case 2: { int i = next() % CStats::TOTAL_HIGHEST_SCORES, t = rint_(); printf(" %d %d =>", i, t); CStats::RegisterHighestScore(i, t); break; }
			case 3: { int t = rint_(); printf(" %d =>", t); CStats::RegisterBestPosition(0, t); break; }
			case 4: { int k2 = next() % 5; printf(" %d =>", k2);
				switch (k2) { case 0: CStats::AnotherLifeSavedWithAmbulance(); break; case 1: CStats::AnotherCriminalCaught(); break;
				case 2: CStats::AnotherFireExtinguished(); break; case 3: CStats::AnotherKillFrenzyPassed(); break; default: CStats::CheckPointReachedUnsuccessfully(); }
				break; }
			case 5: { int k2 = next() % 3, l = rint_(); printf(" %d %d =>", k2, l);
				if (k2 == 0) CStats::RegisterLevelAmbulanceMission(l); else if (k2 == 1) CStats::RegisterLevelVigilanteMission(l); else CStats::RegisterLevelFireMission(l);
				break; }
			case 6: { int a = rint_(), b = rint_(); printf(" %d %d =>", a, b); CStats::SetTotalNumberKillFrenzies(a); CStats::SetTotalNumberMissions(b); break; }
			case 7: printf(" =>"); CStats::CheckPointReachedSuccessfully(); break;
			case 8: case 9: { int k2 = next() % 11, n = rint_(); printf(" %d %d =>", k2, n);
				switch (k2) {
				case 0: CStats::MoneySpentOnWeapons(n); break; case 1: CStats::MoneySpentOnProperty(n); break;
				case 2: CStats::MoneySpentOnAutoPainting(n); break; case 3: CStats::MoneySpentOnFashion(n); break;
				case 4: CStats::NumOfVisitsFromLoanSharks(n); break; case 5: CStats::NumOfStoresKnockedOff(n); break;
				case 6: CStats::NumOfMovieStunts(n); break; case 7: CStats::NumOfAssassinations(n); break;
				case 8: CStats::NumOfPizzasDelivered(n); break; case 9: CStats::NumOfGarbagePickups(n); break;
				default: CStats::NumOfIceCreamSold(n); }
				break; }
			case 10: { int n = rint_(); printf(" %d =>", n); CStats::AddNumBloodRingKills(n); break; }
			case 11: { int n = rint_(); printf(" %d =>", n); CStats::LongestTimeInBloodRing(n); break; }
			case 12: { int n = next() % CStats::TOTAL_PROPERTIES; printf(" %d =>", n); CStats::AddPropertyAsOwned(n); break; }
			case 13: case 14: case 15: {
				CWorld::Players[0].m_nMoney = rint_(); CWorld::Players[0].m_nVisibleMoney = next() % 2 ? rint_() : 10000000 + (int)(next() % 3) - 1;
				CPad::bHasPlayerCheated = next() % 5 == 0;
				printf(" %d %d %d =>", CWorld::Players[0].m_nMoney, CWorld::Players[0].m_nVisibleMoney, CPad::bHasPlayerCheated);
				int r = CStats::FindCriminalRatingNumber();
				printf(" %d %s", r, (const char *)CStats::FindCriminalRatingString());
				break; }
			case 16: { float f = next() % 4 ? uni(-50, 2000) : (float)(int)(next() % 2000); printf(" %08x =>", fbits(f)); printf(" %s", (const char *)CStats::FindChaseString(f)); break; }
			case 17: { CGame::nastyGame = next() % 2; printf(" %d =>", CGame::nastyGame); printf(" %08x", fbits(CStats::GetPercentageProgress())); break; }
			case 18: radio(); printf(" =>"); CStats::SaveStats(saved, &savedSize); printf(" %u ", savedSize); for (uint32 i = 0; i < savedSize; i++) printf("%02x", saved[i]); break;
			case 19: {
				if (!savedSize) { printf(" =>"); break; }
				int m = next() % 4; printf(" %d", m);
				for (int i = 0; i < m; i++) { int at = next() % savedSize, v = next() % 256;
					if (at >= 400 && at < 415) v &= 1; // PropertyOwned: only 0/1 are bools (else UB)
					saved[at] = v; printf(" %d:%d", at, v); }
				printf(" =>"); CStats::LoadStats(saved, savedSize); break; }
			default: { int i = next() % NUM_SCALARS; unsigned b = next() % 3 == 0 ? fbits(uni(-100, 3000)) : (unsigned)rint_();
				if (next() % 8 == 0) b = next() % 2 ? fbits(1.0f) : fbits(0.0f);
				printf(" %d %u =>", i, b); set_scalar(i, b); break; }
			}
			printf(" |"); dump(); printf("\n");
		}
	}
}

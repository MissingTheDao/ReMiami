// Runs the original CWanted (all but WorkOutPolicePresence and ResetPolicePursuit, which touch
// peds: stubs below) through random op sequences: tests/golden/wanted.txt, one op per line
// "op args => results | state" (audio reports and pursuit-list messages inline).
#include "common.h"
#include <stdio.h>
#include "Timer.h"
#include "Darkel.h"
#include "ZoneCull.h"
#include "DMAudio.h"
#include "Stats.h"
class CCopPed;
#include "Wanted.h"
static int32 g_presence;
int32 CWanted::WorkOutPolicePresence(CVector, float radius) { printf(" presence %08x", *(uint32 *)&radius); return g_presence; }
void CWanted::ResetPolicePursuit(void) { printf(" resetpursuit"); for (int i = 0; i < 10; i++) m_pCops[i] = nil; m_CurrentCops = 0; }
CVector FindPlayerCoors(void) { return CVector(0, 0, 0); }
#define printf(...) printf(" msg")
#include "wanted.inc"
#undef printf

static uint32 rng = 0x3a77ed01u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static unsigned bits(float f) { unsigned u; memcpy(&u, &f, 4); return u; }
static void state(CWanted &w) {
	printf(" | %d %d %d %u %u %08x %d %d %d %d %d %d%d%d%d%d %d %d", w.m_nChaos, w.m_nMinChaos, w.m_nLastUpdateTime, w.m_nLastWantedLevelChange,
	       w.m_nLastTimeSuspended, bits(w.m_fCrimeSensitivity), w.m_CurrentCops, w.m_MaxCops, w.m_MaximumLawEnforcerVehicles, w.m_CopsBeatingSuspect,
	       w.m_RoadblockDensity, w.m_bIgnoredByCops, w.m_bIgnoredByEveryone, w.m_bSwatRequired, w.m_bFbiRequired, w.m_bArmyRequired,
	       w.m_nWantedLevel, w.m_nMinWantedLevel);
	for (int i = 0; i < 16; i++) {
		CCrimeBeingQd &c = w.m_aCrimes[i];
		if (c.m_nType == CRIME_NONE) printf(" -");
		else printf(" %d:%d:%u:%08x:%08x:%08x:%d%d", c.m_nType, c.m_nId, c.m_nTime, bits(c.m_vecPosn.x), bits(c.m_vecPosn.y), bits(c.m_vecPosn.z), c.m_bReported, c.m_bPoliceDoesntCare);
	}
	for (int i = 0; i < 10; i++) printf(" %d", (int)(uintptr_t)w.m_pCops[i]);
	printf(" %d %d %d %d\n", CWanted::MaximumWantedLevel, CWanted::nMaximumWantedLevel, CStats::WantedStarsAttained, CStats::WantedStarsEvaded);
}
int main() {
	static CWanted w;
	memset(&w, 0, sizeof w);
	uint32 now = 1000;
	for (int s = 0; s < 6; s++) {
		CTimer::now = now; w.Initialise(); printf("init %u =>", now); state(w);
		if (s == 5) { CTimer::now = now = 0xffff0000u; }
		for (int k = 0; k < 500; k++) {
			now += next() % 4 == 0 ? next() % 3000 : next() % 200;
			CTimer::now = now;
			CDarkel::frenzy = next() % 6 == 0;
			CCullZones::drop = next() % 3 ? 0 : (int)(next() % 140);
			g_presence = next() % 3 ? 0 : 1 + next() % 3;
			CVector c(uni(-1000, 1000), uni(-1000, 1000), uni(0, 50));
			int type = next() % 21, id = next() % 6;
			bool pdc = next() % 4 == 0;
			printf("op %u %d %d %d %d %d %08x %08x %08x", now, CDarkel::frenzy, CCullZones::drop, g_presence, type, id, bits(c.x), bits(c.y), bits(c.z));
			int op = next() % 16;
			printf(" %d %d =>", op, pdc);
			switch (op) {
			case 0: { int l = (int)(next() % 9) - 1; printf(" set %d", l); w.SetWantedLevel(l); break; }
			case 1: { int l = (int)(next() % 9) - 1; printf(" nodrop %d", l); w.SetWantedLevelNoDrop(l); break; }
			case 2: { int l = next() % 8; printf(" cheat %d", l); w.CheatWantedLevel(l); break; }
			case 3: { int l = (int)(next() % 9) - 1; printf(" max %d", l); CWanted::SetMaximumWantedLevel(l); break; }
			case 4: case 5: w.RegisterCrime((eCrimeType)type, c, id, pdc); break;
			case 6: w.RegisterCrime_Immediately((eCrimeType)type, c, id, pdc); break;
			case 7: { bool rep = next() % 2; printf(" add %d", rep); printf(" r%d", w.AddCrimeToQ((eCrimeType)type, id, c, rep, pdc)); break; }
			case 8: w.ReportCrimeNow((eCrimeType)type, c, pdc); break;
			case 9: case 10: case 11: w.Update(); break;
			case 12: w.UpdateCrimesQ(); break;
			case 13: if (next() % 3 == 0) { printf(" suspend"); w.Suspend(); } else { printf(" clear"); w.ClearQdCrimes(); } break;
			case 14: { // the cop list: some slots filled (fake pointers), flags, sensitivity
				int i = next() % 10; w.m_pCops[i] = w.m_pCops[i] ? nil : (CCopPed *)(uintptr_t)(1 + next() % 50);
				if (next() % 4 == 0) w.m_CurrentCops = next() % 11;
				w.m_bIgnoredByCops = next() % 8 == 0; w.m_bSwatRequired = next() % 6 == 0; w.m_bFbiRequired = next() % 6 == 0; w.m_bArmyRequired = next() % 6 == 0;
				if (next() % 5 == 0) w.m_fCrimeSensitivity = uni(0, 3);
				printf(" edit %d %d %d %d%d%d%d %08x", i, (int)(uintptr_t)w.m_pCops[i], w.m_CurrentCops, w.m_bIgnoredByCops, w.m_bSwatRequired,
				       w.m_bFbiRequired, w.m_bArmyRequired, bits(w.m_fCrimeSensitivity));
				break;
			}
			default: if (next() % 4 == 0) { printf(" reset"); w.Reset(); } else { printf(" updlevel"); w.UpdateWantedLevel(); } break;
			}
			printf(" q %d %d %d %d %d", w.AreMiamiViceRequired(), w.AreSwatRequired(), w.AreFbiRequired(), w.AreArmyRequired(), w.NumOfHelisRequired());
			state(w);
		}
	}
}

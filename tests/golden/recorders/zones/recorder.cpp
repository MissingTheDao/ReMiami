// Runs the original CTheZones logic (Init, CreateZone, PostZoneCreation, lookups, zone info and
// its setters, audio zones) through random scenarios: tests/golden/zones.txt, one op per line
// "op args => results" (floats as hex bits); "state" dumps every array (pointers as nav indices).
#include "common.h"
#include <stdio.h>
#include <ctype.h>
#include <string>
#include "Clock.h"
#include "SaveBuf.h"
#define class struct
#include "Zones.h"
#undef class
uint8 g_hour;
CVector FindPlayerCoors(void) { return CVector(0, 0, 0); }
#include "zones.inc"

static uint32 rng = 0x2a2e5eedu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static unsigned bits(float f) { unsigned u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static int nav(CZone *z) { return z ? (int)(z - CTheZones::NavigationZoneArray) : -1; }
static void dumpzone(const CZone &z) {
	uint32 a, b; memcpy(&a, z.name, 4); memcpy(&b, z.name + 4, 4); printf(" %08x%08x", a, b);
	f(z.minx); f(z.miny); f(z.minz); f(z.maxx); f(z.maxy); f(z.maxz);
	printf(" %d %d %d %d %d %d %d", z.type, z.level, z.zoneinfoDay, z.zoneinfoNight, nav(z.child), nav(z.parent), nav(z.next));
}
static void dumpinfo(const CZoneInfo &i) {
	const int16 *p = (const int16 *)&i;
	for (int k = 0; k < (int)(sizeof(CZoneInfo) / 2); k++) printf(" %d", p[k]);
}
static void state() {
	printf("state =>");
	printf(" %d %d %d %d %d %d %d", CTheZones::TotalNumberOfNavigationZones, CTheZones::TotalNumberOfInfoZones, CTheZones::TotalNumberOfMapZones,
	       CTheZones::TotalNumberOfZoneInfos, CTheZones::NumberOfAudioZones, CTheZones::FindIndex, CTheZones::m_CurrLevel);
	for (int i = 0; i < NUMNAVIGZONES; i++) dumpzone(CTheZones::NavigationZoneArray[i]);
	for (int i = 0; i < NUMINFOZONES; i++) dumpzone(CTheZones::InfoZoneArray[i]);
	for (int i = 0; i < NUMMAPZONES; i++) dumpzone(CTheZones::MapZoneArray[i]);
	for (int i = 0; i < 2 * NUMINFOZONES; i++) dumpinfo(CTheZones::ZoneInfoArray[i]);
	for (int i = 0; i < NUMAUDIOZONES; i++) printf(" %d", CTheZones::AudioZoneArray[i]);
	printf("\n");
}
static CVector pt() { return CVector(uni(-2500, 1700), uni(-2100, 2100), uni(-60, 120)); }
static std::string zname(uint32 i) { static const char *b[] = { "beach", "Downtn", "VICE_C", "hav", "AIRPORT_LONG", "golf", "STARisl" }; return std::string(b[i % 7]) + std::to_string(i % 3); }

int main() {
	setvbuf(stdout, NULL, _IONBF, 0);
	for (int s = 0; s < 6; s++) {
		printf("init =>\n"); CTheZones::Init();
		// Nested boxes so the hierarchy has depth: each new navigation zone is mostly inside a random earlier one.
		struct Box { float x0, y0, z0, x1, y1, z1; } boxes[64]; int nb = 0;
		boxes[nb++] = { -2400, -2000, -500, 1600, 2000, 500 };
		int nz = 10 + next() % 60, defaults = 0;
		for (int k = 0; k < nz; k++) {
			int type = next() % 4;
			if (type <= 1 && CTheZones::TotalNumberOfNavigationZones >= NUMNAVIGZONES) type = 2;
			// At most 14 audio zones (default navigation zones, VICE_C included): the original asserts, then writes past its array.
			if (type == 0 && ++defaults > 12) type = 1;
			if (type == 3 && CTheZones::TotalNumberOfMapZones >= NUMMAPZONES) type = 2;
			if (type == 2 && CTheZones::TotalNumberOfInfoZones >= NUMINFOZONES) continue;
			Box o = boxes[next() % nb];
			float fx = uni(0, 0.7f), fy = uni(0, 0.7f), fz = uni(0, 0.7f);
			float w = (o.x1 - o.x0) * uni(0.1f, 0.6f), h = (o.y1 - o.y0) * uni(0.1f, 0.6f), d = (o.z1 - o.z0) * uni(0.2f, 0.9f);
			Box b = { o.x0 + (o.x1 - o.x0) * fx * 0.5f, o.y0 + (o.y1 - o.y0) * fy * 0.5f, o.z0 + (o.z1 - o.z0) * fz * 0.1f, 0, 0, 0 };
			b.x1 = b.x0 + w; b.y1 = b.y0 + h; b.z1 = b.z0 + d;
			if (next() % 8 == 0) { b.x1 += 3000; } // sticks out: overlaps instead of nesting
			if (nb < 64) boxes[nb++] = b;
			float c[6] = { b.x0, b.y0, b.z0, b.x1, b.y1, b.z1 };
			if (next() % 4 == 0) { float t = c[0]; c[0] = c[3]; c[3] = t; } // swapped corners
			char name[32]; strcpy(name, zname(next()).c_str());
			int level = next() % 3;
			printf("create %s %d", name, type); for (int q = 0; q < 6; q++) f(c[q]); printf(" %d =>", level);
			CTheZones::CreateZone(name, (eZoneType)type, c[0], c[1], c[2], c[3], c[4], c[5], (eLevelName)level);
			printf(" %s\n", name); // upper-cased in place
		}
		printf("post =>\n"); CTheZones::PostZoneCreation();
		state();
		for (int k = 0; k < 60; k++) {
			CVector p = pt();
			switch (next() % 5) {
			case 0: printf("level"); f(p.x); f(p.y); f(p.z); printf(" => %d\n", CTheZones::GetLevelFromPosition(&p)); break;
			case 1: printf("infozone"); f(p.x); f(p.y); f(p.z); printf(" => %d\n", (int)(CTheZones::FindInformationZoneForPosition(&p) - CTheZones::InfoZoneArray)); break;
			case 2: { int a = next() % 2, b = next() % 2; printf("navzone"); f(p.x); f(p.y); f(p.z); printf(" %d %d => %d\n", a, b, nav(CTheZones::FindSmallestNavigationZoneForPosition(&p, a, b))); break; }
			case 3: printf("audio"); f(p.x); f(p.y); f(p.z); printf(" => %d\n", CTheZones::FindAudioZone(&p)); break;
			default: {
				char name[32]; strcpy(name, zname(next()).c_str()); for (char *q = name; *q; q++) *q = toupper(*q); name[7] = 0;
				int type = next() % 5;
				printf("find %s %d => %d", name, type, CTheZones::FindZoneByLabelAndReturnIndex(name, (eZoneType)type));
				printf(" %d %d\n", CTheZones::FindNextZoneByLabelAndReturnIndex(name, (eZoneType)type), CTheZones::FindIndex);
			}
			}
		}
		for (int k = 0; k < 40; k++) {
			uint16 z = next() % CTheZones::TotalNumberOfInfoZones; uint8 day = next() % 2;
			int16 v[11]; for (int q = 0; q < 11; q++) v[q] = (int16)(next() % 900) - 100;
			switch (next() % 4) {
			case 0: printf("carinfo %d %d", z, day); for (int q = 0; q < 11; q++) printf(" %d", v[q]); printf(" =>\n");
				CTheZones::SetZoneCarInfo(z, day, v[0], v[1], &v[2]); break;
			case 1: printf("civcar %d %d", z, day); for (int q = 0; q < 11; q++) printf(" %d", v[q]); printf(" =>\n");
				CTheZones::SetZoneCivilianCarInfo(z, day, &v[0], &v[9]); break;
			case 2: printf("pedinfo %d %d", z, day); for (int q = 0; q < 11; q++) printf(" %d", v[q]); printf(" =>\n");
				CTheZones::SetZonePedInfo(z, day, v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10]); break;
			default: { int what = next() % 3; printf("setval %d %d %d %d =>\n", z, day, what, (uint16)v[0]);
				if (what == 0) CTheZones::SetCarDensity(z, day, v[0]); else if (what == 1) CTheZones::SetPedDensity(z, day, v[0]); else CTheZones::SetPedGroup(z, day, v[0]); }
			}
		}
		for (int k = 0; k < 30; k++) {
			CVector p = pt(); g_hour = next() % 24;
			CZoneInfo info; memset(&info, 0, sizeof info);
			CTheZones::GetZoneInfoForTimeOfDay(&p, &info);
			printf("tod"); f(p.x); f(p.y); f(p.z); printf(" %d =>", g_hour); dumpinfo(info); printf("\n");
		}
		state();
		// Save, then load the save into freshly initialised zones.
		static uint8 buf[64 * 1024];
		uint32 size = 0;
		memset(buf, 0xA5, sizeof(buf));
		CTheZones::SaveAllZones(buf, &size);
		uint64_t h = 0xcbf29ce484222325ull;
		for (uint32 k = 0; k < size; k++) h = (h ^ buf[k]) * 0x100000001b3ull;
		printf("save => %u %016llx\n", size, (unsigned long long)h);
		CTheZones::Init();
		CTheZones::LoadAllZones(buf, size);
		printf("load =>\n");
		state();
	}
}

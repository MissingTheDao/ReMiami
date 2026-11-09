// Runs the original CClock through random op sequences: tests/golden/clock.txt, one op per line
// "op args => result | hours minutes seconds storedH storedM storedS msPerMinute lastTick stored days".
#include "common.h"
#include <stdio.h>
#include "Timer.h"
#include "Pad.h"
#include "Stats.h"
#include "VarConsole.h"
#include "Clock.h"
#include "clock.inc"

static uint32 rng = 0xc10c4a11u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static void st() {
	printf(" | %d %d %d %d %d %d %u %u %d %d\n", CClock::ms_nGameClockHours, CClock::ms_nGameClockMinutes, CClock::ms_nGameClockSeconds,
	       CClock::ms_Stored_nGameClockHours, CClock::ms_Stored_nGameClockMinutes, CClock::ms_Stored_nGameClockSeconds,
	       CClock::ms_nMillisecondsPerGameMinute, CClock::ms_nLastClockTick, CClock::ms_bClockHasBeenStored, CStats::DaysPassed);
}
int main() {
	uint32 now = 0;
	for (int s = 0; s < 8; s++) {
		static const uint32 scales[] = { 1000, 1, 37, 60000 };
		uint32 scale = scales[s % 4];
		now = s == 7 ? 0xfffff000u : next() % 100000; // the last run wraps the millisecond counter
		CTimer::now = now;
		CClock::Initialise(scale);
		printf("init %u %u =>", scale, now); st();
		for (int k = 0; k < 400; k++) {
			switch (next() % 10) {
			default: {
				now += next() % 4 == 0 ? next() % (scale * 3 + 10) : next() % 40;
				CTimer::now = now;
				CPad::GetPad(1)->shoulder = next() % 15 == 0;
				gbFreezeTime = next() % 25 == 0;
				gbFastTime = next() % 20 == 0;
				CClock::Update();
				printf("update %u %d %d %d =>", now, CPad::GetPad(1)->shoulder, gbFreezeTime, gbFastTime); st();
				break;
			}
			case 0: { uint8 h = next() % 80, m = next() % 250; CClock::SetGameClock(h, m); printf("set %d %d %u =>", h, m, now); st(); break; }
			case 1: { uint8 h = next() % 24, m = next() % 60; printf("until %d %d => %d", h, m, CClock::GetGameClockMinutesUntil(h, m)); st(); break; }
			case 2: { uint8 a = next() % 25, b = next() % 25; printf("range %d %d => %d", a, b, CClock::GetIsTimeInRange(a, b)); st(); break; }
			case 3: CClock::StoreClock(); printf("store =>"); st(); break;
			case 4: CClock::RestoreClock(); printf("restore =>"); st(); break;
			}
		}
	}
}

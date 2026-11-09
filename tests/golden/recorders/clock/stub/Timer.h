#pragma once
struct CTimer { static uint32 now; static uint32 GetTimeInMilliseconds(void) { return now; } };
uint32 CTimer::now;

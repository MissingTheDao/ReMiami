#pragma once
// Recorder stand-in: the hour is set by the recorder.
extern uint8 g_hour;
struct CClock {
	static uint8 GetHours(void) { return g_hour; }
	static bool GetIsTimeInRange(uint8 h1, uint8 h2) { return h1 > h2 ? g_hour >= h1 || g_hour < h2 : g_hour >= h1 && g_hour < h2; }
};

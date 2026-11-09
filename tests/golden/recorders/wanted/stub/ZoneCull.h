#pragma once
struct CCullZones { static int32 drop; static int32 GetWantedLevelDrop(void) { return drop; } };
int32 CCullZones::drop;

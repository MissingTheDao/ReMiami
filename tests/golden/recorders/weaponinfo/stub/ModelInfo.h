#pragma once
// Recorder stand-in: SetWeaponInfo calls are printed ("model <id> <weapon>").
struct CWeaponModelInfo { int id; void SetWeaponInfo(int w) { printf(" model %d %d", id, w); } };
struct CModelInfo { static CWeaponModelInfo *GetModelInfo(int id) { static CWeaponModelInfo m; m.id = id; return &m; } };

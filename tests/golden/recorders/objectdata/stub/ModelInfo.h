#pragma once
// Recorder stand-in: models named "mdl<N>" exist (N < 400), anything else doesn't; SetObjectID is printed.
struct CBaseModelInfo { int index; void SetObjectID(int16 id) { printf(" set %d %d", index, id); } };
struct CModelInfo {
	static CBaseModelInfo *GetModelInfo(const char *name, int *) {
		static CBaseModelInfo m; int n;
		if (sscanf(name, "mdl%d", &n) == 1 && n >= 0 && n < 400) { m.index = n; return &m; }
		return nullptr;
	}
};

#pragma once
// Recorder stand-in: LoadFile reads g_dataFile (binary mode: bytes as they are).
extern const char *g_dataFile;
struct CFileMgr {
	static void SetDir(const char *) {}
	static int LoadFile(const char *, uint8 *buf, int maxlen, const char *) {
		FILE *f = fopen(g_dataFile, "rb"); int len = (int)fread(buf, 1, maxlen - 1, f); buf[len] = 0; fclose(f); return len;
	}
};

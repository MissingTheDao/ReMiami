#pragma once
// Recorder stand-in: LoadFile reads g_pedFile (PED.DAT) or g_statsFile (PEDSTATS.DAT) in text mode.
extern const char *g_pedFile, *g_statsFile;
struct CFileMgr {
	static void SetDir(const char *) {}
	static int LoadFile(const char *name, uint8 *buf, int maxlen, const char *) {
		FILE *f = fopen(strcmp(name, "PED.DAT") == 0 ? g_pedFile : g_statsFile, "rb"); int len = 0, c;
		while ((c = fgetc(f)) != EOF) {
			if (c == '\r') { int d = fgetc(f); if (d == '\n') c = d; else if (d != EOF) ungetc(d, f); }
			buf[len++] = c;
		}
		buf[len] = 0; fclose(f); return len;
	}
};

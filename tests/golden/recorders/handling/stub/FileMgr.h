#pragma once
// Recorder stand-in: LoadFile reads the file named by g_handlingFile, translating CRLF to LF
// like the game's text mode.
extern const char *g_handlingFile;
struct CFileMgr {
	static void SetDir(const char *) {}
	static int LoadFile(const char *, uint8 *buf, int maxlen, const char *) {
		FILE *f = fopen(g_handlingFile, "rb"); int len = 0, c;
		while ((c = fgetc(f)) != EOF) {
			if (c == '\r') { int d = fgetc(f); if (d == '\n') c = d; else if (d != EOF) ungetc(d, f); }
			buf[len++] = c;
		}
		buf[len] = 0; fclose(f); return len;
	}
};

#pragma once
// Recorder stand-in: plain stdio files behind small int handles.
static FILE *g_files[8];
struct CFileMgr {
	static int Put(FILE *f) { for (int i = 1; i < 8; i++) if (!g_files[i]) { g_files[i] = f; return i; } return 0; }
	static int OpenFile(const char *n, const char *m) { FILE *f = fopen(n, m); return f ? Put(f) : 0; }
	static int OpenFileForWriting(const char *n) { FILE *f = fopen(n, "wb"); return f ? Put(f) : 0; }
	static int Read(int fd, char *buf, int n) { return (int)fread(buf, 1, n, g_files[fd]); }
	static int Write(int fd, const char *buf, int n) { return (int)fwrite(buf, 1, n, g_files[fd]); }
	static void CloseFile(int fd) { fclose(g_files[fd]); g_files[fd] = nullptr; }
};

// Stand-ins for the game headers the original FIX_INCOMPATIBLE_SAVES code uses (recorder only).
#pragma once
#include <cstdio>
#include <cstring>
#include <cstdint>
#include <ctime>
#include <cassert>
#include <cstdlib>
typedef uint8_t uint8; typedef int8_t int8; typedef uint16_t uint16; typedef int16_t int16;
typedef uint32_t uint32; typedef int32_t int32;
#define nil nullptr
#define MAX_PATH 260
struct CVector { float x, y, z; };
struct CVector2D { float x, y; };
struct CRGBA { uint8 r, g, b, a; };
struct CStoredCar { uint8 b[40]; };
struct CPlaneNode { CVector p; float t; bool bOnGround; };
enum { TOTAL_COLLECTCARS_GARAGES = 4, TOTAL_HIDEOUT_GARAGES = 12, NUM_GARAGE_STORED_CARS = 4,
       NUM_GARAGES = 32, NUM_CRANES = 8, NUMPICKUPS = 336, NUMCOLLECTEDPICKUPS = 20, NUMPHONES = 50 };
static_assert(sizeof(CPlaneNode) == 20, "");
const uint32 SIZE_OF_ONE_GAME_IN_BYTES = 201729;
uint8 work_buff[55000];
int32 CheckSum;
char DefaultPCSaveFileName[MAX_PATH];
static int caserename(const char *a, const char *b) { return rename(a, b); }

static FILE *files[16];
struct CFileMgr {
	static int Open(const char *n, const char *m) { for (int i = 1; i < 16; i++) if (!files[i]) { files[i] = fopen(n, m); return files[i] ? i : 0; } return 0; }
	static int OpenFile(const char *n, const char *m) { return Open(n, m); }
	static int OpenFileForWriting(const char *n) { return Open(n, "wb"); }
	static int Read(int f, const char *b, int n) { return (int)fread((void *)b, 1, n, files[f]); }
	static bool Write(int f, const char *b, int n) { return fwrite(b, 1, n, files[f]) == (size_t)n; }
	static bool GetErrorReadWrite(int f) { return ferror(files[f]) != 0; }
	static void CloseFile(int f) { if (files[f]) fclose(files[f]); files[f] = nullptr; }
};

static int align4bytes(int32 size) { return (size + 3) & 0xFFFFFFFC; }
#define ReadDataFromBufferPointer(buf, to) memcpy(&to, buf, sizeof(to)); buf += align4bytes(sizeof(to));

struct C_PcSave {
	bool PcClassSaveRoutine(int32 file, uint8 *data, uint32 size) {
		CFileMgr::Write(file, (const char*)&size, sizeof(size));
		CFileMgr::Write(file, (const char*)data, align4bytes(size));
		CheckSum += (uint8) size; CheckSum += (uint8) (size >> 8);
		CheckSum += (uint8) (size >> 16); CheckSum += (uint8) (size >> 24);
		for (int i = 0; i < align4bytes(size); i++) CheckSum += *data++;
		return true;
	}
} PcSaveHelper;

static void MakeSpaceForSizeInBufferPointer(uint8 *&presize, uint8 *&buf, uint8 *&postsize)
{ presize = buf; buf += sizeof(uint32); postsize = buf; }
static void CopySizeAndPreparePointer(uint8 *&buf, uint8 *&postbuf, uint8 *&postbuf2, uint32 &unused, uint32 &size)
{ memcpy(buf, &size, sizeof(size)); size = align4bytes(size); postbuf2 += size; postbuf = postbuf2; }

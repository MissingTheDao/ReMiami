// Records librw's plugin lists (plg.cpp: register, registerStream, the rights and always
// callbacks, getPluginOffset, construct/destruct/copy, streamRead/streamSkip/streamWrite/
// streamGetSize, assertRights) and memory streams (base.cpp: StreamMemory, the Stream read/write
// helpers, writeChunkHeader, readChunkHeaderInfo, findChunk) for the Rust port
// (rust/revc/src/rwplugin.rs). Plugins use one of eight fixed callback sets (k = registration
// count % 8), which the Rust test has too. Lines: op args [= result] => H | log.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <new>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"

using namespace rw;

static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void setError(Error *) {}
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
char *strdup_LOC(const char *s, uint32, const char *) { return strdup(s); }
}

// the eight callback sets
template<int K> static void *ctor(void *o, int32 off, int32 size) { LOG(" c%d@%d", K, off); for (int i = 0; i < size; i++) ((uint8*)o)[off + i] = K * 16 + i; return o; }
template<int K> static void *dtor(void *o, int32 off, int32 size) { LOG(" d%d@%d", K, off); for (int i = 0; i < size; i++) ((uint8*)o)[off + i] ^= 0x5A; return o; }
template<int K> static void *copy(void *d, void *s, int32 off, int32 size) { LOG(" y%d@%d", K, off); memcpy((uint8*)d + off, (uint8*)s + off, size); return d; }
template<int K> static Stream *rd(Stream *st, int32 len, void *o, int32 off, int32 size) {
	// even sets read the whole chunk, odd ones half of it (librw doesn't skip the rest)
	int32 n = K % 2 ? len / 2 : len;
	uint8 buf[256];
	uint32 got = st->read8(buf, n);
	LOG(" r%d:%d:%u", K, len, got);
	for (uint32 i = 0; i < got && size > 0; i++) ((uint8*)o)[off + i % size] ^= buf[i];
	return st;
}
template<int K> static Stream *wr(Stream *st, int32 len, void *o, int32 off, int32 size) {
	LOG(" w%d:%d", K, len);
	for (int32 i = 0; i < len; i++) st->writeU8(size ? ((uint8*)o)[off + i % size] : K);
	return st;
}
template<int K> static int32 gs(void *o, int32 off, int32 size) { switch (K % 4) { case 0: return size; case 1: return -1; case 2: return 0; default: return size + 3; } }
template<int K> static void rights(void *o, int32 off, int32 size, uint32 data) { LOG(" R%d:%u", K, data); }
template<int K> static void always(void *o, int32 off, int32 size) { LOG(" A%d", K); }
#define SET(K) { ctor<K>, dtor<K>, copy<K>, rd<K>, wr<K>, gs<K>, rights<K>, always<K> }
struct CbSet { Constructor c; Destructor d; CopyConstructor y; StreamRead r; StreamWrite w; StreamGetSize g; RightsCallback rc; AlwaysCallback ac; };
static CbSet sets[8] = { SET(0), SET(1), SET(2), SET(3), SET(4), SET(5), SET(6), SET(7) };

static uint32 rng = 0x2f8f3a17u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }

static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static void hbytes(const void *p, int n) { for (int i = 0; i < n; i++) hb(((const uint8*)p)[i]); }

enum { NL = 4 };
static PluginList lists[NL];
static int nreg;
static uint32 ids[NL][64];
static int nids[NL];
static uint8 memdata[1024];
static StreamMemory mem;

static void hashLists(void) {
	for (int l = 0; l < NL; l++) {
		hi(lists[l].size); hi(lists[l].defaultSize);
		FORLIST(lnk, lists[l].plugins) {
			Plugin *p = LLLinkGetData(lnk, Plugin, inParentList);
			hi(p->offset); hi(p->size); hi(p->id);
			hi(p->read != nil); hi(p->write != nil); hi(p->getSize != nil); hi(p->rightsCallback != nil); hi(p->alwaysCallback != nil);
		}
	}
}
static void state(const uint8 *obj, int n) {
	h = 0xcbf29ce484222325ull;
	hashLists();
	if (obj) hbytes(obj, n);
	hi(mem.length); hi(mem.capacity); hi(mem.position); hbytes(memdata, sizeof(memdata));
	printf(" => %016llx |%s\n", h, plog);
	plog[0] = 0;
}
static void fillObj(uint8 *o, int n, uint32 seed) { uint32 s = seed | 1; for (int i = 0; i < n; i++) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; o[i] = s; } }
static void hex(const uint8 *p, int n) { for (int i = 0; i < n; i++) printf("%02x", p[i]); }

// an extension chunk for list l: chunks of its plugins' ids and others
static int extStream(uint8 *buf, int l) {
	int n = 0;
	auto put32 = [&](uint32 v) { memcpy(buf + n, &v, 4); n += 4; };
	if (next() % 5 == 0) {	// a chunk findChunk skips first, or a NAOBJECT that stops it
		uint32 t = next() % 3 == 0 ? ID_NAOBJECT : ID_STRUCT;
		int len = next() % 8;
		put32(t); put32(len); put32(libraryIDPack(0x36003, 0xFFFF));
		for (int i = 0; i < len; i++) buf[n++] = next();
	}
	put32(ID_EXTENSION);
	int lenpos = n; put32(0); put32(libraryIDPack(0x36003, 0xFFFF));
	int start = n, m = next() % 6;
	for (int c = 0; c < m; c++) {
		uint32 t = nids[l] && next() % 4 ? ids[l][next() % nids[l]] : 0x100 + next() % 8;
		int len = next() % 20;
		put32(t); put32(len); put32(libraryIDPack(0x36003, 0xFFFF));
		for (int i = 0; i < len; i++) buf[n++] = next();
	}
	uint32 total = n - start;
	if (next() % 8 == 0) total += next() % 10;	// a wrong length runs past the end
	memcpy(buf + lenpos, &total, 4);
	return n;
}

int
main(void)
{
	PluginList::open();
	static const int defs[NL] = { 16, 24, 40, 64 };
	for (int l = 0; l < NL; l++) new (&lists[l]) PluginList(defs[l]);
	mem.open(memdata, 0, sizeof(memdata));
	uint8 obj[1024], obj2[1024];
	for (int step = 0; step < 6000; step++) {
		int l = next() % NL;
		PluginList *L = &lists[l];
		switch (next() % 22) {
		case 0: case 1: {
			if (nids[l] >= 30) break;
			int size = (int[]){ 0, 1, 3, 4, 8, 12, 16 }[next() % 7];
			uint32 id = next() % 3 == 0 && nids[l] ? ids[l][next() % nids[l]] : 0x100 + next() % 32;
			int mode = next() % 8, k = nreg % 8;
			CbSet &s = sets[k];
			printf("reg %d %d %u %d %d =", l, size, id, mode, k);
			int32 off = L->registerPlugin(size, id, mode & 1 ? s.c : nil, mode & 2 ? s.d : nil, mode & 4 ? s.y : nil);
			nreg++;
			ids[l][nids[l]++] = id;
			printf(" %d", off); state(nil, 0); break;
		}
		case 2: case 3: {
			if (!nids[l]) break;
			uint32 id = next() % 6 == 0 ? 0x200 : ids[l][next() % nids[l]];
			int k = next() % 8, mode = next() % 8;
			if (mode & 4) mode |= 2;	// streamWrite calls write whenever getSize is set: the game registers both
			CbSet &s = sets[k];
			printf("regstream %d %u %d %d = %d", l, id, k, mode, L->registerStream(id, mode & 1 ? s.r : nil, mode & 2 ? s.w : nil, mode & 4 ? s.g : nil));
			state(nil, 0); break;
		}
		case 4: {
			if (!nids[l]) break;
			uint32 id = next() % 6 == 0 ? 0x200 : ids[l][next() % nids[l]];
			int k = next() % 8, which = next() % 2;
			printf("%s %d %u %d = %d", which ? "setalways" : "setrights", l, id, k, which ? L->setStreamAlwaysCallback(id, sets[k].ac) : L->setStreamRightsCallback(id, sets[k].rc));
			state(nil, 0); break;
		}
		case 5: { uint32 id = nids[l] && next() % 4 ? ids[l][next() % nids[l]] : 0x200 + next() % 4; printf("getoffset %d %u = %d", l, id, L->getPluginOffset(id)); state(nil, 0); break; }
		case 6: case 7: { uint32 seed = next(); fillObj(obj, L->size, seed); printf("construct %d %u", l, seed); L->construct(obj); state(obj, L->size); break; }
		case 8: { uint32 seed = next(); fillObj(obj, L->size, seed); printf("destruct %d %u", l, seed); L->destruct(obj); state(obj, L->size); break; }
		case 9: { uint32 a = next(), b = next(); fillObj(obj, L->size, a); fillObj(obj2, L->size, b); printf("copy %d %u %u", l, a, b); L->copy(obj, obj2); state(obj, L->size); break; }
		case 10: {
			uint32 data = next() % 1000, id = nids[l] && next() % 4 ? ids[l][next() % nids[l]] : 0x200;
			uint32 seed = next(); fillObj(obj, L->size, seed);
			printf("rights %d %u %u %u", l, seed, id, data); L->assertRights(obj, id, data); state(obj, L->size); break;
		}
		case 11: case 12: {
			uint8 buf[512];
			int n = extStream(buf, l);
			uint32 seed = next(); fillObj(obj, L->size, seed);
			StreamMemory s; s.open(buf, n, n);
			printf("read %d %u ", l, seed); hex(buf, n);
			bool ok = L->streamRead(&s, obj);
			printf(" = %d %u", ok, s.tell());
			state(obj, L->size); break;
		}
		case 13: {
			uint8 buf[512];
			int n = extStream(buf, l);
			StreamMemory s; s.open(buf, n, n);
			printf("skip %d ", l); hex(buf, n);
			L->streamSkip(&s);
			printf(" = %u", s.tell());
			state(nil, 0); break;
		}
		case 14: {
			uint32 seed = next(); fillObj(obj, L->size, seed);
			uint8 buf[1024] = {};
			StreamMemory s; s.open(buf, 0, sizeof(buf));
			printf("write %d %u = %d", l, seed, L->streamGetSize(obj));
			L->streamWrite(&s, obj);
			printf(" %u ", s.getLength()); hex(buf, s.getLength());
			state(obj, L->size); break;
		}
		// the memory stream
		case 15: {
			int cap = 1 + next() % 64, len = next() % (cap + 1);
			for (int i = 0; i < (int)sizeof(memdata); i++) memdata[i] = next() % 2 ? next() : 0;
			printf("mopen %d %d %u", len, cap, (uint32)(memdata[0] | memdata[1] << 8));
			uint32 seed = memdata[0] | memdata[1] << 8;
			uint32 s = seed | 1;
			for (int i = 0; i < (int)sizeof(memdata); i++) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; memdata[i] = s; }
			mem.open(memdata, len, cap);
			state(nil, 0); break;
		}
		case 16: {
			int op = next() % 6;
			if (op == 0) { uint32 n = next() % 24; uint8 b[32]; for (uint32 i = 0; i < n; i++) b[i] = next(); printf("mwrite8 %u ", n); hex(b, n); printf(" = %u", mem.write8(b, n)); }
			else if (op == 1) { uint32 n = next() % 24; uint8 b[32] = {}; uint32 r = mem.read8(b, n); printf("mread8 %u = %u ", n, r); hex(b, r); }
			else if (op == 2) { int32 off = (int32)(next() % 80) - 20; int w = next() % 3; printf("mseek %d %d", off, w); mem.seek(off, w); }
			else if (op == 3) printf("mtell = %u", mem.tell());
			else if (op == 4) printf("meof = %d", mem.eof());
			else printf("mlen = %u", mem.getLength());
			state(nil, 0); break;
		}
		case 17: {
			int op = next() % 14;
			uint32 v = next();
			// a typed read past the end returns an uninitialised local: only reads that fit
			if (op < 7 && (mem.eof() || mem.position + 4 > mem.length)) break;
			switch (op) {
			case 0: printf("mri8 = %d", (int)mem.readI8()); break;
			case 1: printf("mru8 = %u", (uint32)mem.readU8()); break;
			case 2: printf("mri16 = %d", (int)mem.readI16()); break;
			case 3: printf("mru16 = %u", (uint32)mem.readU16()); break;
			case 4: printf("mri32 = %d", mem.readI32()); break;
			case 5: printf("mru32 = %u", mem.readU32()); break;
			case 6: { float f = mem.readF32(); printf("mrf32 = %08x", *(uint32*)&f); break; }
			case 7: printf("mwi8 %u = %d", v & 0xFF, mem.writeI8(v)); break;
			case 8: printf("mwu8 %u = %d", v & 0xFF, mem.writeU8(v)); break;
			case 9: printf("mwi16 %u = %d", v & 0xFFFF, mem.writeI16(v)); break;
			case 10: printf("mwu16 %u = %d", v & 0xFFFF, mem.writeU16(v)); break;
			case 11: printf("mwi32 %u = %d", v, mem.writeI32(v)); break;
			case 12: printf("mwu32 %u = %d", v, mem.writeU32(v)); break;
			default: printf("mwf32 %u = %d", v, mem.writeF32(*(float*)&v)); break;
			}
			state(nil, 0); break;
		}
		case 18: {
			// a read whose bytes ran out reads garbage on the stack: only reads that fit
			if (mem.eof() || mem.position + 8 > mem.length) break;
			uint8 b[8]; uint32 n = 4 * (1 + next() % 2);
			int which = next() % 2;
			uint32 r = which ? mem.read32(b, n) : mem.read16(b, n);
			printf("%s %u = %u ", which ? "mread32" : "mread16", n, r); hex(b, r);
			state(nil, 0); break;
		}
		case 19: {
			int32 v = next() % 2 ? 0x36003 : (int[]){ 0x30000, 0x31000, 0x33002, 0x34003, 0x37002 }[next() % 5], b = next() % 2 ? 0xFFFF : next() & 0xFFFF;
			version = v; build = b;
			uint32 type = next() % 4 ? (int[]){ ID_STRUCT, ID_EXTENSION, ID_NAOBJECT, 0x100 + 3 }[next() % 4] : next();
			int32 size = next() % 100;
			printf("mchunkw %d %d %u %d = %d", v, b, type, size, writeChunkHeader(&mem, type, size));
			state(nil, 0); break;
		}
		case 20: {
			if (mem.eof() || mem.position + 12 > mem.length) break;
			ChunkHeaderInfo hdr;
			bool ok = readChunkHeaderInfo(&mem, &hdr);
			printf("mchunkr = %d %u %u %u %u", ok, hdr.type, hdr.length, hdr.version, hdr.build);
			state(nil, 0); break;
		}
		default: {
			// findChunk over a stream of chunks (the lengths inside the data)
			uint8 buf[256]; int n = 0;
			int m = 1 + next() % 5;
			for (int c = 0; c < m; c++) {
				uint32 t = (int[]){ ID_STRUCT, ID_EXTENSION, ID_NAOBJECT, 0x100 + 3, 0x50e }[next() % 5], len = next() % 16, id = libraryIDPack(next() % 2 ? 0x36003 : 0x31000, next() & 0xFFFF);
				memcpy(buf + n, &t, 4); memcpy(buf + n + 4, &len, 4); memcpy(buf + n + 8, &id, 4); n += 12;
				for (uint32 i = 0; i < len; i++) buf[n++] = next();
			}
			uint32 want = (int[]){ ID_STRUCT, ID_EXTENSION, 0x100 + 3, 0x50e, 0x999 }[next() % 5];
			StreamMemory s; s.open(buf, n, n);
			uint32 len = 7, ver = 7;
			printf("find %u ", want); hex(buf, n);
			bool ok = findChunk(&s, want, &len, &ver);
			printf(" = %d %u %u %u", ok, len, ver, s.tell());
			state(nil, 0); break;
		}
		}
	}
	return 0;
}

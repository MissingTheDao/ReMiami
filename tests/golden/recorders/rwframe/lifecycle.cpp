// Records librw's frame lifecycle (frame.cpp: create, destroy, destroyHierarchy, cloneAndLink,
// purgeClone, cloneHierarchy) for the Rust port. Frames get ids from the plugin constructor hook
// (in creation order); every line is one operation and the state after it:
//   op args => frames: H | dirty: ids
//              | plugins: c<id> d<id> y<dst>,<src> (construct/destruct/copy calls in order)
//              | allocated: Frame::numAllocated
// H is the 64-bit FNV-1a hash of the text " [id parent child next root privflags matrix]" for
// every live frame (the full text would be ~500 MB); "create" logs the new frame's matrix.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"

using namespace rw;

enum { MAXID = 4096 };
static Frame *byId[MAXID];
static int nextId;
static char plog[1 << 20];	// cloning a big hierarchy logs thousands of calls
static int idOf(Frame *f) { if (!f) return -1; for (int i = 0; i < nextId; i++) if (byId[i] == f) return i; return -2; }

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void PluginList::construct(void *o) {
	if (this != &Frame::s_plglist) return;
	byId[nextId] = (Frame*)o;
	sprintf(plog + strlen(plog), " c%d", nextId++);
}
void PluginList::destruct(void *o) {
	if (this != &Frame::s_plglist) return;
	int id = idOf((Frame*)o);
	sprintf(plog + strlen(plog), " d%d", id);
	if (id >= 0) byId[id] = nil;
}
void PluginList::copy(void *dst, void *src) {
	if (this != &Frame::s_plglist) return;
	sprintf(plog + strlen(plog), " y%d,%d", idOf((Frame*)dst), idOf((Frame*)src));
}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
void setError(Error *) {}
PluginList Engine::s_plglist;
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool writeChunkHeader(Stream *, int32, int32) { return true; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
int32 Stream::writeU32(uint32) { return 4; }
int32 findPointer(void *, void **, int32) { return -1; }
}

static uint32 rng = 0x7f4a7c15u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static char sbuf[1 << 22];
static char *sp;
#define out(...) (sp += sprintf(sp, __VA_ARGS__))
static void f(float v) { out(" %08x", bits(v)); }
static void m(const Matrix &x) {
	f(x.right.x); f(x.right.y); f(x.right.z); out(" %08x", x.flags);
	f(x.up.x); f(x.up.y); f(x.up.z); out(" --------");
	f(x.at.x); f(x.at.y); f(x.at.z); out(" --------");
	f(x.pos.x); f(x.pos.y); f(x.pos.z); out(" --------");
}
static unsigned long long fnv(const char *s) {
	unsigned long long h = 0xcbf29ce484222325ull;
	for (; *s; s++) { h ^= (unsigned char)*s; h *= 0x100000001b3ull; }
	return h;
}

static void state(void) {
	sp = sbuf; *sp = 0;
	for (int i = 0; i < nextId; i++) {
		Frame *p = byId[i];
		if (!p) continue;
		out(" [%d %d %d %d %d %02x", i, idOf(p->getParent()), idOf(p->child), idOf(p->next), idOf(p->root), p->object.privateFlags);
		m(p->matrix); out("]");
	}
	printf(" => frames: %016llx | dirty:", fnv(sbuf));
	FORLIST(lnk, engine->frameDirtyList)
		printf(" %d", idOf(LLLinkGetData(lnk, Frame, inDirtyList)));
	printf(" | plugins:%s | allocated: %d\n", plog, Frame::numAllocated);
	plog[0] = 0;
}

// librw bug (kept in the port, avoided here): cloneRecurse copies a frame's private flags, so an
// inner clone of a frame that was a dirty root carries HIERARCHYSYNC without being in the dirty
// list; addChild/updateObjects then unlink garbage pointers. Operations on such frames are
// skipped so the recording only covers defined behaviour.
static bool inDirty(Frame *f) {
	FORLIST(lnk, engine->frameDirtyList)
		if (LLLinkGetData(lnk, Frame, inDirtyList) == f) return true;
	return false;
}
static bool stale(Frame *f) { return (f->object.privateFlags & Frame::HIERARCHYSYNC) && !inDirty(f); }

// a random live frame id, or -1
static int pick(void) {
	int live[MAXID], n = 0;
	for (int i = 0; i < nextId; i++) if (byId[i]) live[n++] = i;
	return n ? live[next() % n] : -1;
}

int
main(void)
{
	engine = (Engine*)calloc(1, sizeof(Engine));
	engine->frameDirtyList.init();
	for (int step = 0; step < 3000 && nextId < MAXID - 64; step++) {
		int a = pick(), b = pick();
		switch (next() % 10) {
		case 0: case 1: case 2: {
			Frame *nf = Frame::create();
			V3d v = { uni(-5, 5), uni(-5, 5), uni(-5, 5) };
			nf->matrix.translate(&v, COMBINEREPLACE);
			sp = sbuf; m(nf->matrix); printf("create%s", sbuf);
			state(); break;
		}
		case 3: case 4: {
			if (a < 0 || b < 0) break;
			Frame *p = byId[a]; bool cyc = false;
			for (; p; p = p->getParent()) if (p == byId[b]) cyc = true;
			if (cyc) break;
			if (stale(byId[b]) || stale(byId[a]->root) || stale(byId[b]->root)) break;
			int app = next() % 2;
			printf("addchild %d %d %d", a, b, app); byId[a]->addChild(byId[b], app); state(); break;
		}
		case 5:
			if (a < 0 || byId[a]->child) break;	// destroy leaves children dangling; keep to leaves
			printf("destroy %d", a); byId[a]->destroy(); state(); break;
		case 6: {
			if (a < 0) break;
			Frame *r = byId[a]; if (r->getParent()) break;	// a whole hierarchy
			printf("destroyhierarchy %d", a); r->destroyHierarchy(); state(); break;
		}
		case 7: case 8: {
			if (a < 0) break;
			printf("clonehierarchy %d", a); byId[a]->cloneHierarchy(); state(); break;
		}
		default:
			if (a < 0) break;
			printf("cloneandlink %d", a); byId[a]->cloneAndLink();
			state();
			printf("purgeclone %d", a); byId[a]->purgeClone(); state(); break;
		}
	}
	return 0;
}

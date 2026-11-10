// Records librw's frame hierarchy logic (frame.cpp, compiled like the game: clang++ -O2) for the
// Rust port (rust/revc/src/rwframe.rs). Each scenario: "new N" creates N frames (and objects
// attached to some of them); every following line is one operation and the full state after it:
//   op args => frames: parent child next root privflags matrix ltm (per frame) | dirty: indices
//              | synced: object ids in callback order
// Floats and flags as 8-digit hex; matrix padding printed as --------.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"

namespace rw {
// stubs: engine, memory, plugin lists (frames get no plugin data here)
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void PluginList::construct(void *) {}
void PluginList::destruct(void *) {}
void PluginList::copy(void *, void *) {}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
void setError(Error *) {}
// unused here: frame.cpp's stream writer and engine plugin registration
PluginList Engine::s_plglist;
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool writeChunkHeader(Stream *, int32, int32) { return true; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
int32 Stream::writeU32(uint32) { return 4; }
int32 findPointer(void *, void **, int32) { return -1; }
int32 pluginOffset;
}

using namespace rw;

static uint32 rng = 0x2545f491u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void m(const Matrix &x) {
	f(x.right.x); f(x.right.y); f(x.right.z); printf(" %08x", x.flags);
	f(x.up.x); f(x.up.y); f(x.up.z); printf(" --------");
	f(x.at.x); f(x.at.y); f(x.at.z); printf(" --------");
	f(x.pos.x); f(x.pos.y); f(x.pos.z); printf(" --------");
}

enum { MAXF = 8, MAXO = 10 };
static Frame *fr[MAXF];
static int nf;
struct Obj { ObjectWithFrame owf; int id; };
static Obj objs[MAXO];
static int synced[256], nsynced;
static void syncCB(ObjectWithFrame *o) { if (nsynced < 256) synced[nsynced++] = ((Obj*)o)->id; }

static int idx(Frame *p) { for (int i = 0; i < nf; i++) if (fr[i] == p) return i; return -1; }

static void state(void) {
	printf(" => frames:");
	for (int i = 0; i < nf; i++) {
		Frame *p = fr[i];
		printf(" [%d %d %d %d %02x", idx(p->getParent()), idx(p->child), idx(p->next), idx(p->root), p->object.privateFlags);
		m(p->matrix); m(p->ltm); printf("]");
	}
	printf(" | dirty:");
	FORLIST(lnk, engine->frameDirtyList)
		printf(" %d", idx(LLLinkGetData(lnk, Frame, inDirtyList)));
	printf(" | synced:");
	for (int i = 0; i < nsynced; i++) printf(" %d", synced[i]);
	nsynced = 0;
	printf("\n");
}

static V3d vec(void) { V3d v = { uni(-10, 10), uni(-10, 10), uni(-10, 10) }; return v; }

int
main(void)
{
	engine = (Engine*)calloc(1, sizeof(Engine));
	engine->frameDirtyList.init();
	for (int sc = 0; sc < 60; sc++) {
		// fresh scenario
		engine->frameDirtyList.init();
		nf = MAXF;
		printf("new %d\n", nf);
		for (int i = 0; i < nf; i++) fr[i] = Frame::create();
		for (int o = 0; o < MAXO; o++) {
			objs[o].id = o;
			objs[o].owf.object.init(1, 0);
			objs[o].owf.syncCB = syncCB;
			int at = next() % nf;
			printf("attach %d %d", o, at);
			objs[o].owf.setFrame(fr[at]);
			state();
		}
		for (int step = 0; step < 40; step++) {
			int a = next() % nf, b = next() % nf;
			switch (next() % 9) {
			case 0: case 1: {
				// no cycles: b must not be an ancestor of a (or a itself)
				Frame *p = fr[a]; bool cyc = false;
				for (; p; p = p->getParent()) if (p == fr[b]) cyc = true;
				if (cyc) { printf("count %d", a); printf(" %d", fr[a]->count()); state(); break; }
				int app = next() % 2;
				printf("addchild %d %d %d", a, b, app); fr[a]->addChild(fr[b], app); state(); break;
			}
			case 2:
				if (fr[a]->getParent()) { printf("removechild %d", a); fr[a]->removeChild(); state(); }
				else { printf("count %d", a); printf(" %d", fr[a]->count()); state(); }
				break;
			case 3: { V3d v = vec(); float ang = uni(-180, 180); int op = next() % 3;
				printf("rotate %d", a); f(v.x); f(v.y); f(v.z); f(ang); printf(" %d", op); fr[a]->rotate(&v, ang, (CombineOp)op); state(); break; }
			case 4: { V3d v = vec(); int op = next() % 3;
				printf("translate %d", a); f(v.x); f(v.y); f(v.z); printf(" %d", op); fr[a]->translate(&v, (CombineOp)op); state(); break; }
			case 5: { V3d v = { uni(0.5f, 2), uni(0.5f, 2), uni(0.5f, 2) }; int op = next() % 3;
				printf("scale %d", a); f(v.x); f(v.y); f(v.z); printf(" %d", op); fr[a]->scale(&v, (CombineOp)op); state(); break; }
			case 6: { Matrix t; t.setIdentity(); V3d v = vec(); t.rotate(&v, uni(-90, 90), COMBINEREPLACE); t.pos = vec(); int op = next() % 3;
				printf("transform %d", a); m(t); printf(" %d", op); fr[a]->transform(&t, (CombineOp)op); state(); break; }
			case 7: { printf("getltm %d", a); Matrix *l = fr[a]->getLTM(); m(*l); state(); break; }
			default: printf("syncdirty"); Frame::syncDirty(); state(); break;
			}
		}
		// clean up: detach objects, free frames
		for (int o = 0; o < MAXO; o++) objs[o].owf.setFrame(nil);
		for (int i = 0; i < nf; i++) { if (fr[i]->object.privateFlags & Frame::HIERARCHYSYNC) fr[i]->inDirtyList.remove(); fr[i]->child = fr[i]->next = nil; fr[i]->object.parent = nil; }
		for (int i = 0; i < nf; i++) free(fr[i]);
		engine->frameDirtyList.init();
	}
	return 0;
}

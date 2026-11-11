// Records librw's scene objects (clump.cpp: Clump and Atomic; light.cpp; world.cpp; camera.cpp's
// Camera create/clone/destroy), with real frames and geometries, for the Rust port
// (rust/revc/src/rwscene.rs). Objects get ids from their plugin constructors (f, g, a, c, l, k
// for frames, geometries, atomics, clumps, lights, cameras; worlds w). Lines:
//   op args [= result] => H | log
// H: FNV-1a 64 of every live object's fields and links; log: plugin and render calls in order.
// The frames' and geometries' own code is the C++ of that commit too (ported earlier).
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"

using namespace rw;

enum { MAXID = 4096 };
static void *fr[MAXID], *geo[MAXID], *at[MAXID], *cl[MAXID], *li[MAXID], *ca[MAXID], *wo[MAXID];
static int nfr, ngeo, nat, ncl, nli, nca, nwo;
static int gref[MAXID];	// geometry references the recorder holds itself
static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)
static int idOf(void **arr, int n, void *p) { if (!p) return -1; for (int i = 0; i < n; i++) if (arr[i] == p) return i; return -2; }
#define FID(p) idOf(fr, nfr, p)
#define GID(p) idOf(geo, ngeo, p)
#define AID(p) idOf(at, nat, p)
#define CID(p) idOf(cl, ncl, p)
#define LID(p) idOf(li, nli, p)
#define KID(p) idOf(ca, nca, p)
#define WID(p) idOf(wo, nwo, p)

namespace rw {
Engine *engine;
int32 version = 0x36003;
int32 platform = 0;
// zeroed blocks with their size in front (grown blocks zeroed too)
static void *xmalloc(size_t sz, uint32) { size_t *p = (size_t*)calloc(1, sz + 16); *p = sz; return (uint8*)p + 16; }
static void xfree(void *p) { if (p) free((uint8*)p - 16); }
static void *xrealloc(void *p, size_t sz, uint32) {
	void *q = xmalloc(sz, 0);
	if (p) { size_t old = *(size_t*)((uint8*)p - 16); memcpy(q, p, old < sz ? old : sz); xfree(p); }
	return q;
}
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void setError(Error *) {}
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
PluginList Engine::s_plglist;
#define REG(list, arr, n, tag) if (this == &list) { arr[n] = o; LOG(" c" tag "%d", n++); }
#define UNREG(list, arr, n, tag) if (this == &list) { int i = idOf(arr, n, o); LOG(" d" tag "%d", i); if (i >= 0) arr[i] = nil; }
void PluginList::construct(void *o) {
	REG(Frame::s_plglist, fr, nfr, "f") REG(Geometry::s_plglist, geo, ngeo, "g") REG(Atomic::s_plglist, at, nat, "a")
	REG(Clump::s_plglist, cl, ncl, "c") REG(Light::s_plglist, li, nli, "l") REG(Camera::s_plglist, ca, nca, "k") REG(World::s_plglist, wo, nwo, "w")
}
void PluginList::destruct(void *o) {
	UNREG(Frame::s_plglist, fr, nfr, "f") UNREG(Geometry::s_plglist, geo, ngeo, "g") UNREG(Atomic::s_plglist, at, nat, "a")
	UNREG(Clump::s_plglist, cl, ncl, "c") UNREG(Light::s_plglist, li, nli, "l") UNREG(Camera::s_plglist, ca, nca, "k") UNREG(World::s_plglist, wo, nwo, "w")
}
void PluginList::copy(void *dst, void *src) {
	if (this == &Atomic::s_plglist) LOG(" ya%d,%d", AID(dst), AID(src));
	if (this == &Clump::s_plglist) LOG(" yc%d,%d", CID(dst), CID(src));
	if (this == &Camera::s_plglist) LOG(" yk%d,%d", KID(dst), KID(src));
	if (this == &Frame::s_plglist) LOG(" yf%d,%d", FID(dst), FID(src));
}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
int32 PluginList::registerStream(uint32, StreamRead, StreamWrite, StreamGetSize) { return 0; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool PluginList::streamRead(Stream *, void *) { return true; }
bool writeChunkHeader(Stream *, int32, int32) { return true; }
bool findChunk(Stream *, uint32, uint32 *, uint32 *) { return false; }
uint32 Stream::write8(const void *, uint32 n) { return n; }
uint32 Stream::write16(const void *, uint32 n) { return n; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
uint32 Stream::read32(void *, uint32 n) { return n; }
int32 Stream::writeI16(int16) { return 2; }
int32 Stream::writeI32(int32) { return 4; }
int32 Stream::writeU32(uint32) { return 4; }
int32 Stream::writeF32(float32) { return 4; }
int16 Stream::readI16(void) { return 0; }
int32 Stream::readI32(void) { return 0; }
int32 findPointer(void *, void **, int32) { return -1; }
int strncmp_ci(const char *s1, const char *s2, int n) { return strncasecmp(s1, s2, n); }
Raster *Raster::create(int32, int32, int32, int32, int32) { return nil; }
void Raster::destroy(void) {}
void Raster::show(uint32) {}
Raster *Raster::createFromImage(Image *, int32) { return nil; }
Image *Image::readMasked(const char *, const char *) { return nil; }
void Image::destroy(void) {}
namespace wgpu { void *destroyNativeData(void *o, int32, int32) { return o; } }
}

static void recRender(Atomic *a) { LOG(" r%d", AID(a)); }

static uint32 rng = 0x5be0cd19u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }

static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static void hbytes(const void *p, int n) { for (int i = 0; i < n; i++) hb(((const uint8*)p)[i]); }
static void hm(const Matrix &m) { hbytes(&m.right, 12); hi(m.flags); hbytes(&m.up, 12); hbytes(&m.at, 12); hbytes(&m.pos, 12); }
static void hobj(const Object &o) { hb(o.type); hb(o.subType); hb(o.flags); hb(o.privateFlags); }
// an ObjectWithFrame: object, frame, sync callbacks as "set" bits
static void howf(const ObjectWithFrame &o) { hobj(o.object); hi(FID((Frame*)o.object.parent)); hi(o.syncCB != nil); }
#define HLIST(list, T, field, ID) do { int n_ = 0; FORLIST(lnk, (list)) { hi(ID(LLLinkGetData(lnk, T, field))); n_++; } hi(n_); } while (0)

static void state(void) {
	h = 0xcbf29ce484222325ull;
	for (int i = 0; i < nfr; i++) {
		Frame *f = (Frame*)fr[i];
		if (!f) continue;
		hi(i); hobj(f->object); hi(FID(f->getParent())); hi(FID(f->child)); hi(FID(f->next)); hi(FID(f->root));
		hm(f->matrix); hm(f->ltm);
		int n = 0;
		FORLIST(lnk, f->objectList) {
			ObjectWithFrame *o = ObjectWithFrame::fromFrame(lnk);
			int t = o->object.type;
			hi(t);
			hi(t == Atomic::ID ? AID(o) : t == Light::ID ? LID(o) : t == Camera::ID ? KID(o) : -3);
			n++;
		}
		hi(n);
	}
	for (int i = 0; i < ngeo; i++) { Geometry *g = (Geometry*)geo[i]; if (!g) continue; hi(i); hi(g->refCount); }
	for (int i = 0; i < nat; i++) {
		Atomic *a = (Atomic*)at[i];
		if (!a) continue;
		hi(i); howf(a->object); hi(GID(a->geometry)); hbytes(&a->boundingSphere, 16); hbytes(&a->worldBoundingSphere, 16);
		hi(CID(a->clump)); hi(a->pipeline != nil); hi(a->renderCB == recRender); hi(WID(a->world)); hi(a->originalSync != nil);
	}
	for (int i = 0; i < ncl; i++) {
		Clump *c = (Clump*)cl[i];
		if (!c) continue;
		hi(i); hobj(c->object); hi(FID(c->getFrame())); hi(WID(c->world));
		HLIST(c->atomics, Atomic, inClump, AID); HLIST(c->lights, Light, inClump, LID); HLIST(c->cameras, Camera, inClump, KID);
	}
	for (int i = 0; i < nli; i++) {
		Light *l = (Light*)li[i];
		if (!l) continue;
		hi(i); howf(l->object); hbytes(&l->radius, 4); hbytes(&l->color, 16); hbytes(&l->minusCosAngle, 4);
		hi(CID(l->clump)); hi(WID(l->world)); hi(l->originalSync != nil);
	}
	for (int i = 0; i < nca; i++) {
		Camera *k = (Camera*)ca[i];
		if (!k) continue;
		hi(i); howf(k->object); hi(k->beginUpdateCB != nil); hi(k->endUpdateCB != nil);
		hbytes(&k->viewWindow, 8); hbytes(&k->viewOffset, 8); hbytes(&k->nearPlane, 4); hbytes(&k->farPlane, 4); hbytes(&k->fogPlane, 4); hi(k->projection);
		hi(k->frameBuffer != nil); hi(k->zBuffer != nil); hi(CID(k->clump)); hi(WID(k->world));
		hi(k->originalSync != nil); hi(k->originalBeginUpdate != nil); hi(k->originalEndUpdate != nil);
	}
	for (int i = 0; i < nwo; i++) {
		World *w = (World*)wo[i];
		if (!w) continue;
		hi(i); hobj(w->object);
		HLIST(w->localLights, Light, inWorld, LID); HLIST(w->globalLights, Light, inWorld, LID); HLIST(w->clumps, Clump, inWorld, CID);
	}
	FORLIST(lnk, engine->frameDirtyList) hi(FID(LLLinkGetData(lnk, Frame, inDirtyList)));
	printf(" => %016llx |%s\n", h, plog);
	plog[0] = 0;
}

static int pick(void **arr, int n) { int l[MAXID], k = 0; for (int i = 0; i < n; i++) if (arr[i]) l[k++] = i; return k ? l[next() % k] : -1; }
// frame f's hierarchy contains g
static bool inHier(Frame *root, Frame *g) { for (; g; g = g->getParent()) if (g == root) return true; return false; }

int
main(void)
{
	engine = (Engine*)calloc(1, 16384);
	engine->frameDirtyList.init();
	Texture::registerModule();
	for (int step = 0; step < 9000 && nfr < MAXID - 64 && nat < MAXID - 64; step++) {
		int f = pick(fr, nfr), f2 = pick(fr, nfr), g = pick(geo, ngeo), a = pick(at, nat), c = pick(cl, ncl), l = pick(li, nli), k = pick(ca, nca), w = pick(wo, nwo);
		Frame *F = f >= 0 ? (Frame*)fr[f] : nil;
		Atomic *A = a >= 0 ? (Atomic*)at[a] : nil;
		Clump *C = c >= 0 ? (Clump*)cl[c] : nil;
		Light *L = l >= 0 ? (Light*)li[l] : nil;
		Camera *K = k >= 0 ? (Camera*)ca[k] : nil;
		World *W = w >= 0 ? (World*)wo[w] : nil;
		// more weight on clump members, worlds and light enumeration
		int op = next() % 44;
		if (op >= 32) op = (int[]){ 15, 15, 15, 28, 28, 28, 28, 30, 30, 30, 12, 27 }[op - 32];
		switch (op) {
		case 0: case 1: {
			V3d v = { uni(-50, 50), uni(-50, 50), uni(-5, 5) };
			printf("newframe %08x %08x %08x =", *(uint32*)&v.x, *(uint32*)&v.y, *(uint32*)&v.z);
			Frame *x = Frame::create(); x->translate(&v, COMBINEREPLACE);
			printf(" %d", FID(x)); state(); break;
		}
		case 2: {
			if (!F || f2 < 0 || inHier((Frame*)fr[f2], F)) break;
			printf("addchild %d %d", f, f2); F->addChild((Frame*)fr[f2]); state(); break;
		}
		case 3: {
			if (!F) break;
			V3d v = { uni(-10, 10), uni(-10, 10), uni(-1, 1) };
			printf("move %d %08x %08x %08x", f, *(uint32*)&v.x, *(uint32*)&v.y, *(uint32*)&v.z); F->translate(&v, COMBINEPOSTCONCAT); state(); break;
		}
		case 4: printf("syncdirty"); Frame::syncDirty(); state(); break;
		case 5: {
			int nv = 1 + next() % 12;
			uint32 seed = next();
			printf("newgeo %d %u =", nv, seed);
			Geometry *x = Geometry::create(nv, 0, 0);
			uint32 s = seed | 1;
			for (int i = 0; i < nv * 3; i++) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; ((float*)x->morphTargets[0].vertices)[i] = (int32)(s % 2001 - 1000) / 100.0f; }
			x->calculateBoundingSphere();
			printf(" %d", GID(x)); gref[GID(x)] = 1; state(); break;
		}
		case 6: if (g < 0 || gref[g] == 0) break; gref[g]--; printf("delgeo %d", g); ((Geometry*)geo[g])->destroy(); state(); break;
		case 7: { printf("newatomic ="); Atomic *x = Atomic::create(); x->renderCB = recRender; printf(" %d", AID(x)); state(); break; }
		case 8: if (!A) break; if (next() % 3 == 0) { printf("atomicframe %d -1", a); A->setFrame(nil); } else { if (!F) break; printf("atomicframe %d %d", a, f); A->setFrame(F); } state(); break;
		case 9: {
			if (!A) break;
			int fl = next() % 4 == 0;
			if (next() % 4 == 0) { printf("setgeo %d -1 %d", a, fl); A->setGeometry(nil, fl); }
			else { if (g < 0) break; printf("setgeo %d %d %d", a, g, fl); A->setGeometry((Geometry*)geo[g], fl); }
			state(); break;
		}
		case 10: { if (!A) break; printf("cloneatomic %d =", a); Atomic *x = A->clone(); printf(" %d", AID(x)); state(); break; }
		case 11: if (!A || A->clump || A->world) break; printf("delatomic %d", a); A->destroy(); state(); break;
		case 12: {
			if (!A || !A->getFrame()) break;
			printf("wbs %d =", a); Sphere *s = A->getWorldBoundingSphere();
			printf(" %08x %08x %08x %08x", *(uint32*)&s->center.x, *(uint32*)&s->center.y, *(uint32*)&s->center.z, *(uint32*)&s->radius);
			state(); break;
		}
		case 13: { printf("newclump ="); Clump *x = Clump::create(); printf(" %d", CID(x)); state(); break; }
		case 14: {
			// a clump's frame is a root
			if (!C) break;
			if (next() % 4 == 0) { printf("clumpframe %d -1", c); C->setFrame(nil); }
			else { if (!F || F->getParent()) break; printf("clumpframe %d %d", c, f); C->setFrame(F); }
			state(); break;
		}
		case 15: {
			// objects join a clump before it goes into a world, as the game does it
			if (!C) break;
			int op = next() % 6;
			if (op == 0) { if (!A || A->clump || A->world || C->world) break; printf("addatomic %d %d", c, a); C->addAtomic(A); }
			else if (op == 1) { if (!A || A->clump != C || C->world) break; printf("removeatomic %d %d", c, a); C->removeAtomic(A); }
			else if (op == 2) { if (!L || L->clump || L->world || C->world) break; printf("addlight %d %d", c, l); C->addLight(L); }
			else if (op == 3) { if (!L || L->clump != C || C->world) break; printf("removelight %d %d", c, l); C->removeLight(L); }
			else if (op == 4) { if (!K || K->clump || K->world || C->world) break; printf("addcamera %d %d", c, k); C->addCamera(K); }
			else { if (!K || K->clump != C || C->world) break; printf("removecamera %d %d", c, k); C->removeCamera(K); }
			state(); break;
		}
		case 16: {
			// as the game clones: a clump with a root frame whose atomics hang in that hierarchy
			if (!C || !C->getFrame() || C->getFrame()->getParent()) break;
			bool ok = true;
			FORLIST(lnk, C->atomics) { Atomic *x = Atomic::fromClump(lnk); if (!x->getFrame() || !inHier(C->getFrame(), x->getFrame())) ok = false; }
			if (!ok) break;
			printf("cloneclump %d =", c); Clump *x = C->clone(); printf(" %d", CID(x)); state(); break;
		}
		case 17: {
			if (!C || C->world || (C->getFrame() && C->getFrame()->getParent())) break;
			// its frames carry only its own objects (destroyHierarchy asserts that)
			if (C->getFrame()) {
				bool ok = true;
				for (int i = 0; i < nat; i++) { Atomic *x = (Atomic*)at[i]; if (x && x->clump != C && x->getFrame() && inHier(C->getFrame(), x->getFrame())) ok = false; }
				for (int i = 0; i < nli; i++) { Light *x = (Light*)li[i]; if (x && x->clump != C && x->getFrame() && inHier(C->getFrame(), x->getFrame())) ok = false; }
				for (int i = 0; i < nca; i++) { Camera *x = (Camera*)ca[i]; if (x && x->clump != C && x->getFrame() && inHier(C->getFrame(), x->getFrame())) ok = false; }
				if (!ok) break;
			}
			printf("delclump %d", c); C->destroy(); state(); break;
		}
		case 18: if (!C) break; printf("render %d", c); C->render(); state(); break;
		case 19: {
			static const int types[] = { Light::DIRECTIONAL, Light::AMBIENT, Light::POINT, Light::SPOT, Light::SOFTSPOT };
			int t = types[next() % 5];
			printf("newlight %d =", t); Light *x = Light::create(t); printf(" %d", LID(x)); state(); break;
		}
		case 20: if (!L) break; if (next() % 3 == 0) { printf("lightframe %d -1", l); L->setFrame(nil); } else { if (!F) break; printf("lightframe %d %d", l, f); L->setFrame(F); } state(); break;
		case 21: {
			if (!L) break;
			int op = next() % 4;
			if (op == 0) { float r = uni(0, 1), gg = next() % 2 ? r : uni(0, 1), b = next() % 2 ? r : uni(0, 1); printf("color %d %08x %08x %08x", l, *(uint32*)&r, *(uint32*)&gg, *(uint32*)&b); L->setColor(r, gg, b); }
			else if (op == 1) { float an = uni(-3.2f, 3.2f); printf("angle %d %08x", l, *(uint32*)&an); L->setAngle(an); }
			else if (op == 2) { float an = L->getAngle(); printf("getangle %d = %08x", l, *(uint32*)&an); }
			else { float rad = uni(0, 100); printf("radius %d %08x", l, *(uint32*)&rad); L->radius = rad; }
			state(); break;
		}
		case 22: if (!L || L->clump || L->world) break; printf("dellight %d", l); L->destroy(); state(); break;
		case 23: { printf("newcamera ="); Camera *x = Camera::create(); printf(" %d", KID(x)); state(); break; }
		case 24: if (!K) break; if (next() % 3 == 0) { printf("cameraframe %d -1", k); K->setFrame(nil); } else { if (!F) break; printf("cameraframe %d %d", k, f); K->setFrame(F); } state(); break;
		case 25: { if (!K) break; printf("clonecamera %d =", k); Camera *x = K->clone(); printf(" %d", KID(x)); state(); break; }
		case 26: if (!K || K->clump || K->world) break; printf("delcamera %d", k); K->destroy(); state(); break;
		case 27: { if (nwo > 12) break; printf("newworld ="); World *x = World::create(); printf(" %d", WID(x)); state(); break; }
		case 28: {
			if (!W) break;
			// a clump's members enter a world with their clump only
			int op = next() % 8;
			if (op == 0) { if (!C || C->world) break; printf("waddclump %d %d", w, c); W->addClump(C); }
			else if (op == 1) { if (!C || C->world != W) break; printf("wremoveclump %d %d", w, c); W->removeClump(C); }
			else if (op == 2) { if (!L || L->world || L->clump) break; printf("waddlight %d %d", w, l); W->addLight(L); }
			else if (op == 3) { if (!L || L->world != W || (L->clump && L->clump->world)) break; printf("wremovelight %d %d", w, l); W->removeLight(L); }
			else if (op == 4) { if (!A || A->world || A->clump) break; printf("waddatomic %d %d", w, a); W->addAtomic(A); }
			else if (op == 5) { if (!A || A->world != W || (A->clump && A->clump->world)) break; printf("wremoveatomic %d %d", w, a); W->removeAtomic(A); }
			else if (op == 6) { if (!K || K->world || K->clump) break; printf("waddcamera %d %d", w, k); W->addCamera(K); }
			else { if (!K || K->world != W || (K->clump && K->clump->world)) break; printf("wremovecamera %d %d", w, k); W->removeCamera(K); }
			state(); break;
		}
		case 29: if (!W) break; printf("wrender %d", w); W->render(); state(); break;
		case 30: {
			if (!W || !A || !A->geometry) break;
			// local lights need frames (positions) and the atomic a frame (world sphere)
			bool ok = A->getFrame() != nil;
			FORLIST(lnk, W->localLights) if (!Light::fromWorld(lnk)->getFrame()) ok = false;
			if (!ok) break;
			Light *dirs[8], *locs[8];
			WorldLights wl;
			wl.numDirectionals = next() % 9; wl.numLocals = next() % 9; wl.directionals = dirs; wl.locals = locs;
			printf("enumlights %d %d %d %d =", w, a, wl.numDirectionals, wl.numLocals);
			W->enumerateLights(A, &wl);
			printf(" %d %08x %08x %08x %08x %d", wl.numAmbients, *(uint32*)&wl.ambient.red, *(uint32*)&wl.ambient.green, *(uint32*)&wl.ambient.blue, *(uint32*)&wl.ambient.alpha, wl.numDirectionals);
			for (int i = 0; i < wl.numDirectionals; i++) printf(" %d", LID(dirs[i]));
			printf(" %d", wl.numLocals);
			for (int i = 0; i < wl.numLocals; i++) printf(" %d", LID(locs[i]));
			state(); break;
		}
		default:
			// a world goes once nothing is in it
			if (!W || !W->clumps.isEmpty() || !W->localLights.isEmpty() || !W->globalLights.isEmpty()) break;
			{ bool used = false; for (int i = 0; i < nat; i++) if (at[i] && ((Atomic*)at[i])->world == W) used = true;
			  for (int i = 0; i < nca; i++) if (ca[i] && ((Camera*)ca[i])->world == W) used = true; if (used) break; }
			printf("delworld %d", w); W->destroy(); state(); break;
		}
	}
	return 0;
}

// Records librw's plugin data (hanim.cpp: HAnimHierarchy create/destroy/attach/getIndex/get/find
// and the frame plugin; anim.cpp: AnimInterpolator create/destroy; skin.cpp: Skin init,
// findNumWeights, findUsedBones and the geometry plugin; matfx.cpp: setEffects, the effect
// setters and getters and the material plugin) for the Rust port (rust/revc/src/rwplugins.rs).
// The plugin lists are small working ones here, and the original register*Plugin functions fill
// them, so frames, geometries and materials carry the real plugin data through create, clone and
// destroy. Ids come from the plugin constructors (f, g, m, t). Lines:
//   op args [= result] => H | log
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <vector>
#include <map>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"
#include "rwanim.h"
#include "rwplugins.h"

using namespace rw;

enum { MAXID = 4096 };
static void *fr[MAXID], *geo[MAXID], *mat[MAXID], *tex[MAXID];
static int nfr, ngeo, nmat, ntex;
static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)
static int idOf(void **arr, int n, const void *p) { if (!p) return -1; for (int i = 0; i < n; i++) if (arr[i] == p) return i; return -2; }
#define FID(p) idOf(fr, nfr, p)
#define GID(p) idOf(geo, ngeo, p)
#define MID(p) idOf(mat, nmat, p)
#define TID(p) idOf(tex, ntex, p)

struct Plg { int32 offset, size; Constructor ctor; Destructor dtor; CopyConstructor copy; };
static std::map<PluginList*, std::vector<Plg>> plgTable;

namespace rw {
Engine *engine;
int32 version = 0x36003;
int32 platform = 0;
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
PluginList Driver::s_plglist[NUM_PLATFORMS];
PluginList Atomic::s_plglist(sizeof(Atomic));	// skin and MatFX register atomic plugins too
#define REG(list, arr, n, tag) if (this == &list) { arr[n] = o; LOG(" c" tag "%d", n++); }
#define UNREG(list, arr, n, tag) if (this == &list) { int i = idOf(arr, n, o); LOG(" d" tag "%d", i); if (i >= 0) arr[i] = nil; }
int32 PluginList::registerPlugin(int32 size, uint32, Constructor ctor, Destructor dtor, CopyConstructor copy) {
	Plg p = { (int32)this->size, size, ctor, dtor, copy };
	plgTable[this].push_back(p);
	this->size += size;
	return p.offset;
}
void PluginList::construct(void *o) {
	REG(Frame::s_plglist, fr, nfr, "f") REG(Geometry::s_plglist, geo, ngeo, "g") REG(Material::s_plglist, mat, nmat, "m") REG(Texture::s_plglist, tex, ntex, "t")
	for (auto &p : plgTable[this]) if (p.ctor) p.ctor(o, p.offset, p.size);
}
void PluginList::destruct(void *o) {
	for (auto &p : plgTable[this]) if (p.dtor) p.dtor(o, p.offset, p.size);
	UNREG(Frame::s_plglist, fr, nfr, "f") UNREG(Geometry::s_plglist, geo, ngeo, "g") UNREG(Material::s_plglist, mat, nmat, "m") UNREG(Texture::s_plglist, tex, ntex, "t")
}
void PluginList::copy(void *dst, void *src) {
	for (auto &p : plgTable[this]) if (p.copy) p.copy(dst, src, p.offset, p.size);
}
int32 PluginList::registerStream(uint32, StreamRead, StreamWrite, StreamGetSize) { return 0; }
int32 PluginList::setStreamRightsCallback(uint32, RightsCallback) { return 0; }
int32 PluginList::setStreamAlwaysCallback(uint32, AlwaysCallback) { return 0; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool PluginList::streamRead(Stream *, void *) { return true; }
bool writeChunkHeader(Stream *, int32, int32) { return true; }
bool findChunk(Stream *, uint32, uint32 *, uint32 *) { return false; }
uint32 Stream::write8(const void *, uint32 n) { return n; }
uint32 Stream::write16(const void *, uint32 n) { return n; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
uint32 Stream::read8(void *, uint32 n) { return n; }
uint32 Stream::read32(void *, uint32 n) { return n; }
int32 Stream::writeI16(int16) { return 2; }
int32 Stream::writeI32(int32) { return 4; }
int32 Stream::writeU32(uint32) { return 4; }
int32 Stream::writeF32(float32) { return 4; }
int16 Stream::readI16(void) { return 0; }
int32 Stream::readI32(void) { return 0; }
uint32 Stream::readU32(void) { return 0; }
float32 Stream::readF32(void) { return 0; }
int32 findPointer(void *, void **, int32) { return -1; }
int strncmp_ci(const char *s1, const char *s2, int n) { return strncasecmp(s1, s2, n); }
Raster *Raster::create(int32, int32, int32, int32, int32) { return nil; }
void Raster::destroy(void) {}
Raster *Raster::createFromImage(Image *, int32) { return nil; }
Image *Image::readMasked(const char *, const char *) { return nil; }
void Image::destroy(void) {}
ObjPipeline *ObjPipeline::create(void) { return nil; }
void ObjPipeline::destroy(void) {}
namespace wgpu { void *destroyNativeData(void *o, int32, int32) { return o; } void initSkin(void) {} void initMatFX(void) {} }
}

static uint32 rng = 0x6a09e667u ^ 0x1234;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
struct Gen { uint32 s; uint32 next(void) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; } };

static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static void hbytes(const void *p, int n) { for (int i = 0; i < n; i++) hb(((const uint8*)p)[i]); }
static void hptr(const void *p, const void *base) { hi(p == nil ? -1 : (int32)((const uint8*)p - (const uint8*)base)); }
static Matrix uvA, uvB;	// the uv transform matrices the recorder hands out (hashed as 1 and 2)
static int32 uvId(const Matrix *m) { return m == nil ? 0 : m == &uvA ? 1 : m == &uvB ? 2 : 3; }

static void hashHier(HAnimHierarchy *hr) {
	hi(hr->flags); hi(hr->numNodes);
	hi(hr->matrices != nil); hi(hr->matricesUnaligned != nil);
	hi(hr->matrices ? (int32)((uintptr)hr->matrices & 0xF) : -1);
	hi(FID(hr->parentFrame)); hi(hr->parentHierarchy == hr);
	for (int i = 0; i < hr->numNodes; i++) { hi(hr->nodeInfo[i].id); hi(hr->nodeInfo[i].index); hi(hr->nodeInfo[i].flags); hi(FID(hr->nodeInfo[i].frame)); }
	AnimInterpolator *in = hr->interpolator;
	hi(in->currentAnim != nil); hbytes(&in->currentTime, 4); hi(in->nextFrame != nil);
	hi(in->maxInterpKeyFrameSize); hi(in->currentInterpKeyFrameSize); hi(in->currentAnimKeyFrameSize); hi(in->numNodes);
}

static void state(void) {
	h = 0xcbf29ce484222325ull;
	for (int i = 0; i < nfr; i++) {
		Frame *f = (Frame*)fr[i];
		if (!f) continue;
		HAnimData *d = HAnimData::get(f);
		hi(i); hi(FID(f->getParent())); hi(FID(f->child)); hi(FID(f->next)); hi(FID(f->root));
		hi(d->id); hi(d->hierarchy != nil);
		if (d->hierarchy) { hi(FID(d->hierarchy->parentFrame)); if (d->hierarchy->parentFrame == f) hashHier(d->hierarchy); }
	}
	for (int i = 0; i < ngeo; i++) {
		Geometry *g = (Geometry*)geo[i];
		if (!g) continue;
		Skin *s = Skin::get(g);
		hi(i); hi(g->numVertices); hi(s != nil);
		if (!s) continue;
		hi(s->numBones); hi(s->numUsedBones); hi(s->numWeights);
		hptr(s->usedBones, s->data); hptr(s->inverseMatrices, s->data); hptr(s->indices, s->data); hptr(s->weights, s->data);
		hi(s->inverseMatrices ? (int32)((uintptr)s->inverseMatrices & 0xF) : -1);
		if (s->usedBones) hbytes(s->usedBones, s->numUsedBones);
		if (s->inverseMatrices) hbytes(s->inverseMatrices, s->numBones * 64);
		if (s->indices) hbytes(s->indices, g->numVertices * 4);
		if (s->weights) hbytes(s->weights, g->numVertices * 16);
		hi(s->boneLimit); hi(s->numMeshes); hi(s->rleSize); hi(s->remapIndices != nil); hi(s->rleCount != nil); hi(s->rle != nil);
		hi(s->platformData != nil); hi(s->legacyType);
	}
	for (int i = 0; i < nmat; i++) {
		Material *m = (Material*)mat[i];
		if (!m) continue;
		MatFX *fx = MatFX::get(m);
		hi(i); hi(m->refCount); hi(TID(m->texture)); hi(fx != nil); hi(MatFX::getEffects(m));
		if (!fx) continue;
		hi(fx->type);
		for (int k = 0; k < 2; k++) {
			hi(fx->fx[k].type);
			switch (fx->fx[k].type) {
			case MatFX::BUMPMAP: hi(FID(fx->fx[k].bump.frame)); hi(TID(fx->fx[k].bump.bumpedTex)); hi(TID(fx->fx[k].bump.tex)); hbytes(&fx->fx[k].bump.coefficient, 4); break;
			case MatFX::ENVMAP: hi(FID(fx->fx[k].env.frame)); hi(TID(fx->fx[k].env.tex)); hbytes(&fx->fx[k].env.coefficient, 4); hi(fx->fx[k].env.fbAlpha); break;
			case MatFX::DUAL: hi(TID(fx->fx[k].dual.tex)); hi(fx->fx[k].dual.srcBlend); hi(fx->fx[k].dual.dstBlend); break;
			case MatFX::UVTRANSFORM: hi(uvId(fx->fx[k].uvtransform.baseTransform)); hi(uvId(fx->fx[k].uvtransform.dualTransform)); break;
			}
		}
	}
	for (int i = 0; i < ntex; i++) { Texture *t = (Texture*)tex[i]; if (!t) continue; hi(i); hi(t->refCount); }
	printf(" => %016llx |%s\n", h, plog);
	plog[0] = 0;
}

static int pick(void **arr, int n) { int l[MAXID], k = 0; for (int i = 0; i < n; i++) if (arr[i]) l[k++] = i; return k ? l[next() % k] : -1; }
static bool inHier(Frame *root, Frame *g) { for (; g; g = g->getParent()) if (g == root) return true; return false; }
// librw's stale-flag case (see the frame recorder): a cloned frame can carry HIERARCHYSYNC without
// being in the dirty list; destroying it unlinks garbage. Trees with one are left alone.
static bool inDirty(Frame *f) { FORLIST(lnk, engine->frameDirtyList) if (LLLinkGetData(lnk, Frame, inDirtyList) == f) return true; return false; }
static bool anyStale(Frame *f) {
	if (f == nil) return false;
	if ((f->object.privateFlags & Frame::HIERARCHYSYNC) && !inDirty(f)) return true;
	return anyStale(f->child) || anyStale(f->next);
}
static int mref[MAXID], tref[MAXID];	// references the recorder holds itself

int
main(void)
{
	engine = (Engine*)calloc(1, 16384);
	engine->frameDirtyList.init();
	Texture::registerModule();
	extern void *textureOpenForRecorder(void);
	textureOpenForRecorder();
	registerHAnimPlugin();
	registerSkinPlugin();
	registerMatFXPlugin();
	for (int step = 0; step < 9000 && nfr < MAXID - 200 && ngeo < MAXID - 4 && nmat < MAXID - 4; step++) {
		int f = pick(fr, nfr), f2 = pick(fr, nfr), g = pick(geo, ngeo), m = pick(mat, nmat), t = pick(tex, ntex);
		Frame *F = f >= 0 ? (Frame*)fr[f] : nil;
		Geometry *G = g >= 0 ? (Geometry*)geo[g] : nil;
		Material *M = m >= 0 ? (Material*)mat[m] : nil;
		Texture *T = t >= 0 ? (Texture*)tex[t] : nil;
		HAnimHierarchy *H = F ? HAnimData::get(F)->hierarchy : nil;
		switch (next() % 30) {
		case 0: case 1: { printf("newframe ="); Frame *x = Frame::create(); printf(" %d", FID(x)); state(); break; }
		case 2: case 3: if (!F || f2 < 0 || inHier((Frame*)fr[f2], F)) break; printf("addchild %d %d", f, f2); F->addChild((Frame*)fr[f2]); state(); break;
		case 4: { if (!F) break; int id = next() % 3 == 0 ? -1 : next() % 16; printf("hanimid %d %d", f, id); HAnimData::get(F)->id = id; state(); break; }
		case 5: {
			// as rw/DffRead.cpp builds them: on a frame without one
			if (!F || H) break;
			int n = 1 + next() % 20, hflags = (int[]){ 0, HAnimHierarchy::NOMATRICES, HAnimHierarchy::LOCALSPACEMATRICES, HAnimHierarchy::SUBHIERARCHY }[next() % 4];
			uint32 seed = next();
			int maxkey = next() % 2 ? 36 : 24;
			int32 ids[20], flags[20];
			Gen gg = { seed | 1 };
			for (int i = 0; i < n; i++) { ids[i] = gg.next() % 16; flags[i] = gg.next() % 4; }
			bool noIds = next() % 5 == 0;
			printf("newhier %d %d %d %u %d %d", f, n, hflags, seed, maxkey, noIds);
			HAnimHierarchy *x = HAnimHierarchy::create(n, noIds ? nil : flags, noIds ? nil : ids, hflags, maxkey);
			HAnimData::get(F)->hierarchy = x;
			x->parentFrame = F;
			state(); break;
		}
		case 6: if (!H || H->parentFrame != F) break; printf("attach %d", f); H->attach(); state(); break;
		case 7: { if (!H || H->parentFrame != F) break; int i = next() % H->numNodes; printf("attachidx %d %d", f, i); H->attachByIndex(i); state(); break; }
		case 8: { if (!H) break; int id = next() % 18 - 1; printf("getindex %d %d = %d", f, id, H->getIndex(id)); state(); break; }
		case 9: { if (!H || f2 < 0) break; printf("getindexframe %d %d = %d", f, f2, H->getIndex((Frame*)fr[f2])); state(); break; }
		case 10: { if (!F) break; HAnimHierarchy *x = HAnimHierarchy::find(F); printf("find %d = %d", f, x ? FID(x->parentFrame) : -1); state(); break; }
		case 11: { if (!F) break; HAnimHierarchy *x = HAnimHierarchy::get(F); printf("get %d = %d", f, x ? FID(x->parentFrame) : -1); state(); break; }
		case 12: { if (!F || F->getParent()) break; printf("clone %d =", f); Frame *x = F->cloneHierarchy(); printf(" %d", FID(x)); state(); break; }
		case 13: {
			// whole hierarchies only (node frames point into them)
			if (!F || F->getParent() || (F->object.privateFlags & Frame::HIERARCHYSYNC && !inDirty(F)) || anyStale(F->child)) break;
			bool ok = true;	// no hierarchy elsewhere points into this tree
			for (int i = 0; i < nfr; i++) {
				Frame *o = (Frame*)fr[i];
				if (!o || inHier(F, o)) continue;
				HAnimHierarchy *x = HAnimData::get(o)->hierarchy;
				if (x) for (int k = 0; k < x->numNodes; k++) if (x->nodeInfo[k].frame && inHier(F, x->nodeInfo[k].frame)) ok = false;
			}
			for (int i = 0; i < nmat; i++) {
				MatFX *fx = mat[i] ? MatFX::get((Material*)mat[i]) : nil;
				if (fx) for (int k = 0; k < 2; k++)
					if ((fx->fx[k].type == MatFX::ENVMAP && fx->fx[k].env.frame && inHier(F, fx->fx[k].env.frame)) ||
					    (fx->fx[k].type == MatFX::BUMPMAP && fx->fx[k].bump.frame && inHier(F, fx->fx[k].bump.frame))) ok = false;
			}
			if (!ok) break;
			printf("delhier %d", f); F->destroyHierarchy(); state(); break;
		}
		case 14: { int nv = 1 + next() % 30; printf("newgeo %d =", nv); Geometry *x = Geometry::create(nv, 0, 0); printf(" %d", GID(x)); state(); break; }
		case 15: {
			// rw/DffRead.cpp's skin: allocated, set, init(numBones, numUsedBones, numVertices)
			if (!G || Skin::get(G)) break;
			int nb = next() % 40, nub = next() % 2 ? nb : next() % (nb + 1);
			uint32 seed = next();
			printf("skin %d %d %d %u", g, nb, nub, seed);
			Skin *s = rwNewT(Skin, 1, MEMDUR_EVENT | ID_SKIN);
			Skin::set(G, s);
			s->init(nb, nub, G->numVertices);
			Gen gg = { seed | 1 };
			if (s->usedBones) for (int i = 0; i < nub; i++) s->usedBones[i] = gg.next();
			if (s->inverseMatrices) for (int i = 0; i < nb * 16; i++) s->inverseMatrices[i] = (int32)(gg.next() % 2001 - 1000) / 100.0f;
			if (s->indices) for (int i = 0; i < G->numVertices * 4; i++) s->indices[i] = nb ? gg.next() % nb : 0;
			if (s->weights) for (int i = 0; i < G->numVertices; i++) {
				int used = gg.next() % 5;
				for (int k = 0; k < 4; k++) s->weights[i * 4 + k] = k < used ? (gg.next() % 100) / 100.0f : 0.0f;
			}
			s->numWeights = gg.next() % 5;
			state(); break;
		}
		case 16: { if (!G || !Skin::get(G)) break; printf("numweights %d", g); Skin::get(G)->findNumWeights(G->numVertices); state(); break; }
		case 17: {
			Skin *s = G ? Skin::get(G) : nil;
			// usedBones has room for numUsedBones entries; findUsedBones fills one per bone in use
			if (!s || !s->usedBones || !s->indices || s->numUsedBones < s->numBones) break;
			printf("usedbones %d", g); s->findUsedBones(G->numVertices); state(); break;
		}
		case 18: if (!G) break; printf("delgeo %d", g); G->destroy(); state(); break;
		case 19: { printf("newmat ="); Material *x = Material::create(); printf(" %d", MID(x)); mref[MID(x)] = 1; state(); break; }
		case 20: { if (!M) break; printf("clonemat %d =", m); Material *x = M->clone(); printf(" %d", MID(x)); mref[MID(x)] = 1; state(); break; }
		case 21: if (!M || mref[m] == 0) break; mref[m]--; printf("delmat %d", m); M->destroy(); state(); break;
		case 22: { if (!M) break; int type = next() % 7; printf("effects %d %d", m, type); MatFX::setEffects(M, type); state(); break; }
		case 23: {
			MatFX *fx = M ? MatFX::get(M) : nil;
			if (!fx) break;
			int op = next() % 8;
			Texture *tt = next() % 3 == 0 ? nil : T;
			float c = uni(-2, 2);
			switch (op) {
			case 0: printf("bumptex %d %d", m, TID(tt)); fx->setBumpTexture(tt); break;
			case 1: printf("bumpcoef %d %08x", m, *(uint32*)&c); fx->setBumpCoefficient(c); break;
			case 2: printf("envtex %d %d", m, TID(tt)); fx->setEnvTexture(tt); break;
			case 3: { Frame *ff = next() % 3 == 0 ? nil : F; printf("envframe %d %d", m, FID(ff)); fx->setEnvFrame(ff); break; }
			case 4: printf("envcoef %d %08x", m, *(uint32*)&c); fx->setEnvCoefficient(c); break;
			case 5: { int a = next() % 2; printf("envfbalpha %d %d", m, a); fx->setEnvFBAlpha(a); break; }
			case 6: printf("dualtex %d %d", m, TID(tt)); fx->setDualTexture(tt); break;
			default: { int s = next() % 12, d = next() % 12; printf("dualblend %d %d %d", m, s, d); fx->setDualSrcBlend(s); fx->setDualDestBlend(d); break; }
			}
			state(); break;
		}
		case 24: {
			MatFX *fx = M ? MatFX::get(M) : nil;
			if (!fx) break;
			int a = next() % 3, b = next() % 3;
			Matrix *ma = a == 0 ? nil : a == 1 ? &uvA : &uvB, *mb = b == 0 ? nil : b == 1 ? &uvA : &uvB;
			printf("uvxform %d %d %d", m, a, b); fx->setUVTransformMatrices(ma, mb); state(); break;
		}
		case 25: {
			MatFX *fx = M ? MatFX::get(M) : nil;
			if (!fx) break;
			Matrix *ma, *mb;
			fx->getUVTransformMatrices(&ma, &mb);
			float bc = fx->getBumpCoefficient(), ec = fx->getEnvCoefficient();
			printf("getters %d = %d %08x %d %d %08x %d %d %d %d %d %d", m, TID(fx->getBumpTexture()), *(uint32*)&bc, TID(fx->getEnvTexture()), FID(fx->getEnvFrame()),
			       *(uint32*)&ec, fx->getEnvFBAlpha(), TID(fx->getDualTexture()), fx->getDualSrcBlend(), fx->getDualDestBlend(), uvId(ma), uvId(mb));
			printf(" %d %d %d %d %d %d %d", fx->getEffectIndex(0), fx->getEffectIndex(1), fx->getEffectIndex(2), fx->getEffectIndex(3), fx->getEffectIndex(4), fx->getEffectIndex(5), fx->getEffectIndex(6));
			state(); break;
		}
		case 26: { printf("newtex ="); Texture *x = Texture::create(nil); printf(" %d", TID(x)); tref[TID(x)] = 1; state(); break; }
		case 27: if (!T || tref[t] == 0) break; tref[t]--; printf("deltex %d", t); T->destroy(); state(); break;
		case 28: if (!M) break; printf("mattex %d %d", m, TID(T)); M->setTexture(T); state(); break;
		default: { if (!M) break; printf("geteffects %d = %u", m, MatFX::getEffects(M)); state(); break; }
		}
	}
	return 0;
}

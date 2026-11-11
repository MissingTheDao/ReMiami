// Records librw's geometry and material objects (geometry.cpp: Geometry create/destroy/lock/
// unlock/addMorphTargets/allocateData/calculateBoundingSphere/hasColoredMaterial/buildMeshes,
// MaterialList, Material create/clone/destroy/setTexture; geoplg.cpp: allocateMeshes,
// setupIndices, guessNumTriangles; with texture.cpp's textures) for the Rust port
// (rust/revc/src/rwgeomobj.rs). Geometries g<n>, materials m<n>, textures t<n> get ids from
// their plugin constructors. Every line: op args [= result] => H | log, H an FNV-1a 64 of every
// live object's fields and data (pointers into an object's own blocks as offsets), log the
// plugin calls.
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
static void *geos[MAXID], *mats[MAXID], *texs[MAXID];
static int ngeos, nmats, ntexs;
// references the recorder itself holds (it releases only those, as a well-behaved user does)
static int gref[MAXID], mref[MAXID], tref[MAXID];
static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)
static int idOf(void **arr, int n, void *p) { if (!p) return -1; for (int i = 0; i < n; i++) if (arr[i] == p) return i; return -2; }
#define GID(p) idOf(geos, ngeos, p)
#define MID(p) idOf(mats, nmats, p)
#define TID(p) idOf(texs, ntexs, p)

namespace rw {
Engine *engine;
int32 version = 0x36003;
// zeroed blocks with their size in front, so a grown block's new part is zero too (librw's
// realloc leaves it undefined; the hash reads it)
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
void PluginList::construct(void *o) {
	if (this == &Geometry::s_plglist) { geos[ngeos] = o; LOG(" cg%d", ngeos++); }
	if (this == &Material::s_plglist) { mats[nmats] = o; LOG(" cm%d", nmats++); }
	if (this == &Texture::s_plglist) { texs[ntexs] = o; LOG(" ct%d", ntexs++); }
}
void PluginList::destruct(void *o) {
	if (this == &Geometry::s_plglist) { int i = GID(o); LOG(" dg%d", i); if (i >= 0) geos[i] = nil; }
	if (this == &Material::s_plglist) { int i = MID(o); LOG(" dm%d", i); if (i >= 0) mats[i] = nil; }
	if (this == &Texture::s_plglist) { int i = TID(o); LOG(" dt%d", i); if (i >= 0) texs[i] = nil; }
}
void PluginList::copy(void *dst, void *src) {
	if (this == &Material::s_plglist) LOG(" ym%d,%d", MID(dst), MID(src));
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
int32 Stream::writeI16(int16) { return 2; }
int32 Stream::writeI32(int32) { return 4; }
int32 Stream::writeU32(uint32) { return 4; }
int32 Stream::writeF32(float32) { return 4; }
int16 Stream::readI16(void) { return 0; }
int32 Stream::readI32(void) { return 0; }
int strncmp_ci(const char *s1, const char *s2, int n) { return strncasecmp(s1, s2, n); }
Raster *Raster::create(int32, int32, int32, int32, int32) { return nil; }
void Raster::destroy(void) {}
Raster *Raster::createFromImage(Image *, int32) { return nil; }
Image *Image::readMasked(const char *, const char *) { return nil; }
void Image::destroy(void) {}
namespace wgpu { void *destroyNativeData(void *o, int32, int32) { return o; } }
}

static uint32 rng = 0x1f83d9abu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }

static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static void hbytes(const void *p, int n) { for (int i = 0; i < n; i++) hb(((const uint8*)p)[i]); }
// a pointer: -1 null, else its offset from base (or -2 when outside [base, base+len))
static void hptr(const void *p, const void *base, long len) {
	if (!p) { hi(-1); return; }
	long d = (const uint8*)p - (const uint8*)base;
	hi(base && d >= 0 && d < len + 1 ? (int32)d : -2);
}

static void state(void) {
	h = 0xcbf29ce484222325ull;
	for (int i = 0; i < ngeos; i++) {
		Geometry *g = (Geometry*)geos[i];
		if (!g) continue;
		hi(i); hb(g->object.type); hb(g->object.subType); hb(g->object.flags); hb(g->object.privateFlags);
		hi(g->flags); hi(g->lockedSinceInst); hi(g->numTriangles); hi(g->numVertices); hi(g->numMorphTargets); hi(g->numTexCoordSets);
		// the attribute block: triangles, colours, tex coords
		long alen = g->numTriangles * 8 + ((g->flags & Geometry::PRELIT) ? g->numVertices * 4 : 0) + g->numTexCoordSets * g->numVertices * 8;
		uint8 *ab = (uint8*)g->triangles;
		hi(ab != nil);
		if (ab) hbytes(ab, alen);
		hptr(g->colors, ab, alen);
		for (int k = 0; k < 8; k++) hptr(g->texCoords[k], ab, alen);
		// morph targets and their vertex data block
		uint8 *mb = (uint8*)g->morphTargets;
		long mlen = g->numMorphTargets * (long)sizeof(MorphTarget);
		if (!(g->flags & Geometry::NATIVE)) mlen += g->numMorphTargets * g->numVertices * 12L * ((g->flags & Geometry::NORMALS) ? 2 : 1);
		for (int k = 0; k < g->numMorphTargets; k++) {
			MorphTarget *m = &g->morphTargets[k];
			hi(GID(m->parent)); hbytes(&m->boundingSphere, 16);
			hptr(m->vertices, mb, mlen); hptr(m->normals, mb, mlen);
			if (m->vertices) hbytes(m->vertices, g->numVertices * 12);
			if (m->normals) hbytes(m->normals, g->numVertices * 12);
		}
		hi(g->matList.numMaterials); hi(g->matList.space);
		for (int k = 0; k < g->matList.numMaterials; k++) hi(MID(g->matList.materials[k]));
		MeshHeader *mh = g->meshHeader;
		hi(mh != nil);
		if (mh) {
			hi(mh->flags); hi(mh->numMeshes); hi(mh->serialNum); hi(mh->totalIndices);
			Mesh *ms = mh->getMeshes();
			long ilen = mh->totalIndices * 2L;
			uint8 *ib = (uint8*)&ms[mh->numMeshes];
			for (int k = 0; k < mh->numMeshes; k++) {
				hi(ms[k].numIndices); hi(MID(ms[k].material)); hptr(ms[k].indices, ib, ilen);
				if (ms[k].indices) hbytes(ms[k].indices, ms[k].numIndices * 2);
			}
		}
		hi(g->instData != nil); hi(g->refCount);
	}
	for (int i = 0; i < nmats; i++) {
		Material *m = (Material*)mats[i];
		if (!m) continue;
		hi(i); hi(TID(m->texture)); hbytes(&m->color, 4); hbytes(&m->surfaceProps, 12); hi(m->pipeline != nil); hi(m->refCount);
	}
	for (int i = 0; i < ntexs; i++) {
		Texture *t = (Texture*)texs[i];
		if (!t) continue;
		hi(i); hi(t->refCount);
	}
	printf(" => %016llx |%s | %d %d\n", h, plog, Geometry::numAllocated, Material::numAllocated);
	plog[0] = 0;
}

static int pick(void **arr, int n) { int l[MAXID], k = 0; for (int i = 0; i < n; i++) if (arr[i]) l[k++] = i; return k ? l[next() % k] : -1; }

int
main(void)
{
	engine = (Engine*)calloc(1, 8192);
	Texture::registerModule();
	// textureOpen's state, as the texture recorder sets it up
	extern void *textureOpenForRecorder(void);
	textureOpenForRecorder();
	for (int step = 0; step < 3000 && ngeos < MAXID - 4 && nmats < MAXID - 4; step++) {
		int g = pick(geos, ngeos), m = pick(mats, nmats), m2 = pick(mats, nmats), t = pick(texs, ntexs);
		Geometry *G = g >= 0 ? (Geometry*)geos[g] : nil;
		Material *M = m >= 0 ? (Material*)mats[m] : nil;
		switch (next() % 20) {
		case 0: case 1: {
			int nv = next() % 6 == 0 ? 0 : 1 + next() % 40, nt = next() % 6 == 0 ? 0 : 1 + next() % 60;
			uint32 fl = 0;
			if (next() % 2) fl |= Geometry::TRISTRIP;
			if (next() % 2) fl |= Geometry::POSITIONS;
			if (next() % 2) fl |= Geometry::TEXTURED;
			if (next() % 3 == 0) fl |= Geometry::TEXTURED2;
			if (next() % 2) fl |= Geometry::PRELIT;
			if (next() % 2) fl |= Geometry::NORMALS;
			if (next() % 2) fl |= Geometry::LIGHT;
			if (next() % 3 == 0) fl |= Geometry::MODULATE;
			if (next() % 4 == 0) fl |= (next() % 9) << 16;
			if (next() % 8 == 0) fl |= Geometry::NATIVE;
			printf("newgeo %d %d %u =", nv, nt, fl);
			Geometry *x = Geometry::create(nv, nt, fl);
			printf(" %d", GID(x));
			gref[GID(x)] = 1;
			state(); break;
		}
		case 2: if (!G || gref[g] == 0) break; gref[g]--; printf("delgeo %d", g); G->destroy(); state(); break;
		case 3: if (!G) break; gref[g]++; printf("addrefgeo %d", g); G->addRef(); state(); break;
		case 4: { if (!G || G->numMorphTargets > 6) break; int n = next() % 3; printf("addmorph %d %d", g, n); G->addMorphTargets(n); state(); break; }
		case 5: {
			// fill the attributes and vertices so the data moves are visible
			if (!G) break;
			uint32 seed = next();
			printf("fill %d %u", g, seed);
			uint32 s = seed | 1;
			#define NX (s ^= s << 13, s ^= s >> 17, s ^= s << 5, s)
			int nm = G->matList.numMaterials;
			if (G->triangles)
				for (int i = 0; i < G->numTriangles; i++) {
					Triangle *tr = &G->triangles[i];
					if (G->numVertices >= 3) {
						tr->v[0] = NX % G->numVertices;
						do tr->v[1] = NX % G->numVertices; while (tr->v[1] == tr->v[0]);
						do tr->v[2] = NX % G->numVertices; while (tr->v[2] == tr->v[0] || tr->v[2] == tr->v[1]);
					}
					tr->matId = nm ? NX % nm : 0xFFFF;
				}
			if (G->colors) for (int i = 0; i < G->numVertices * 4; i++) ((uint8*)G->colors)[i] = NX;
			for (int k = 0; k < G->numTexCoordSets; k++) if (G->texCoords[k]) for (int i = 0; i < G->numVertices * 2; i++) ((float*)G->texCoords[k])[i] = (NX & 0xFFFF) / 256.0f;
			for (int k = 0; k < G->numMorphTargets; k++) {
				MorphTarget *mt = &G->morphTargets[k];
				if (mt->vertices) for (int i = 0; i < G->numVertices * 3; i++) ((float*)mt->vertices)[i] = (int32)(NX % 20001 - 10000) / 100.0f;
				if (mt->normals) for (int i = 0; i < G->numVertices * 3; i++) ((float*)mt->normals)[i] = (int32)(NX % 2001 - 1000) / 1000.0f;
			}
			state(); break;
		}
		case 6: if (!G || !(G->flags & Geometry::NATIVE)) break; printf("allocdata %d", g); G->allocateData(); G->flags &= ~Geometry::NATIVE; state(); break;
		case 7: if (!G || (G->flags & Geometry::NATIVE)) break; printf("spheres %d", g); G->calculateBoundingSphere(); state(); break;
		case 8: if (!G) break; printf("colored %d = %d", g, G->hasColoredMaterial() ? 1 : 0); state(); break;
		case 9: { if (!G) break; int lf = next() % 2 ? Geometry::LOCKALL : next() & 0xFFF; printf("lock %d %d", g, lf); G->lock(lf); state(); break; }
		case 10: {
			// unlock rebuilds the meshes when they were dropped: needs valid triangles
			if (!G || G->meshHeader || (G->flags & Geometry::NATIVE) || G->matList.numMaterials == 0 || G->numVertices < 3) break;
			bool ok = true;
			for (int i = 0; i < G->numTriangles; i++) if (G->triangles[i].matId >= G->matList.numMaterials || G->triangles[i].v[0] == G->triangles[i].v[1]) ok = false;
			if (!ok) break;
			if (G->flags & Geometry::TRISTRIP) {
				// librw's strip check needs every material used
				bool used[64] = {};
				for (int i = 0; i < G->numTriangles; i++) used[G->triangles[i].matId] = true;
				for (int i = 0; i < G->matList.numMaterials; i++) if (!used[i]) ok = false;
				if (!ok || G->numTriangles == 0) break;
			}
			printf("unlock %d", g); G->unlock(); state(); break;
		}
		case 11: { if (!G || G->matList.numMaterials > 40) break; if (!M) break; printf("append %d %d =", g, m); printf(" %d", G->matList.appendMaterial(M)); state(); break; }
		case 12: if (!G || !M) break; printf("find %d %d = %d", g, m, G->matList.findIndex(M)); state(); break;
		case 13: {
			if (!G || G->numTriangles == 0 || (G->flags & Geometry::NATIVE)) break;
			int n = next() % 4, idx = next() % 300, noidx = next() % 3 == 0;
			// meshes that are kept keep their index counts: room for them, as callers make sure
			if (G->meshHeader) {
				int keep = 0;
				for (int k = 0; k < n && k < G->meshHeader->numMeshes; k++) keep += G->meshHeader->getMeshes()[k].numIndices;
				if (idx < keep) idx = keep;
			}
			printf("allocmeshes %d %d %d %d", g, n, idx, noidx); G->allocateMeshes(n, idx, noidx); state(); break;
		}
		case 14: if (!G || !G->meshHeader || G->meshHeader->numMeshes == 0) break;	// reads the first mesh printf("setupidx %d", g); G->meshHeader->setupIndices(); state(); break;
		case 15: if (!G || !G->meshHeader) break; printf("guess %d = %u", g, G->meshHeader->guessNumTriangles()); state(); break;
		case 16: {
			printf("newmat =");
			Material *x = Material::create();
			printf(" %d", MID(x));
			mref[MID(x)] = 1;
			if (next() % 2) { x->color.red = next(); x->color.green = next(); x->color.blue = next(); x->color.alpha = next(); printf(" color %08x", *(uint32*)&x->color); }
			state(); break;
		}
		case 17: {
			if (!M) break;
			int op = next() % 3;
			if (op == 0) { printf("clonemat %d =", m); Material *x = M->clone(); printf(" %d", MID(x)); mref[MID(x)] = 1; }
			else if (op == 1) { if (mref[m] == 0) break; mref[m]--; printf("delmat %d", m); M->destroy(); }
			else { mref[m]++; printf("addrefmat %d", m); M->addRef(); }
			state(); break;
		}
		case 18: {
			if (!M) break;
			int op = next() % 3;
			if (op == 0) { printf("newtex ="); Texture *x = Texture::create(nil); printf(" %d", TID(x)); tref[TID(x)] = 1; }
			else if (op == 1 && t >= 0) { printf("settex %d %d", m, t); M->setTexture((Texture*)texs[t]); }
			else { printf("settex %d -1", m); M->setTexture(nil); }
			state(); break;
		}
		default: if (t < 0 || tref[t] == 0) break; tref[t]--; printf("deltex %d", t); ((Texture*)texs[t])->destroy(); state(); break;
		}
	}
	return 0;
}

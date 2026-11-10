// Records librw's geometry math (base.cpp: V3d::transformPoints/transformVectors; geometry.cpp:
// MorphTarget::calculateBoundingSphere, Geometry::buildMeshes with tristrip.cpp's strip builder),
// compiled like the game (clang++ -O2), for the Rust port (rust/revc/src/rwgeometry.rs).
// Floats as 8-digit hex. Lines:
//   tpoints|tvectors matrix n points => points
//   sphere n vertices => center radius
//   meshes numVertices tristrip numMaterials | triangles (v0 v1 v2 matId)
//       => flags | per mesh: numIndices indices
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

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void setError(Error *) {}
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
// unused here: plugin lists, streams, textures, the wgpu instance data
int32 version;
void PluginList::construct(void *) {}
void PluginList::destruct(void *) {}
void PluginList::copy(void *, void *) {}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
int32 PluginList::registerStream(uint32, StreamRead, StreamWrite, StreamGetSize) { return 0; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
uint32 Stream::write16(const void *, uint32 n) { return n; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
int32 Stream::writeI32(int32) { return 4; }
bool writeChunkHeader(Stream *, int32, int32) { return true; }
void Texture::destroy(void) {}
uint32 Texture::streamGetSize(void) { return 0; }
bool Texture::streamWrite(Stream *) { return true; }
namespace wgpu { void *destroyNativeData(void *o, int32, int32) { return o; } }
}

static uint32 rng = 0xbb67ae85u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void v3(const V3d &v) { f(v.x); f(v.y); f(v.z); }
static void m(const Matrix &x) { v3(x.right); printf(" %08x", x.flags); v3(x.up); printf(" --------"); v3(x.at); printf(" --------"); v3(x.pos); printf(" --------"); }

static Material *mats[8];

static void
meshes(int nverts, Triangle *tris, int ntris, int tristrip, int nmats)
{
	Geometry *g = (Geometry*)calloc(1, sizeof(Geometry));
	g->flags = tristrip ? Geometry::TRISTRIP : 0;
	g->numVertices = nverts;
	g->numTriangles = ntris;
	g->triangles = tris;
	g->matList.numMaterials = nmats;
	g->matList.materials = mats;
	printf("meshes %d %d %d |", nverts, tristrip, nmats);
	for (int i = 0; i < ntris; i++) printf(" %d %d %d %d", tris[i].v[0], tris[i].v[1], tris[i].v[2], tris[i].matId);
	fflush(stdout);
	g->buildMeshes();
	MeshHeader *h = g->meshHeader;
	printf(" => %d |", h->flags);
	Mesh *ms = h->getMeshes();
	for (uint32 i = 0; i < h->numMeshes; i++) {
		printf(" %d", ms[i].numIndices);
		for (uint32 j = 0; j < ms[i].numIndices; j++) printf(" %d", ms[i].indices[j]);
	}
	printf("\n");
	free(h);
	free(g);
}

int
main(void)
{
	for (int i = 0; i < 8; i++) mats[i] = (Material*)calloc(1, 64);

	// transforms
	for (int i = 0; i < 3000; i++) {
		Matrix x;
		V3d axis = { uni(-1, 1), uni(-1, 1), uni(-1, 1) };
		Matrix::makeRotation(&x, &axis, uni(-180, 180));
		x.pos.set(uni(-2000, 2000), uni(-2000, 2000), uni(-100, 300));
		if (next() % 4 == 0) { V3d s = { uni(0.1f, 3), uni(0.1f, 3), uni(0.1f, 3) }; x.scale(&s, COMBINEPOSTCONCAT); }
		int n = 1 + next() % 4;
		V3d p[4], o[4];
		for (int k = 0; k < n; k++) p[k].set(uni(-100, 100), uni(-100, 100), uni(-100, 100));
		bool pts = i % 2 == 0;
		printf(pts ? "tpoints" : "tvectors"); m(x); printf(" %d", n); for (int k = 0; k < n; k++) v3(p[k]);
		bool alias = next() % 2;
		V3d *out = alias ? p : o;
		if (pts) V3d::transformPoints(out, p, n, &x); else V3d::transformVectors(out, p, n, &x);
		printf(" =>"); for (int k = 0; k < n; k++) v3(out[k]); printf("\n");
	}

	// morph target bounding spheres
	for (int i = 0; i < 2000; i++) {
		Geometry g = {};
		MorphTarget mt = {};
		int n = i % 50 == 0 ? 0 : 1 + next() % 40;
		V3d v[40];
		float sc = next() % 2 ? uni(0.1f, 5) : uni(10, 3000);
		V3d c = { uni(-2000, 2000), uni(-2000, 2000), uni(-100, 300) };
		for (int k = 0; k < n; k++) v[k].set(c.x + uni(-sc, sc), c.y + uni(-sc, sc), c.z + uni(-sc, sc));
		g.numVertices = n;
		mt.parent = &g;
		mt.vertices = v;
		printf("sphere %d", n); for (int k = 0; k < n; k++) v3(v[k]);
		Sphere s = mt.calculateBoundingSphere();
		printf(" =>"); v3(s.center); f(s.radius); printf("\n");
	}

	// meshes: grids like the game's water (17x17 and 33x33 vertices), with dropped and shuffled
	// triangles and several materials; random triangle soups over small vertex pools
	static Triangle tris[4096];
	for (int c = 0; c < 600; c++) {
		int tristrip = c % 3 != 0;
		int nmats = 1 + next() % 4;
		int ntris = 0, nverts;
		if (c % 2 == 0) {
			int w = 2 + next() % 32, h = 2 + next() % 32;
			if (c % 20 == 0) w = h = 17;
			if (c % 20 == 10) w = h = 33;
			nverts = w * h;
			bool drop = next() % 3 == 0, flip = next() % 2;
			for (int y = 0; y < h - 1; y++)
				for (int x = 0; x < w - 1; x++) {
					int a = y * w + x, b = a + 1, d = a + w, e = d + 1;
					int mat = nmats == 1 ? 0 : next() % 4 == 0 ? next() % nmats : (x * nmats / w);
					Triangle t1 = { { (uint16)a, (uint16)d, (uint16)b }, (uint16)mat };
					Triangle t2 = { { (uint16)b, (uint16)d, (uint16)e }, (uint16)mat };
					if (flip) { t1.v[0] = a; t1.v[1] = d; t1.v[2] = e; t2.v[0] = a; t2.v[1] = e; t2.v[2] = b; }
					if (!(drop && next() % 8 == 0)) tris[ntris++] = t1;
					if (!(drop && next() % 8 == 0)) tris[ntris++] = t2;
				}
			if (next() % 3 == 0)
				for (int i = ntris - 1; i > 0; i--) { int j = next() % (i + 1); Triangle t = tris[i]; tris[i] = tris[j]; tris[j] = t; }
		} else {
			nverts = 3 + next() % 30;
			ntris = 1 + next() % 60;
			for (int i = 0; i < ntris; i++) {
				uint16 a = next() % nverts, b, d;
				do b = next() % nverts; while (b == a);
				do d = next() % nverts; while (d == a || d == b);
				tris[i].v[0] = a; tris[i].v[1] = b; tris[i].v[2] = d;
				tris[i].matId = next() % nmats;
			}
		}
		if (ntris == 0) { tris[0].v[0] = 0; tris[0].v[1] = 1; tris[0].v[2] = 2; tris[0].matId = 0; ntris = 1; }
		// the strip builder needs every material used (librw reads past an empty mesh)
		if (tristrip) {
			bool used[8] = {};
			for (int i = 0; i < ntris; i++) used[tris[i].matId] = true;
			int remap[8], k = 0;
			for (int i = 0; i < nmats; i++) remap[i] = used[i] ? k++ : -1;
			for (int i = 0; i < ntris; i++) tris[i].matId = remap[tris[i].matId];
			nmats = k;
		}
		meshes(nverts, tris, ntris, tristrip, nmats);
	}
	return 0;
}

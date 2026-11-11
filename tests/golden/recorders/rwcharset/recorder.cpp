// Records librw's debug font (charset.cpp: open/close, create/setColors/destroy, print,
// printBuffered, flushBuffer with its render states and Im2D batch) for the Rust port
// (rust/revc/src/rwcharset.rs). Images, rasters, render states and Im2D are stubs that log
// what the charset asks for. Lines: op args [= result] => H | log.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwrender.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"
#include "rwcharset.h"
#include "wgpu/rwwgpu.h"

using namespace rw;

static char plog[1 << 20];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)

static uint64 fnv(const void *p, size_t n, uint64 h = 0xcbf29ce484222325ull) {
	for (size_t i = 0; i < n; i++) { h ^= ((const uint8*)p)[i]; h *= 0x100000001b3ull; }
	return h;
}

// rasters are never freed (a flush after destroy logs a destroyed raster's id)
static Raster *rasters[4096];
static int nrasters;
static int rid(Raster *r) { for (int i = 0; i < nrasters; i++) if (rasters[i] == r) return i + 1; return r ? -1 : 0; }

static Camera camera;
static float32 nearZ;
static uint32 states[32];

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void setError(Error *) {}
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }

Image *Image::create(int32 w, int32 h, int32 d) { Image *i = (Image*)calloc(1, sizeof(Image)); i->width = w; i->height = h; i->depth = d; return i; }
void Image::allocate(void) { if (this->palette == nil) this->palette = (uint8*)calloc(1, 256 * 4); }
void Image::destroy(void) { ::free(this->palette); ::free(this); }
Raster *Raster::createFromImage(Image *img, int32) {
	Raster *r = (Raster*)calloc(1, sizeof(Raster));
	r->width = img->width; r->height = img->height;
	rasters[nrasters++] = r;
	LOG(" raster%d:%dx%dx%d:s%d:pal", nrasters, img->width, img->height, img->depth, img->stride);
	for (int i = 0; i < 8; i++) LOG("%02x", img->palette[i]);
	LOG(":%016llx", (unsigned long long)fnv(img->pixels, img->stride * img->height));
	return r;
}
void Raster::destroy(void) { LOG(" rdestroy%d", rid(this)); }
void SetRenderState(int32 s, uint32 v) { states[s] = v; LOG(" rs%d=%u", s, v); }
void SetRenderStatePtr(int32 s, void *v) { LOG(" rsp%d=r%d", s, rid((Raster*)v)); }
uint32 GetRenderState(int32 s) { return states[s]; }
namespace im2d {
float32 GetNearZ(void) { return nearZ; }
void RenderIndexedPrimitive(PrimitiveType t, void *v, int32 nv, void *ix, int32 ni) {
	LOG(" draw%d:%d:%d:%016llx:%016llx", t, nv, ni, (unsigned long long)fnv(v, nv * sizeof(wgpu::Im2DVertex)),
	    (unsigned long long)fnv(ix, ni * 2));
}
}
}

static uint32 rng = 0x6c1b2d33u;
static uint32 rnd(void) { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float32 fbits(uint32 u) { float32 f; memcpy(&f, &u, 4); return f; }

static Charset *cs[8];
static bool opened;

int main(void) {
	static Engine eng;
	engine = &eng;
	eng.currentCamera = &camera;
	camera.nearPlane = 0.9f;
	for (int n = 0; n < 6000; n++) {
		char line[4096];
		int res = -1000000;
		plog[0] = 0;
		uint32 op = rnd() % 100;
		int k = rnd() % 8;
		if (op < 4) { res = Charset::open(); opened = opened || res; sprintf(line, "open"); }
		else if (op < 6) { Charset::close(); opened = false; sprintf(line, "close"); }
		else if (op < 14) {
			RGBA fg, bg; uint32 a = rnd(), b = rnd();
			memcpy(&fg, &a, 4); memcpy(&bg, &b, 4);
			if (cs[k]) cs[k]->destroy();
			cs[k] = Charset::create(&fg, &bg);
			res = cs[k] != nil;
			sprintf(line, "create %d %u %u", k, a, b);
		} else if (op < 18) {
			if (cs[k]) { cs[k]->destroy(); cs[k] = nil; }
			sprintf(line, "destroy %d", k);
		} else if (op < 24) {
			RGBA fg, bg; uint32 a = rnd(), b = rnd();
			memcpy(&fg, &a, 4); memcpy(&bg, &b, 4);
			if (cs[k]) res = cs[k]->setColors(&fg, &bg) == cs[k];
			sprintf(line, "setcolors %d %u %u", k, a, b);
		} else if (op < 30) {
			float32 f = (rnd() % 2000 + 1) / 1000.0f;
			uint32 u; memcpy(&u, &f, 4);
			camera.nearPlane = f;
			sprintf(line, "near %u", u);
		} else if (op < 32) {
			nearZ = (rnd() % 3) / 2.0f;
			uint32 u; memcpy(&u, &nearZ, 4);
			sprintf(line, "nearz %u", u);
		} else if (op < 36) {
			uint32 s = rnd() % 32, v = rnd() % 4;
			states[s] = v;
			sprintf(line, "state %u %u", s, v);
		} else if (op < 42) { if (opened) Charset::flushBuffer(); sprintf(line, "flush %d", opened); }
		else {
			// print / printBuffered: only with the buffers open and a charset
			char str[64]; int len = rnd() % 48;
			for (int i = 0; i < len; i++) { uint32 c = rnd() % 4 ? 32 + rnd() % 95 : 1 + rnd() % 255; str[i] = rnd() % 6 ? (char)c : ' '; }
			str[len] = 0;
			int x = (int)(rnd() % 700) - 30, y = (int)(rnd() % 500) - 20, hide = rnd() % 2;
			bool buffered = op < 80;
			if (opened && cs[k]) {
				if (buffered) cs[k]->printBuffered(str, x, y, hide);
				else cs[k]->print(str, x, y, hide);
			}
			char hex[160]; hex[0] = 0;
			for (int i = 0; i < len; i++) sprintf(hex + 2 * i, "%02x", (uint8)str[i]);
			sprintf(line, "%s %d %d %d %d %d %s", buffered ? "printb" : "print", k, x, y, hide, opened && cs[k] ? 1 : 0, len ? hex : "-");
		}
		uint64 h = 0xcbf29ce484222325ull;
		for (int i = 0; i < 8; i++) {
			int32 v[9] = { cs[i] != nil, 0, 0, 0, 0, 0, 0, 0, 0 };
			if (cs[i]) { memcpy(&v[1], &cs[i]->desc, 7 * 4); v[8] = rid(cs[i]->raster); }
			h = fnv(v, sizeof(v), h);
		}
		h = fnv(&opened, 1, h);
		if (res != -1000000) printf("%s = %d => %016llx |%s\n", line, res, (unsigned long long)h, plog);
		else printf("%s => %016llx |%s\n", line, (unsigned long long)h, plog);
	}
	return 0;
}

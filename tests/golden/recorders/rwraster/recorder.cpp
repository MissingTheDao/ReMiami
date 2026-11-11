// Records librw's rasters with revc's wgpu raster driver (raster.cpp: create, destroy, subRaster,
// lock/unlock, getNumLevels, calculateNumLevels, formatHasAlpha, imageFindRasterFormat,
// setFromImage, toImage, the context stack; wgpu/wgpuraster.cpp: rasterCreate, rasterLock,
// rasterUnlock, rasterFromImage, rasterToImage, allocateDXT, uploadRaster and the native raster
// plugin) for the Rust port (rust/revc/src/rwraster.rs). Rasters get ids r<n> in creation order.
// Every line is an operation, its result and the state after it:
//   op args [= result] => H | log
// H: FNV-1a 64 of every live raster's fields (levels as their sizes and contents, locked pixels as
// the level they point at) and the context stack; log: the plugin, upload and evict calls.
// Images come from rwimage's seeded generator (P = seed depth width height pad ncolors alpha).
#include <string.h>
#include "wgpu/wgpuraster.cpp"

using namespace rw;
using namespace rw::wgpu;

// raster.cpp's (file-local) module globals, same layout
struct RasterGlobals { int32 sp; Raster *stack[32]; };
namespace rw { extern int32 rasterModuleOffset; }

enum { MAXID = 4096 };
static Raster *ras[MAXID];
static int nras;
static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)
static int rid(Raster *r) { if (!r) return -1; for (int i = 0; i < nras; i++) if (ras[i] == r) return i; return -2; }

namespace rw {
Engine *engine;
int32 platform = PLATFORM_WGPU;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void setError(Error *) {}
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
PluginList Engine::s_plglist;
PluginList Raster::s_plglist(sizeof(Raster));
int32 PluginList::registerPlugin(int32 size, uint32, Constructor, Destructor, CopyConstructor) {
	int32 off = this->size; this->size += size; return off;
}
void PluginList::construct(void *o) {
	if (this != &Raster::s_plglist) return;
	ras[nras] = (Raster*)o; LOG(" c%d", nras++);
	createNativeRaster(o, nativeRasterOffset, 0);
}
void PluginList::destruct(void *o) {
	if (this != &Raster::s_plglist) return;
	int i = rid((Raster*)o); LOG(" d%d", i);
	destroyNativeRaster(o, nativeRasterOffset, 0);
	if (i >= 0) ras[i] = nil;
}
char *strdup_LOC(const char *s, uint32, const char *) { return strdup(s); }
void makePath(char *) {}	// unused here: image files
namespace wgpu { void evictRaster(Raster *r) { LOG(" evict%d", rid(r)); } }
}
static uint32 nextTex = 1;
extern "C" uint32_t revc_rw_texture_upload(uint32_t id, int32_t w, int32_t h, int32_t dxt, int32_t n, const uint8_t *const *levels, int32_t autogen) {
	LOG(" up%u:%dx%d,%d,%d,%d", id, w, h, dxt, n, autogen);
	return id ? id : nextTex++;
}
extern "C" void revc_rw_texture_destroy(uint32_t id) { LOG(" tdel%u", id); }

struct Gen { uint32 s; uint32 next(void) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; } };
struct P { uint32 seed; int depth, w, h, pad, ncolors, amode; };
static Image *
makeImage(P p)	// rwimage's generator
{
	Gen g = { p.seed | 1 };
	uint8 tab[256][4];
	for (int i = 0; i < p.ncolors; i++) {
		tab[i][0] = g.next() & 255;
		tab[i][1] = g.next() & 255;
		tab[i][2] = g.next() & 255;
		tab[i][3] = p.amode == 0 ? 255 : p.amode == 1 ? g.next() & 255 : (g.next() & 1 ? 255 : 0);
	}
	int bpp = p.depth < 8 ? 1 : p.depth / 8;
	int stride = p.w * bpp + p.pad;
	uint8 *pixels = (uint8*)calloc(1, stride * p.h + 1);
	for (int y = 0; y < p.h; y++) {
		uint8 *row = pixels + y * stride;
		for (int x = 0; x < p.w; x++) {
			int k = g.next() % p.ncolors;
			uint8 *q = row + x * bpp, *c = tab[k];
			switch (p.depth) {
			case 4: case 8: q[0] = k; break;
			case 16: { uint32 v = (c[3] >= 128) << 15 | (c[0] >> 3) << 10 | (c[1] >> 3) << 5 | c[2] >> 3; q[0] = v & 255; q[1] = v >> 8; break; }
			case 24: q[0] = c[0]; q[1] = c[1]; q[2] = c[2]; break;
			case 32: q[0] = c[0]; q[1] = c[1]; q[2] = c[2]; q[3] = c[3]; break;
			}
		}
		memset(row + p.w * bpp, 0xCD, p.pad);
	}
	uint8 *pal = nil;
	if (p.depth <= 8) {
		int n = 1 << p.depth;
		pal = (uint8*)malloc(n * 4);
		for (int i = 0; i < n; i++)
			for (int j = 0; j < 4; j++)
				pal[i * 4 + j] = i < p.ncolors ? tab[i][j] : g.next() & 255;
	}
	Image *img = Image::create(p.w, p.h, p.depth);
	img->stride = stride;
	img->pixels = pixels;
	img->palette = pal;
	img->flags = 3;
	return img;
}

static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static int32 levelSize(Raster *r, WgpuRaster *n, int i) {
	int32 w = n->baseWidth >> i, hh = n->baseHeight >> i;
	if (w < 1) w = 1;
	if (hh < 1) hh = 1;
	return n->dxt ? dxtLevelSize(n->dxt, w, hh) : w * hh * 4;
}
static void state(void) {
	h = 0xcbf29ce484222325ull;
	for (int i = 0; i < nras; i++) {
		Raster *r = ras[i];
		if (!r) continue;
		WgpuRaster *n = GETWGPURASTEREXT(r);
		hi(i); hi(r->platform); hi(r->type); hi(r->flags); hi(r->privateFlags); hi(r->format);
		hi(r->width); hi(r->height); hi(r->depth); hi(r->stride);
		int pl = -1;
		for (int k = 0; k < n->numLevels; k++) if (r->pixels == n->levels[k]) pl = k;
		hi(r->pixels ? pl : -2); hi(r->palette != nil); hi(rid(r->parent)); hi(r->offsetX); hi(r->offsetY);
		hi(n->numLevels); hi(n->hasAlpha); hi(n->baseWidth); hi(n->baseHeight); hi(n->dxt); hi(n->texId);
		hi(n->dirty); hi(n->autogenMipmap); hi(n->maxAnisotropy); hi(n->filterMode); hi(n->addressU); hi(n->addressV);
		for (int k = 0; k < n->numLevels; k++) {
			int32 s = levelSize(r, n, k);
			hi(s);
			for (int j = 0; j < s; j++) hb(n->levels[k][j]);
		}
	}
	RasterGlobals *g = PLUGINOFFSET(RasterGlobals, engine, rasterModuleOffset);
	hi(g->sp);
	for (int i = 0; i <= g->sp && i < 32; i++) hi(rid(g->stack[i]));
	printf(" => %016llx |%s\n", h, plog);
	plog[0] = 0;
}
static void hashImage(Image *img) {
	if (!img) { printf(" nil"); return; }
	unsigned long long s = h;
	h = 0xcbf29ce484222325ull;
	hi(img->width); hi(img->height); hi(img->depth); hi(img->stride);
	for (int y = 0; y < img->height; y++)
		for (int x = 0; x < img->width * img->bpp; x++) hb(img->pixels[y * img->stride + x]);
	printf(" %016llx", h);
	h = s;
}

static uint32 rng = 0x9b05688cu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static int pick(void) { int l[MAXID], n = 0; for (int i = 0; i < nras; i++) if (ras[i]) l[n++] = i; return n ? l[next() % n] : -1; }
static const int depths[] = { 4, 8, 16, 24, 32 };
static P rp(void) {
	P p;
	p.seed = next(); p.depth = depths[next() % 5];
	p.w = 1 + next() % 40; p.h = 1 + next() % 40; p.pad = next() % 3 ? 0 : next() % 8;
	int maxc = p.depth <= 8 ? 1 << p.depth : 256;
	p.ncolors = 1 + (next() % 2 ? next() % maxc : next() % (maxc < 20 ? maxc : 20));
	p.amode = next() % 3;
	return p;
}
static void pp(P p) { printf(" %u %d %d %d %d %d %d", p.seed, p.depth, p.w, p.h, p.pad, p.ncolors, p.amode); }

int
main(void)
{
	engine = (Engine*)calloc(1, 8192);
	static Driver drv;
	engine->driver[PLATFORM_WGPU] = &drv;
	drv.rasterCreate = rasterCreate;
	drv.rasterLock = rasterLock;
	drv.rasterUnlock = rasterUnlock;
	drv.rasterNumLevels = rasterNumLevels;
	drv.imageFindRasterFormat = imageFindRasterFormat;
	drv.rasterFromImage = rasterFromImage;
	drv.rasterToImage = rasterToImage;
	registerNativeRaster();
	rasterModuleOffset = 4096;	// as rasterOpen sets it up
	PLUGINOFFSET(RasterGlobals, engine, rasterModuleOffset)->sp = -1;
	static const int types[] = { Raster::NORMAL, Raster::ZBUFFER, Raster::CAMERA, Raster::TEXTURE, Raster::TEXTURE, Raster::CAMERATEXTURE, 3 };
	static const int fmts[] = { 0, Raster::C1555, Raster::C565, Raster::C4444, Raster::LUM8, Raster::C8888, Raster::C888, Raster::C555 };
	for (int step = 0; step < 2500 && nras < MAXID - 4; step++) {
		int a = pick(), b = pick();
		Raster *r = a >= 0 ? ras[a] : nil;
		switch (next() % 18) {
		case 0: case 1: case 2: {
			int w = 1 + next() % 70, hh = 1 + next() % 70;
			if (next() % 4 == 0) { w = 1 << next() % 7; hh = 1 << next() % 7; }
			int fmt = types[next() % 7] | fmts[next() % 8];
			if (next() % 3 == 0) fmt |= Raster::MIPMAP;
			if (next() % 4 == 0) fmt |= Raster::AUTOMIPMAP;
			if (next() % 5 == 0) fmt |= next() % 2 ? Raster::PAL8 : Raster::PAL4;
			if (next() % 3 == 0) fmt |= Raster::DONTALLOCATE;
			int depth = (int[]){ 0, 8, 16, 24, 32 }[next() % 5];
			printf("create %d %d %d %d =", w, hh, depth, fmt);
			Raster *x = Raster::create(w, hh, depth, fmt);
			printf(" %d", rid(x));
			state(); break;
		}
		case 3: if (!r || r->privateFlags) break; printf("destroy %d", a); r->destroy(); state(); break;
		case 4: {
			if (!r || b < 0 || !(r->flags & Raster::DONTALLOCATE)) break;
			Rect rc = { (int32)(next() % 20), (int32)(next() % 20), (int32)(1 + next() % 30), (int32)(1 + next() % 30) };
			printf("sub %d %d %d %d %d %d", a, b, rc.x, rc.y, rc.w, rc.h);
			r->subRaster(ras[b], &rc); state(); break;
		}
		case 5: case 6: {
			if (!r || r->privateFlags) break;
			int lv = next() % 4, mode = (int[]){ Raster::LOCKREAD, Raster::LOCKWRITE, Raster::LOCKWRITE | Raster::LOCKNOFETCH, Raster::LOCKREAD | Raster::LOCKWRITE }[next() % 4];
			printf("lock %d %d %d =", a, lv, mode);
			uint8 *p = r->lock(lv, mode);
			printf(" %d", p != nil);
			state(); break;
		}
		case 7: if (!r) break; printf("unlock %d", a); r->unlock(0); state(); break;
		case 8: if (!r) break; printf("numlevels %d = %d", a, r->getNumLevels()); state(); break;
		case 9: case 10: {
			// the driver writes through a null pointer for a raster without levels (made with
			// DONTALLOCATE) that isn't locked; the game never does that
			if (!r || (!r->pixels && GETWGPURASTEREXT(r)->numLevels == 0)) break;
			// a locked mip level of an odd width: rasterLock halves width and stride separately,
			// so the stride no longer matches and the copy runs past the level (not done by the game)
			if (r->pixels && r->stride != r->width * 4) break;
			P p = rp();
			Image *img = makeImage(p);
			printf("fromimage %d", a); pp(p); printf(" =");
			Raster *x = r->setFromImage(img);
			printf(" %d", x != nil);
			img->destroy();
			state(); break;
		}
		case 11: {
			// librw's DXT decoder writes whole 4x4 blocks
			if (!r || (GETWGPURASTEREXT(r)->dxt && (r->width % 4 || r->height % 4))) break;
			if (!GETWGPURASTEREXT(r)->dxt && r->pixels && r->stride != r->width * 4) break;	// as above
			printf("toimage %d =", a); Image *img = r->toImage(); hashImage(img); if (img) img->destroy(); state(); break; }
		case 12: {
			// as TXD loading does: a fresh texture raster without levels, then allocateDXT
			int w = 4 * (1 + next() % 16), hh = 4 * (1 + next() % 16);
			int fmt = Raster::TEXTURE | Raster::DONTALLOCATE | (next() % 2 ? Raster::C8888 : Raster::C565);
			if (next() % 2) fmt |= Raster::MIPMAP;
			if (next() % 4 == 0) fmt |= Raster::AUTOMIPMAP;
			int dxt = (int[]){ 1, 3, 5 }[next() % 3], nl = 1 + next() % 8, ha = next() % 2;
			uint32 seed = next();
			printf("dxt %d %d %d %d %d %d %u =", w, hh, fmt, dxt, nl, ha, seed);
			Raster *x = Raster::create(w, hh, 16, fmt);
			printf(" %d", rid(x));
			allocateDXT(x, dxt, nl, ha);
			WgpuRaster *n = GETWGPURASTEREXT(x);
			Gen g = { seed | 1 };
			for (int k = 0; k < n->numLevels; k++) {
				int32 s = levelSize(x, n, k);
				for (int j = 0; j < s; j++) n->levels[k][j] = g.next() & 255;
			}
			state(); break;
		}
		case 13: if (!r) break; printf("upload %d = %u", a, uploadRaster(r)); state(); break;
		case 14: {
			int w = next() % 3000, hh = next() % 3000, f = next() & 0xFFFF;
			printf("calc %d %d %d = %d %d", w, hh, f, Raster::calculateNumLevels(w, hh), Raster::formatHasAlpha(f) ? 1 : 0);
			state(); break;
		}
		case 15: {
			P p = rp();
			Image *img = makeImage(p);
			int type = next() % 8;
			int32 w = -1, hh = -1, d = -1, f = -1;
			printf("findfmt %d", type); pp(p);
			int ok = Raster::imageFindRasterFormat(img, type, &w, &hh, &d, &f);
			printf(" = %d %d %d %d %d", ok, w, hh, d, f);
			img->destroy();
			state(); break;
		}
		case 16: if (!r) break; printf("push %d = %d", a, rid(Raster::pushContext(r))); state(); break;
		default:
			if (next() % 2) { printf("pop = %d", rid(Raster::popContext())); }
			else printf("current = %d", rid(Raster::getCurrentContext()));
			state(); break;
		}
	}
	return 0;
}

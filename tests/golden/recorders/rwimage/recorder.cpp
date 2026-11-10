// Records librw's image pixel functions (image.cpp: setPixelsDXT with decompressDXT1/3/5,
// hasAlpha, convertTo32, unpalettize, palettize with ColorQuant, makeMask, applyMask,
// removeMask), compiled like the game (clang++ -O2), for the Rust port (rust/revc/src/rwimage.rs).
// Test images come from a seed (makeImage() below; the Rust test has the same generator), so a line
// holds the parameters and a hash of the result:
//   P = seed depth width height pad ncolors alphamode
//   hasalpha P => 0|1        unpal P force => H     to32 P => H       rmmask P => H
//   mkmask P => H            apmask P | P(mask) => H                 pal P depth => H
//   dxt type seed width height => H
// H: FNV-1a 64 over depth, bpp, stride, flags (4 bytes each, little endian), each row's
// width*bpp bytes, and for depth <= 8 the palette entries up to the highest index used.
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
// unused here: the file search
void makePath(char *) {}
char *strdup_LOC(const char *s, uint32, const char *) { return strdup(s); }
PluginList Engine::s_plglist;
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
}

struct Gen {
	uint32 s;
	uint32 next(void) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; }
};

struct P { uint32 seed; int depth, w, h, pad, ncolors, amode; };

static Image *
makeImage(P p)
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
	uint8 *pixels = (uint8*)malloc(stride * p.h + 1);
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
static void hash(Image *img) {
	h = 0xcbf29ce484222325ull;
	hi(img->depth); hi(img->bpp); hi(img->stride); hi(img->flags);
	int maxi = -1;
	for (int y = 0; y < img->height; y++)
		for (int x = 0; x < img->width * img->bpp; x++) {
			uint8 b = img->pixels[y * img->stride + x];
			hb(b);
			if (b > maxi) maxi = b;
		}
	if (img->depth <= 8)
		for (int i = 0; i < (maxi + 1) * 4; i++) hb(img->palette[i]);
	printf(" => %016llx\n", h);
}

static uint32 rng = 0xa54ff53au;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static const int depths[] = { 4, 8, 16, 24, 32 };

static P rp(void) {
	P p;
	p.seed = next();
	p.depth = depths[next() % 5];
	p.w = 1 + next() % 40;
	p.h = 1 + next() % 40;
	p.pad = next() % 3 ? 0 : next() % 8;
	int maxc = p.depth <= 8 ? 1 << p.depth : 256;
	p.ncolors = 1 + (next() % 2 ? next() % maxc : next() % (maxc < 20 ? maxc : 20));
	p.amode = next() % 3;
	return p;
}
static void pp(P p) { printf(" %u %d %d %d %d %d %d", p.seed, p.depth, p.w, p.h, p.pad, p.ncolors, p.amode); }

int
main(void)
{
	for (int i = 0; i < 4000; i++) {
		P p = rp();
		Image *img = makeImage(p);
		switch (i % 8) {
		case 0: printf("hasalpha"); pp(p); printf(" => %d\n", img->hasAlpha() ? 1 : 0); break;
		case 1: { int f = next() % 2; printf("unpal"); pp(p); printf(" %d", f); img->unpalettize(f); hash(img); break; }
		case 2: printf("to32"); pp(p); img->convertTo32(); hash(img); break;
		case 3: printf("rmmask"); pp(p); img->removeMask(); hash(img); break;
		case 4: printf("mkmask"); pp(p); img->makeMask(); hash(img); break;
		case 5: {
			P q = rp();
			if (next() % 4) { q.w = p.w; q.h = p.h; }
			Image *m = makeImage(q);
			printf("apmask"); pp(p); printf(" |"); pp(q);
			img->applyMask(m); hash(img);
			m->destroy();
			break;
		}
		case 6: { int d = next() % 2 ? 4 : 8; printf("pal"); pp(p); printf(" %d", d); img->palettize(d); hash(img); break; }
		default: {
			static const int types[] = { 1, 3, 5 };
			int t = types[next() % 3];
			uint32 seed = next();
			int w = 4 * (1 + next() % 16), hh = 4 * (1 + next() % 16);
			Gen g = { seed | 1 };
			int n = t == 1 ? w * hh / 2 : w * hh;
			uint8 *src = (uint8*)malloc(n);
			for (int k = 0; k < n; k++) src[k] = g.next() & 255;
			Image *d = Image::create(w, hh, 32);
			d->allocate();
			printf("dxt %d %u %d %d", t, seed, w, hh);
			d->setPixelsDXT(t, src);
			hash(d);
			d->destroy();
			free(src);
			break;
		}
		}
		img->destroy();
	}
	return 0;
}

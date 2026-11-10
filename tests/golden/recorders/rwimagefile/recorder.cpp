// Records librw's image files (png.cpp, bmp.cpp, tga.cpp with lodepng; image.cpp's search path
// and getFilename; base.cpp's makePath/correctPathCase) for the Rust port
// (rust/revc/src/rwimagefile.rs). It writes a corpus of image files of every kind it reads to
// tests/golden/imagefiles/ (made here, synthetic) and, to the golden file (argv[1]):
//   png|bmp|tga FILE => H | nil          what the reader makes of the corpus file
//   wbmp|wtga P => F                     F: FNV-1a 64 of the written file
//   wpng P => D                          D: FNV-1a 64 of width, height and the written file
//                                        decoded to RGBA (lodepng's conversion)
//   tree PATH ...                        a directory tree (files), made in a temp directory
//   path NAME => RESULT                  makePath
//   search PATHS NAME => RESULT | -      setSearchPath + getFilename
// H as in rwimage (depth, bpp, stride, flags, rows, palette up to the highest index used);
// P = seed depth width height pad ncolors alphamode, the rwimage generator.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>
#include "rwbase.h"
#include "rwerror.h"
#include "rwplg.h"
#include "rwpipeline.h"
#include "rwobjects.h"
#include "rwengine.h"
#include "lodepng/lodepng.h"

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
PluginList Engine::s_plglist;
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
char *strdup_LOC(const char *s, uint32, const char *) { return strdup(s); }
}
extern int32 imageModuleOffset_;

static FILE *out;

struct Gen {
	uint32 s;
	uint32 next(void) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; }
};
struct P { uint32 seed; int depth, w, h, pad, ncolors, amode; };

// rwimage's generator (same code as tests/golden/recorders/rwimage)
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
static void hashImage(Image *img) {
	if (img == nil) { fprintf(out, " => nil\n"); return; }
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
	fprintf(out, " => %016llx\n", h);
}
static unsigned long long hashFile(const char *name, bool decode) {
	h = 0xcbf29ce484222325ull;
	if (decode) {
		uint8 *px; uint32 w, hh;
		if (lodepng_decode32_file(&px, &w, &hh, name)) return 0;
		hi(w); hi(hh);
		for (uint32 i = 0; i < w * hh * 4; i++) hb(px[i]);
		free(px);
		return h;
	}
	FILE *f = fopen(name, "rb");
	int c;
	while ((c = fgetc(f)) != EOF) hb(c);
	fclose(f);
	return h;
}

static uint32 rng = 0x510e527fu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static const int depths[] = { 4, 8, 16, 24, 32 };
static P rp(int depth) {
	P p;
	p.seed = next();
	p.depth = depth ? depth : depths[next() % 5];
	p.w = 2 + next() % 24;
	p.h = 2 + next() % 24;
	p.pad = 0;
	int maxc = p.depth <= 8 ? 1 << p.depth : 256;
	p.ncolors = 1 + (next() % 2 ? next() % maxc : next() % (maxc < 20 ? maxc : 20));
	p.amode = next() % 3;
	return p;
}
static void pp(P p) { fprintf(out, " %u %d %d %d %d %d %d", p.seed, p.depth, p.w, p.h, p.pad, p.ncolors, p.amode); }

static char corpus[4096];
static int nfiles;
static const char *cname(const char *ext) {
	static char buf[4200];
	sprintf(buf, "%s/%03d.%s", corpus, nfiles++, ext);
	return buf;
}

// PNG files of every colour type and bit depth lodepng writes, some interlaced, some with tRNS
static void
pngs(void)
{
	struct { LodePNGColorType ct; int bd; } kinds[] = {
		{ LCT_PALETTE, 1 }, { LCT_PALETTE, 2 }, { LCT_PALETTE, 4 }, { LCT_PALETTE, 8 },
		{ LCT_GREY, 1 }, { LCT_GREY, 2 }, { LCT_GREY, 4 }, { LCT_GREY, 8 }, { LCT_GREY, 16 },
		{ LCT_GREY_ALPHA, 8 }, { LCT_GREY_ALPHA, 16 }, { LCT_RGB, 8 }, { LCT_RGB, 16 },
		{ LCT_RGBA, 8 }, { LCT_RGBA, 16 },
	};
	for (int n = 0; n < 90; n++) {
		int k = n % 15;
		LodePNGState st;
		lodepng_state_init(&st);
		st.encoder.auto_convert = 0;
		st.info_png.interlace_method = next() % 4 == 0;
		LodePNGColorMode *cm = &st.info_png.color;
		cm->colortype = kinds[k].ct;
		cm->bitdepth = kinds[k].bd;
		uint32 w = 1 + next() % 23, hh = 1 + next() % 23;
		int palsize = 0;
		if (kinds[k].ct == LCT_PALETTE) {
			static const int sizes[] = { 5, 16, 17, 256 };
			palsize = kinds[k].bd < 8 ? 1 << kinds[k].bd : sizes[next() % 4];
			bool alpha = next() % 2;
			for (int i = 0; i < palsize; i++)
				lodepng_palette_add(cm, next() & 255, next() & 255, next() & 255, alpha ? next() & 255 : 255);
		} else if (kinds[k].ct != LCT_GREY_ALPHA && kinds[k].ct != LCT_RGBA && next() % 3 == 0) {
			cm->key_defined = 1;
			uint32 mx = (1u << kinds[k].bd) - 1;
			cm->key_r = next() & mx; cm->key_g = next() & mx; cm->key_b = next() & mx;
			if (kinds[k].ct == LCT_GREY) cm->key_g = cm->key_b = cm->key_r;
		}
		lodepng_color_mode_copy(&st.info_raw, cm);
		size_t rawsize = lodepng_get_raw_size(w, hh, cm);
		uint8 *raw = (uint8*)malloc(rawsize);
		for (size_t i = 0; i < rawsize; i++) raw[i] = next() & 255;
		if (kinds[k].ct == LCT_PALETTE && kinds[k].bd == 8)
			for (size_t i = 0; i < rawsize; i++) raw[i] %= palsize;
		// some pixels equal to the key colour
		if (cm->key_defined && kinds[k].bd == 8 && rawsize >= 3)
			for (size_t i = 0; i + 3 <= rawsize; i += 7 * (kinds[k].ct == LCT_RGB ? 3 : 1)) {
				raw[i] = cm->key_r;
				if (kinds[k].ct == LCT_RGB) { raw[i + 1] = cm->key_g; raw[i + 2] = cm->key_b; }
			}
		uint8 *png; size_t pngsize;
		if (lodepng_encode(&png, &pngsize, raw, w, hh, &st)) { fprintf(stderr, "encode failed\n"); exit(1); }
		const char *name = cname("png");
		lodepng_save_file(png, pngsize, name);
		free(png); free(raw);
		lodepng_state_cleanup(&st);
		fprintf(out, "png %s", strrchr(name, '/') + 1);
		hashImage(readPNG(name));
	}
}

static void put16(FILE *f, int v) { fputc(v & 255, f); fputc((v >> 8) & 255, f); }
static void put32(FILE *f, uint32 v) { for (int i = 0; i < 4; i++) fputc((v >> (8 * i)) & 255, f); }

// BMP files: writeBMP's own and hand-made ones (bitfields, bigger headers, odd 4-bit widths)
static void
bmps(void)
{
	for (int n = 0; n < 60; n++) {
		const char *name = cname("bmp");
		if (n < 30) {
			P p = rp(n % 5 == 0 ? 4 : 0);
			if (p.depth == 4 && n % 10 == 5) p.w |= 1;
			Image *img = makeImage(p);
			writeBMP(img, name);
			img->destroy();
			// librw's reader skips width*bpp % 4 bytes per row, not the writer's padding, and
			// runs past the end of some of its own files (reading stack garbage there); slack
			// keeps the recording to defined reads
			FILE *f = fopen(name, "ab");
			for (int i = 0; i < 256; i++) fputc(0x55, f);
			fclose(f);
		} else {
			// headerSize 40/52/56/108, BI_RGB or BI_BITFIELDS, depth 8/24/32
			static const int hs[] = { 40, 52, 56, 108 };
			int hsz = hs[next() % 4], depth = (int[]){ 8, 24, 32, 32, 16 }[next() % 5];
			int comp = depth == 32 && next() % 2 ? 3 : 0;
			int w = 1 + next() % 20, hh = 1 + next() % 20;
			int pallen = depth == 8 ? 256 : 0;
			int rowbytes = w * depth / 8, pad = rowbytes % 4;
			int offset = 14 + hsz + pallen * 4 + (next() % 3) * 4;
			FILE *f = fopen(name, "wb");
			put16(f, 0x4D42); put32(f, 0); put16(f, 0); put16(f, 0); put32(f, offset);
			put32(f, hsz); put32(f, w); put32(f, hh); put16(f, 1); put16(f, depth); put32(f, comp);
			put32(f, 0); put32(f, 2835); put32(f, 2835); put32(f, 0); put32(f, 0);
			for (int i = 40; i < hsz; i += 4) {
				uint32 v = next();
				if (i == 40) v = comp == 3 && next() % 4 ? 0xFF0000 : v;
				if (i == 44) v = 0x00FF00;
				if (i == 48) v = 0x0000FF;
				if (i == 52) v = next() % 2 ? 0xFF000000 : 0;
				put32(f, v);
			}
			for (int i = 0; i < pallen * 4; i++) fputc(next() & 255, f);
			while (ftell(f) < offset) fputc(0x77, f);
			for (int y = 0; y < hh; y++) {
				for (int i = 0; i < rowbytes; i++) fputc(next() & 255, f);
				for (int i = 0; i < pad; i++) fputc(0, f);
			}
			for (int i = 0; i < 64; i++) fputc(0x55, f);	// slack: librw skips by the wrong padding
			fclose(f);
		}
		fprintf(out, "bmp %s", strrchr(name, '/') + 1);
		hashImage(readBMP(name));
	}
}

// TGA files: writeTGA's own and hand-made ones (bottom-up, 24-bit palettes, short palettes)
static void
tgas(void)
{
	for (int n = 0; n < 40; n++) {
		const char *name = cname("tga");
		if (n < 20) {
			Image *img = makeImage(rp(0));
			writeTGA(img, name);
			img->destroy();
		} else {
			int cmap = next() % 2, w = 1 + next() % 20, hh = 1 + next() % 20;
			int maplen = cmap ? (int[]){ 3, 16, 17, 200, 256 }[next() % 5] : 0;
			int mapdepth = cmap ? (next() % 2 ? 24 : 32) : 0;
			int depth = cmap ? 8 : (int[]){ 16, 24, 32 }[next() % 3];
			int idlen = next() % 4;
			FILE *f = fopen(name, "wb");
			fputc(idlen, f); fputc(cmap, f); fputc(cmap ? 1 : 2, f);
			put16(f, 0); put16(f, maplen); fputc(mapdepth, f);
			put16(f, 0); put16(f, 0); put16(f, w); put16(f, hh); fputc(depth, f);
			fputc(next() % 2 ? 0x20 : 0, f);
			for (int i = 0; i < idlen; i++) fputc(next() & 255, f);
			for (int i = 0; i < maplen * mapdepth / 8; i++) fputc(next() & 255, f);
			int bpp = cmap ? 1 : depth / 8;
			for (int i = 0; i < w * hh * bpp; i++) fputc(cmap ? next() % maplen : next() & 255, f);
			fclose(f);
		}
		fprintf(out, "tga %s", strrchr(name, '/') + 1);
		hashImage(readTGA(name));
	}
}

static void
writers(void)
{
	char tmp[4200];
	sprintf(tmp, "%s/../.write-tmp", corpus);
	for (int n = 0; n < 300; n++) {
		P p = rp(0);
		if (p.depth == 4) p.w &= ~1;	// librw reads past odd 4-bit rows
		int kind = n % 3;
		// librw's writePNG converts 16-bit images to 32, frees the pixels and then encodes and
		// frees them again; the game only writes 32-bit images
		if (kind == 2 && p.depth == 16) p.depth = 32;
		Image *img = makeImage(p);
		fprintf(out, kind == 0 ? "wbmp" : kind == 1 ? "wtga" : "wpng");
		pp(p);
		if (kind == 0) writeBMP(img, tmp);
		else if (kind == 1) writeTGA(img, tmp);
		else writePNG(img, tmp);
		fprintf(out, " => %016llx\n", hashFile(tmp, kind == 2));
		img->destroy();
	}
	unlink(tmp);
}

static void mk(const char *path) {
	char b[1024];
	strcpy(b, path);
	for (char *s = b + 1; *s; s++)
		if (*s == '/') { *s = 0; mkdir(b, 0755); *s = '/'; }
	FILE *f = fopen(b, "w"); fputs("x", f); fclose(f);
}

static void
paths(void)
{
	char dir[] = "/tmp/rwimagefile-XXXXXX";
	if (!mkdtemp(dir)) exit(1);
	chdir(dir);
	static const char *tree[] = {
		"Textures/Road.TGA", "Textures/grass.png", "Textures/Sub/Deep/Leaf.Bmp",
		"models/GENERIC/Fonts.png", "MixedCase/AnOtHeR/file.TXT", "flat.png", "dots.in.name.bmp",
	};
	fprintf(out, "tree");
	for (auto t : tree) { mk(t); fprintf(out, " %s", t); }
	fprintf(out, "\n");
	static const char *names[] = {
		"textures/road.tga", "TEXTURES/SUB/deep/leaf.bmp", "textures\\grass.PNG", "Models\\Generic\\fonts.png",
		"mixedcase/another/FILE.txt", "missing/file.png", "textures/missing.tga", "FLAT.PNG",
		"./textures/road.tga", "textures//road.tga", "Textures/../flat.png", "DOTS.IN.NAME.BMP", "",
	};
	for (auto n : names) {
		char b[1024];
		strcpy(b, n);
		makePath(b);
		fprintf(out, "path %s => %s\n", *n ? n : "~", *b ? b : "~");
	}
	static const char *searches[] = { nil, "textures/", "missing/;Textures/", "models/generic/;textures/sub/deep/;", "a;;textures/" };
	static const char *queries[] = { "road.tga", "Fonts.PNG", "leaf.bmp", "flat.png", "grass", "textures/grass.png" };
	for (auto s : searches) {
		Image::setSearchPath(s);
		for (auto q : queries) {
			char *r = Image::getFilename(q);
			fprintf(out, "search %s %s => %s\n", s ? s : "-", q, r ? r : "-");
			rwFree(r);
		}
	}
	Image::setSearchPath(nil);
}

int
main(int argc, char **argv)
{
	engine = (Engine*)calloc(1, 4096);
	Image::registerModule();
	out = fopen(argv[1], "w");
	strcpy(corpus, argv[2]);
	pngs();
	bmps();
	tgas();
	writers();
	paths();
	fclose(out);
	return 0;
}

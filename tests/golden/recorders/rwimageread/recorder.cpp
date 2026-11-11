// Records librw's Image::read and Image::readMasked with the file format registry
// (image.cpp: registerFileFormat, the extension loop with getFilename, the mask applied and the
// image palettized back) for the Rust port (rust/revc/src/rwimagefile.rs). Files are copies of
// tests/golden/imagefiles/ (argv[2]) under names and extensions that may not match their format,
// in a temp directory. Lines:
//   file NAME SOURCE                     a file made for the run
//   reg EXT R = ok                       registerFileFormat (R: reader 0 none, 1 png, 2 bmp, 3 tga; the extension's own or none)
//   clear                                the module closed and opened (formats and search path)
//   search PATHS|-                       setSearchPath
//   read NAME => H | nil                 Image::read
//   readm NAME MASK|~ => H | nil         Image::readMasked (~: empty mask name)
// H as in rwimagefile.
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

static FILE *out;
static unsigned long long h;
static void hb(uint8 b) { h ^= b; h *= 0x100000001b3ull; }
static void hi(int32 v) { for (int i = 0; i < 4; i++) hb(v >> (8 * i)); }
static void hashImage(Image *img) {
	if (img == nil) { fprintf(out, " => nil\n"); return; }
	h = 0xcbf29ce484222325ull;
	hi(img->width); hi(img->height);
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

static uint32 rng = 0x3c6ef372u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }

// image.cpp's ImageGlobals at 05e88f2 (the module's engine plugin, at offset 0 here)
struct ImageGlobalsMirror { char *searchPaths; int numSearchPaths; struct { char *e; void *r, *w; } fileFormats[10]; int numFileFormats; };

static Image::fileRead readers[4] = { nil, readPNG, readBMP, readTGA };
static const char *exts[] = { "png", "bmp", "tga", "PNG", "Tga", "jpg" };

static void
copyFile(const char *src, const char *dst)
{
	FILE *a = fopen(src, "rb"), *b = fopen(dst, "wb");
	int c;
	while ((c = fgetc(a)) != EOF) fputc(c, b);
	fclose(a); fclose(b);
}

int
main(int argc, char **argv)
{
	engine = (Engine*)calloc(1, 4096);
	Image::registerModule();
	out = fopen(argv[1], "w");
	char dir[] = "/tmp/rwimageread-XXXXXX";
	if (!mkdtemp(dir)) exit(1);
	chdir(dir);
	mkdir("sub", 0755);
	// files: f00..f39 (some in sub/), each a corpus file under some extension, some names twice
	for (int i = 0; i < 48; i++) {
		char name[64], src[4200];
		// a corpus file of the extension's format (librw's readers assert on other formats)
		int e = next() % 5, f;
		static const char *kinds[] = { "png", "bmp", "tga", "png", "tga" };
		do { f = next() % 190; sprintf(src, "%s/%03d.%s", argv[2], f, kinds[e]); } while (access(src, R_OK) != 0);
		sprintf(name, "%sf%02d.%s", next() % 4 == 0 ? "sub/" : "", i % 40, exts[e]);
		copyFile(src, name);
		fprintf(out, "file %s %03d.%s\n", name, f, strrchr(src, '.') + 1);
	}
	for (int n = 0; n < 4000; n++) {
		uint32 op = next() % 100;
		if (op < 10) {
			// the extension's own reader or none
			int k = next() % 6;
			const char *e = exts[k];
			static const int own[] = { 1, 2, 3, 1, 3, 0 };
			int r = next() % 3 ? own[k] : 0;
			int ok = Image::registerFileFormat(e, readers[r], nil);
			fprintf(out, "reg %s %d = %d\n", e, r, ok);
		} else if (op < 13) {
			// imageClose then imageOpen
			Image::setSearchPath(nil);
			((ImageGlobalsMirror*)engine)->numFileFormats = 0;
			fprintf(out, "clear\n");
		} else if (op < 18) {
			static const char *s[] = { nil, "sub/", "nope/;sub/", "sub/;" };
			const char *p = s[next() % 4];
			Image::setSearchPath(p);
			fprintf(out, "search %s\n", p ? p : "-");
		} else {
			char name[32], mask[32];
			int a = next() % 44, b = next() % 44;
			sprintf(name, a < 40 ? "f%02d" : "none%d", a);
			sprintf(mask, b < 40 ? "f%02d" : b < 42 ? "none%d" : "", b);
			Image *img;
			if (op < 55) { fprintf(out, "read %s", name); img = Image::read(name); }
			else { fprintf(out, "readm %s %s", name, *mask ? mask : "~"); img = Image::readMasked(name, mask); }
			hashImage(img);
			if (img) img->destroy();
		}
	}
	fclose(out);
	char cmd[128];
	sprintf(cmd, "rm -rf %s", dir);
	system(cmd);
	return 0;
}

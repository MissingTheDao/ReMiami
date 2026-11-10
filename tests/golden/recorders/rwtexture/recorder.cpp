// Records librw's texture object model (texture.cpp: TexDictionary create/destroy/add/addFront/
// remove/find/setCurrent, Texture create/destroy/addRef/read with the default find and read
// callbacks) for the Rust port (rust/revc/src/rwtexture.rs). Dictionaries and textures get ids
// from the plugin constructor hooks (d<n>, t<n>); rasters are fake pointers r<n>. Lines:
//   op args => H | log: plugin and raster calls in order | allocated: textures dicts
// H: FNV-1a 64 of the state text: per dictionary in the global list " D<id> current? :" and its
// textures in order, then per texture in the global list " T<id> ref dict name|mask filter
// raster" (the full text would be ~30 MB).
#include <string.h>
#include <ctype.h>
#include "texture.cpp"

using namespace rw;

enum { MAXID = 4096 };
static void *dicts[MAXID], *texs[MAXID], *rasters[MAXID];
static int ndicts, ntexs, nrasters;
static char plog[1 << 16];
#define LOG(...) sprintf(plog + strlen(plog), __VA_ARGS__)
static int idOf(void **arr, int n, void *p) { if (!p) return -1; for (int i = 0; i < n; i++) if (arr[i] == p) return i; return -2; }
#define DID(p) idOf(dicts, ndicts, p)
#define TID(p) idOf(texs, ntexs, p)
#define RID(p) idOf(rasters, nrasters, p)
static Raster *newRaster(void) { Raster *r = (Raster*)malloc(8); rasters[nrasters++] = r; return r; }

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void PluginList::construct(void *o) {
	if (this == &TexDictionary::s_plglist) { dicts[ndicts] = o; LOG(" cd%d", ndicts++); }
	if (this == &Texture::s_plglist) { texs[ntexs] = o; LOG(" ct%d", ntexs++); }
}
void PluginList::destruct(void *o) {
	if (this == &TexDictionary::s_plglist) { int i = DID(o); LOG(" dd%d", i); if (i >= 0) dicts[i] = nil; }
	if (this == &Texture::s_plglist) { int i = TID(o); LOG(" dt%d", i); if (i >= 0) texs[i] = nil; }
}
void PluginList::copy(void *, void *) {}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
int32 PluginList::registerStream(uint32, StreamRead, StreamWrite, StreamGetSize) { return 0; }
void setError(Error *) {}
PluginList Engine::s_plglist;
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool PluginList::streamRead(Stream *, void *) { return true; }
bool writeChunkHeader(Stream *, int32, int32) { return true; }
bool findChunk(Stream *, uint32, uint32 *, uint32 *) { return false; }
uint32 Stream::write8(const void *, uint32 n) { return n; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
int32 Stream::writeI16(int16) { return 2; }
int32 Stream::writeI32(int32) { return 4; }
int32 Stream::writeU32(uint32) { return 4; }
int16 Stream::readI16(void) { return 0; }
int32 Stream::readI32(void) { return 0; }
// base.cpp's strncmp_ci, verbatim
int strncmp_ci(const char *s1, const char *s2, int n) {
	char c1, c2;
	while(n--){
		c1 = tolower(*s1);
		c2 = tolower(*s2);
		if(c1 != c2)
			return c1 - c2;
		if(c1 == '\0')
			return 0;
		s1++;
		s2++;
	}
	return 0;
}
// rasters and images: fake objects, logged
Raster *Raster::create(int32, int32, int32, int32 flags, int32) { Raster *r = newRaster(); LOG(" rc%d", nrasters - 1); return r; }
void Raster::destroy(void) { LOG(" rd%d", RID(this)); }
Raster *Raster::createFromImage(Image *, int32) { Raster *r = newRaster(); LOG(" ri%d", nrasters - 1); return r; }
Image *Image::readMasked(const char *name, const char *mask) { LOG(" read:%.32s", name); return name[0] == 'f' || name[0] == 'F' ? (Image*)1 : nil; }
void Image::destroy(void) {}
}

static uint32 rng = 0x3c6ef372u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }

static const char *names[] = {
	"road", "ROAD", "Road", "grass", "GrassLight", "file_tex", "FILE_TEX", "fence", "wall",
	"a_name_that_is_32_characters_ok", "a_name_that_is_32_characters_okX", "a_name_that_is_32_characters_okY",
	"water", "Water", "missing", "fx", "",
};
enum { NNAMES = sizeof(names) / sizeof(names[0]) };
static const char *show(const char *s) { return *s ? s : "~"; }	// empty names, printed


static unsigned long long fnv(const char *s) {
	unsigned long long h = 0xcbf29ce484222325ull;
	for (; *s; s++) { h ^= (unsigned char)*s; h *= 0x100000001b3ull; }
	return h;
}
static char sbuf[1 << 20];

static void state(void) {
	char *sp = sbuf; *sp = 0;
	FORLIST(lnk, TEXTUREGLOBAL(texDicts)) {
		TexDictionary *d = TexDictionary::fromLink(lnk);
		sp += sprintf(sp, " D%d %d :", DID(d), d == TEXTUREGLOBAL(currentTexDict));
		FORLIST(l2, d->textures) sp += sprintf(sp, " %d", TID(Texture::fromDict(l2)));
	}
	FORLIST(lnk, TEXTUREGLOBAL(textures)) {
		Texture *t = LLLinkGetData(lnk, Texture, inGlobalList);
		sp += sprintf(sp, " T%d %d %d %.32s|%.32s %x %d", TID(t), t->refCount, DID(t->dict), t->name, t->mask, t->filterAddressing, RID(t->raster));
	}
	printf(" => %016llx | log:%s | allocated: %d %d\n", fnv(sbuf), plog, Texture::numAllocated, TexDictionary::numAllocated);
	plog[0] = 0;
}

static int pickd(void) { int l[MAXID], n = 0; for (int i = 0; i < ndicts; i++) if (dicts[i]) l[n++] = i; return n ? l[next() % n] : -1; }
static int pickt(void) { int l[MAXID], n = 0; for (int i = 0; i < ntexs; i++) if (texs[i]) l[n++] = i; return n ? l[next() % n] : -1; }

int
main(void)
{
	engine = (Engine*)calloc(1, 4096);
	textureOpen(engine, 1024, sizeof(TextureGlobals));
	printf("open"); state();
	for (int step = 0; step < 4000 && ndicts < MAXID - 8 && ntexs < MAXID - 8; step++) {
		int d = pickd(), t = pickt();
		const char *nm = names[next() % NNAMES];
		switch (next() % 16) {
		case 0: printf("newdict"); TexDictionary::create(); state(); break;
		case 1: if (d < 0 || next() % 3) break; printf("deldict %d", d); ((TexDictionary*)dicts[d])->destroy(); state(); break;
		case 2: case 3: {
			int withRaster = next() % 2;
			printf("newtex %d %s", withRaster, show(nm));
			Texture *x = Texture::create(withRaster ? newRaster() : nil);
			strncpy(x->name, nm, 32);
			if (next() % 4 == 0) x->filterAddressing = next() & 0xffff;
			printf(" %x", x->filterAddressing);
			state(); break;
		}
		case 4: if (t < 0) break; printf("deltex %d", t); ((Texture*)texs[t])->destroy(); state(); break;
		case 5: if (t < 0) break; printf("addref %d", t); ((Texture*)texs[t])->addRef(); state(); break;
		case 6: if (d < 0 || t < 0) break; printf("add %d %d", d, t); ((TexDictionary*)dicts[d])->add((Texture*)texs[t]); state(); break;
		case 7: if (d < 0 || t < 0) break; printf("addfront %d %d", d, t); ((TexDictionary*)dicts[d])->addFront((Texture*)texs[t]); state(); break;
		case 8: if (t < 0 || !((Texture*)texs[t])->dict) break;
			printf("remove %d", t); { Texture *x = (Texture*)texs[t]; x->dict->remove(x); } state(); break;
		case 9: if (d < 0) break; printf("find %d %s =", d, show(nm)); printf(" %d", TID(((TexDictionary*)dicts[d])->find(nm))); state(); break;
		case 10: if (next() % 4 == 0) { printf("setcurrent -1"); TexDictionary::setCurrent(nil); }
			else { if (d < 0) break; printf("setcurrent %d", d); TexDictionary::setCurrent((TexDictionary*)dicts[d]); }
			state(); break;
		case 11: { int v = next() % 2; printf("setload %d", v); Texture::setLoadTextures(v); state(); break; }
		case 12: { int v = next() % 2; printf("setdummies %d", v); Texture::setCreateDummies(v); state(); break; }
		default: {
			const char *mk = next() % 3 ? nil : names[next() % NNAMES];
			printf("read %s %s =", show(nm), mk ? show(mk) : "-");
			Texture *x = Texture::read(nm, mk);
			printf(" %d", TID(x));
			state(); break;
		}
		}
	}
	return 0;
}

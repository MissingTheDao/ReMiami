// revc (docs/RENDERER_PORT.md, stage 2 step 2a): DFF clumps read in Rust (rust/revc/src/dffload.rs)
// and built into librw objects here, mirroring librw's Clump::streamRead, FrameList_::streamRead,
// Geometry::streamRead, MaterialList::streamRead, Material::streamRead, Texture::streamRead and
// Atomic::streamReadClump line by line. Extension chunks still go through librw's plugin readers
// (memory stream over the raw chunk), until those move to Rust too.
// Checked against librw by revc_test_check_dff (every DFF of the game, scenario formats).
#if defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "rpworld.h"
#include "NodeName.h"

extern "C" {
struct RevcExt { const uint8 *data; uint32 len; };
struct RevcDffFrame { float m[12]; int32 parent; RevcExt ext; };
struct RevcDffTexture { char name[128]; char mask[128]; uint32 filterAddressing; RevcExt ext; };
struct RevcDffMaterial { uint8 color[4]; float surface[3]; int32 defaultSurface, textured; RevcDffTexture texture; RevcExt ext; };
struct RevcDffMorph { float sphere[4]; int32 hasVertices, hasNormals; const float *vertices, *normals; };
struct RevcDffGeometry {
	uint32 flags;
	int32 numTriangles, numVertices, numMorphTargets;
	int32 hasSurface;
	float surface[3];
	const uint8 *colors;
	int32 numTexSets;
	const float *texCoords[8];
	const uint32 *triangles;
	const RevcDffMorph *morphs;
	int32 numMatEntries;
	const int32 *matIndices;
	const RevcDffMaterial *materials;
	RevcExt ext;
};
struct RevcDffAtomic { int32 frame, geometry, flags; RevcExt ext; };
struct RevcDffLight { int32 frame; uint32 version; float data[5]; uint32 typeFlags; RevcExt ext; };
struct RevcDffClump {
	int32 numAtomics, numFrames;
	const RevcDffFrame *frames;
	int32 numGeometries;
	const RevcDffGeometry *geometries;
	const RevcDffAtomic *atomics;
	int32 numLights;
	const RevcDffLight *lights;
	RevcExt ext;
};
void *revc_dff_read(const uint8 *data, size_t len, const RevcDffClump **out);
void revc_dff_free(void *handle);
}

namespace rw {
extern uint32 atomicRights[2];
extern uint32 materialRights[2];
}

using namespace rw;

// PluginList::streamRead over the raw extension chunk (false when there is none, as librw's
// findChunk fails then).
static bool
ReadExt(PluginList &plugins, const RevcExt &ext, void *object)
{
	if(ext.len == 0)
		return false;
	StreamMemory ms;
	ms.open((uint8*)ext.data, ext.len);
	bool ok = plugins.streamRead(&ms, object) != 0;
	ms.close();
	return ok;
}

static Texture*
BuildTexture(const RevcDffTexture &t)
{
	uint32 filterAddressing = t.filterAddressing;
	if((filterAddressing & 0xF000) == 0)
		filterAddressing |= (filterAddressing&0xF00) << 4;
	rw::bool32 mipState = rw::Texture::getMipmapping();
	rw::bool32 autoMipState = rw::Texture::getAutoMipmapping();
	int32 filter = filterAddressing&0xFF;
	if(filter == Texture::MIPNEAREST || filter == Texture::MIPLINEAR ||
	   filter == Texture::LINEARMIPNEAREST || filter == Texture::LINEARMIPLINEAR){
		Texture::setMipmapping(1);
		Texture::setAutoMipmapping((filterAddressing&0x10000) == 0);
	}else{
		Texture::setMipmapping(0);
		Texture::setAutoMipmapping(0);
	}
	Texture *tex = Texture::read(t.name, t.mask);
	Texture::setMipmapping(mipState);
	Texture::setAutoMipmapping(autoMipState);
	if(tex == nil)
		return nil;	// librw skips the texture's extension then
	if(tex->refCount == 1)
		tex->filterAddressing = filterAddressing&0xFFFF;
	if(ReadExt(Texture::s_plglist, t.ext, tex))
		return tex;
	tex->destroy();
	return nil;
}

static Material*
BuildMaterial(const RevcDffMaterial &m, const RevcDffGeometry &g)
{
	Material *mat = Material::create();
	if(mat == nil)
		return nil;
	mat->color.red = m.color[0];
	mat->color.green = m.color[1];
	mat->color.blue = m.color[2];
	mat->color.alpha = m.color[3];
	if(m.defaultSurface){
		if(g.hasSurface){
			mat->surfaceProps.ambient = g.surface[0];
			mat->surfaceProps.specular = g.surface[1];
			mat->surfaceProps.diffuse = g.surface[2];
		}
	}else{
		mat->surfaceProps.ambient = m.surface[0];
		mat->surfaceProps.specular = m.surface[1];
		mat->surfaceProps.diffuse = m.surface[2];
	}
	if(m.textured)
		mat->texture = BuildTexture(m.texture);
	materialRights[0] = 0;
	if(!ReadExt(Material::s_plglist, m.ext, mat)){
		mat->destroy();
		return nil;
	}
	if(materialRights[0])
		Material::s_plglist.assertRights(mat, materialRights[0], materialRights[1]);
	return mat;
}

static Geometry*
BuildGeometry(const RevcDffGeometry &g)
{
	Geometry *geo = Geometry::create(g.numVertices, g.numTriangles, g.flags);
	if(geo == nil)
		return nil;
	geo->addMorphTargets(g.numMorphTargets-1);
	if(!(geo->flags & Geometry::NATIVE)){
		if(geo->flags & Geometry::PRELIT && g.colors)
			memcpy(geo->colors, g.colors, 4*geo->numVertices);
		for(int32 i = 0; i < geo->numTexCoordSets && i < 8; i++)
			if(g.texCoords[i])
				memcpy(geo->texCoords[i], g.texCoords[i], 2*geo->numVertices*4);
		for(int32 i = 0; i < geo->numTriangles; i++){
			uint32 a = g.triangles[2*i], b = g.triangles[2*i+1];
			geo->triangles[i].v[0] = a >> 16;
			geo->triangles[i].v[1] = a;
			geo->triangles[i].v[2] = b >> 16;
			geo->triangles[i].matId = b;
		}
	}
	for(int32 i = 0; i < geo->numMorphTargets; i++){
		MorphTarget *mt = &geo->morphTargets[i];
		const RevcDffMorph &m = g.morphs[i];
		memcpy(&mt->boundingSphere, m.sphere, 16);
		if(m.hasVertices)
			memcpy(mt->vertices, m.vertices, 3*geo->numVertices*4);
		if(m.hasNormals)
			memcpy(mt->normals, m.normals, 3*geo->numVertices*4);
	}

	// MaterialList::streamRead
	MaterialList *matlist = &geo->matList;
	matlist->init();
	if(g.numMatEntries){
		matlist->materials = rwMallocT(Material*, g.numMatEntries, MEMDUR_EVENT | ID_MATERIAL);
		matlist->space = g.numMatEntries;
		int32 next = 0;
		for(int32 i = 0; i < g.numMatEntries; i++){
			Material *m;
			if(g.matIndices[i] >= 0){
				m = matlist->materials[g.matIndices[i]];
				m->addRef();
			}else{
				m = BuildMaterial(g.materials[next++], g);
				if(m == nil){
					matlist->deinit();
					geo->destroy();
					return nil;
				}
			}
			matlist->appendMaterial(m);
			m->destroy();
		}
	}
	if(ReadExt(Geometry::s_plglist, g.ext, geo))
		return geo;
	geo->destroy();
	return nil;
}

// The clump as librw's Clump::streamRead builds it (nil on failure).
static Clump*
BuildClump(const RevcDffClump &c)
{
	Clump *clump = Clump::create();
	if(clump == nil)
		return nil;
	Frame **frames = rwMallocT(Frame*, c.numFrames, MEMDUR_EVENT | ID_FRAMELIST);
	for(int32 i = 0; i < c.numFrames; i++){
		const RevcDffFrame &fd = c.frames[i];
		Frame *f = frames[i] = Frame::create();
		memcpy(&f->matrix.right, &fd.m[0], 12);
		memcpy(&f->matrix.up, &fd.m[3], 12);
		memcpy(&f->matrix.at, &fd.m[6], 12);
		memcpy(&f->matrix.pos, &fd.m[9], 12);
		f->matrix.optimize();
		f->matrix.flags &= ~Matrix::IDENTITY;
		if(fd.parent >= 0)
			frames[fd.parent]->addChild(f, rw::streamAppendFrames);
	}
	for(int32 i = 0; i < c.numFrames; i++)
		ReadExt(Frame::s_plglist, c.frames[i].ext, frames[i]);
	clump->setFrame(frames[0]);

	Geometry **geos = c.numGeometries ? rwMallocT(Geometry*, c.numGeometries, MEMDUR_FUNCTION | ID_CLUMP) : nil;
	bool ok = true;
	for(int32 i = 0; i < c.numGeometries; i++){
		geos[i] = ok ? BuildGeometry(c.geometries[i]) : nil;
		if(geos[i] == nil)
			ok = false;
	}
	for(int32 i = 0; ok && i < c.numAtomics; i++){
		const RevcDffAtomic &ad = c.atomics[i];
		Atomic *a = Atomic::create();
		a->setFrame(frames[ad.frame]);
		a->setGeometry(geos[ad.geometry], 0);
		a->object.object.flags = ad.flags;
		atomicRights[0] = 0;
		if(!ReadExt(Atomic::s_plglist, ad.ext, a)){
			a->destroy();
			ok = false;
			break;
		}
		if(atomicRights[0])
			Atomic::s_plglist.assertRights(a, atomicRights[0], atomicRights[1]);
		clump->addAtomic(a);
	}
	// Light::streamRead
	for(int32 i = 0; ok && i < c.numLights; i++){
		const RevcDffLight &ld = c.lights[i];
		Light *l = Light::create(ld.typeFlags>>16);
		if(l == nil){
			ok = false;
			break;
		}
		l->radius = ld.data[0];
		l->setColor(ld.data[1], ld.data[2], ld.data[3]);
		float32 a = ld.data[4];
		if(ld.version >= 0x30300)
			l->minusCosAngle = a;
		else
			l->minusCosAngle = -1.0f/sqrtf(a*a+1.0f);
		l->object.object.flags = (uint8)ld.typeFlags;
		if(!ReadExt(Light::s_plglist, ld.ext, l)){
			l->destroy();
			ok = false;
			break;
		}
		l->setFrame(frames[ld.frame]);
		clump->addLight(l);
	}
	for(int32 i = 0; i < c.numGeometries; i++)
		if(geos[i])
			geos[i]->destroy();
	rwFree(geos);
	rwFree(frames);
	if(ok && ReadExt(Clump::s_plglist, c.ext, clump))
		return clump;
	clump->destroy();
	return nil;
}

// Reads a CLUMP chunk (data starts with its header) with the Rust reader. nil on failure.
Clump*
RevcClumpRead(const uint8 *data, size_t len)
{
	const RevcDffClump *c = nil;
	void *h = revc_dff_read(data, len, &c);
	if(h == nil)
		return nil;
	Clump *clump = BuildClump(*c);
	revc_dff_free(h);
	return clump;
}

// REVC_DFF=librw keeps librw's readers (for comparisons); the default is the Rust reader.
bool
RevcDffUseRust(void)
{
	static int rust = -1;
	if(rust < 0){
		const char *e = getenv("REVC_DFF");
		rust = !(e && strcmp(e, "librw") == 0);
	}
	return rust != 0;
}

// The CLUMP chunk the stream is in (just after RwStreamFindChunk(rwID_CLUMP)): its 12-byte header
// and body, copied out, plus an EXTENSION chunk right after it: older files (RW 3.1, e.g.
// models/generic/air_vlo.dff) keep the clump's extension outside the CLUMP chunk, where librw's
// PluginList::streamRead finds it. The stream ends up where librw's reader leaves it. nil on error.
static uint8*
ClumpChunkBytes(RwStream *stream, size_t *len)
{
	uint32 header[3], next[3];
	stream->seek(-12);
	if(stream->read32(header, 12) != 12)
		return nil;
	uint32 extLen = 0;
	uint32 pos = stream->tell();
	stream->seek(header[1]);
	if(stream->read32(next, 12) == 12 && next[0] == ID_EXTENSION)
		extLen = 12 + next[1];
	stream->seek(pos, 0);
	uint8 *buf = (uint8*)rwMalloc(12 + header[1] + extLen, MEMDUR_FUNCTION | ID_CLUMP);
	if(buf == nil)
		return nil;
	memcpy(buf, header, 12);
	if(stream->read8(buf + 12, header[1] + extLen) != header[1] + extLen){
		rwFree(buf);
		return nil;
	}
	*len = 12 + header[1] + extLen;
	return buf;
}

// RpClumpStreamRead with the Rust reader.
Clump*
RevcClumpStreamRead(RwStream *stream)
{
	size_t len;
	uint8 *buf = ClumpChunkBytes(stream, &len);
	if(buf == nil)
		return nil;
	Clump *clump = RevcClumpRead(buf, len);
	rwFree(buf);
	return clump;
}

// RpClumpGtaStreamRead1/2 (rw/ClumpRead.cpp) split a vehicle's load over two frames; both get the
// whole model's buffer. With the Rust reader the first half keeps the chunk and the second builds
// the clump (the Gta readers skip lights and the clump's extension; vehicles have neither).
static uint8 *pendingClump;
static size_t pendingLen;
void RevcClumpGtaCancel(void);

bool
RevcClumpGtaStreamRead1(RwStream *stream)
{
	RevcClumpGtaCancel();
	pendingClump = ClumpChunkBytes(stream, &pendingLen);
	return pendingClump != nil;
}

Clump*
RevcClumpGtaStreamRead2(RwStream *)
{
	if(pendingClump == nil)
		return nil;
	Clump *clump = RevcClumpRead(pendingClump, pendingLen);
	RevcClumpGtaCancel();
	return clump;
}

void
RevcClumpGtaCancel(void)
{
	if(pendingClump)
		rwFree(pendingClump);
	pendingClump = nil;
}

// ---------------------------------------------------------------- comparison with librw

static char cmpWhat[512];

#define DIFF(...) do { if(cmpWhat[0] == 0) snprintf(cmpWhat, sizeof(cmpWhat), __VA_ARGS__); return false; } while(0)

static bool
SameBytes(const void *a, const void *b, size_t n, const char *what)
{
	if((a == nil) != (b == nil))
		DIFF("%s allocated %d vs %d", what, a != nil, b != nil);
	if(a && n && memcmp(a, b, n))
		DIFF("%s differs", what);
	return true;
}

static bool
SameMatrix(const Matrix &a, const Matrix &b, const char *what)
{
	if(memcmp(&a.right, &b.right, 12) || memcmp(&a.up, &b.up, 12) || memcmp(&a.at, &b.at, 12) ||
	   memcmp(&a.pos, &b.pos, 12) || a.flags != b.flags)
		DIFF("%s matrix", what);
	return true;
}

static bool
SameFrames(Frame *a, Frame *b, const char *path)
{
	char p[256];
	if((a == nil) != (b == nil))
		DIFF("frame %s missing on one side", path);
	if(a == nil)
		return true;
	if(!SameMatrix(a->matrix, b->matrix, path))
		return false;
	if(strcmp(GetFrameNodeName(a), GetFrameNodeName(b)))
		DIFF("frame %s name '%s' vs '%s'", path, GetFrameNodeName(a), GetFrameNodeName(b));
	// plugin data (HAnim, NodeName, user data): compare the frames' plugin blocks byte by byte,
	// pointers inside excepted (HAnim hierarchies are compared below by content)
	int32 i = 0;
	Frame *ca = a->child, *cb = b->child;
	for(; ca || cb; ca = ca ? ca->next : nil, cb = cb ? cb->next : nil, i++){
		snprintf(p, sizeof(p), "%s/%d", path, i);
		if(!SameFrames(ca, cb, p))
			return false;
	}
	return true;
}

static bool
SameTexture(Texture *a, Texture *b, const char *what)
{
	if((a == nil) != (b == nil))
		DIFF("%s texture present %d vs %d", what, a != nil, b != nil);
	if(a == nil)
		return true;
	// In the game both readers find the same texture object in the current dictionary; without
	// one (the test sweep) each read makes its own dummy, so compare what was asked for.
	if(a != b && (strncmp(a->name, b->name, 32) || strncmp(a->mask, b->mask, 32) ||
	              a->filterAddressing != b->filterAddressing || a->refCount != b->refCount))
		DIFF("%s texture '%.32s'/'%.32s' %x vs '%.32s'/'%.32s' %x", what, a->name, a->mask, a->filterAddressing,
			b->name, b->mask, b->filterAddressing);
	return true;
}

static bool
SameMaterial(Material *a, Material *b, const char *what)
{
	if(memcmp(&a->color, &b->color, 4) || memcmp(&a->surfaceProps, &b->surfaceProps, sizeof(a->surfaceProps)))
		DIFF("%s colour/surface", what);
	if(!SameTexture(a->texture, b->texture, what))
		return false;
	if(a->refCount != b->refCount)
		DIFF("%s refcount %d vs %d", what, a->refCount, b->refCount);
	return true;
}

static bool
SameGeometry(Geometry *a, Geometry *b, const char *what)
{
	if(a->flags != b->flags || a->numTriangles != b->numTriangles || a->numVertices != b->numVertices ||
	   a->numMorphTargets != b->numMorphTargets || a->numTexCoordSets != b->numTexCoordSets)
		DIFF("%s header: flags %x/%x tris %d/%d verts %d/%d morphs %d/%d sets %d/%d", what,
			a->flags, b->flags, a->numTriangles, b->numTriangles, a->numVertices, b->numVertices,
			a->numMorphTargets, b->numMorphTargets, a->numTexCoordSets, b->numTexCoordSets);
	char w[256];
	snprintf(w, sizeof(w), "%s colours", what);
	if(!SameBytes(a->colors, b->colors, 4*a->numVertices, w)) return false;
	for(int32 i = 0; i < a->numTexCoordSets; i++){
		snprintf(w, sizeof(w), "%s uv set %d", what, i);
		if(!SameBytes(a->texCoords[i], b->texCoords[i], 8*a->numVertices, w)) return false;
	}
	snprintf(w, sizeof(w), "%s triangles", what);
	if(!SameBytes(a->triangles, b->triangles, sizeof(Triangle)*a->numTriangles, w)) return false;
	for(int32 i = 0; i < a->numMorphTargets; i++){
		MorphTarget *ma = &a->morphTargets[i], *mb = &b->morphTargets[i];
		snprintf(w, sizeof(w), "%s morph %d sphere", what, i);
		if(!SameBytes(&ma->boundingSphere, &mb->boundingSphere, sizeof(Sphere), w)) return false;
		snprintf(w, sizeof(w), "%s morph %d vertices", what, i);
		if(!SameBytes(ma->vertices, mb->vertices, 12*a->numVertices, w)) return false;
		snprintf(w, sizeof(w), "%s morph %d normals", what, i);
		if(!SameBytes(ma->normals, mb->normals, 12*a->numVertices, w)) return false;
	}
	if(a->matList.numMaterials != b->matList.numMaterials)
		DIFF("%s materials %d vs %d", what, a->matList.numMaterials, b->matList.numMaterials);
	for(int32 i = 0; i < a->matList.numMaterials; i++){
		snprintf(w, sizeof(w), "%s material %d", what, i);
		// shared entries: the same index on both sides
		for(int32 j = 0; j < i; j++)
			if((a->matList.materials[i] == a->matList.materials[j]) != (b->matList.materials[i] == b->matList.materials[j]))
				DIFF("%s material %d sharing", what, i);
		if(!SameMaterial(a->matList.materials[i], b->matList.materials[i], w))
			return false;
	}
	// mesh header (BinMesh plugin)
	MeshHeader *ha = a->meshHeader, *hb = b->meshHeader;
	if((ha == nil) != (hb == nil))
		DIFF("%s mesh header present %d vs %d", what, ha != nil, hb != nil);
	if(ha){
		if(ha->flags != hb->flags || ha->numMeshes != hb->numMeshes || ha->totalIndices != hb->totalIndices)
			DIFF("%s mesh header", what);
		for(uint32 i = 0; i < ha->numMeshes; i++){
			Mesh *ma = &ha->getMeshes()[i], *mb = &hb->getMeshes()[i];
			snprintf(w, sizeof(w), "%s mesh %d", what, i);
			if(ma->numIndices != mb->numIndices) DIFF("%s index count", w);
			if(!SameBytes(ma->indices, mb->indices, 2*ma->numIndices, w)) return false;
			int32 ia = a->matList.findIndex(ma->material), ib = b->matList.findIndex(mb->material);
			if(ia != ib) DIFF("%s material %d vs %d", w, ia, ib);
		}
	}
	return true;
}

static bool
SameClump(Clump *a, Clump *b)
{
	if(!SameFrames(a->getFrame(), b->getFrame(), "root"))
		return false;
	if(a->countAtomics() != b->countAtomics())
		DIFF("atomics %d vs %d", a->countAtomics(), b->countAtomics());
	LLLink *la = a->atomics.link.next, *lb = b->atomics.link.next;
	for(int32 i = 0; la != a->atomics.end(); la = la->next, lb = lb->next, i++){
		Atomic *aa = Atomic::fromClump(la), *ab = Atomic::fromClump(lb);
		char w[64];
		snprintf(w, sizeof(w), "atomic %d", i);
		if(aa->object.object.flags != ab->object.object.flags)
			DIFF("%s flags %x vs %x", w, aa->object.object.flags, ab->object.object.flags);
		if(aa->pipeline != ab->pipeline)
			DIFF("%s pipeline", w);
		if(!SameGeometry(aa->geometry, ab->geometry, w))
			return false;
	}
	if(a->countLights() != b->countLights())
		DIFF("lights %d vs %d", a->countLights(), b->countLights());
	la = a->lights.link.next, lb = b->lights.link.next;
	for(int32 i = 0; la != a->lights.end(); la = la->next, lb = lb->next, i++){
		Light *xa = Light::fromClump(la), *xb = Light::fromClump(lb);
		if(xa->radius != xb->radius || memcmp(&xa->color, &xb->color, sizeof(xa->color)) ||
		   xa->minusCosAngle != xb->minusCosAngle || xa->object.object.flags != xb->object.object.flags ||
		   xa->getType() != xb->getType())
			DIFF("light %d", i);
	}
	return true;
}

static int dffChecked, dffDiffering;

extern "C" void revc_dff_check_counts(int *checked, int *differing) { *checked = dffChecked; *differing = dffDiffering; }

// Test hook: reads one DFF (CLUMP chunk at `data`) with librw and with the Rust reader and
// compares the two clumps. Returns 1 if they match, 0 if not (logged), -1 if librw can't read it.
extern "C" int
revc_test_check_dff(const unsigned char *data, size_t len, const char *name)
{
	StreamMemory ms;
	ms.open((uint8*)data, len);
	Clump *lc = nil;
	if(findChunk(&ms, ID_CLUMP, nil, nil))
		lc = Clump::streamRead(&ms);
	ms.close();
	if(lc == nil)
		return -1;
	Clump *rc = RevcClumpRead(data, len);
	dffChecked++;
	cmpWhat[0] = 0;
	bool same = rc && SameClump(lc, rc);
	if(!same && dffDiffering++ < 40)
		printf("[revc-dff] %s: librw vs rust: %s\n", name, rc ? cmpWhat : "rust reader failed");
	lc->destroy();
	if(rc)
		rc->destroy();
	return same;
}

#endif

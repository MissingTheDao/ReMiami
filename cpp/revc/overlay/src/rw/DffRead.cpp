// revc (docs/RENDERER_PORT.md, stage 2 step 2a): DFF clumps read in Rust (rust/revc/src/dffload.rs)
// and built into librw objects here, mirroring librw's Clump::streamRead, FrameList_::streamRead,
// Geometry::streamRead, MaterialList::streamRead, Material::streamRead, Texture::streamRead and
// Atomic::streamReadClump line by line. Extension chunks still go through librw's plugin readers
// (memory stream over the raw chunk), until those move to Rust too.
// Checked against hashes recorded from librw's readers before they were removed
// (tests/golden/models.txt, revc_test_hash_dff, scenario formats).
#if defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "rpworld.h"
#include "NodeName.h"

extern "C" {
struct RevcExt { const uint8 *data; uint32 len; };
struct RevcDffHAnim { int32 id, numNodes, flags, maxKeySize; const int32 *nodeIds, *nodeFlags; };
struct RevcDffFrame { float m[12]; int32 parent; const char *name; uint32 nameLen; const RevcDffHAnim *hanim; RevcExt ext; };
struct RevcDffTexture { char name[128]; char mask[128]; uint32 filterAddressing; RevcExt ext; };
struct RevcDffFxSlot {
	uint32 kind;
	float coefficient;
	int32 fbAlpha, srcBlend, dstBlend, hasTex1, hasTex2;
	RevcDffTexture tex1, tex2;
};
struct RevcDffMatFx { uint32 effects; RevcDffFxSlot slots[2]; };
struct RevcDffMaterial {
	uint8 color[4]; float surface[3]; int32 defaultSurface, textured; RevcDffTexture texture;
	const RevcDffMatFx *matfx;
	int32 hasRights; uint32 rights[2];
	RevcExt ext;
};
struct RevcDffMorph { float sphere[4]; int32 hasVertices, hasNormals; const float *vertices, *normals; };
struct RevcDffMesh { uint32 numIndices, matIndex; const uint16 *indices; };
struct RevcDffBinMesh { uint32 flags, numMeshes, totalIndices; const RevcDffMesh *meshes; };
struct RevcDffSkin {
	uint8 header[4];
	const uint8 *usedBones, *indices, *weights, *inverseMatrices;
	int32 boneLimit, numMeshes, rleSize;
	const uint8 *split;
};
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
	const RevcDffBinMesh *binmesh;
	const RevcDffSkin *skin;
	RevcExt ext;
};
struct RevcDffAtomic { int32 frame, geometry, flags, matfx; int32 hasRights; uint32 rights[2]; RevcExt ext; };
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

// A plugin read from the Rust data instead of librw's stream reader: returns true if it took
// the chunk.
typedef bool (*RustPlugin)(uint32 id, void *object, const void *data);

// Plugin chunks taken by the Rust data vs. left to librw's readers (the sweep logs them).
static int pluginsRust, pluginsLibrw;
// librw readers still used, by chunk id (for the migration; logged by the sweep)
static uint32 librwIds[32], librwIdCount[32];
extern "C" void revc_dff_plugin_counts(int *rust, int *librw)
{
	*rust = pluginsRust;
	*librw = pluginsLibrw;
	for(int k = 0; k < 32 && librwIdCount[k]; k++)
		printf("[revc-test] models: librw still reads plugin %#x: %u chunks\n", librwIds[k], librwIdCount[k]);
}

// PluginList::streamRead over the raw extension chunk (false when there is none, as librw's
// findChunk fails then): each plugin chunk goes to `rust` first, else to librw's reader for that
// id (on a memory stream over the chunk, header read with readChunkHeaderInfo as librw does:
// readers look at the chunk version); then the always callbacks.
static bool
ReadExt(PluginList &plugins, const RevcExt &ext, void *object, RustPlugin rust = nil, const void *data = nil)
{
	if(ext.len < 12)
		return false;
	int32 length = ((const uint32*)ext.data)[1];
	const uint8 *p = ext.data + 12;
	while(length > 0){
		StreamMemory ms;
		ms.open((uint8*)p, ext.data + ext.len - p);
		ChunkHeaderInfo header;
		if(!readChunkHeaderInfo(&ms, &header)){
			ms.close();
			return false;
		}
		length -= 12;
		if(rust && rust(header.type, object, data))
			pluginsRust++;
		else{
			pluginsLibrw++;
			FORLIST(lnk, plugins.plugins){
				Plugin *pl = LLLinkGetData(lnk, Plugin, inParentList);
				if(pl->id == header.type && pl->read){
					pl->read(&ms, header.length, object, pl->offset, pl->size);
					for(int k = 0; k < 32; k++)
						if(librwIds[k] == header.type || librwIdCount[k] == 0){
							librwIds[k] = header.type;
							librwIdCount[k]++;
							break;
						}
					break;
				}
			}
		}
		ms.close();
		p += 12 + header.length;
		length -= header.length;
	}
	FORLIST(lnk, plugins.plugins){
		Plugin *pl = LLLinkGetData(lnk, Plugin, inParentList);
		if(pl->alwaysCallback)
			pl->alwaysCallback(object, pl->offset, pl->size);
	}
	return true;
}

void SetFrameNodeNameData(RwFrame *frame, const char *name, RwInt32 len);	// rw/NodeName.cpp

// The game's NodeName (rw/NodeName.cpp NodeNameStreamRead) and librw's HAnim (hanim.cpp readHAnim).
static bool
FramePlugin(uint32 id, void *object, const void *data)
{
	const RevcDffFrame *f = (const RevcDffFrame*)data;
	Frame *frame = (Frame*)object;
	if(id == 0x253F2FE && f->name){
		SetFrameNodeNameData(frame, f->name, f->nameLen);
		return true;
	}
	if(id == 0x11E && f->hanim){
		const RevcDffHAnim *h = f->hanim;
		HAnimData *hanim = HAnimData::get(frame);
		hanim->id = h->id;
		if(h->numNodes != 0){
			hanim->hierarchy = HAnimHierarchy::create(h->numNodes, (int32*)h->nodeFlags, (int32*)h->nodeIds, h->flags, h->maxKeySize);
			hanim->hierarchy->parentFrame = frame;
		}
		return true;
	}
	return false;
}

// BinMesh (librw geoplg.cpp readMesh) for geometry that isn't NATIVE.
static bool
GeometryPlugin(uint32 id, void *object, const void *data)
{
	const RevcDffGeometry *g = (const RevcDffGeometry*)data;
	Geometry *geo = (Geometry*)object;
	if(id == 0x50E && g->binmesh){
		const RevcDffBinMesh *bm = g->binmesh;
		geo->meshHeader = nil;
		MeshHeader *mh = geo->allocateMeshes(bm->numMeshes, bm->totalIndices, 0);
		mh->flags = bm->flags;
		Mesh *mesh = mh->getMeshes();
		uint16 *indices = mesh->indices;
		for(uint32 i = 0; i < mh->numMeshes; i++, mesh++){
			mesh->numIndices = bm->meshes[i].numIndices;
			mesh->material = geo->matList.materials[bm->meshes[i].matIndex];
			mesh->indices = indices;
			memcpy(indices, bm->meshes[i].indices, 2*mesh->numIndices);
			indices += mesh->numIndices;
		}
		return true;
	}
	// Skin (librw skin.cpp readSkin + readSkinSplitData)
	if(id == 0x116 && g->skin && geo->instData == nil){
		const RevcDffSkin *s = g->skin;
		Skin *skin = rwNewT(Skin, 1, MEMDUR_EVENT | ID_SKIN);
		*PLUGINOFFSET(Skin*, geo, Geometry::s_plglist.getPluginOffset(0x116)) = skin;
		bool oldFormat = s->header[1] == 0;
		if(oldFormat)
			skin->init(s->header[0], s->header[0], geo->numVertices);
		else
			skin->init(s->header[0], s->header[1], geo->numVertices);
		skin->numWeights = s->header[2];
		if(!oldFormat)
			memcpy(skin->usedBones, s->usedBones, skin->numUsedBones);
		if(skin->indices)
			memcpy(skin->indices, s->indices, geo->numVertices*4);
		if(skin->weights)
			memcpy(skin->weights, s->weights, geo->numVertices*16);
		memcpy(skin->inverseMatrices, s->inverseMatrices, skin->numBones*64);
		if(oldFormat){
			skin->findNumWeights(geo->numVertices);
			skin->findUsedBones(geo->numVertices);
		}else{
			skin->boneLimit = s->boneLimit;
			skin->numMeshes = s->numMeshes;
			skin->rleSize = s->rleSize;
			if(skin->numMeshes){
				uint32 sz = skin->numBones + 2*(skin->numMeshes+skin->rleSize);
				int8 *d = (int8*)rwMalloc(sz, MEMDUR_EVENT | ID_SKIN);
				memcpy(d, s->split, sz);
				skin->remapIndices = d;
				skin->rleCount = (Skin::RLEcount*)(d + skin->numBones);
				skin->rle = (Skin::RLE*)(d + skin->numBones + 2*skin->numMeshes);
			}
		}
		return true;
	}
	return false;
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

static Texture *BuildTexture(const RevcDffTexture &t);

// MatFX (librw matfx.cpp readMaterialMatFX) and right-to-render (geometry.cpp readMaterialRights).
static bool
MaterialPlugin(uint32 id, void *object, const void *data)
{
	const RevcDffMaterial *m = (const RevcDffMaterial*)data;
	Material *mat = (Material*)object;
	if(id == 0x120 && m->matfx){
		MatFX::setEffects(mat, m->matfx->effects);
		MatFX *matfx = MatFX::get(mat);
		for(int i = 0; i < 2; i++){
			const RevcDffFxSlot &s = m->matfx->slots[i];
			int32 idx;
			switch(s.kind){
			case MatFX::BUMPMAP: {
				Texture *bumped = s.hasTex1 ? BuildTexture(s.tex1) : nil;
				Texture *tex = s.hasTex2 ? BuildTexture(s.tex2) : nil;
				idx = matfx->getEffectIndex(s.kind);
				matfx->fx[idx].bump.bumpedTex = bumped;
				matfx->fx[idx].bump.tex = tex;
				matfx->fx[idx].bump.coefficient = s.coefficient;
				break;
			}
			case MatFX::ENVMAP: {
				Texture *tex = s.hasTex1 ? BuildTexture(s.tex1) : nil;
				idx = matfx->getEffectIndex(s.kind);
				matfx->fx[idx].env.tex = tex;
				matfx->fx[idx].env.fbAlpha = s.fbAlpha;
				matfx->fx[idx].env.coefficient = s.coefficient;
				break;
			}
			case MatFX::DUAL: {
				Texture *tex = s.hasTex1 ? BuildTexture(s.tex1) : nil;
				idx = matfx->getEffectIndex(s.kind);
				matfx->fx[idx].dual.tex = tex;
				matfx->fx[idx].dual.srcBlend = s.srcBlend;
				matfx->fx[idx].dual.dstBlend = s.dstBlend;
				break;
			}
			}
		}
		return true;
	}
	if(id == 0x1F && m->hasRights){
		materialRights[0] = m->rights[0];
		materialRights[1] = m->rights[1];
		return true;
	}
	return false;
}

// MatFX (matfx.cpp readAtomicMatFX) and right-to-render (clump.cpp readAtomicRights).
static bool
AtomicPlugin(uint32 id, void *object, const void *data)
{
	const RevcDffAtomic *a = (const RevcDffAtomic*)data;
	if(id == 0x120 && a->matfx >= 0){
		if(a->matfx)
			MatFX::enableEffects((Atomic*)object);
		return true;
	}
	if(id == 0x1F && a->hasRights){
		atomicRights[0] = a->rights[0];
		atomicRights[1] = a->rights[1];
		return true;
	}
	return false;
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
	if(!ReadExt(Material::s_plglist, m.ext, mat, MaterialPlugin, &m)){
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
	if(ReadExt(Geometry::s_plglist, g.ext, geo, GeometryPlugin, &g))
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
		ReadExt(Frame::s_plglist, c.frames[i].ext, frames[i], FramePlugin, &c.frames[i]);
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
		if(!ReadExt(Atomic::s_plglist, ad.ext, a, AtomicPlugin, &ad)){
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

// ---------------------------------------------------------------- golden hashes

// FNV-1a 64 over everything the readers build (frames, names, HAnim, geometry arrays, materials
// and sharing, textures, MatFX, skin, meshes, atomics, lights), for tests/golden/models.txt
// (recorded from librw's readers before they were removed; checked with REVC_DFF_GOLDEN).
struct Fnv {
	uint64 h = 0xcbf29ce484222325ull;
	void mix(const void *p, size_t n) { const uint8 *b = (const uint8*)p; for(size_t i = 0; i < n; i++){ h ^= b[i]; h *= 0x100000001b3ull; } }
	void i(int32 v) { mix(&v, 4); }
	void s(const char *str, size_t max) { mix(str, strnlen(str, max)); i(0); }
	void bytes(const void *p, size_t n) { i(p != nil); if(p) mix(p, n); }
};

static void
HashTexture(Fnv &f, Texture *t)
{
	f.i(t != nil);
	if(t){
		f.s(t->name, 32);
		f.s(t->mask, 32);
		f.i(t->filterAddressing);
	}
}

static void
HashFrames(Fnv &f, Frame *fr)
{
	f.i(fr != nil);
	if(fr == nil)
		return;
	f.mix(&fr->matrix.right, 12); f.mix(&fr->matrix.up, 12); f.mix(&fr->matrix.at, 12); f.mix(&fr->matrix.pos, 12);
	f.i(fr->matrix.flags);
	f.s(GetFrameNodeName(fr), 24);
	HAnimData *h = HAnimData::get(fr);
	f.i(h->id);
	f.i(h->hierarchy != nil);
	if(h->hierarchy){
		f.i(h->hierarchy->numNodes);
		f.i(h->hierarchy->flags);
		for(int32 n = 0; n < h->hierarchy->numNodes; n++){
			f.i(h->hierarchy->nodeInfo[n].id);
			f.i(h->hierarchy->nodeInfo[n].index);
			f.i(h->hierarchy->nodeInfo[n].flags);
		}
	}
	for(Frame *c = fr->child; c; c = c->next)
		HashFrames(f, c);
	f.i(-1);
}

static void
HashGeometry(Fnv &f, Geometry *g)
{
	f.i(g->flags); f.i(g->numTriangles); f.i(g->numVertices); f.i(g->numMorphTargets); f.i(g->numTexCoordSets);
	f.bytes(g->colors, 4*g->numVertices);
	for(int32 i = 0; i < g->numTexCoordSets; i++)
		f.bytes(g->texCoords[i], 8*g->numVertices);
	f.bytes(g->triangles, sizeof(Triangle)*g->numTriangles);
	for(int32 i = 0; i < g->numMorphTargets; i++){
		f.mix(&g->morphTargets[i].boundingSphere, sizeof(Sphere));
		f.bytes(g->morphTargets[i].vertices, 12*g->numVertices);
		f.bytes(g->morphTargets[i].normals, 12*g->numVertices);
	}
	f.i(g->matList.numMaterials);
	for(int32 i = 0; i < g->matList.numMaterials; i++){
		Material *m = g->matList.materials[i];
		f.i(g->matList.findIndex(m));	// sharing
		f.mix(&m->color, 4);
		f.mix(&m->surfaceProps, sizeof(m->surfaceProps));
		f.i(m->refCount);
		HashTexture(f, m->texture);
		MatFX *fx = MatFX::get(m);
		f.i(fx != nil);
		if(fx){
			f.i(fx->type);
			for(int k = 0; k < 2; k++){
				f.i(fx->fx[k].type);
				switch(fx->fx[k].type){
				case MatFX::BUMPMAP: f.mix(&fx->fx[k].bump.coefficient, 4); HashTexture(f, fx->fx[k].bump.bumpedTex); HashTexture(f, fx->fx[k].bump.tex); break;
				case MatFX::ENVMAP: f.mix(&fx->fx[k].env.coefficient, 4); f.i(fx->fx[k].env.fbAlpha); HashTexture(f, fx->fx[k].env.tex); break;
				case MatFX::DUAL: f.i(fx->fx[k].dual.srcBlend); f.i(fx->fx[k].dual.dstBlend); HashTexture(f, fx->fx[k].dual.tex); break;
				}
			}
		}
		f.i(m->pipeline ? (int32)m->pipeline->pluginID : -1);
	}
	Skin *s = Skin::get(g);
	f.i(s != nil);
	if(s){
		f.i(s->numBones); f.i(s->numUsedBones); f.i(s->numWeights); f.i(s->boneLimit); f.i(s->numMeshes); f.i(s->rleSize);
		f.bytes(s->usedBones, s->numUsedBones);
		f.bytes(s->indices, 4*g->numVertices);
		f.bytes(s->weights, 16*g->numVertices);
		f.bytes(s->inverseMatrices, 64*s->numBones);
		if(s->numMeshes)
			f.bytes(s->remapIndices, s->numBones + 2*(s->numMeshes+s->rleSize));
	}
	MeshHeader *mh = g->meshHeader;
	f.i(mh != nil);
	if(mh){
		f.i(mh->flags); f.i(mh->numMeshes); f.i(mh->totalIndices);
		for(uint32 i = 0; i < mh->numMeshes; i++){
			Mesh *m = &mh->getMeshes()[i];
			f.i(m->numIndices);
			f.bytes(m->indices, 2*m->numIndices);
			f.i(g->matList.findIndex(m->material));
		}
	}
}

static uint64
HashClump(Clump *c)
{
	Fnv f;
	HashFrames(f, c->getFrame());
	f.i(c->countAtomics());
	FORLIST(lnk, c->atomics){
		Atomic *a = Atomic::fromClump(lnk);
		f.i(a->object.object.flags);
		f.i(a->pipeline ? (int32)a->pipeline->pluginID : -1);
		HashGeometry(f, a->geometry);
	}
	f.i(c->countLights());
	FORLIST(lnk, c->lights){
		Light *l = Light::fromClump(lnk);
		f.mix(&l->radius, 4); f.mix(&l->color, sizeof(l->color)); f.mix(&l->minusCosAngle, 4);
		f.i(l->object.object.flags); f.i(l->getType());
	}
	return f.h;
}

// Test hook: the Rust reader's clump for one DFF, hashed (tests/golden/models.txt). 0 on failure.
extern "C" unsigned long long
revc_test_hash_dff(const unsigned char *data, size_t len)
{
	Clump *c = RevcClumpRead(data, len);
	if(c == nil)
		return 0;
	unsigned long long h = HashClump(c);
	c->destroy();
	return h;
}

#endif

// revc (plan task 13b/13i, FixMods S11): the Xbox world lightmaps. On the Xbox every world material
// has a MatFX dual pass with a per-model lightmap (`<model>@`) on a second set of texture
// coordinates; the Neo world pipe (custompipes, from the Xbox's own "neo" pipelines) draws it,
// blended by neo/worldTweakingTable.dat over the day. The PC models have neither, so the disc
// import keeps what they lack (revc-import xboxlm): models/lightmaps.bin (per model and geometry
// each Xbox vertex's key, its position and first coordinates, with its second coordinates, and
// each material's lightmap name) and models/lightmaps.txd (the lightmaps). As a world model loads
// (CSimpleModelInfo::SetAtomic) each PC vertex gets the coordinates of the Xbox vertex with its key
// (the order and duplicates differ) and each material the lightmap; a geometry with any vertex
// unmatched (a mod's model) stays as it is. Drawn in the Xbox UI style only
// (custompipes_wgpu.cpp worldMeshes), and loaded and attached in it only (Load). Without an Xbox import
// nothing changes.
#include "common.h"
#ifdef EXTENDED_PIPELINES
#include "RwHelper.h"
#include "XboxLightmaps.h"
#include <string>
#include <unordered_map>
#include <vector>

extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);
extern int8 gUiStyle;
extern "C" int32 revc_geo_add_lightmap_coords(rw::Geometry *g, const void *xbox, int32 n);

namespace XboxLightmaps {

rw::TexDictionary *txd;

struct Geo {
	uint32 numVertices;
	size_t verts;	// offset in data of the vertices: u32 key, float u, v
	std::vector<std::string> lightmaps;
};
static std::vector<uint8> data;
static std::unordered_map<std::string, std::vector<Geo>> models;

static std::string
Lower(const char *s, size_t n)
{
	std::string r(s, n);
	for(char &c : r)
		if(c >= 'A' && c <= 'Z')
			c += 'a' - 'A';
	return r;
}

static void
ReadSidecar(void)
{
	const size_t cap = 64 << 20;
	data.resize(cap);
	ssize_t len = revc_import_read(0, "models/lightmaps.bin", data.data(), cap);
	uint32 version = 0;
	if(len >= 8)
		memcpy(&version, data.data() + 4, 4);
	if(len < 12 || memcmp(data.data(), "RVLM", 4) != 0 || version != 2){
		data.clear();
		data.shrink_to_fit();
		return;
	}
	data.resize(len);
	data.shrink_to_fit();
	const uint8 *b = data.data();
	size_t o = 12;
	uint32 n;
	memcpy(&n, b + 8, 4);
	auto u16at = [&](size_t at) { uint16 v; memcpy(&v, b + at, 2); return v; };
	auto u32at = [&](size_t at) { uint32 v; memcpy(&v, b + at, 4); return v; };
	for(uint32 i = 0; i < n && o < (size_t)len; i++){
		std::string name = Lower((const char*)b + o + 1, b[o]);
		o += 1 + b[o];
		uint16 ngeo = u16at(o);
		o += 2;
		std::vector<Geo> geos(ngeo);
		for(Geo &g : geos){
			g.numVertices = u32at(o);
			g.verts = o + 4;
			o += 4 + (size_t)g.numVertices * 12;
			uint16 nmat = u16at(o);
			o += 2;
			for(int m = 0; m < nmat; m++){
				g.lightmaps.push_back(std::string((const char*)b + o + 1, b[o]));
				o += 1 + b[o];
			}
		}
		if(o > (size_t)len)
			break;
		models[name] = std::move(geos);
	}
}

static void
ReadTxd(void)
{
	const size_t cap = 64 << 20;
	uint8 *buf = new uint8[cap];
	ssize_t len = revc_import_read(0, "models/lightmaps.txd", buf, cap);
	if(len > 0){
		RwMemory mem;
		mem.start = buf;
		mem.length = (RwUInt32)len;
		RwStream *stream = RwStreamOpen(rwSTREAMMEMORY, rwSTREAMREAD, &mem);
		if(stream){
			if(RwStreamFindChunk(stream, rwID_TEXDICTIONARY, nil, nil))
				txd = RwTexDictionaryGtaStreamRead(stream);
			RwStreamClose(stream, &mem);
		}
	}
	delete[] buf;
}

// Loads the lightmaps once, on the first model attached while the Xbox style is active: in other
// styles nothing from the Xbox import is read or attached (a PC-style game with an imported Xbox
// disc loads exactly what one without it does). A switch to the Xbox style later in a session
// gives the models streamed in from then on their lightmaps; the rest get them at the next start.
static bool tried;

static void
Load(void)
{
	tried = true;
	ReadTxd();
	if(txd == nil)
		return;
	ReadSidecar();
	if(models.empty()){
		txd->destroy();
		txd = nil;
		return;
	}
	debug("Xbox lightmaps: %d models\n", (int)models.size());
}

void
Init(void)
{
	// (nothing read here; see Load)
}

void
Shutdown(void)
{
	if(txd){
		txd->destroy();
		txd = nil;
	}
	models.clear();
	data.clear();
	data.shrink_to_fit();
	tried = false;
}

void
Attach(const char *modelName, int n, rw::Atomic *atomic)
{
	if(gUiStyle != 2)
		return;
	if(txd == nil && !tried)
		Load();
	if(txd == nil || atomic == nil || atomic->geometry == nil)
		return;
	auto it = models.find(Lower(modelName, strlen(modelName)));
	if(it == models.end())
		return;
	rw::Geometry *geo = atomic->geometry;
	const std::vector<Geo> &all = it->second;
	if(geo->numTexCoordSets == 2 && !all.empty()){
		// the Xbox's own model (rust/revc/src/ps2models.rs): its second set is the lightmap's
		// already; its materials' dual passes get the lightmaps (their names from the sidecar)
		const Geo &c = all[n >= 0 && n < (int)all.size() ? n : 0];
		for(int32 i = 0; i < geo->matList.numMaterials && !c.lightmaps.empty(); i++){
			const std::string &name = c.lightmaps[i < (int32)c.lightmaps.size() ? i : c.lightmaps.size() - 1];
			rw::Texture *tex = name.empty() ? nil : txd->find(name.c_str());
			if(tex == nil)
				continue;
			rw::Material *m = geo->matList.materials[i];
			rw::MatFX::setEffects(m, rw::MatFX::DUAL);
			rw::MatFX *fx = rw::MatFX::get(m);
			fx->setDualTexture(tex);
			fx->setDualSrcBlend(rw::BLENDSRCALPHA);
			fx->setDualDestBlend(rw::BLENDINVSRCALPHA);
		}
		return;
	}
	if(geo->numTexCoordSets != 1)
		return;	// already done, or not a plain world mesh
	// the Xbox geometry every vertex of this one is in: the atomic's (n-th) first, then the others
	const std::vector<Geo> &geos = it->second;
	const Geo *g = nil;
	for(int i = -1; g == nil && i < (int)geos.size(); i++){
		int k = i < 0 ? n : i;
		if(k < 0 || k >= (int)geos.size() || (i >= 0 && i == n))
			continue;
		const Geo &c = geos[k];
		if(c.numVertices && !c.lightmaps.empty() &&
		   revc_geo_add_lightmap_coords(geo, data.data() + c.verts, c.numVertices))
			g = &c;
	}
	if(g == nil)
		return;
	for(int32 i = 0; i < geo->matList.numMaterials; i++){
		const std::string &name = g->lightmaps[i < (int32)g->lightmaps.size() ? i : g->lightmaps.size() - 1];
		if(name.empty())
			continue;
		rw::Texture *tex = txd->find(name.c_str());
		if(tex == nil)
			continue;
		rw::Material *m = geo->matList.materials[i];
		if(rw::MatFX::getEffects(m) != rw::MatFX::NOTHING)
			continue;
		rw::MatFX::setEffects(m, rw::MatFX::DUAL);
		rw::MatFX *fx = rw::MatFX::get(m);
		fx->setDualTexture(tex);
		fx->setDualSrcBlend(rw::BLENDSRCALPHA);
		fx->setDualDestBlend(rw::BLENDINVSRCALPHA);
	}
}

}
#endif

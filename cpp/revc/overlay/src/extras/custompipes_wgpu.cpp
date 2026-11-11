// revc: re3's Neo pipelines (EXTENDED_PIPELINES: the Xbox look) for librw's WGPU platform. A port
// of re3's custompipes_gl.cpp (GL3, removed): the same render callbacks and parameters, drawn through the WGPU
// device's model API (rwwgpu.h) with the WGSL versions of the Neo shaders
// (rust/revc-rw/src/frame.rs, rw::wgpu::MESH_NEO_*). The generic parts (tweak tables, env map camera, gloss
// textures) stay in custompipes.cpp.
#include "common.h"

#ifdef RW_WGPU
#include "main.h"
#include "RwHelper.h"
#include "Lights.h"
#include "Timecycle.h"
#include "FileMgr.h"
#include "Clock.h"
#include "Weather.h"
#include "TxdStore.h"
#include "Renderer.h"
#include "World.h"
#include "custompipes.h"
#include "XboxLightmaps.h"

extern int8 gUiStyle;

#ifdef EXTENDED_PIPELINES

namespace CustomPipes {

using namespace rw;
using rw::wgpu::ModelState;
typedef rw::wgpu::InstanceDataHeader WgpuHeader;
typedef rw::wgpu::InstanceData WgpuInst;
using rw::wgpu::instanceAtomic;
using rw::wgpu::beginModel;
using rw::wgpu::setModelMaterial;
using rw::wgpu::setTexture;
using rw::wgpu::drawMesh;
using rw::wgpu::renderDefault;
using rw::wgpu::makeDefaultPipeline;

static void
setVec(float32 *d, float32 x, float32 y, float32 z, float32 w)
{
	d[0] = x; d[1] = y; d[2] = z; d[3] = w;
}

/*
 * Neo Vehicle pipe
 */

// gl3 uploadSpecLights: the sun plus the extra directionals that light atomics.
static void
setSpecLights(ModelState *model)
{
	float power = Power.Get();
	Color speccol = SpecColor.Get();
	for(int i = 0; i < 1+NUMEXTRADIRECTIONALS; i++){
		setVec(model->extra[2+i], 0.0f, 0.0f, 0.0f, 1.0f);
		setVec(model->extra[7+i], 0.0f, 0.0f, 0.0f, 0.0f);
	}
	V3d at = pDirect->getFrame()->getLTM()->at;
	setVec(model->extra[2], at.x, at.y, at.z, power);
	setVec(model->extra[7], speccol.r, speccol.g, speccol.b, 0.0f);
	for(int i = 0; i < NUMEXTRADIRECTIONALS; i++){
		if(pExtraDirectionals[i]->getFlags() & Light::LIGHTATOMICS){
			V3d d = pExtraDirectionals[i]->getFrame()->getLTM()->at;
			RGBAf c = pExtraDirectionals[i]->color;
			setVec(model->extra[2+1+i], d.x, d.y, d.z, power*2.0f);
			setVec(model->extra[7+1+i], c.red, c.green, c.blue, c.alpha);
		}
	}
}

static void
vehicleRender(rw::ObjPipeline *pipe, Atomic *atomic)
{
	// TODO (as gl3): make this less of a kludge
	if(VehiclePipeSwitch == VEHICLEPIPE_MATFX){
		matFXGlobals.pipelines[rw::platform]->render(atomic);
		return;
	}
	WgpuHeader *header = instanceAtomic(pipe, atomic);
	if(header == nil)
		return;

	uint32 flags = atomic->geometry->flags;
	ModelState model;
	beginModel(atomic, header, &model);

	V3d eyePos = ((Camera*)engine->currentCamera)->getFrame()->getLTM()->pos;
	setVec(model.extra[0], eyePos.x, eyePos.y, eyePos.z, 0.0f);
	setSpecLights(&model);
	float fresnel = Fresnel.Get();
	float lightStrength = SpecColor.Get().a;

	SetRenderState(SRCBLEND, BLENDONE);

	static RGBA white = { 255, 255, 255, 255 };
	WgpuInst *inst = header->inst;
	for(int32 n = 0; n < header->numMeshes; n++, inst++){
		Material *m = inst->material;
		setModelMaterial(&model, flags & Geometry::MODULATE ? m->color : white, m->surfaceProps);
		setTexture(m->texture);
		SetRenderState(VERTEXALPHA, inst->vertexAlpha || m->color.alpha != 0xFF);
		setVec(model.extra[1], fresnel, lightStrength,
			m->surfaceProps.specular * VehicleShininess,
			m->surfaceProps.specular == 0.0f ? 0.0f : VehicleSpecularity);
		drawMesh(header, inst, &model, rw::wgpu::MESH_NEO_VEHICLE, TRUE, EnvMapTex);
	}

	SetRenderState(SRCBLEND, BLENDSRCALPHA);
}

static rw::ObjPipeline*
makePipe(void (*render)(rw::ObjPipeline*, Atomic*))
{
	rw::ObjPipeline *pipe = makeDefaultPipeline();
	pipe->impl.render = render;
	return pipe;
}

void
CreateVehiclePipe(void)
{
	if(CFileMgr::LoadFile("neo/carTweakingTable.dat", work_buff, sizeof(work_buff), "r") <= 0)
		printf("Error: couldn't open 'neo/carTweakingTable.dat'\n");
	else{
		char *fp = (char*)work_buff;
		fp = ReadTweakValueTable(fp, Fresnel);
		fp = ReadTweakValueTable(fp, Power);
		fp = ReadTweakValueTable(fp, DiffColor);
		fp = ReadTweakValueTable(fp, SpecColor);
	}
	vehiclePipe = makePipe(vehicleRender);
}

void
DestroyVehiclePipe(void)
{
	vehiclePipe->destroy();
	vehiclePipe = nil;
}

/*
 * Neo World pipe
 */

static void
worldMeshes(rw::ObjPipeline *pipe, Atomic *atomic, WgpuHeader *header)
{
	if(!LightmapEnable){
		renderDefault(pipe, atomic, header);
		return;
	}
	ModelState model;
	beginModel(atomic, header, &model);

	WgpuInst *inst = header->inst;
	for(int32 n = 0; n < header->numMeshes; n++, inst++){
		Material *m = inst->material;
		Texture *dualtex = nil;
		float32 f = 0.0f;
		if(MatFX::getEffects(m) == MatFX::DUAL){
			dualtex = MatFX::get(m)->getDualTexture();
			// revc: the imported Xbox lightmaps only in the Xbox style (XboxLightmaps.cpp)
			if(dualtex && dualtex->dict == XboxLightmaps::txd && gUiStyle != 2)
				dualtex = nil;
			if(dualtex)
				f = WorldLightmapBlend.Get()*LightmapMult;
		}
		setVec(model.extra[0], f, f, f, m->color.alpha/255.0f);

		RGBA color = { 255, 255, 255, m->color.alpha };
		setModelMaterial(&model, color, m->surfaceProps);
		setTexture(m->texture);
		SetRenderState(VERTEXALPHA, inst->vertexAlpha || m->color.alpha != 0xFF);
		drawMesh(header, inst, &model, rw::wgpu::MESH_NEO_WORLD, TRUE, dualtex);
	}
}

static void
worldRender(rw::ObjPipeline *pipe, Atomic *atomic)
{
	WgpuHeader *header = instanceAtomic(pipe, atomic);
	if(header)
		worldMeshes(pipe, atomic, header);
}

void
CreateWorldPipe(void)
{
	if(CFileMgr::LoadFile("neo/worldTweakingTable.dat", work_buff, sizeof(work_buff), "r") <= 0)
		printf("Error: couldn't open 'neo/worldTweakingTable.dat'\n");
	else
		ReadTweakValueTable((char*)work_buff, WorldLightmapBlend);
	worldPipe = makePipe(worldRender);
}

void
DestroyWorldPipe(void)
{
	worldPipe->destroy();
	worldPipe = nil;
}

/*
 * Neo Gloss pipe
 */

static void
glossRender(rw::ObjPipeline *pipe, Atomic *atomic)
{
	WgpuHeader *header = instanceAtomic(pipe, atomic);
	if(header == nil)
		return;
	worldMeshes(pipe, atomic, header);
	if(!GlossEnable)
		return;

	ModelState model;
	beginModel(atomic, header, &model);
	V3d eyePos = ((Camera*)engine->currentCamera)->getFrame()->getLTM()->pos;
	setVec(model.extra[0], eyePos.x, eyePos.y, eyePos.z, 0.0f);
	setVec(model.extra[1], GlossMult, 0.0f, 0.0f, 0.0f);
	// The shader reads light 0's direction; gl3 leaves the last one in place for unlit roads.
	if(model.lights[0][0][0] == 0.0f)
		memcpy(model.lights[0][2], rw::wgpu::lastLightDirection(), 4*sizeof(float32));

	SetRenderState(VERTEXALPHA, TRUE);
	SetRenderState(SRCBLEND, BLENDONE);
	SetRenderState(DESTBLEND, BLENDONE);
	SetRenderState(ZWRITEENABLE, FALSE);
	SetRenderState(ALPHATESTFUNC, ALPHAALWAYS);

	WgpuInst *inst = header->inst;
	for(int32 n = 0; n < header->numMeshes; n++, inst++){
		Material *m = inst->material;
		RGBA color = { 255, 255, 255, m->color.alpha };
		setModelMaterial(&model, color, m->surfaceProps);
		if(m->texture){
			Texture *tex = GetGlossTex(m);
			if(tex){
				setTexture(tex);
				drawMesh(header, inst, &model, rw::wgpu::MESH_NEO_GLOSS, FALSE, nil);
			}
		}
	}

	SetRenderState(ZWRITEENABLE, TRUE);
	SetRenderState(ALPHATESTFUNC, ALPHAGREATEREQUAL);
	SetRenderState(SRCBLEND, BLENDSRCALPHA);
	SetRenderState(DESTBLEND, BLENDINVSRCALPHA);
}

void
CreateGlossPipe(void)
{
	glossPipe = makePipe(glossRender);
}

void
DestroyGlossPipe(void)
{
	glossPipe->destroy();
	glossPipe = nil;
}

/*
 * Neo Rim pipes
 */

static void
setRimData(ModelState *model, bool enable)
{
	V3d viewVec = ((Camera*)engine->currentCamera)->getFrame()->getLTM()->at;
	setVec(model->extra[0], viewVec.x, viewVec.y, viewVec.z, 0.0f);
	Color col = RampStart.Get();
	setVec(model->extra[1], col.r, col.g, col.b, col.a);
	col = RampEnd.Get();
	setVec(model->extra[2], col.r, col.g, col.b, col.a);
	setVec(model->extra[3], Offset.Get(), Scale.Get(), enable ? Scaling.Get()*RimlightMult : 0.0f, 0.0f);
}

static void
rimRender(rw::ObjPipeline *pipe, Atomic *atomic)
{
	WgpuHeader *header = instanceAtomic(pipe, atomic);
	if(header == nil)
		return;
	if(!RimlightEnable){
		renderDefault(pipe, atomic, header);
		return;
	}
	uint32 flags = atomic->geometry->flags;
	ModelState model;
	beginModel(atomic, header, &model);
	setRimData(&model, flags & Geometry::LIGHT);

	static RGBA white = { 255, 255, 255, 255 };
	WgpuInst *inst = header->inst;
	for(int32 n = 0; n < header->numMeshes; n++, inst++){
		Material *m = inst->material;
		setModelMaterial(&model, flags & Geometry::MODULATE ? m->color : white, m->surfaceProps);
		setTexture(m->texture);
		SetRenderState(VERTEXALPHA, inst->vertexAlpha || m->color.alpha != 0xFF);
		drawMesh(header, inst, &model, rw::wgpu::MESH_NEO_RIM, FALSE, nil);
	}
}

void
CreateRimLightPipes(void)
{
	if(CFileMgr::LoadFile("neo/rimTweakingTable.dat", work_buff, sizeof(work_buff), "r") <= 0)
		printf("Error: couldn't open 'neo/rimTweakingTable.dat'\n");
	else{
		char *fp = (char*)work_buff;
		fp = ReadTweakValueTable(fp, RampStart);
		fp = ReadTweakValueTable(fp, RampEnd);
		fp = ReadTweakValueTable(fp, Offset);
		fp = ReadTweakValueTable(fp, Scale);
		fp = ReadTweakValueTable(fp, Scaling);
	}
	rimPipe = makePipe(rimRender);
	// Skinned peds: the same callback; the skin plugin id makes the instancing skinned.
	rimSkinPipe = makePipe(rimRender);
	rimSkinPipe->pluginID = ID_SKIN;
	rimSkinPipe->pluginData = 1;
}

void
DestroyRimLightPipes(void)
{
	rimPipe->destroy();
	rimPipe = nil;
	rimSkinPipe->destroy();
	rimSkinPipe = nil;
}

}

#endif

#ifdef NEW_RENDERER
// re3's leeds-like world rendering (Rendering > NewRenderer): custompipes_gl.cpp's WorldRender
// on the WGPU model API. Buildings are lit by the ambient light only (no directionals); opaque
// meshes draw in the first pass, atomics with blended meshes are deferred to RenderBlendPass.
namespace WorldRender
{

typedef rw::wgpu::InstanceDataHeader WgpuHeader;
typedef rw::wgpu::InstanceData WgpuInst;

struct BuildingInst
{
	rw::Matrix matrix;
	WgpuHeader *instHeader;
	uint8 fadeAlpha;
	bool lighting;
};
BuildingInst blendInsts[3][2000];
int numBlendInsts[3];

static bool
IsTextureTransparent(RwTexture *tex)
{
	if(tex == nil || tex->raster == nil)
		return false;
	return !!GETWGPURASTEREXT(tex->raster)->hasAlpha;
}

// gl3 setWorldMatrix + setLights with one ambient (pAmbient or black) and no other lights.
static void
buildingModel(BuildingInst *building, rw::wgpu::ModelState *model)
{
	memset(model, 0, sizeof(*model));
	rw::Matrix *m = &building->matrix;
	rw::float32 *w = model->world;
	w[0] = m->right.x; w[1] = m->right.y; w[2] = m->right.z; w[3] = 0.0f;
	w[4] = m->up.x;    w[5] = m->up.y;    w[6] = m->up.z;    w[7] = 0.0f;
	w[8] = m->at.x;    w[9] = m->at.y;    w[10] = m->at.z;   w[11] = 0.0f;
	w[12] = m->pos.x;  w[13] = m->pos.y;  w[14] = m->pos.z;  w[15] = 1.0f;
	if(building->lighting){
		model->ambient[0] = pAmbient->color.red;
		model->ambient[1] = pAmbient->color.green;
		model->ambient[2] = pAmbient->color.blue;
		model->ambient[3] = pAmbient->color.alpha;
	}
}

static WgpuHeader*
instanceBuilding(RpAtomic *atomic, BuildingInst *building)
{
	WgpuHeader *header = rw::wgpu::instanceAtomic(atomic->getPipeline(), atomic);
	building->instHeader = header;
	building->lighting = !!(atomic->geometry->flags & rw::Geometry::LIGHT);
	building->matrix = *atomic->getFrame()->getLTM();
	return header;
}

// Render all opaque meshes and put atomics that need blending into the deferred list.
void
AtomicFirstPass(RpAtomic *atomic, int pass)
{
	BuildingInst *building = &blendInsts[pass][numBlendInsts[pass]];
	WgpuHeader *header = instanceBuilding(atomic, building);
	if(header == nil)
		return;
	building->fadeAlpha = 255;
	rw::uint32 flags = atomic->geometry->flags;

	rw::wgpu::ModelState model;
	buildingModel(building, &model);
	static rw::RGBA white = { 255, 255, 255, 255 };
	bool defer = false;
	WgpuInst *inst = header->inst;
	for(rw::int32 i = 0; i < header->numMeshes; i++, inst++){
		rw::Material *m = inst->material;
		if(inst->vertexAlpha || m->color.alpha != 255 || IsTextureTransparent(m->texture)){
			defer = true;
			continue;
		}
		rw::wgpu::setModelMaterial(&model, flags & rw::Geometry::MODULATE ? m->color : white, m->surfaceProps);
		rw::wgpu::setTexture(m->texture);
		rw::wgpu::drawMesh(header, inst, &model, 0, FALSE, nil);
	}
	if(defer)
		numBlendInsts[pass]++;
}

void
AtomicFullyTransparent(RpAtomic *atomic, int pass, int fadeAlpha)
{
	BuildingInst *building = &blendInsts[pass][numBlendInsts[pass]];
	if(instanceBuilding(atomic, building) == nil)
		return;
	building->fadeAlpha = fadeAlpha;
	numBlendInsts[pass]++;
}

void
RenderBlendPass(int pass)
{
	for(int i = 0; i < numBlendInsts[pass]; i++){
		BuildingInst *building = &blendInsts[pass][i];
		WgpuHeader *header = building->instHeader;
		rw::wgpu::ModelState model;
		buildingModel(building, &model);
		WgpuInst *inst = header->inst;
		for(rw::int32 j = 0; j < header->numMeshes; j++, inst++){
			rw::Material *m = inst->material;
			if(!inst->vertexAlpha && m->color.alpha == 255 && !IsTextureTransparent(m->texture) && building->fadeAlpha == 255)
				continue;	// already done this one
			rw::RGBA color = m->color;
			color.alpha = (color.alpha * building->fadeAlpha)/255;
			rw::wgpu::setModelMaterial(&model, color, m->surfaceProps);	// always modulate here
			rw::wgpu::setTexture(m->texture);
			rw::wgpu::drawMesh(header, inst, &model, 0, FALSE, nil);
		}
	}
}

}
#endif

#endif

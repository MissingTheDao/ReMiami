// revc: re3's Neo pipelines (EXTENDED_PIPELINES: the Xbox look) for librw's WGPU platform. A port
// of custompipes_gl.cpp: the same render callbacks and parameters, drawn through the WGPU
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
#endif

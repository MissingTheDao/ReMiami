// Records librw's camera math (camera.cpp: cameraSync with buildClipPersp/buildClipParallel and
// buildPlanes, frustumTestSphere, calczShiftScale, setFOV), compiled like the game (clang++ -O2),
// for the Rust port (rust/revc/src/rwcamera.rs). One case per line, floats as 8-digit hex:
//   sync ltm vw.x vw.y vo.x vo.y near far proj => viewMatrix planes(normal dist cx cy cz)x6
//        corners x8 bbox(sup inf)
//   sphere cx cy cz r => result            (against the planes of the sync line before it)
//   zss near far proj zNear zFar => zScale zShift
//   fov hfov ratio width height => viewWindow
#include <string.h>
#include "camera.cpp"	// the original, with its static functions

using namespace rw;

namespace rw {
Engine *engine;
static void *xmalloc(size_t sz, uint32) { return calloc(1, sz); }
static void *xrealloc(void *p, size_t sz, uint32) { return realloc(p, sz); }
static void xfree(void *p) { free(p); }
MemoryFunctions Engine::memfuncs = { xmalloc, xrealloc, xfree, xmalloc, xrealloc };
void PluginList::construct(void *) {}
void PluginList::destruct(void *) {}
void PluginList::copy(void *, void *) {}
int32 PluginList::registerPlugin(int32, uint32, Constructor, Destructor, CopyConstructor) { return 0; }
void setError(Error *) {}
PluginList Engine::s_plglist;
const char *allocLocation;
char *dbgsprint(uint32, ...) { return (char*)""; }
int32 PluginList::streamGetSize(void *) { return 0; }
void PluginList::streamWrite(Stream *, void *) {}
bool PluginList::streamRead(Stream *, void *) { return true; }
bool writeChunkHeader(Stream *, int32, int32) { return true; }
bool findChunk(Stream *, uint32, uint32 *, uint32 *) { return false; }
uint32 Stream::write32(const void *, uint32 n) { return n; }
uint32 Stream::read32(void *, uint32 n) { return n; }
int32 Stream::writeU32(uint32) { return 4; }
int32 findPointer(void *, void **, int32) { return -1; }
void World::addCamera(Camera *) {}
void Raster::show(uint32) {}
void Raster::destroy(void) {}
Raster *Raster::create(int32, int32, int32, int32, int32) { return nil; }
}

static uint32 rng = 0x51ed2705u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void v3(const V3d &v) { f(v.x); f(v.y); f(v.z); }
static void m(const Matrix &x) { v3(x.right); printf(" %08x", x.flags); v3(x.up); printf(" --------"); v3(x.at); printf(" --------"); v3(x.pos); printf(" --------"); }

int
main(void)
{
	engine = (Engine*)calloc(1, sizeof(Engine));
	engine->frameDirtyList.init();
	Frame *fr = Frame::create();
	Camera *cam = (Camera*)calloc(1, sizeof(Camera));
	cam->object.object.parent = fr;
	for (int i = 0; i < 3000; i++) {
		// a camera frame: a rotation and position as in the game, now and then scaled or random
		Matrix x;
		V3d axis = { uni(-1, 1), uni(-1, 1), uni(-1, 1) };
		Matrix::makeRotation(&x, &axis, uni(-180, 180));
		x.pos.set(uni(-2000, 2000), uni(-2000, 2000), uni(-100, 300));
		switch (next() % 8) {
		case 0: { V3d s = { uni(0.5f, 2), uni(0.5f, 2), uni(0.5f, 2) }; x.scale(&s, COMBINEPOSTCONCAT); break; }
		case 1: x.right.set(uni(-2, 2), uni(-2, 2), uni(-2, 2)); x.up.set(uni(-2, 2), uni(-2, 2), uni(-2, 2)); x.at.set(uni(-2, 2), uni(-2, 2), uni(-2, 2)); x.flags = 0; break;
		default: break;
		}
		fr->matrix = x;
		fr->updateObjects();
		Matrix ltm = *fr->getLTM();
		cam->viewWindow.set(uni(0.1f, 2), uni(0.1f, 2));
		if (next() % 4) cam->viewOffset.set(0, 0); else cam->viewOffset.set(uni(-0.5f, 0.5f), uni(-0.5f, 0.5f));
		cam->nearPlane = uni(0.05f, 2);
		cam->farPlane = uni(10, 2000);
		cam->projection = next() % 4 ? Camera::PERSPECTIVE : Camera::PARALLEL;
		printf("sync"); m(ltm); f(cam->viewWindow.x); f(cam->viewWindow.y); f(cam->viewOffset.x); f(cam->viewOffset.y);
		f(cam->nearPlane); f(cam->farPlane); printf(" %d =>", cam->projection);
		cameraSync(&cam->object);
		m(cam->viewMatrix);
		for (int p = 0; p < 6; p++) { FrustumPlane *pl = &cam->frustumPlanes[p]; v3(pl->plane.normal); f(pl->plane.distance); printf(" %d %d %d", pl->closestX, pl->closestY, pl->closestZ); }
		for (int c = 0; c < 8; c++) v3(cam->frustumCorners[c]);
		v3(cam->frustumBoundBox.sup); v3(cam->frustumBoundBox.inf);
		printf("\n");
		for (int s = 0; s < 4; s++) {
			Sphere sp;
			// near the frustum: around a corner or the camera, or anywhere
			V3d c = s < 2 ? cam->frustumCorners[next() % 8] : ltm.pos;
			sp.center.set(c.x + uni(-50, 50), c.y + uni(-50, 50), c.z + uni(-50, 50));
			if (s == 3) sp.center.set(uni(-3000, 3000), uni(-3000, 3000), uni(-300, 300));
			sp.radius = next() % 8 ? uni(0, 40) : uni(0, 500);
			printf("sphere"); v3(sp.center); f(sp.radius); printf(" => %d\n", cam->frustumTestSphere(&sp));
		}
		{
			float N = next() % 2 ? 0.0f : uni(-1, 0.5f), F = next() % 2 ? 1.0f : uni(0.6f, 2);
			engine->device.zNear = N; engine->device.zFar = F;
			printf("zss"); f(cam->nearPlane); f(cam->farPlane); printf(" %d", cam->projection); f(N); f(F);
			calczShiftScale(cam);
			printf(" =>"); f(cam->zScale); f(cam->zShift); printf("\n");
		}
		{
			Camera fc = {};
			Raster fb = {};
			fc.frameBuffer = &fb;
			fb.width = next() % 10 ? 320 + next() % 3520 : 0;
			fb.height = next() % 10 ? 200 + next() % 2000 : 0;
			float hfov = uni(30, 120), ratio = next() % 2 ? 4.0f / 3.0f : uni(0.5f, 3);
			printf("fov"); f(hfov); f(ratio); printf(" %d %d", fb.width, fb.height);
			fc.setFOV(hfov, ratio);
			printf(" =>"); f(fc.viewWindow.x); f(fc.viewWindow.y); printf("\n");
		}
	}
	return 0;
}

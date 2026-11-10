// Records librw's HAnim bone matrices and keyframe interpolation (hanim.cpp: updateMatrices,
// hanimApplyCB, hanimInterpCB; anim.cpp: AnimInterpolator::setCurrentAnim, addTime; base.cpp:
// the Quat mult/lerp/slerp), compiled like the game (clang++ -O2), for the Rust port
// (rust/revc/src/rwhanim.rs). Floats as 8-digit hex. Lines:
//   qmult|qlerp|qslerp q p [a] => r
//   hier n flags ltmmode [ltm] | node flags x n          (ltmmode: 0 no parent frame, 1 parent
//                                                          frame with the LTM, 2 + LOCALSPACE)
//   anim numFrames duration | keyframes: prev time q t   (prev as a keyframe index, -1 = none)
//   set => H        addtime dt => H
// H: FNV-1a 64 of " current nextFrame" then per node " k1 k2 q t" (keyframe indices) and the
// node matrices, as the same hex text (the full text would be ~100 MB).
#include <string.h>
#include "hanim.cpp"

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
uint32 Stream::write32(const void *, uint32 n) { return n; }
uint32 Stream::read32(void *, uint32 n) { return n; }
int32 Stream::writeI32(int32) { return 4; }
int32 Stream::writeU32(uint32) { return 4; }
int32 Stream::readI32(void) { return 0; }
uint32 Stream::readU32(void) { return 0; }
float32 Stream::readF32(void) { return 0; }
int32 Stream::writeF32(float32) { return 4; }
int32 findPointer(void *, void **, int32) { return -1; }
}

static uint32 rng = 0x6a09e667u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }

static char sbuf[1 << 20];
static char *sp;
#define out(...) (sp += sprintf(sp, __VA_ARGS__))
static void f(float v) { out(" %08x", bits(v)); }
static void q4(const Quat &q) { f(q.x); f(q.y); f(q.z); f(q.w); }
static void v3(const V3d &v) { f(v.x); f(v.y); f(v.z); }
static void m(const Matrix &x) { v3(x.right); out(" %08x", x.flags); v3(x.up); out(" --------"); v3(x.at); out(" --------"); v3(x.pos); out(" --------"); }
static void flush(void) { fputs(sbuf, stdout); sp = sbuf; *sp = 0; }
static unsigned long long fnv(const char *s) {
	unsigned long long h = 0xcbf29ce484222325ull;
	for (; *s; s++) { h ^= (unsigned char)*s; h *= 0x100000001b3ull; }
	return h;
}

static Quat rquat(void) {
	Quat q = makeQuat(uni(-1, 1), uni(-1, 1), uni(-1, 1), uni(-1, 1));
	if (next() % 4) q = normalize(q);
	return q;
}

static HAnimKeyFrame *kfs;
static int kidx(void *k) { return k ? (int)((HAnimKeyFrame*)k - kfs) : -1; }

static void state(HAnimHierarchy *h) {
	sp = sbuf; *sp = 0;
	AnimInterpolator *in = h->interpolator;
	f(in->currentTime); out(" %d", kidx(in->nextFrame));
	for (int i = 0; i < h->numNodes; i++) {
		HAnimInterpFrame *fr = (HAnimInterpFrame*)in->getInterpFrame(i);
		out(" %d %d", kidx(fr->keyFrame1), kidx(fr->keyFrame2)); q4(fr->q); v3(fr->t);
		m(h->matrices[i]);
	}
	unsigned long long hs = fnv(sbuf);
	sp = sbuf; *sp = 0;
	printf(" => %016llx\n", hs);
}

int
main(void)
{
	engine = (Engine*)calloc(1, sizeof(Engine));
	engine->frameDirtyList.init();
	sp = sbuf;
	// the quaternion functions, with the corner cases (opposite signs, identical, tiny angles)
	for (int i = 0; i < 3000; i++) {
		Quat q = rquat(), p = rquat();
		switch (next() % 6) {
		case 0: p = negate(q); break;
		case 1: p = q; break;
		case 2: p = normalize(add(q, scale(rquat(), 1e-4f))); break;
		default: break;
		}
		float a = next() % 8 ? uni(0, 1) : uni(-0.5f, 1.5f);
		const char *op = i % 3 == 0 ? "qmult" : i % 3 == 1 ? "qlerp" : "qslerp";
		Quat r = i % 3 == 0 ? mult(q, p) : i % 3 == 1 ? lerp(q, p, a) : slerp(q, p, a);
		out("%s", op); q4(q); q4(p); if (i % 3) f(a); out(" =>"); q4(r); out("\n"); flush();
	}

	// the interpolator info as hanimOpen registers it
	AnimInterpolatorInfo info = {};
	info.id = 1;
	info.interpKeyFrameSize = sizeof(HAnimInterpFrame);
	info.animKeyFrameSize = sizeof(HAnimKeyFrame);
	info.applyCB = hanimApplyCB;
	info.interpCB = hanimInterpCB;

	Frame *pf = Frame::create(), *pfp = Frame::create();
	for (int sc = 0; sc < 400; sc++) {
		int n = 1 + next() % 40;
		if (sc % 50 == 0) n = 64;
		int nf[64], depth = 1;
		for (int i = 0; i < n; i++) {
			int fl = 0;
			if (depth < 60 && next() % 3 == 0) { fl |= HAnimHierarchy::PUSH; depth++; }
			if (depth >= 1 && next() % 3 == 0) { fl |= HAnimHierarchy::POP; depth--; }
			nf[i] = fl;
		}
		int mode = next() % 3;
		int hflags = mode == 2 ? HAnimHierarchy::LOCALSPACEMATRICES : 0;
		HAnimHierarchy *h = HAnimHierarchy::create(n, nf, nil, hflags, sizeof(HAnimInterpFrame));
		out("hier %d %d %d", n, hflags, mode);
		if (mode) {
			Matrix x;
			V3d axis = { uni(-1, 1), uni(-1, 1), uni(-1, 1) };
			Matrix::makeRotation(&x, &axis, uni(-180, 180));
			x.pos.set(uni(-2000, 2000), uni(-2000, 2000), uni(-100, 300));
			pfp->matrix = x; pfp->updateObjects();
			if (!pf->getParent()) pfp->addChild(pf);
			h->parentFrame = pf;
			m(*pfp->getLTM());
		} else
			h->parentFrame = nil;
		out(" |"); for (int i = 0; i < n; i++) out(" %d", nf[i]);
		out("\n"); flush();

		// keyframes: every node gets 2..6 with times 0 = t0 < ... < tk = duration; the first n
		// frames are the nodes' first keys, the next n their second ones, then the rest sorted by
		// the time of the key they follow (the order addTime walks them)
		float duration = uni(0.2f, 3);
		int nk[64]; float tm[64][6];
		int total = 0;
		for (int i = 0; i < n; i++) {
			nk[i] = 2 + next() % 5;
			tm[i][0] = 0;
			for (int k = 1; k < nk[i] - 1; k++) tm[i][k] = duration * (k + uni(-0.4f, 0.4f)) / (nk[i] - 1);
			tm[i][nk[i] - 1] = duration;
			total += nk[i];
		}
		kfs = (HAnimKeyFrame*)calloc(total, sizeof(HAnimKeyFrame));
		int at[64][6], cnt = 0;
		for (int k = 0; k < 2; k++) for (int i = 0; i < n; i++) at[i][k] = cnt++;
		// the rest: by the time of the key they follow, then by node
		for (int i = 0; i < n; i++) for (int k = 2; k < 6; k++) at[i][k] = -2;
		for (;;) {
			int bi = -1, bk = 0; float bt = 0;
			for (int i = 0; i < n; i++)
				for (int k = 2; k < nk[i]; k++)
					if (at[i][k] == -2) { if (bi < 0 || tm[i][k - 1] < bt) { bi = i; bk = k; bt = tm[i][k - 1]; } break; }
			if (bi < 0) break;
			at[bi][bk] = cnt++;
		}
		Quat prevq[64];
		for (int i = 0; i < n; i++)
			for (int k = 0; k < nk[i]; k++) {
				HAnimKeyFrame *kf = &kfs[at[i][k]];
				kf->prev = k ? &kfs[at[i][k - 1]] : nil;
				kf->time = tm[i][k];
				Quat q = rquat();
				if (k && next() % 4 == 0) q = negate(prevq[i]);	// the short way round
				prevq[i] = q;
				kf->q = q;
				kf->t.set(uni(-2, 2), uni(-2, 2), uni(-2, 2));
			}
		Animation anim = {};
		anim.interpInfo = &info;
		anim.numFrames = total;
		anim.duration = duration;
		anim.keyframes = kfs;
		out("anim %d", total); f(duration); out(" |");
		for (int j = 0; j < total; j++) { out(" %d", kidx(kfs[j].prev)); f(kfs[j].time); q4(kfs[j].q); v3(kfs[j].t); }
		out("\n"); flush();

		printf("set");
		h->interpolator->setCurrentAnim(&anim);
		h->updateMatrices();
		state(h);
		for (int s = 0; s < 40; s++) {
			float dt = next() % 10 ? uni(0, duration / 6) : next() % 2 ? 0.0f : uni(-0.1f, 0.0f);
			if (s % 13 == 12) dt = duration * uni(0.5f, 1.5f);
			sp = sbuf; f(dt); printf("addtime%s", sbuf);
			h->interpolator->addTime(dt);
			h->updateMatrices();
			state(h);
		}
		free(kfs);
		h->destroy();
	}
	return 0;
}

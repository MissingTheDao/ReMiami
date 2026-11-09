// Records the original collision functions (compiled like the game: clang++ -O2) for the Rust
// port: tests/golden/collision.txt, format in tests/golden/README.md (collision.txt row).
// (Drafted by the local model, rewritten in review.)
#include "common.h"
#include "colstub.h"
#include "collision.inc"
#include <stdio.h>

template <class T> static T Min(T a, T b) { return a < b ? a : b; }
template <class T> static T Max(T a, T b) { return a > b ? a : b; }
static uint32 rng = 0x12345678u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void n(uint32 v) { printf(" %08x", v); }
static void sep() { printf(" =>"); }
static void end() { printf("\n"); }

static const uint8 SURFACES[] = { 0, 1, 7, 8, 12, 16, 31 };
static uint8 surf() { return SURFACES[next() % 7]; }

static CVector vec() { return CVector(uni(-3, 3), uni(-3, 3), uni(-3, 3)); }
static CColSphere sphere() {
	CColSphere s;
	s.Set(uni(0.05f, 2.5f), vec(), surf(), next() % 6);
	return s;
}
static CColBox box() {
	CVector a = vec(), b = vec();
	CVector lo(Min(a.x, b.x), Min(a.y, b.y), Min(a.z, b.z)), hi(Max(a.x, b.x), Max(a.y, b.y), Max(a.z, b.z));
	if (next() % 10 == 0) {
		switch (next() % 3) { case 0: lo.x = hi.x; break; case 1: lo.y = hi.y; break; default: lo.z = hi.z; }
	}
	CColBox bx;
	bx.Set(lo, hi, surf(), next() % 6);
	return bx;
}
static CColLine line() {
	CVector p0 = vec(), p1 = vec();
	if (next() % 4 == 0) p1 = CVector(p0.x, p0.y, uni(-3, 3));
	if (next() % 20 == 0) p1 = p0;
	return CColLine(p0, p1);
}
// One triangle with its own three vertices (indices 0, 1, 2) and its plane.
struct Tri { CompressedVector v[3]; CColTriangle t; CColTrianglePlane p; };
static Tri tri() {
	Tri r;
	for (int i = 0; i < 3; i++) { CVector v = vec(); r.v[i].Set(v.x, v.y, v.z); }
	if (next() % 15 == 0) r.v[next() % 3] = r.v[(next() % 2) + 0];
	r.t.Set(0, 1, 2, surf());
	r.p.Set(r.v, r.t);
	return r;
}
static float sphere_md() { return next() % 2 ? 1.0e24f : uni(0, 9); }
static float line_md() { return next() % 2 ? 1.0f : uni(0, 1); }
static CMatrix matrix() {
	CMatrix m;
	m.m_attachment = nil;
	m.m_hasRwMatrix = false;
	if (next() % 6 == 0) m.SetUnity();
	else { m.SetRotate(uni(-4, 4), uni(-4, 4), uni(-4, 4)); m.SetTranslateOnly(uni(-3, 3), uni(-3, 3), uni(-3, 3)); }
	return m;
}

static void pv(const CVector &v) { f(v.x); f(v.y); f(v.z); }
static void ps(const CColSphere &s) { pv(s.center); f(s.radius); n(s.surface); n(s.piece); }
static void pb(const CColBox &b) { pv(b.min); pv(b.max); n(b.surface); n(b.piece); }
static void pl(const CColLine &l) { pv(l.p0); pv(l.p1); }
static void pt(const Tri &t) { for (int i = 0; i < 3; i++) pv(t.v[i].Get()); n(t.t.surface); }
static void words(const void *p) { uint32 w[10]; memcpy(w, p, 40); for (int i = 0; i < 10; i++) n(w[i]); }
static void pm(const CMatrix &m) { for (int i = 0; i < 16; i++) f(m.f[i / 4][i % 4]); }
static CColPoint fresh_point() { CColPoint p; memset(&p, 0xA5, sizeof(p)); return p; }
static CStoredCollPoly fresh_poly() { CStoredCollPoly p; memset(&p, 0x5A, sizeof(p)); return p; }

static CColModel *model() {
	CColModel *m = new CColModel;
	memset(m, 0, sizeof(*m));
	int nS = next() % 6, nB = next() % 4, nV = 3 + next() % 7, nT = next() % 7, nL = next() % 5;
	m->numSpheres = nS; m->numBoxes = nB; m->numTriangles = nT; m->numLines = nL;
	m->spheres = new CColSphere[nS + 1];
	m->boxes = new CColBox[nB + 1];
	m->vertices = new CompressedVector[nV];
	m->triangles = new CColTriangle[nT + 1];
	m->lines = new CColLine[nL + 1];
	m->trianglePlanes = nil;
	CVector lo(1e30f, 1e30f, 1e30f), hi(-1e30f, -1e30f, -1e30f);
	auto grow = [&](const CVector &c, float r) {
		lo.x = Min(lo.x, c.x - r); lo.y = Min(lo.y, c.y - r); lo.z = Min(lo.z, c.z - r);
		hi.x = Max(hi.x, c.x + r); hi.y = Max(hi.y, c.y + r); hi.z = Max(hi.z, c.z + r);
	};
	for (int i = 0; i < nS; i++) { m->spheres[i] = sphere(); grow(m->spheres[i].center, m->spheres[i].radius); }
	for (int i = 0; i < nB; i++) { m->boxes[i] = box(); grow(m->boxes[i].min, 0); grow(m->boxes[i].max, 0); }
	for (int i = 0; i < nV; i++) { CVector v = vec(); m->vertices[i].Set(v.x, v.y, v.z); grow(v, 0); }
	for (int i = 0; i < nT; i++) m->triangles[i].Set(next() % nV, next() % nV, next() % nV, surf());
	for (int i = 0; i < nL; i++) { m->lines[i] = line(); grow(m->lines[i].p0, 0); grow(m->lines[i].p1, 0); }
	float g = uni(0, 0.5f);
	lo -= CVector(g, g, g); hi += CVector(g, g, g);
	m->boundingBox.Set(lo, hi);
	m->boundingSphere.Set((hi - lo).Magnitude() * 0.5f, (lo + hi) * 0.5f);
	// keep nV for printing
	m->level = nV;
	return m;
}
static void pmodel(const CColModel &m) {
	n(m.numSpheres); for (int i = 0; i < m.numSpheres; i++) ps(m.spheres[i]);
	n(m.numBoxes); for (int i = 0; i < m.numBoxes; i++) pb(m.boxes[i]);
	n(m.level); for (int i = 0; i < m.level; i++) pv(m.vertices[i].Get());
	n(m.numTriangles); for (int i = 0; i < m.numTriangles; i++) { n(m.triangles[i].a); n(m.triangles[i].b); n(m.triangles[i].c); n(m.triangles[i].surface); }
	n(m.numLines); for (int i = 0; i < m.numLines; i++) pl(m.lines[i]);
	pv(m.boundingSphere.center); f(m.boundingSphere.radius);
	pv(m.boundingBox.min); pv(m.boundingBox.max);
}

int main() {
	const int N = 600, NM = 400;
	for (int i = 0; i < N; i++) { CColSphere a = sphere(), b = sphere(); printf("tss"); ps(a); ps(b); sep(); n(CCollision::TestSphereSphere(a, b)); end(); }
	for (int i = 0; i < N; i++) { CColSphere a = sphere(); CColBox b = box(); printf("tsb"); ps(a); pb(b); sep(); n(CCollision::TestSphereBox(a, b)); end(); }
	for (int i = 0; i < N; i++) { CColLine l = line(); CColBox b = box(); printf("tlb"); pl(l); pb(b); sep(); n(CCollision::TestLineBox(l, b)); end(); }
	for (int i = 0; i < N; i++) { CColLine l = line(); CColBox b = box(); printf("tvlb"); pl(l); pb(b); sep(); n(CCollision::TestVerticalLineBox(l, b)); end(); }
	for (int i = 0; i < N; i++) { CColLine l = line(); Tri t = tri(); printf("tlt"); pl(l); pt(t); sep(); n(CCollision::TestLineTriangle(l, t.v, t.t, t.p)); end(); }
	for (int i = 0; i < N; i++) { CColLine l = line(); CColSphere s = sphere(); printf("tls"); pl(l); ps(s); sep(); n(CCollision::TestLineSphere(l, s)); end(); }
	for (int i = 0; i < N; i++) { CColSphere s = sphere(); Tri t = tri(); printf("tst"); ps(s); pt(t); sep(); n(CCollision::TestSphereTriangle(s, t.v, t.t, t.p)); end(); }

	for (int i = 0; i < N; i++) {
		CColSphere a = sphere(), b = sphere(); float md = sphere_md(); CColPoint p = fresh_point();
		printf("pss"); ps(a); ps(b); f(md); sep(); n(CCollision::ProcessSphereSphere(a, b, p, md)); words(&p); f(md); end();
	}
	for (int i = 0; i < N; i++) {
		CColSphere a = sphere(); CColBox b = box(); float md = sphere_md(); CColPoint p = fresh_point();
		printf("psb"); ps(a); pb(b); f(md); sep(); n(CCollision::ProcessSphereBox(a, b, p, md)); words(&p); f(md); end();
	}
	for (int i = 0; i < N; i++) {
		CColLine l = line(); CColBox b = box(); float md = line_md(); CColPoint p = fresh_point();
		printf("plb"); pl(l); pb(b); f(md); sep(); n(CCollision::ProcessLineBox(l, b, p, md)); words(&p); f(md); end();
	}
	for (int i = 0; i < N; i++) {
		CColLine l = line(); CColSphere s = sphere(); float md = line_md(); CColPoint p = fresh_point();
		printf("pls"); pl(l); ps(s); f(md); sep(); n(CCollision::ProcessLineSphere(l, s, p, md)); words(&p); f(md); end();
	}
	for (int i = 0; i < N; i++) {
		CColLine l = line(); Tri t = tri(); float md = line_md(); CColPoint p = fresh_point(); CStoredCollPoly q = fresh_poly();
		printf("pvlt"); pl(l); pt(t); f(md); sep(); n(CCollision::ProcessVerticalLineTriangle(l, t.v, t.t, t.p, p, md, &q)); words(&p); f(md); words(&q); end();
	}
	for (int i = 0; i < N; i++) {
		CColLine l = line(); Tri t = tri(); float md = line_md(); CColPoint p = fresh_point(); CStoredCollPoly q = fresh_poly();
		printf("plt"); pl(l); pt(t); f(md); sep(); n(CCollision::ProcessLineTriangle(l, t.v, t.t, t.p, p, md, &q)); words(&p); f(md); words(&q); end();
	}
	for (int i = 0; i < N; i++) {
		CColSphere s = sphere(); Tri t = tri(); float md = sphere_md(); CColPoint p = fresh_point();
		printf("pst"); ps(s); pt(t); f(md); sep(); n(CCollision::ProcessSphereTriangle(s, t.v, t.t, t.p, p, md)); words(&p); f(md); end();
	}
	for (int i = 0; i < N; i++) {
		CStoredCollPoly q = fresh_poly();
		CVector pos = vec();
		Tri t = tri();
		for (int k = 0; k < 3; k++) q.verts[k] = t.v[k].Get();
		if (next() % 2) {
			CVector c = (q.verts[0] + q.verts[1] + q.verts[2]) / 3.0f;
			pos = CVector(c.x + uni(-0.3f, 0.3f), c.y + uni(-0.3f, 0.3f), c.z + uni(0.5f, 3));
		}
		float z = pos.z - uni(1, 6);
		q.valid = next() % 5 != 0;
		CColPoint p = fresh_point();
		printf("spv"); pv(pos); f(z); words(&q); sep(); n(CCollision::IsStoredPolyStillValidVerticalLine(pos, z, p, &q)); words(&p); words(&q); end();
	}
	for (int i = 0; i < N; i++) { CVector a = vec(), b = vec(), c = vec(); if (i % 9 == 0) b = a; printf("d2l"); pv(a); pv(b); pv(c); sep(); f(CCollision::DistToLine(&a, &b, &c)); end(); }
	for (int i = 0; i < N; i++) {
		CVector a = vec(), b = vec(), c = vec(), cl; if (i % 9 == 0) b = a;
		printf("d2lc"); pv(a); pv(b); pv(c); sep(); float d = CCollision::DistToLine(&a, &b, &c, cl); f(d); pv(cl); end();
	}
	for (int i = 0; i < N; i++) {
		CVector a = vec(), b = vec(), c = vec(); if (i % 15 == 0) c = a;
		CColTrianglePlane pl; memset(&pl, 0x5A, sizeof(pl)); pl.Set(a, b, c);
		printf("plane"); pv(a); pv(b); pv(c); sep(); pv(pl.normal); f(pl.dist); n(pl.dir); end();
	}

	for (int i = 0; i < NM; i++) {
		CColLine l = line(); CMatrix m = matrix(); CColModel *md = model(); uint32 see = next() % 2, shoot = next() % 2;
		printf("tlos"); pl(l); pm(m); pmodel(*md); n(see); n(shoot); sep(); n(CCollision::TestLineOfSight(l, m, *md, see, shoot)); end();
	}
	for (int i = 0; i < NM; i++) {
		CColLine l = line(); CMatrix m = matrix(); CColModel *md = model(); float d = line_md(); uint32 see = next() % 2, shoot = next() % 2;
		CColPoint p = fresh_point();
		printf("plos"); pl(l); pm(m); pmodel(*md); f(d); n(see); n(shoot); sep();
		n(CCollision::ProcessLineOfSight(l, m, *md, p, d, see, shoot)); words(&p); f(d); end();
	}
	for (int i = 0; i < NM; i++) {
		CColLine l = line(); CMatrix m = matrix(); CColModel *md = model(); float d = line_md(); uint32 see = next() % 2;
		CColPoint p = fresh_point(); CStoredCollPoly q = fresh_poly();
		printf("pvl"); pl(l); pm(m); pmodel(*md); f(d); n(see); sep();
		n(CCollision::ProcessVerticalLine(l, m, *md, p, d, see, false, &q)); words(&p); f(d); words(&q); end();
	}
	for (int i = 0; i < NM; i++) {
		CMatrix ma = matrix(); CColModel *a = model(); CMatrix mb = matrix(); CColModel *b = model();
		// Every other case: B near A (shared position) so the volumes overlap more often.
		if (i % 2) { mb.GetPosition() = ma.GetPosition() + CVector(uni(-1, 1), uni(-1, 1), uni(-1, 1)); }
		CColPoint sp[16], lp[16]; float ld[16];
		memset(sp, 0xA5, sizeof(sp)); memset(lp, 0xA5, sizeof(lp)); for (int k = 0; k < 16; k++) ld[k] = 1.0f;
		printf("pcm"); pm(ma); pmodel(*a); pm(mb); pmodel(*b); sep();
		int cnt = CCollision::ProcessColModels(ma, *a, mb, *b, sp, lp, ld);
		n(cnt);
		for (int k = 0; k < cnt; k++) words(&sp[k]);
		for (int k = 0; k < a->numLines; k++) { words(&lp[k]); f(ld[k]); }
		end();
	}
	return 0;
}

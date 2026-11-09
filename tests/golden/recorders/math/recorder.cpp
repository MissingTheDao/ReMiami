// Records the original src/math functions (compiled like the game: clang++ -O2) for the Rust port.
// Output: one case per line, "op inputs => outputs", every float as its 8-digit hex bit pattern.
#include "common.h"
#include "Quaternion.h"
#include <stdio.h>

static uint32 rng = 0x12345678u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static float any() {
	switch (next() % 10) {
	case 0: { static const float sp[] = { 0.0f, -0.0f, 1.0f, -1.0f, 0.5f, PI, -PI, PI / 2, 1e-20f, -1e-20f, 1e20f, 127.0f, 0.99999f };
		return sp[next() % (sizeof(sp) / sizeof(sp[0]))]; }
	case 1: return uni(-1000.0f, 1000.0f);
	case 2: return uni(-0.001f, 0.001f);
	case 3: case 4: return uni(-4 * PI, 4 * PI);
	default: return uni(-2.0f, 2.0f);
	}
}
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void fs(const float *v, int n) { for (int i = 0; i < n; i++) f(v[i]); }
static void sep() { printf(" =>"); }
static void end() { printf("\n"); }

static CVector vec() { return CVector(any(), any(), any()); }
static CQuaternion quat() {
	CQuaternion q(any(), any(), any(), any());
	if (next() % 2) q.Normalise();
	return q;
}
// A matrix: random rotation (most game matrices), scaled rotation or random numbers; the w column
// and the attachment are random/garbage so stray writes show.
static void mat(CMatrix &m) {
	m.m_attachment = nil;
	m.m_hasRwMatrix = false;
	switch (next() % 3) {
	case 0: m.SetRotate(any(), any(), any()); m.SetTranslateOnly(any(), any(), any()); break;
	case 1: m.SetRotate(any(), any(), any()); m.Scale(any()); m.SetTranslateOnly(any(), any(), any()); break;
	default: for (int i = 0; i < 16; i++) m.f[i / 4][i % 4] = any(); break;
	}
	m.rw = any(); m.fw = any(); m.uw = any(); m.pw = any();
}
static void pm(const CMatrix &m) { fs(&m.f[0][0], 16); }
static void pv(const RwV3d &v) { f(v.x); f(v.y); f(v.z); }
static void pq(const CQuaternion &q) { f(q.x); f(q.y); f(q.z); f(q.w); }
static void rwmat(RwMatrix &r) {
	r.right = vec(); r.up = vec(); r.at = vec(); r.pos = vec();
	r.flags = next(); r.pad1 = next(); r.pad2 = next(); r.pad3 = next();
}
static void prw(const RwMatrix &r) { printf(" %08x", r.flags); pv(r.right); pv(r.up); pv(r.at); pv(r.pos); }

int main() {
	const int N = 150;
	for (int i = 0; i < N; i++) { CVector v = vec(); if (i % 7 == 0) v = CVector(0, 0, 0); printf("vnorm"); pv(v); sep(); v.Normalise(); pv(v); end(); }
	for (int i = 0; i < N; i++) { CVector a = vec(), b = vec(); printf("cross"); pv(a); pv(b); sep(); pv(CrossProduct(a, b)); end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); CVector v = vec(); printf("m3x3"); pm(m); pv(v); sep(); pv(Multiply3x3(m, v)); end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); CVector v = vec(); printf("v3x3"); pm(m); pv(v); sep(); pv(Multiply3x3(v, m)); end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); CVector v = vec(); printf("mulv"); pm(m); pv(v); sep(); pv(m * v); end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); CVector v = vec(); printf("tpoint"); pm(m); pv(v); sep(); CVuVector o; TransformPoint(o, m, (RwV3d &)v); pv(o); end(); }

	for (int i = 0; i < N; i++) { CQuaternion q = quat(); if (i % 9 == 0) q = CQuaternion(0, 0, 0, 0); printf("qnorm"); pq(q); sep(); q.Normalise(); pq(q); end(); }
	for (int i = 0; i < N; i++) {
		CQuaternion a = quat(), b = quat(), o;
		float theta = i % 8 == 0 ? 0.0f : uni(0.0f, PI), invSin = uni(-3.0f, 3.0f), t = uni(-0.2f, 1.2f);
		printf("slerp"); pq(a); pq(b); f(theta); f(invSin); f(t); sep(); o.Slerp(a, b, theta, invSin, t); pq(o); end();
	}
	for (int i = 0; i < N; i++) { CQuaternion a = quat(), b = quat(), o; printf("qmul"); pq(a); pq(b); sep(); o.Multiply(a, b); pq(o); end(); }
	for (int i = 0; i < N; i++) { CQuaternion q = quat(); q.w = uni(-1.0f, 1.0f); RwV3d ax; float an; printf("qgetaxis"); pq(q); sep(); q.Get(&ax, &an); pv(ax); f(an); end(); }
	for (int i = 0; i < N; i++) { CQuaternion q; RwV3d ax = vec(); float an = any(); printf("qsetaxis"); pv(ax); f(an); sep(); q.Set(&ax, an); pq(q); end(); }
	for (int i = 0; i < N; i++) { CQuaternion q = quat(); RwMatrix r; rwmat(r); printf("qgetmat"); pq(q); prw(r); sep(); q.Get(&r); prw(r); end(); }
	for (int i = 0; i < N; i++) {
		RwMatrix r; rwmat(r);
		if (i % 2) { CMatrix m; m.m_attachment = nil; m.SetRotate(any(), any(), any()); m.CopyToRwMatrix(&r); }
		CQuaternion q; printf("qsetmat"); prw(r); sep(); q.Set(r); pq(q); end();
	}
	for (int i = 0; i < N; i++) { CQuaternion q = quat(); float a, b, c; printf("qgeteuler"); pq(q); sep(); q.Get(&a, &b, &c); f(a); f(b); f(c); end(); }
	for (int i = 0; i < N; i++) { CQuaternion q; float a = any(), b = any(), c = any(); printf("qseteuler"); f(a); f(b); f(c); sep(); q.Set(a, b, c); pq(q); end(); }

#define MOP(name, call, ...) for (int i = 0; i < N; i++) { CMatrix m; mat(m); float a = any(), b = any(), c = any(); \
	printf(name); pm(m); f(a); f(b); f(c); sep(); call; pm(m); end(); }
	MOP("setunity", m.SetUnity())
	MOP("resetorient", m.ResetOrientation())
	MOP("setscale", m.SetScale(a))
	MOP("settranslate", m.SetTranslate(a, b, c))
	MOP("setrotxonly", m.SetRotateXOnly(a))
	MOP("setrotyonly", m.SetRotateYOnly(a))
	MOP("setrotzonly", m.SetRotateZOnly(a))
	MOP("setrotx", m.SetRotateX(a))
	MOP("setroty", m.SetRotateY(a))
	MOP("setrotz", m.SetRotateZ(a))
	MOP("setrotate", m.SetRotate(a, b, c))
	MOP("rotx", m.RotateX(a))
	MOP("roty", m.RotateY(a))
	MOP("rotz", m.RotateZ(a))
	MOP("rotate", m.Rotate(a, b, c))
	MOP("reortho", m.Reorthogonalise())
	for (int i = 0; i < N; i++) { CMatrix a, b; mat(a); mat(b); printf("mmul"); pm(a); pm(b); sep(); CMatrix o = a * b; pm(o); end(); }
	for (int i = 0; i < N; i++) { CMatrix a, b; mat(a); mat(b); printf("mmuleq"); pm(a); pm(b); sep(); a *= b; pm(a); end(); }
	for (int i = 0; i < N; i++) { CMatrix a, b; mat(a); mat(b); printf("maddeq"); pm(a); pm(b); sep(); a += b; pm(a); end(); }
	for (int i = 0; i < N; i++) { CMatrix s, d; mat(s); mat(d); printf("invert"); pm(s); pm(d); sep(); Invert(s, d); pm(d); end(); }
	for (int i = 0; i < N; i++) {
		CMatrix m; mat(m);
		m.SetRotate(any(), any(), any());	// game use: orthonormal (float->int8 is undefined out of range)
		CCompressedMatrixNotAligned c; memset(&c, 0, sizeof(c));
		printf("compress"); pm(m); sep(); c.CompressFromFullMatrix(m);
		const unsigned char *p = (const unsigned char *)&c; for (unsigned k = 0; k < sizeof(c); k++) printf(" %02x", p[k]); end();
	}
	for (int i = 0; i < N; i++) {
		CMatrix m; mat(m); CCompressedMatrixNotAligned c; unsigned char *p = (unsigned char *)&c;
		for (unsigned k = 0; k < 12; k++) p[k] = 0; CVector pos = vec(); memcpy(p, &pos, 12);
		for (unsigned k = 12; k < sizeof(c); k++) p[k] = next() & 0xff;
		printf("decompress"); for (unsigned k = 0; k < sizeof(c); k++) printf(" %02x", p[k]); pm(m); sep(); c.DecompressIntoFullMatrix(m); pm(m); end();
	}
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); RwMatrix r; rwmat(r); m.m_attachment = &r; printf("update"); pm(m); prw(r); sep(); m.Update(); pm(m); end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); RwMatrix r; rwmat(r); m.m_attachment = &r; printf("updaterw"); pm(m); prw(r); sep(); m.UpdateRW(); prw(r); m.m_attachment = nil; end(); }
	for (int i = 0; i < N; i++) { CMatrix m; mat(m); RwMatrix r; rwmat(r); printf("copytorw"); pm(m); prw(r); sep(); m.CopyToRwMatrix(&r); prw(r); end(); }
	return 0;
}

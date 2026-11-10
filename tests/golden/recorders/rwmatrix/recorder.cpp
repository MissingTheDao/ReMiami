// Records librw's rw::Matrix functions (compiled like the game: clang++ -O2) for the Rust port
// (rust/revc/src/rwmatrix.rs). One case per line: "op inputs => outputs", floats and flags as
// 8-digit hex; a matrix is right flags up pad at pad pos pad with the pads printed as --------.
#include <stdio.h>
#define private public
#include "rwbase.h"
#undef private
#include <stdio.h>
#include <string.h>

using namespace rw;

static uint32 rng = 0x9e3779b9u;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float uni(float lo, float hi) { return lo + (hi - lo) * (next() >> 8) * (1.0f / 16777216.0f); }
static float any() {
	switch (next() % 10) {
	case 0: { static const float sp[] = { 0.0f, -0.0f, 1.0f, -1.0f, 0.5f, 3.14159265f, 90.0f, 180.0f, 1e-20f, 1e20f, 0.99999f };
		return sp[next() % (sizeof(sp) / sizeof(sp[0]))]; }
	case 1: return uni(-1000.0f, 1000.0f);
	case 2: return uni(-0.001f, 0.001f);
	case 3: case 4: return uni(-360.0f, 360.0f);
	default: return uni(-2.0f, 2.0f);
	}
}
static uint32 bits(float f) { uint32 u; memcpy(&u, &f, 4); return u; }
static void f(float v) { printf(" %08x", bits(v)); }
static void u(uint32 v) { printf(" %08x", v); }
static void v3(const V3d &v) { f(v.x); f(v.y); f(v.z); }
static void q4(const Quat &q) { f(q.x); f(q.y); f(q.z); f(q.w); }
static void m(const Matrix &x) { v3(x.right); u(x.flags); v3(x.up); printf(" --------"); v3(x.at); printf(" --------"); v3(x.pos); printf(" --------"); }
static void sep() { printf(" =>"); }
static void end() { printf("\n"); }

static V3d vec() { V3d v = { any(), any(), any() }; return v; }
static Quat quat() {
	Quat q = { any(), any(), any(), any() };
	if (next() % 2) { float l = sqrtf(q.x*q.x + q.y*q.y + q.z*q.z + q.w*q.w); if (l > 0) { q.x /= l; q.y /= l; q.z /= l; q.w /= l; } }
	return q;
}
// A matrix: a rotation (most game matrices), a scaled rotation, or random numbers; flags from
// optimize(), or random type bits, or identity.
static Matrix mat() {
	Matrix x;
	memset(&x, 0, sizeof(x));
	V3d axis = vec();
	switch (next() % 4) {
	case 0: Matrix::makeRotation(&x, &axis, any()); x.pos = vec(); break;
	case 1: { Quat q = quat(); Matrix::makeRotation(&x, q); x.pos = vec(); break; }
	case 2: Matrix::makeRotation(&x, &axis, any()); x.right = scale(x.right, any()); x.pos = vec(); break;
	default: x.right = vec(); x.up = vec(); x.at = vec(); x.pos = vec(); break;
	}
	switch (next() % 4) {
	case 0: x.optimize(); break;
	case 1: x.flags = next() & (Matrix::TYPEMASK | Matrix::IDENTITY); break;
	case 2: x.setIdentity(); if (next() % 2) x.pos = vec(); break;
	default: x.flags = Matrix::TYPEORTHONORMAL; break;
	}
	return x;
}

int
main(void)
{
	for (int i = 0; i < 6000; i++) {
		Matrix a = mat(), b = mat(), d = mat();
		V3d v = vec(), w = vec();
		Quat q = quat();
		CombineOp op = (CombineOp)(next() % 3);
		switch (i % 19) {
		case 0: { Matrix::Tolerance t = { any(), any(), any() }; bool def = next() % 2; printf("optimize"); m(a); if (!def) { f(t.normal); f(t.orthogonal); f(t.identity); } else printf(" default"); sep(); a.optimize(def ? nil : &t); m(a); end(); break; }
		case 1: printf("mult"); m(a); m(b); sep(); Matrix::mult(&d, &a, &b); m(d); end(); break;
		case 2: printf("mult_d=a"); m(a); m(b); sep(); Matrix::mult(&a, &a, &b); m(a); end(); break;
		case 3: printf("mult_d=b"); m(a); m(b); sep(); Matrix::mult(&b, &a, &b); m(b); end(); break;
		case 4: printf("invert"); m(a); m(d); sep(); Matrix::invert(&d, &a); m(d); end(); break;
		case 5: printf("invert_d=s"); m(a); sep(); Matrix::invert(&a, &a); m(a); end(); break;
		case 6: printf("invertgeneral"); m(a); m(d); sep(); Matrix::invertGeneral(&d, &a); m(d); end(); break;
		case 7: printf("transpose"); m(a); m(d); sep(); Matrix::transpose(&d, &a); m(d); end(); break;
		case 8: { float ang = any(); printf("rotateaxis"); m(a); v3(v); f(ang); u(op); sep(); a.rotate(&v, ang, op); m(a); end(); break; }
		case 9: printf("rotatequat"); m(a); q4(q); u(op); sep(); a.rotate(q, op); m(a); end(); break;
		case 10: printf("translate"); m(a); v3(v); u(op); sep(); a.translate(&v, op); m(a); end(); break;
		case 11: printf("scale"); m(a); v3(v); u(op); sep(); a.scale(&v, op); m(a); end(); break;
		case 12: printf("transform"); m(a); m(b); u(op); sep(); a.transform(&b, op); m(a); end(); break;
		case 13: printf("getrotation"); m(a); sep(); q4(a.getRotation()); end(); break;
		case 14: printf("lookat"); m(a); v3(v); v3(w); sep(); a.lookAt(v, w); m(a); end(); break;
		case 15: { float ang = any(); printf("makerotaxis"); m(d); v3(v); f(ang); sep(); Matrix::makeRotation(&d, &v, ang); m(d); end(); break; }
		case 16: printf("makerotquat"); m(d); q4(q); sep(); Matrix::makeRotation(&d, q); m(d); end(); break;
		case 17: printf("errors"); m(a); sep(); f(a.normalError()); f(a.orthogonalError()); f(a.identityError()); end(); break;
		case 18: printf("invertortho"); m(a); m(d); sep(); Matrix::invertOrthonormal(&d, &a); m(d); end(); break;
		}
	}
	return 0;
}

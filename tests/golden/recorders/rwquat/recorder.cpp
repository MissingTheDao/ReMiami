// Records librw's Quat::rotate (base.cpp: the axis-angle rotation quaternion combined by replace,
// preconcat or postconcat) for the Rust port (rust/revc/src/rwhanim.rs). Lines:
//   rotate Q AXIS ANGLE OP => R      floats as their bit patterns (hex)
#include <stdio.h>
#include <string.h>
#include "rwbase.h"

using namespace rw;

static uint32 rng = 0x9b05688cu;
static uint32 next() { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng; }
static float32 rf(float32 lo, float32 hi) { return lo + (hi - lo) * (next() % 1000001) / 1000000.0f; }
static uint32 bits(float32 f) { uint32 u; memcpy(&u, &f, 4); return u; }

int
main(void)
{
	for (int n = 0; n < 4000; n++) {
		Quat q = { rf(-1, 1), rf(-1, 1), rf(-1, 1), rf(-1, 1) };
		V3d axis;
		switch (next() % 3) {
		case 0: axis = makeV3d(next() % 2 ? 1.0f : -1.0f, 0, 0); if (next() % 2) axis = makeV3d(axis.y, axis.x, 0); break;	// unit axes, as the game uses
		case 1: axis = makeV3d(0, 0, 1); break;
		default: axis = makeV3d(rf(-5, 5), rf(-5, 5), rf(-5, 5)); break;
		}
		float32 angle = rf(-7, 7);
		int op = next() % 3;
		printf("rotate %08x %08x %08x %08x %08x %08x %08x %08x %d", bits(q.x), bits(q.y), bits(q.z), bits(q.w),
		       bits(axis.x), bits(axis.y), bits(axis.z), bits(angle), op);
		q.rotate(&axis, angle, (CombineOp)op);
		printf(" => %08x %08x %08x %08x\n", bits(q.x), bits(q.y), bits(q.z), bits(q.w));
	}
	return 0;
}

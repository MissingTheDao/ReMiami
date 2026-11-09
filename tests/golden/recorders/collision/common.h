// Stub common.h for the collision recorder: the math headers plus what src/collision needs.
#pragma once
#include <math.h>
#include <string.h>
#include <stdint.h>
typedef int8_t int8; typedef int16_t int16; typedef uint16_t uint16; typedef int32_t int32; typedef uint32_t uint32; typedef uint8_t uint8;
#define nil nullptr
#define TYPEALIGN(n) __attribute__ ((aligned (n)))
#define PI (float)M_PI
#define TWOPI (PI*2)
struct RwV3d { float x, y, z; };
struct RwMatrix { RwV3d right; uint32 flags; RwV3d up; uint32 pad1; RwV3d at; uint32 pad2; RwV3d pos; uint32 pad3; };
inline RwMatrix *RwMatrixUpdate(RwMatrix *m) { m->flags &= ~(0x20000u|3u); return m; }
inline int RwMatrixDestroy(RwMatrix *) { return 1; }
#include "maths.h"
#include "Vector.h"
#include "Vector2D.h"
#include "VuVector.h"
#include "Matrix.h"
#include "Rect.h"
#include <assert.h>
#include <stddef.h>
#define FIX_BUGS
#define Const const

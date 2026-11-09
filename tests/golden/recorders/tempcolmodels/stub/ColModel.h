// Stand-in ColModel.h for the recorder: the shape headers and CColModel's data (game layout).
#pragma once
#include "ColBox.h"
#include "ColSphere.h"
#include "ColLine.h"
#include "ColPoint.h"
#include "ColTriangle.h"
struct CColModel
{
	CSphere boundingSphere;
	CBox boundingBox;
	int16 numSpheres;
	int16 numBoxes;
	int16 numTriangles;
	int8 numLines;
	uint8 level;
	bool ownsCollisionVolumes;
	CColSphere *spheres;
	CColLine *lines;
	CColBox *boxes;
	CompressedVector *vertices;
	CColTriangle *triangles;
	CColTrianglePlane *trianglePlanes;
};

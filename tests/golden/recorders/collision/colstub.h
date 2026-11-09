// Stand-in for Collision.h/ColModel.h (no game, streaming or list headers): the real shape
// headers, CColModel with the game's layout, and CCollision's methods.
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

struct CStoredCollPoly
{
	CVector verts[3];
	bool valid;
};

class CCollision
{
public:
	static void CalculateTrianglePlanes(CColModel *model);
	static bool TestSphereSphere(const CSphere &s1, const CSphere &s2);
	static bool TestSphereBox(const CSphere &sph, const CBox &box);
	static bool TestLineBox(const CColLine &line, const CBox &box);
	static bool TestVerticalLineBox(const CColLine &line, const CBox &box);
	static bool TestLineTriangle(const CColLine &line, const CompressedVector *verts, const CColTriangle &tri, const CColTrianglePlane &plane);
	static bool TestLineSphere(const CColLine &line, const CColSphere &sph);
	static bool TestSphereTriangle(const CColSphere &sphere, const CompressedVector *verts, const CColTriangle &tri, const CColTrianglePlane &plane);
	static bool TestLineOfSight(const CColLine &line, const CMatrix &matrix, CColModel &model, bool ignoreSeeThrough, bool ignoreShootThrough);
	static bool ProcessSphereSphere(const CColSphere &s1, const CColSphere &s2, CColPoint &point, float &mindistsq);
	static bool ProcessSphereBox(const CColSphere &sph, const CColBox &box, CColPoint &point, float &mindistsq);
	static bool ProcessLineBox(const CColLine &line, const CColBox &box, CColPoint &point, float &mindist);
	static bool ProcessVerticalLineTriangle(const CColLine &line, const CompressedVector *verts, const CColTriangle &tri, const CColTrianglePlane &plane, CColPoint &point, float &mindist, CStoredCollPoly *poly);
	static bool ProcessLineTriangle(const CColLine &line , const CompressedVector *verts, const CColTriangle &tri, const CColTrianglePlane &plane, CColPoint &point, float &mindist, CStoredCollPoly *poly = nil);
	static bool ProcessLineSphere(const CColLine &line, const CColSphere &sphere, CColPoint &point, float &mindist);
	static bool ProcessSphereTriangle(const CColSphere &sph, const CompressedVector *verts, const CColTriangle &tri, const CColTrianglePlane &plane, CColPoint &point, float &mindistsq);
	static bool ProcessLineOfSight(const CColLine &line, const CMatrix &matrix, CColModel &model, CColPoint &point, float &mindist, bool ignoreSeeThrough, bool ignoreShootThrough);
	static bool ProcessVerticalLine(const CColLine &line, const CMatrix &matrix, CColModel &model, CColPoint &point, float &mindist, bool ignoreSeeThrough, bool ignoreShootThrough, CStoredCollPoly *poly);
	static int32 ProcessColModels(const CMatrix &matrixA, CColModel &modelA, const CMatrix &matrixB, CColModel &modelB, CColPoint *spherepoints, CColPoint *linepoints, float *linedists);
	static bool IsStoredPolyStillValidVerticalLine(const CVector &pos, float z, CColPoint &point, CStoredCollPoly *poly);
	static float DistToLine(const CVector *l0, const CVector *l1, const CVector *point);
	static float DistToLine(const CVector *l0, const CVector *l1, const CVector *point, CVector &closest);
};

// The game caches planes; the recorder computes them once per model.
void CCollision::CalculateTrianglePlanes(CColModel *model)
{
	if (model->numTriangles == 0 || model->trianglePlanes) return;
	model->trianglePlanes = new CColTrianglePlane[model->numTriangles];
	for (int i = 0; i < model->numTriangles; i++) model->trianglePlanes[i].Set(model->vertices, model->triangles[i]);
}
void CColSphere::Set(float radius, const CVector &center, uint8 surf, uint8 piece)
{ this->radius = radius; this->center = center; this->surface = surf; this->piece = piece; }

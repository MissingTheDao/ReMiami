// Prints every CTempColModels model after the original Initialise (tests/golden/tempcolmodels.txt).
#include "common.h"
#include "TempColModels.h"
#include <stdio.h>
static unsigned bits(float f) { unsigned u; memcpy(&u, &f, 4); return u; }
static void pv(const CVector &v) { printf(" %08x %08x %08x", bits(v.x), bits(v.y), bits(v.z)); }
static void pm(const char *name, const CColModel &m) {
	printf("%s", name); pv(m.boundingSphere.center); printf(" %08x", bits(m.boundingSphere.radius));
	pv(m.boundingBox.min); pv(m.boundingBox.max);
	printf(" %d %d %d %d %d %d", m.numSpheres, m.numBoxes, m.numTriangles, m.numLines, m.level, m.ownsCollisionVolumes);
	for (int i = 0; i < m.numSpheres; i++) { const CColSphere &s = m.spheres[i]; printf(" |"); pv(s.center); printf(" %08x %d %d", bits(s.radius), s.surface, s.piece); }
	printf("\n");
}
int main() {
	CTempColModels::Initialise();
	pm("Ped1", CTempColModels::ms_colModelPed1); pm("Ped2", CTempColModels::ms_colModelPed2);
	pm("BBox", CTempColModels::ms_colModelBBox); pm("Bumper1", CTempColModels::ms_colModelBumper1);
	pm("Wheel1", CTempColModels::ms_colModelWheel1); pm("Panel1", CTempColModels::ms_colModelPanel1);
	pm("BodyPart2", CTempColModels::ms_colModelBodyPart2); pm("BodyPart1", CTempColModels::ms_colModelBodyPart1);
	for (int i = 0; i < 5; i++) { char n[16]; sprintf(n, "CutObj%d", i); pm(n, CTempColModels::ms_colModelCutObj[i]); }
	pm("PedGroundHit", CTempColModels::ms_colModelPedGroundHit); pm("Boot1", CTempColModels::ms_colModelBoot1);
	pm("Door1", CTempColModels::ms_colModelDoor1); pm("Bonnet1", CTempColModels::ms_colModelBonnet1);
	pm("Weapon", CTempColModels::ms_colModelWeapon);
}

#include "common.h"
#include "General.h"
#include "World.h"
#include "Timer.h"
#include "Streaming.h"
#include "Bike.h"
#include "PlayerPed.h"
#include "PlayerInfo.h"
#include "Population.h"
#include "Pools.h"
#include "CarCtrl.h"
#include "Script.h"
#include "ModelIndices.h"
#include "NanGuard.h"
#include <cmath>
#include <cstdlib>
#include <vector>

// revc test hooks: bike rider pose vs the drawn bike (REVC_TEST=bikeride), wrecked bikes against a wall at the
// 2026-11-15 crash spot (REVC_TEST=wreckbike, NaN guard), and a height log of the peds near the player.

extern void (*gTestStepHook)(void);
extern void (*gTestRenderHook)(void);
extern "C" void revc_test_goto_spot(float x, float y, float z, float dx, float dy, float dz, int hour, int minute);

// === A. Bike Rider Check ===

static CBike *brBike = nullptr;
static CVector brSpeedVec;
static uint32 brStartMs = 0;
static CVector brPrevLocalHead;
static float brMaxJump = 0.0f;
static int brJumpFrames = 0;
static int brTotalFrames = 0;

static void
BikeStep(void)
{
    if (!brBike) return;
    uint32 t = CTimer::GetTimeInMilliseconds() - brStartMs;
    if (t < 6000) {
        brBike->SetIsStatic(false);
        brBike->SetMoveSpeed(brSpeedVec);
    }
}

static void
BikeRender(void)
{
    if (!brBike) return;
    CPlayerPed *player = FindPlayerPed();
    if (!player || !player->InVehicle() || player->m_pMyVehicle != brBike)
        return;

    uint32 t = CTimer::GetTimeInMilliseconds() - brStartMs;
    if (t < 1500) return;

    CVector head = player->GetNodePosition(PED_HEAD);
    CVector d = head - brBike->GetPosition();
    CVector local;
    local.x = DotProduct(d, brBike->GetRight());
    local.y = DotProduct(d, brBike->GetForward());
    local.z = DotProduct(d, brBike->GetUp());

    float jump = brTotalFrames > 0 ? (local - brPrevLocalHead).Magnitude() : 0.0f;
    brPrevLocalHead = local;

    brTotalFrames++;
    if (jump > brMaxJump) brMaxJump = jump;
    if (jump > 0.05f)
        brJumpFrames++;

    const char *logEnv = getenv("REVC_TEST_BIKERIDE_LOG");
    if (logEnv && logEnv[0]) {
        printf("[revc-test] bikeride frame %d ms %u local %.3f %.3f %.3f jump %.3f\n",
               brTotalFrames, t, local.x, local.y, local.z, jump);
    }
}

extern "C" int
revc_test_bikeride_setup(float mps)
{
    float x = 480.0f, y = -1400.0f;
    revc_test_goto_spot(x - 10.0f, y, -100.0f, -10.0f, -12.0f, 8.0f, 12, 0);

    CStreaming::RequestModel(MI_PCJ600, STREAMFLAGS_DEPENDENCY);
    CStreaming::LoadAllRequestedModels(false);
    if (!CStreaming::HasModelLoaded(MI_PCJ600))
        return 0;

    brBike = new CBike(MI_PCJ600, MISSION_VEHICLE);
    CVector pos(x, y, CWorld::FindGroundZForCoord(x, y));
    pos.z += brBike->GetDistanceFromCentreOfMassToBaseOfModel();
    brBike->SetPosition(pos);
    brBike->SetHeading(0.0f);
    CTheScripts::ClearSpaceForMissionEntity(pos, brBike);
    brBike->SetStatus(STATUS_ABANDONED);
    CWorld::Add(brBike);

    CPlayerPed *player = FindPlayerPed();
    player->WarpPedIntoCar(brBike);

    brSpeedVec = brBike->GetForward() * (mps / 50.0f);
    brStartMs = CTimer::GetTimeInMilliseconds();
    brPrevLocalHead = CVector(0,0,0);
    brMaxJump = 0.0f;
    brJumpFrames = 0;
    brTotalFrames = 0;

    gTestStepHook = BikeStep;
    gTestRenderHook = BikeRender;
    return 1;
}

extern "C" int
revc_test_bikeride_sample(void)
{
    return (int)(CTimer::GetTimeInMilliseconds() - brStartMs);
}

extern "C" void
revc_test_bikeride_result(float *maxJump, int *jumps, int *frames)
{
    *maxJump = brMaxJump;
    *jumps = brJumpFrames;
    *frames = brTotalFrames;
}

// === B. Wrecked-Bike Repro ===

static int wbSpawnCount = 0;
static uint32 wbStartMs = 0;
static CVector wbSpot = CVector(-60.2f, -1560.9f, 9.33f);
static std::vector<CBike *> wbBikes;
static uint32 wbNextMs = 0;

static void
WreckStep(void)
{
    uint32 t = CTimer::GetTimeInMilliseconds() - wbStartMs;
    if (t > 60000 || t < wbNextMs) return;
    wbNextMs += 1500;
    {
        int k = wbSpawnCount;
        if (wbBikes.size() >= 8) {
            // Remove oldest
            CBike *oldest = wbBikes.front();
            wbBikes.erase(wbBikes.begin());
            CWorld::Remove(oldest);
            delete oldest;
        }

        float angleRad = (k * 37.0f) * (3.14159265f / 180.0f);
        float dx = cos(angleRad) * 4.0f;
        float dy = sin(angleRad) * 4.0f;
        float bx = wbSpot.x + dx;
        float by = wbSpot.y + dy;
        CBike *bike = new CBike(MI_FAGGIO, MISSION_VEHICLE);
        float bz = CWorld::FindGroundZForCoord(bx, by) + bike->GetDistanceFromCentreOfMassToBaseOfModel() + 0.5f * (k % 3);
        bike->SetStatus(STATUS_WRECKED);
        bike->m_fHealth = 0.0f;
        
        // Heading pointing at the spot (heading h: forward = (-sin h, cos h))
        CVector toSpot = wbSpot - CVector(bx, by, bz);
        bike->SetHeading(atan2f(-toSpot.x, toSpot.y));
        
        bike->SetPosition(CVector(bx, by, bz));
        CWorld::Add(bike);
        wbBikes.push_back(bike);
        wbSpawnCount++;

        // Move speed towards spot
        float speedVal = (5.0f + 3.0f * (k % 6)) / 50.0f;
        CVector dir = toSpot; dir.z = 0.0f; dir.Normalise();
        bike->SetMoveSpeed(dir * speedVal);
        bike->SetTurnSpeed(0.02f*(k%3), 0.01f, 0.03f);
    }
}

extern "C" int
revc_test_wreckbike_setup(void)
{
    revc_test_goto_spot(-60.2f - 8.0f, -1560.9f, -100.0f, 6.0f, 0.0f, 3.0f, 12, 0);
    wbSpawnCount = 0;
    wbBikes.clear();
    wbNextMs = 0;
    CStreaming::RequestModel(MI_FAGGIO, STREAMFLAGS_DEPENDENCY);
    CStreaming::LoadAllRequestedModels(false);
    if (!CStreaming::HasModelLoaded(MI_FAGGIO))
        return 0;
    wbStartMs = CTimer::GetTimeInMilliseconds();
    gTestStepHook = WreckStep;
    gTestRenderHook = nullptr;
    return 1;
}

extern "C" int
revc_test_wreckbike_sample(void)
{
    return (int)(CTimer::GetTimeInMilliseconds() - wbStartMs);
}

extern "C" void
revc_test_wreckbike_result(int *spawned, int *recoveries)
{
    *spawned = wbSpawnCount;
    *recoveries = NanGuard::RecoveryCount();
}

// === C. Ped Height Log ===

extern "C" void
revc_test_pedlog(void)
{
    CPlayerPed *player = FindPlayerPed();
    if (!player) return;
    CVector pPos = player->GetPosition();

    CPedPool *pool = CPools::GetPedPool();
    for (int i = 0; pool && i < pool->GetSize(); i++) {
        CPed *ped = pool->GetSlot(i);
        if (ped == nil || ped == player) continue;

        CVector pos = ped->GetPosition();
        float dist = (pos - pPos).Magnitude();
        if (dist > 60.0f) continue;

        float ground = CWorld::FindGroundZForCoord(pos.x, pos.y);
        float diff = pos.z - ground;
        
        printf("[revc-test] pedlog model %d pos %.2f %.2f %.2f ground %.2f diff %.2f state %d invehicle %d\n",
               ped->GetModelIndex(), pos.x, pos.y, pos.z, ground, diff,
               ped->GetPedState(), ped->bInVehicle);
    }
}

// revc: NaN guard for physical entities (crash 2026-11-15: a wrecked bike's speeds and matrix went NaN in one
// step and RemoveAndAdd indexed the sectors with it). Changes nothing while all values are finite.
#include "common.h"
#include "Physical.h"
#include "Vehicle.h"
#include "Timer.h"
#include "NanGuard.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <unordered_map>

#ifndef _WIN32
#include <sys/stat.h>
#include <sys/types.h>
#endif

namespace NanGuard {

    static std::unordered_map<const CPhysical*, CMatrix> s_StoredMatrices;
    static bool s_Initialized = false;
    static int s_RecoveryCount = 0;
    static const int MAX_RECOVERIES = 20;

#ifndef _WIN32
    // Helper to get the log file path
    static std::string GetLogPath() {
        std::string path;
        const char* xdg = std::getenv("XDG_STATE_HOME");
        if (xdg && xdg[0]) {
            path = std::string(xdg) + "/remiami/diag.log";
        } else {
            const char* home = std::getenv("HOME");
            if (home && home[0]) {
                path = std::string(home) + "/.local/state/remiami/diag.log";
            } else {
                return "";
            }
        }
        return path;
    }

    static void EnsureLogDir(const std::string& path) {
        // Extract directory part
        size_t pos = path.find_last_of('/');
        if (pos == std::string::npos) return;
        std::string dir = path.substr(0, pos);
        
        // Create each missing directory of the path (mkdir fails harmlessly on existing ones).
        std::string current;
        for (size_t i = 0; i < dir.size(); ++i) {
            current += dir[i];
            if (dir[i] == '/') {
                mkdir(current.c_str(), 0755);
            }
        }
        if (!current.empty() && current.back() != '/') {
            mkdir(current.c_str(), 0755);
        }
    }

#endif

    static std::string GetTypeString(CPhysical* e) {
        if (e->IsVehicle()) return "vehicle";
        if (e->IsPed()) return "ped";
        if (e->IsObject()) return "object";
        return "physical";
    }

    bool Finite(const CPhysical* e) {
        if (!e) return false;

        // Get Matrix (non-const in this code base, so we const_cast)
        CMatrix& mat = const_cast<CPhysical*>(e)->GetMatrix();
        
        // Check MoveSpeed
        if (!std::isfinite(e->m_vecMoveSpeed.x) || !std::isfinite(e->m_vecMoveSpeed.y) || !std::isfinite(e->m_vecMoveSpeed.z)) return false;
        
        // Check TurnSpeed
        if (!std::isfinite(e->m_vecTurnSpeed.x) || !std::isfinite(e->m_vecTurnSpeed.y) || !std::isfinite(e->m_vecTurnSpeed.z)) return false;

        // Check Matrix components
        CVector right = mat.GetRight();
        if (!std::isfinite(right.x) || !std::isfinite(right.y) || !std::isfinite(right.z)) return false;

        CVector forward = mat.GetForward();
        if (!std::isfinite(forward.x) || !std::isfinite(forward.y) || !std::isfinite(forward.z)) return false;

        CVector up = mat.GetUp();
        if (!std::isfinite(up.x) || !std::isfinite(up.y) || !std::isfinite(up.z)) return false;

        CVector pos = mat.GetPosition();
        if (!std::isfinite(pos.x) || !std::isfinite(pos.y) || !std::isfinite(pos.z)) return false;

        return true;
    }

    void Recover(CPhysical* e, const char* stage) {
        bool logIt = s_RecoveryCount < MAX_RECOVERIES;   // the log is capped, the recovery is not
        s_RecoveryCount++;

        std::string typeStr = GetTypeString(e);
        int modelIndex = e->GetModelIndex();
        int status = e->GetStatus();
        int created = -1;
        if (e->IsVehicle()) {
            CVehicle* veh = static_cast<CVehicle*>(e);
            created = veh->VehicleCreatedBy;
        }

        float posX = 0.0f, posY = 0.0f, posZ = 0.0f;
        bool hasStoredMatrix = false;
        
        auto it = s_StoredMatrices.find(e);
        if (it != s_StoredMatrices.end()) {
            posX = it->second.GetPosition().x;
            posY = it->second.GetPosition().y;
            posZ = it->second.GetPosition().z;
            hasStoredMatrix = true;
        }

        float moveAvgX = e->m_vecMoveSpeedAvg.x;
        float moveAvgY = e->m_vecMoveSpeedAvg.y;
        float moveAvgZ = e->m_vecMoveSpeedAvg.z;

        unsigned int lastCollided = e->m_nLastTimeCollided;
        unsigned int now = CTimer::GetTimeInMilliseconds();

        char logLine[1024];
        int len = snprintf(logLine, sizeof(logLine), 
            "revc: NAN %s: %s model %d status %d created %d pos(last finite) %.2f %.2f %.2f moveAvg %.3f %.3f %.3f lastCollided %u now %u\n",
            stage,
            typeStr.c_str(),
            modelIndex,
            status,
            created,
            posX, posY, posZ,
            moveAvgX, moveAvgY, moveAvgZ,
            lastCollided,
            now
        );

        if (logIt) {
        // Print to stdout
        fwrite(logLine, 1, len, stdout);
        fflush(stdout);

        // Append to diagnostic file
#ifndef _WIN32
        std::string logPath = GetLogPath();
        if (!logPath.empty()) {
            EnsureLogDir(logPath);
            FILE* f = fopen(logPath.c_str(), "a");
            if (f) {
                fwrite(logLine, 1, len, f);
                fclose(f);
            }
        }
#endif
        }

        // Recovery logic
        // Deliberate safety deviation from the original game, which would crash here (sector index from a NaN
        // position): restore the last finite matrix of this step, stop the entity.
        if (hasStoredMatrix) {
            e->GetMatrix() = it->second;
        } else {
            e->GetMatrix().SetUnity();
            e->bRemoveFromWorld = true;
        }

        // Zero out speeds and friction
        e->m_vecMoveSpeed = CVector(0.0f, 0.0f, 0.0f);
        e->m_vecTurnSpeed = CVector(0.0f, 0.0f, 0.0f);
        e->m_vecMoveFriction = CVector(0.0f, 0.0f, 0.0f);
        e->m_vecTurnFriction = CVector(0.0f, 0.0f, 0.0f);
        e->m_fDistanceTravelled = 0.0f;

        // Update RW matrix
        e->GetMatrix().UpdateRW();
    }

    int RecoveryCount(void) { return s_RecoveryCount; }

    void StepStart(CPhysical* e) {
        if (!e) return;

        if (Finite(e)) {
            if (!s_Initialized) {
                s_StoredMatrices.reserve(4096);
                s_Initialized = true;
            }
            // Store the current matrix
            s_StoredMatrices[e] = e->GetMatrix();
        } else {
            Recover(e, "step_start");
        }
    }

    void Check(CPhysical* e, const char* stage) {
        if (!e) return;

        if (!Finite(e)) {
            Recover(e, stage);
        }
    }

} // namespace NanGuard

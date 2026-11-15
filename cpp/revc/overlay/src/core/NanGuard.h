#pragma once

#include "Physical.h"
#include "Matrix.h"

#include <unordered_map>
#include <string>

namespace NanGuard {

    void StepStart(CPhysical *e);
    void Check(CPhysical *e, const char *stage);
    bool Finite(const CPhysical *e);
    void Recover(CPhysical *e, const char *stage);
    int RecoveryCount(void);	// NaN recoveries since start (tests)

} // namespace NanGuard

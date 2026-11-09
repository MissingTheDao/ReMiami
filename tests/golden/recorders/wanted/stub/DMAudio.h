#pragma once
struct cDMAudio { void ReportCrime(int type, const CVector &c) { printf(" audio %d %08x", type, *(const uint32 *)&c.x); } };
static cDMAudio DMAudio;

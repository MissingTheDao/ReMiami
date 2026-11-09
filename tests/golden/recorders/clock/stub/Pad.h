#pragma once
struct CPad { bool shoulder; static CPad pads[2]; static CPad *GetPad(int i) { return &pads[i]; } bool GetRightShoulder1(void) { return shoulder; } };
CPad CPad::pads[2];

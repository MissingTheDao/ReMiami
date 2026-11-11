#pragma once

// revc: the Xbox world lightmaps (XboxLightmaps.cpp).
#ifdef EXTENDED_PIPELINES
namespace XboxLightmaps {
extern rw::TexDictionary *txd;
// Reads the imported lightmaps (nothing without an Xbox import).
void Init(void);
void Shutdown(void);
// Adds the lightmap of a world model's atomic n as it loads.
void Attach(const char *modelName, int n, rw::Atomic *atomic);
}
#endif

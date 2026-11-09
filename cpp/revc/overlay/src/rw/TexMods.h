#pragma once

// Texture mods: replaces textures of a freshly loaded dictionary with loose image files from the
// game's mods/textures folder (see docs/MODDING.md). The lookup and image decoding live in Rust
// (rust/revc/src/texmods.rs); this side swaps the rasters. Missing or bad files keep the original.
void TexModsApply(RwTexDictionary *dict, const char *txdName);

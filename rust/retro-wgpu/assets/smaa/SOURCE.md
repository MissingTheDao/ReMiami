# SMAA lookup textures

From https://github.com/iryoku/smaa at commit 71c806a838bdd7d517df19192a20f0c61b3ca29d
(MIT licence, LICENSE.txt), downloaded 2026-11-09 with the user's go-ahead.

- `area_160x560.rg8`: Textures/AreaTexDX9.dds (git blob 611aba02, A8L8) without its 128-byte
  DDS header; byte 0 of each texel (luminance) is R, byte 1 (alpha) is G - the RG8 layout
  SMAA.hlsl reads with `.rg` (its DX9 path reads `.ra`).
- `search_64x16.r8`: Textures/SearchTex.dds (git blob f48ec89d, L8) without its header.

The shader (src/smaa.wgsl, copied from revc-rw) is a WGSL port of SMAA.hlsl from the same commit.

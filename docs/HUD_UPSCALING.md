# HUD and menu upscaling with local AI: feasibility (2026-11-09)

Result of a study for the user; nothing is implemented yet. Upscaled game assets are made by
the user from their own copy of the game and live in `game/mods/`; they are never committed or
shipped.

## What would be upscaled

| Source | Textures | Size / format |
|---|---|---|
| `models/hud.txd` | 48: 38 radar blips, arrow, fist, radardisc, scopes | blips 16x16 DXT1; others 32-128 |
| `models/fonts.txd` (and `fonts_r.txd`) | font1, font2 | 512x512 DXT3, white RGB, glyphs in 4-bit alpha |
| `models/fronten1.txd` | background, vc_logo, mouse | 512 / 256 / 32 |
| `models/fronten2.txd` | 11 radio logos, 4 arrows, 3x3 menu map tiles | 64-256, DXT1/DXT3 |
| `models/frontend_*.txd` | controller diagrams | 256 (Switch 512) |
| `gta3.img:radar00..63.txd` | radar map tiles | 64 x 128x128 DXT1 (a 1024x1024 map) |

(The `*btns.txd` prompt buttons are already replaced by the Kenney glyphs.)

## Does the game take higher-resolution replacements? Yes, without code changes

- Texture mods (`rust/revc/src/texmods.rs`) accept any size up to 8192, and every TXD load
  path goes through them, streamed radar tiles included.
- All 2D code uses normalised texture coordinates. Font cells are fractions of the sheet and
  glyph widths come from the `CFont::Size` table, not texture pixels; radar tiles are placed
  in world units. Nothing in HUD, menu or radar code reads a raster's pixel size.
- Keep the layout: scale by an integer factor and keep font sheets cell for cell.
- Folder names are TXD *slot* names, not file names: `hud`, `fonts`, `frontend1`,
  `frontend2`, `frontend_controller`, `buttons`. So `game/mods/textures/frontend1/background.png`,
  not `fronten1/`. A `fonts/font1.png` also replaces the Russian font (same slot).

## Upscalers (local, AMD + Vulkan on Linux)

Installed (2026-11-09, with the user's go-ahead): Real-ESRGAN ncnn-vulkan v0.2.5.0 in
`tools/realesrgan/` (git-ignored; source and SHA-256 in its SOURCE.md), with the realesrgan-x4plus,
x4plus-anime and animevideov3 x2/x3/x4 models. Usage:
`tools/realesrgan/realesrgan-ncnn-vulkan -i in.png -o out.png -n realesrgan-x4plus-anime -s 4`.
First test: under a second per texture on the RX 9070 XT (512x512 to 2048x2048 in 0.95 s), RGBA
kept; the Vice City logo, radio logos and the radar ring come out clean and sharp, the menu
background sharper with a few specks (check per texture). Other candidates that run on Vulkan
without a Python ML stack:
realesrgan-ncnn-vulkan (BSD-3, x4plus / anime models), waifu2x-ncnn-vulkan (MIT; good on flat
UI art and text), realcugan-ncnn-vulkan (MIT). Models from OpenModelDB vary in licence
(many CC BY-NC-SA: fine for personal use, check each). VRAM is no issue at these sizes.

## Recommended pipeline (a script, about 150 lines of Python)

1. Export the TXDs to RGBA PNG (a prototype exporter on revc-formats exists from the study).
2. Stitch sets that tile (3x3 menu map, 8x8 radar map) into one image, so no seams appear.
3. Split RGB and alpha; upscale them separately (fonts: alpha only, then sharpen edges).
4. x4 for art, x2 for fonts (no mipmaps for them, so 4x would shimmer below 4K).
5. Merge, slice the stitched sets again, threshold 1-bit alpha textures.
6. Write PNGs to `game/mods/textures/<slot>/<name>.png`.

The 16x16 radar blips are too small for ML upscalers: use pixel-art scaling (xBRZ) or redraw.

## Risks

Halos at font edges and bleeding between glyph cells (needs tuning and checking in game);
PNG decoding on every stream-in of a radar tile (some hitching at 512x512 per tile); the colour
filter is not a concern (2D is drawn after it).

## Effort and decisions

About a day of scripting plus one or two days of tuning and checking in game (fonts above all).
Needs the user's go-ahead for: downloading an upscaler release (about 50 MB with models from
GitHub), the choice of models (licences), and whether blips and fonts get hand fixes.

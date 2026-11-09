# Kenney Input Prompts (controller and keyboard glyphs)

- Source: https://kenney.nl/assets/input-prompts (by Kenney, www.kenney.nl)
- Archive: `kenney_input-prompts_1.5.zip`, 5,070,075 bytes,
  SHA-256 `ac2fcf599080b0f3ba2d174c9474db6df1a0e96ff0662580e2da79a122ab78a1`
  (License.txt inside says version 1.5A, created 11-07-2026). Downloaded 2026-11-09.
- License: CC0 1.0 (public domain dedication), see `License.txt`. Credit is appreciated, not
  required.

## What is here

Unmodified files from the archive, for the sets this game can use:

| Folder | Used for (PadKind, rust/revc/src/host/pads.rs) | File prefix |
|---|---|---|
| `Xbox Series` | Xbox360, XboxSeries | `xbox_` (360/One/Series controller pictures too) |
| `PlayStation Series` | PlayStation3/4/5 | `playstation_` shared, `playstation3_`/`4_`/`5_` per model |
| `Nintendo Switch` | Switch | `switch_` |
| `Nintendo Switch 2` | Switch (Switch 2 ids, later) | `switch_` |
| `Nintendo Gamecube` | GameCube | `gamecube_` |
| `Steam Deck` | SteamDeck | `steamdeck_` |
| `Steam Controller` | SteamController | `steam_` |
| `Generic` | Generic joystick | `generic_` |
| `Keyboard & Mouse` | KeyboardMouse | `keyboard_`, `mouse_` |
| `Flairs` | arrows, hold/press overlays | `flair_` |

Each set has `Default/` (64x64 PNG), `Double/` (128x128 PNG), `Vector/` (SVG), `Fonts/` (the
glyphs as a font) and sprite sheets with XML atlases. `previews/` shows every set at a glance.

Left out (not relevant to this game): Meta Quest, Steam Frame, Valve Index, Nintendo Wii and
Wii U, Playdate, Touch, the archive's Overview.html and web links.

To update: download the new version from the page, record name/size/SHA-256 here, replace the
folders, and check the file names the glyph code uses still exist.

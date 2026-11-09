# Controller glyphs (plan task 9): sourcing notes

Goal: every on-screen prompt shows the buttons of the pad in use, switching when another pad is
used. Assets are not in the repo yet; this records what to use and how pads map to glyph sets.

## Recommended asset pack: Kenney "Input Prompts"

- Page: https://kenney.nl/assets/input-prompts (also https://kenney-assets.itch.io/input-prompts)
- License: **CC0** (public domain dedication): no attribution needed, may be redistributed with
  the game and inside the AppImage (plan task 24).
- Version checked 2026-11-09: 1.5a (1.5 added Steam Controller, Steam Frame, Valve Index, Meta
  Quest; 1.4 added sprite sheets and fonts; 1.2 Switch 2; 1.1b updated the PlayStation Options
  button). About 1500 files, 64x64 tiles; vector sources mentioned in the changelog.
- Covers: Xbox 360, Xbox One / Series; PlayStation 1-5; Nintendo Switch, Switch 2, Wii, Wii U,
  GameCube; Steam Deck, Steam Controller; keyboard and mouse; generic controls; Playdate; touch.
- Download is a button on the page (no stable zip URL); version and file list should be pinned in
  `scripts/deps.lock` when it is added, like the other external inputs.

Alternatives considered:
- Kenney "Input Prompts Pixel" (CC0, 16x16 pixel art): wrong style for an HD HUD.
- Mr. Breakfast's Free Prompts (CC0, SVG/PNG; Switch, Xbox Series, PS5, Steam Deck, generic):
  no Steam Controller, and a user reported PS5 L1/L2 swapped.
- Xelu's Free Controller & Key Prompts (CC0): older, no DualSense / Xbox Series; the bundled
  preview shows controller photos that are not CC0.
- Zacksly's Xbox Series / PS5 sets: CC BY 3.0 (attribution needed), fewer devices.

Note: CC0 covers the drawings. Button shapes and names (Cross/Circle, A/B, the Steam logo) are
platform trademarks; showing them to tell the player which button to press is ordinary practice,
but don't use logos as branding.

## Mapping pads to glyph sets

The host classifies each pad (rust/revc/src/host/pads.rs) from SDL3's gamepad type plus USB
vendor/product ids:

| Family | Detected by | Glyph set (Kenney folder names may differ) |
|---|---|---|
| Xbox | SDL type XBOX360 / XBOXONE, or STANDARD/unknown (default) | Xbox Series |
| PlayStation | SDL type PS3 / PS4 / PS5 | PS4 for PS3/PS4, PS5 for PS5 |
| Nintendo | SDL type NINTENDO_SWITCH_PRO / JOYCON_*, GAMECUBE | Switch (Switch 2 when the id says so) |
| Steam Deck | Valve vendor 0x28DE, product 0x1205 | Steam Deck |
| Steam Controller | SDL type STEAM, or Valve vendor 0x28DE with a controller product | Steam Controller |

- Nintendo pads put "A" on the east button; use SDL_GetGamepadButtonLabel per pad rather than a
  fixed south = A table.
- Under Steam Input every pad can appear as a virtual Xbox 360 pad (vendor 0x28DE, product
  0x11FF); SDL reports the physical type when Steam exposes it, so the type takes precedence over
  the Valve vendor id.
- The glyph set follows the **last pad that sent input**, so switching pads mid-game (or going
  back to keyboard and mouse) updates the prompts.

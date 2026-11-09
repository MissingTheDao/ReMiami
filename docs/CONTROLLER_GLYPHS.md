# Controller glyphs (plan task 9): sourcing notes

Goal: every on-screen prompt shows the buttons of the pad in use, switching when another pad is
used. The glyphs are in `assets/glyphs/kenney-input-prompts/` (see its `SOURCE.md` for version,
checksum and which sets are included).

## How it works in the game

- Prompt text uses positional tokens (`~X~` bottom face button, `~K~` left shoulder, `~U~` left
  stick up ...) that CFont draws with its button sprites (`BUTTON_ICONS`).
- Each frame `skel/host/glyphs.cpp` asks the host which glyph set is needed (`revc_glyph_set_id`)
  and, when that changes, rebuilds the sprites from the pack's 128 px PNGs (mipmapped). The host
  (`rust/revc/src/host/glyphs.rs`) picks the files: the pad used last (or the first connected
  one), its kind, and the labels SDL reports for its face buttons, so a Switch pad's bottom
  button shows B and a DualSense's shows the cross.
- Keyboard and mouse use keeps the last pad's set (prompts then show key names anyway).
- Controls menu **BUTTON PROMPTS** (`[Controller] ButtonPrompts` in reVC.ini): AUTO (default)
  follows the pad; XBOX, PLAYSTATION, NINTENDO, STEAM DECK or STEAM CONTROLLER force a style, also
  with no pad connected. The menu texts for revc's own entries are built in (`CText::GetRevcText`,
  English); the game's GXT files are not modified.
- Without the assets folder (`REVC_ASSETS_DIR`, next to the executable, or the repository
  checkout) the game's own `x360btns` / `ps3btns` / `nswbtns` TXDs are used per pad family.
- Tested by `scripts/test.sh gamepad` (virtual DualSense and Steam Deck, the override, screenshots
  in `tests/reference/gamepad/`) and `boot` (menu entry present).

## Recommended asset pack: Kenney "Input Prompts"

- Page: https://kenney.nl/assets/input-prompts (also https://kenney-assets.itch.io/input-prompts)
- License: **CC0** (public domain dedication): no attribution needed, may be redistributed with
  the game and inside the AppImage (plan task 24).
- Version checked 2026-11-09: 1.5a (1.5 added Steam Controller, Steam Frame, Valve Index, Meta
  Quest; 1.4 added sprite sheets and fonts; 1.2 Switch 2; 1.1b updated the PlayStation Options
  button). About 1500 files, 64x64 tiles; vector sources mentioned in the changelog.
- Covers: Xbox 360, Xbox One / Series; PlayStation 1-5; Nintendo Switch, Switch 2, Wii, Wii U,
  GameCube; Steam Deck, Steam Controller; keyboard and mouse; generic controls; Playdate; touch.
- Download is a button on the page (signed, expiring link, no stable URL); the archive's name,
  size and SHA-256 are recorded in `assets/glyphs/kenney-input-prompts/SOURCE.md`.

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

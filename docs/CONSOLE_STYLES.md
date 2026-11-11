# Console styles (plan task 13)

The PS2 and Xbox visual styles must look like the original games on those consoles: world look,
draw distance, UI, menus and their animations, scaled to the output resolution. The references
(emulator captures of the user's own discs, the research notes and measurements) are Rockstar
content and stay outside the repo; this file records what the code does and why.

## Steps

| Step | What | State |
|---|---|---|
| 13a | Reference scenes: the `scenes` test scenario stands the player at the console capture spots (time, weather, follow camera, HUD on) and writes 640x480 frames; `scripts/scene_colours.py` measures them | done |
| 13b | World colours per style (lighting, colour filter, PS2 highlight clipping), checked against the captures with the same measurements | PS2 checked; Xbox lightmaps and gamma open |
| 13c | Trails: console persistence, frame-rate and resolution independent, on in both console styles | done |
| 13d | Style settings: fixed LOD 1.0, Classic camera and movement, 4:3 option, PS2 time cycle from an imported disc | LOD, camera, time cycle done |
| 13e | HUD per style (Xbox safe-area shift and rounder radar), console UI scaled by screen height | done (measured elements) |
| 13f | Button glyph variants per UI style | done |
| 13g | PS2 menu look and animations | started: tabs and help box |
| 13h | Xbox menu (Maestro) look and animations, controller page | |
| 13i | Console data from an imported disc (Xbox maps and flames, PS2 animations), texture packs from zips | zip packs done |

## 13a: reference scenes

`REVC_TEST=scenes` (`scripts/test.sh scenes`, not in the default set) runs the scenes in
`rust/revc/src/testdrive.rs` (`SCENES`): the middle of Ocean Drive in front of the Moonlite Hotel facing
north (noon, night, rain, fog, dawn, east at 07:00) and the Ocean Beach lighthouse. No peds or
traffic, so runs repeat. Frames go to `rust/target/test-output/scenes/`. `REVC_TEST_SCENES=S1,S4`
picks scenes; `REVC_TEST_SCENE=x,y,heading,hour,minute,weather[;...]` adds others (finding a spot).

`tests/config/scenes.ini` is 640x480 with the PS2 filter and the **Classic** control method. With the
PC default (Standard, the mouse camera) the follow camera pitches down: the horizon sits at 31 % of
the frame height instead of 47 % as on both consoles. So the console styles need the Classic camera (13d).

First measurement (S1, noon, region means; `scene_colours.py`):

| | sky RGB | road RGB, V | white wall RGB |
|---|---|---|---|
| PS2 capture | 209 242 244 | 82 60 46, 0.32 | 220 221 214 |
| Xbox capture (PC textures) | 146 199 206 | 104 81 59, 0.41 | |
| revc, no filter | 171 208 233 | 27 33 28, 0.13 | 161 171 181 |
| revc, PS2 filter | 249 250 205 | 45 41 26, 0.20 | 212 213 159 |

Those revc numbers were wrong for a test reason: the save starts inside the hotel, and the
teleport kept the interior's time-cycle colours (the "extra colour" rows: ambient 32 instead of
70 outside), so the street was lit like a room. `revc_test_scene` and `revc_test_goto_spot` now
stop the extra colour as leaving through a door does (the `spots` references were re-made).

## 13d: style settings

- Draw distance: the PS2 and Xbox presets set the LOD scale to 1.0, as both consoles use with no
  slider (PS2 `m_PrefsLOD`, Xbox XBE `ms_lodDistScale` at 0x2c34b8); the PC preset sets the PC
  release's default 1.2. Moving the slider afterwards shows the style as Custom, like the other options.

## 13f: UI style and button glyphs

Graphics menu **UI STYLE** (`[Graphics] UiStyle`: 0 PC, 1 PS2, 2 Xbox; the visual style presets set
it, Mobile leaves it). Prompts keep the glyphs of the pad in use (any controller in any style); the UI
style only picks the variant of that pad's set (`rust/revc/src/host/glyphs.rs`, `styled_file`):

| UI style | Face buttons | D-pad | Why |
|---|---|---|---|
| Xbox | `*_button_color_*` (filled, coloured) | `*_dpad_round_*` | the Xbox menus' buttons are filled coloured balls, its D-pad sprite is round |
| PS2 | `*_button_*_outline` | `*_dpad_*_outline` | the PS2 prints button symbols as plain white glyphs in its font |
| PC | plain | plain | unchanged |

A set without the variant (Steam Deck has no coloured buttons) uses the plain glyph.

## 13e: HUD per UI style

Measured on the console stills (research notes, `CONSOLE_LOOK_SPEC.md` 4b, local only):

| | PC (revc default) | PS2 UI style | Xbox UI style |
|---|---|---|---|
| Vertical layout | the user's ProperScaling | PC constants in 448 lines (`CDraw::ScaleY`) | the same pixel constants in 480 lines |
| Money, weapon, health, armour, wanted, clock | PC positions | PC positions | moved 22 px left and 22 px down (`HUD_FROM_RIGHT`/`HUD_Y` in Hud.cpp) |
| Radar | 82x82 (FixRadar) or 94x76 | 94x76 at left 40, bottom 40 | 102x94 at left 55, bottom 42 |
| Wide screens | stretched to the edges | 4:3 area scaled by the screen height, centred (pillarboxed) | same |

The 4:3 area: `ConsoleHudBegin`/`ConsoleHudEnd` in `core/main.cpp` give the HUD code a 4:3 screen
(width = height x 4/3, aspect 4:3) and the device moves every 2D draw to the centre
(`revc_device_set_im2d_offset`). Text queued before (pickup texts placed in the 3D view) is drawn
first. Not measured yet, so left at the PC positions: zone and vehicle names, subtitles, help and
mission texts, timers and counters, the pager. Checking them needs console captures with those on screen.

## 13b: world colours

With outdoor lighting, revc's PS2 filter (re3's colourfilterVC emulation, `ColourFilter=2`) is
already close to the PS2 capture (S1/S2 region means, `scene_colours.py`):

| | PS2 capture | revc PS2 filter | Xbox capture |
|---|---|---|---|
| Noon road | 82 60 46 | 83 68 51 | 104 81 59 |
| Noon white wall | 220 221 214 | 222 223 220 | |
| Noon sky | 209 242 244 | 249 252 244 | 146 199 206 |
| Night road | 68 46 34 | 80 54 39 | 93 58 47 |
| Night building | 72 79 69 | 79 79 63 | 71 73 55 |
| Night sky | 51 42 37 | 59 43 38 | 52 40 33 |

Open: the noon sky is too red; night is about 15 % bright.

How the Xbox draws it (reverse-engineered from the Xbox executable; local notes
`xbox-research/XBOX_COLOUR_RE.md`):
- Lighting as on PC: buildings get prelight + ambient (Amb_bl with trails, Amb without), no
  directional, no x2; peds, cars and objects get Amb_Obj(_bl) + directional.
- Every world material has a per-model lightmap (`name@` textures in the Xbox `generic.txd`),
  combined as `V0*T0*(1 + LM*(T1-1))` with LM from `neo/worldTweakingTable.dat`. This is re3's
  Neo world pipeline (`neoWorldVC`), i.e. revc's World Lightmaps option with the Xbox's own data.
- The colour filter is one pass over the previous displayed frame (feedback):
  `out = (1-k)*dst + col*T1 + (1+2k)*col*T0`, k = 30/255, col = BlurRGB/255, T1 the previous
  frame at the pixel, T0 one row up (PC: three passes with a +2,+2 px offset). Trails off draws
  nothing.
- A hardware gamma ramp: at the default brightness the output is x^0.909.

## 13c: trails

Both consoles default to trails on, and both draw them as the PC does with its Trails option (the
Xbox in one shader pass with the same weights, see 13b): the previous displayed frame, shifted,
blended in with the time cycle's blur colour, feeding back. The per-channel persistence is about
(2 + 2k) x BlurRGB/255 (noon: R 0.49, G 0.32, B 0.14), which matches the 0.25-0.32 measured on the
console videos. revc's Motion Blur option draws exactly this (`CPostFX::RenderOverlayBlur`); for
the console look:

- The PS2 and Xbox presets turn it on (`MotionBlur`).
- The previous frame is kept once per game step (30 Hz, `CTimer::GetFrameCounter`), as on the
  consoles: frames drawn in between blend with the same image, so trail length and colour are the
  same at 30, 60 or 144 fps.
- The shift is 2 px at 448 lines, scaled with the screen height (was 2 px at any resolution and
  only with the debug menu's Blur switch).

## 13g/13h: console menus (approach, agreed)

re3's PS2 menu (`core/Frontend_PS2.cpp`, `PS2_MENU`) is a separate 3,000-line `CMenuManager`
chosen at compile time: it can't be switched at runtime and lacks the PC/revc options (graphics,
controls, button prompts, console discs). The Xbox menu (Maestro `R3_Menu.anm`) has no code in
re3 at all.

Proposal: a **menu skin** over the existing menu logic. Pages, entries and actions stay the PC
menu's (so every revc option stays reachable); with the PS2 or Xbox UI style the drawing is
replaced:
- PS2: background, page frames, tabs and highlight from the PS2's own sprites (`FRONTEN2.TXD`,
  imported from the player's disc; `ps2-ui-remake` upscales), layout from `Frontend_PS2.cpp`'s
  draw code, timings from the measured PS2 animations (open fade 234 ms, page cross-fade 167 ms,
  highlight slide 133-200 ms, radio logo fade 367 ms).
- Xbox: panels, icon bar, selection bar and dialogs as in `R3_Menu.anm` (decoded keyframes:
  0.2 s linear, nested clips; item pulse 0.8 s; logo glint 3.8 s), sprites from the imported
  `neo/UserInterface` files; the controller page draws the current pad in the Xbox style with
  callout lines.
- Without an imported disc, the skin falls back to the PC menu.

Steps: (1) the skin hook in `CMenuManager::DrawFrontEnd` and an animation clock; (2) PS2 skin;
(3) Xbox skin; (4) captures of each page vs the console references.

## Decisions

- **Camera (user 2026-11-11):** the console styles use the console camera. The PS2 and Xbox presets
  set the Classic control method (level follow camera, Tommy turns toward the stick, as on the
  consoles); the PC preset sets Standard (the PC's mouse camera). Changing the control method
  afterwards shows the style as Custom.
- **Menus (user 2026-11-11):** a skin over the PC menu is fine as long as it feels right.

## 13g: PS2 menu skin (in progress)

`core/MenuSkinPS2.cpp`, drawn from `CMenuManager::DrawBackground` with UI STYLE = PS2 during a
game, on the pages the PS2 pause menu has (map, brief, load, stats, controls, audio, display or
graphics, language). Done: the two rows of page tabs with the current one on the slanted green box,
the green help box (Select / Back / Resume / Navigate with the pad's glyphs), and the list shortened
to 7 rows so it stays above them. Measured on the PS2 Display capture (positions in capture pixels
in the file). `REVC_TEST_MENU_SKIN=1` shows it before a game is loaded (boot screenshots).

Still to do: the D-pad glyphs of the Navigate line don't draw; PS2 item layout ("label : value"
centred on the colon, larger row spacing); L1/R1 or left/right switching between tabs; the
animations (open fade, page cross-fade, highlight slide); the other pages checked against their
captures; the tab names are English built-in texts.

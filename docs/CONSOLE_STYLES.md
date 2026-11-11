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
| 13g | PS2 menu look and animations | done |
| 13h | Xbox menu (Maestro) look and animations, controller page | in progress: layout, icon bar, help line, selection pulse |
| 13i | Console data from an imported disc (Xbox maps and flames, PS2 animations, PS2 world textures as a style pack), texture packs from zips | zip packs and per-style packs done |

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
| Wide screens | stretched to the edges | Graphics menu HUD AREA: CORNERS (default: the console layout anchored to the screen's edges) or 4:3 (the console's 4:3 area scaled by the screen height, centred) | same |

The user (2026-11-11): the HUD doesn't need the 4:3 area in this port; corners by default, 4:3 kept
as an option. The 4:3 area: `ConsoleHudBegin`/`ConsoleHudEnd` in `core/main.cpp` give the HUD code a 4:3 screen
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

Also done: options with a value are drawn as "LABEL : VALUE" lined up on the colon (label ends at
269, colon at 282, value from 302 in 640 wide, as measured); L1/R1 step through the tabs; the page
cross-fade takes 167 ms (measured on the PS2) instead of the PC's ~390 ms; the D-pad and Select/Start
glyphs now load from the game's button TXDs too (they were never set, so they were missing without
a glyph pack).

Then: sliders start right after their label (x 311) with the label right-aligned before them,
other labels without a value are centred, long labels no longer wrap.

Finished (2026-11-11):
- Help box: always on the background panel's bottom-right corner, covering and overhanging it as on
  the PS2 (the user noted this is deliberate); placed relative to the panel corner of each page, so
  it follows the panel's shape per page and on wide screens. It has the PS2's thick black outline
  (about 8 capture pixels) and black text on the green. All four rows use the pad's outline-only glyphs
  tinted black, for one style (user 2026-11-11): Select, Back, Start ("<glyph> button") and the D-pad
  up, down, right, left for Navigate. The PS2 UI style now also takes the outline variant of
  Select/Start. Without the glyph pack: the game's own button textures untinted, "START button" as
  text and the PS2's black arrows. The rows are centred in the box as one block around a common
  dash column (widths measured at run time, so other glyphs and languages stay centred), and
  vertically on the box's centre. (Glyphs: see below.)
- Audio page: the radio picker sits lower left as on the PS2 (moved with the 2D offset, hover areas
  moved with it); the PC's helper line ("LEFT, RIGHT ... TO ADJUST") is hidden under the skin.
- Fades and highlight (measured): opening the menu 234 ms, a page change 167 ms, the highlight
  slide 167 ms (PS2 133-200 ms); the PC keeps ~390 ms and ~100 ms.
- Nothing is drawn in the tab area (the Map page's BACK; the triangle button goes back).
- Tab names MAP, BRIEF, STATS from the game's own texts; LOAD, CONTROLS, AUDIO, DISPLAY, LANGUAGE
  built-in English (the PC text has no short names for those).
- All eight tab pages checked in game (`REVC_TEST_MENU_INGAME=1 REVC_TEST_MENU_PAGE=n` with the
  saveload scenario opens the pause menu on that page).
- Kept from the PC menu on purpose: row spacing (25 px at 448 lines vs the PS2's ~30, so 7 rows fit
  above the tabs) and the PC page contents (revc's options), which the PS2 menu doesn't have.

## 13h: Xbox menu skin (in progress)

`core/MenuSkinXbox.cpp`, with UI STYLE = Xbox during a game, on the seven pages the Xbox pause menu
has (load, map, brief, stats, controls, audio, display/graphics), when the Xbox menu sprites are
imported from the player's disc (`neo/UserInterface`); otherwise the PC look stays. Measured on
native 640x480 xemu captures:
- Page title top left (52, 48), the Xbox's Vice City logo (`r3_menu_228`) top right.
- Items: labels at x 139, values at x 331 (pushed right after a longer label), rows 44.5 px apart
  from y 150, 5 rows (3 on Audio, whose radio picker sits under them at y 283); no scrollbar.
  The menu's own y units are 448 lines, so the row positions are converted.
- Mint selection bar (0, 210, 141) behind the current label with the Xbox's pulse: a white glow
  growing 0.9 to 1.157 x 1.081 while fading 0.5 to 0.059, every 0.8 s (R3_Menu.anm).
- Icon bar: disk, compass, tape, computer, pad, boombox, TV (`r3_menu_238` ... `255`) at the
  measured x, the current page's icon enlarged on a mint box.
- Help line in pink under it with the pad's glyphs (Xbox UI style: coloured): Start resume game,
  left/right select, A select, B back. BACK entries are hidden (B goes back); LT/RT switch pages.
- Sprites go through the texture mods: a loose file or pack entry `textures/ui/r3_menu_241.png`
  replaces the imported `ui/r3_menu_241.dds` (the 4x remake pack works this way); the controller
  page's picture too. `cargo run -p revc-import --example import -- <disc> <dir>` imports a disc
  outside the game (with `REVC_IMPORT_DIR=<dir>` for tests).

- Logo glint: a white flash over "city" (`r3_menu_230`) then "Vice" (`232`), 200 ms each from
  3.4 s after a page starts, every 3.8 s (anim 4); placed from the menu video, slightly fainter than
  the Xbox's. `REVC_TEST_GLINT=1|2` shows a stage for screenshots.
- Panel: per page the Xbox's panel corners (estimated from the extreme points of the camo area in
  each page's capture; bottom edge under the icon bar at y 378), moved on a page change by the PC
  menu's ~380 ms corner animation (Xbox ~400 ms). The exact values are in R3_Menu.anm anim 0 (45
  states), not decoded; not yet compared at 640x480 (the in-game menu capture runs at 1280x720).

- Dialogs (new game, load, delete confirmations): the Load page dimmed behind, the Xbox's layered
  blue dialog (steel-blue layer, dark edge, light-blue panel as measured), the question centred
  in pink capitals, the two choices side by side with the mint box; the Load page's title stays.
  The PC menu's own choice texts (NO/YES) are kept.

- Load (and delete/save) slot lists: the Xbox's light-blue, slightly tilted list box, eight rows 35 px
  apart, each slot name in black in the Xbox menu font with its date under it, the mint bar across
  the selected one (all measured).

- Controller picture page (Controls): keeps its own pad drawing and callouts with the Xbox title,
  logo and icon bar; the layout name moves under the title, the SHOW/context row sits between the
  pad and the icon bar on the mint bar, BACK is hidden.

- Controller page contexts (user 2026-11-11): ON FOOT / IN CAR / FLYING like the Xbox's Walking /
  Driving / Flying. Flying shows what the Xbox page lists: steer, accelerate, decelerate (brake),
  fire, sub-missions, radio, camera, enter/exit. The Xbox's separate "Fire missiles" has no
  action of its own in the controls found so far: not shown.

- Xbox menu font: the help line and the controller page's callout labels use the Xbox's
  RWSFArborcrestHeavy (imported texture + METRICS1 boxes, drawn by `XboxFontPrint`); callouts in
  the Xbox's green with teal lines (measured). Titles and items stay in the game's heading font,
  which is what the Xbox shows there too.

Still to do:
the Xbox fonts (RWFont/RWSFArborcrestHeavy), dialogs (blue layered panels), the Load list and Stats in the Xbox style, checking every page
against the captures.

## 13i: texture packs per UI style

`mods/packs/pc-*.zip`, `ps2-*.zip`, `xbox-*.zip` apply only in that UI style; other packs in every
style. Order: loose `mods/textures`, then the style's packs, then the general packs (each in name
order). One texture index per style (`texmods.rs`); when the UI style changes, `HostUpdateButtonGlyphs`
reloads the fonts and `hud.txd` (`CHud::ReloadTextures`, radar sprites included). Imported console
art (`ui/r3_menu_*`) is looked up in the same index.

Goal (research session, 2026-11-11): import the PS2 world textures as a style pack
(`userfiles/import/ps2/packs/ps2-world.zip`, about 12,000 textures via a Rust port of the GS
memory reader) with an option for exact PS2 versus PC textures in the PS2 style; texmods would
index packs in the import cache too. Details: research notes `importer/IMPORT_LIST.md` (local).

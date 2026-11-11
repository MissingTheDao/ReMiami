# Console styles (plan task 13)

The PS2 and Xbox visual styles must look like the original games on those consoles: world look,
draw distance, UI, menus and their animations, scaled to the output resolution. The references
(emulator captures of the user's own discs, the research notes and measurements) are Rockstar
content and stay outside the repo; this file records what the code does and why.

## Steps

| Step | What | State |
|---|---|---|
| 13a | Reference scenes: the `scenes` test scenario stands the player at the console capture spots (time, weather, follow camera, HUD on) and writes 640x480 frames; `scripts/scene_colours.py` measures them | done |
| 13b | World colours per style (lighting, colour filter, PS2 highlight clipping), checked against the captures with the same measurements | |
| 13c | Trails: console persistence (30/128 per 30 Hz frame), frame-rate and resolution independent, on in both console styles | |
| 13d | Style settings: fixed LOD 1.0, Classic camera and movement, 4:3 option, PS2 time cycle from an imported disc | LOD done |
| 13e | HUD per style (Xbox safe-area shift and rounder radar), console UI scaled by screen height | |
| 13f | Button glyph variants per UI style | |
| 13g | PS2 menu look and animations | |
| 13h | Xbox menu (Maestro) look and animations, controller page | |
| 13i | Console data from an imported disc (Xbox maps and flames, PS2 animations), texture packs from zips | |

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

revc's roads are far darker than on either console, and the PS2 filter tints everything yellow
where the PS2 stays neutral. That is 13b.

## 13d: style settings

- Draw distance: the PS2 and Xbox presets set the LOD scale to 1.0, as both consoles use with no
  slider (PS2 `m_PrefsLOD`, Xbox XBE `ms_lodDistScale` at 0x2c34b8); the PC preset sets the PC
  release's default 1.2. Moving the slider afterwards shows the style as Custom, like the other options.

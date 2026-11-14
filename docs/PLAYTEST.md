# In-person test checklist

Things the automated tests (`scripts/test.sh`) can't judge: they need a person at the machine,
real hardware or a second machine. Tick items off here (or tell Claude the result) and the plan
task can be marked Done.

Start the game with `cargo run` in `rust/` (settings in `game/reVC.ini`).

## Task 8: fixed 30 Hz simulation with render interpolation (high refresh rate)

On a 120/144 Hz display, frame limiter off (Display Setup), VSync on:

- [ ] Walk, run and turn the camera with the mouse: movement looks smooth, no judder or
      "double images" compared with 30/60 fps.
- [ ] Drive fast along a straight road and through turns: car, wheels and camera smooth; no
      jumps when the frame rate dips.
- [ ] Other cars and peds look smooth too.
- [ ] Handling feels the same as before (the simulation itself still runs at 30 Hz).
- [ ] Teleports and respawns (death, Busted, cutscene starts, loading a save): no streak or
      sliding from the old position.

Known limits, not bugs: shadows, lights and particles still move in 30 Hz steps; ped limbs
animate at 30 Hz (only the body moves smoothly).

## Task 9: controllers

Plug in each pad you have (Xbox, DualSense / DualShock, Switch Pro, Steam Deck, Steam
Controller):

- [ ] Detected when plugged in after the game started, and when unplugged / replugged.
- [ ] Prompts with buttons (e.g. a weapon pickup's "Press ~ to ...", tutorial tips) show the
      glyphs of that pad; a Switch pad shows B on the bottom button.
- [ ] Use another pad, or the keyboard then a pad: prompts switch to the pad just used.
- [ ] Controls menu, BUTTON PROMPTS: each style shows its glyphs; AUTO follows the pad again.
- [ ] Rumble: set `Vibration = 1` under `[Controller]` in `game/reVC.ini`; gunfire, explosions
      and crashes vibrate, and it stops afterwards (no menu toggle on Linux yet).
- [ ] Two pads: the second one doesn't steal player 1's input.

## Task 11: texture mods

- [ ] Put a PNG (any size) at `game/mods/textures/<txd>/<texture>.png` for a texture you can
      see (list names with `cargo run -p revc-formats --example txdlist -- ../game/models/gta3.img:<model>.txd`),
      restart: it shows in game, correct way up, not blurry up close, fine in the distance.
- [ ] Same with a DDS (DXT1 or DXT5) file.
- [ ] A broken file: the console says `texmods: ... keeping the original` and the game shows
      the original texture.

## Task 12: wgpu renderer

`scripts/build.sh` (or `cd rust && cargo build`). Two renderers, both on wgpu:

- **retro-wgpu (default since 2026-11-14):** the engine-neutral core
  (github.com/MissingTheDao/retro-wgpu) with reMiami's RenderWare shading on top.
- **The older wgpu back end (fallback):** `REVC_RENDERER=wgpu`. It also presents automatically,
  with a `WARNING` in the log, if retro-wgpu fails to start.

Either runs on `REVC_RENDERER_BACKEND=vulkan` (default) or `gl`. Untested: Windows and DX12.

- [ ] Play a while with the default and once with `REVC_RENDERER=wgpu`: both look the same.

## Console text (R4)

- [ ] PS2 style, English: an Ammu-Nation shop lists items in arrows ("< Hammer >", "< Grenades >").
- [ ] PS2 style: Stats shows "Best INSANE stunt so far:" with a colon (needs a save with stunts).

## Xbox fire (R4)

- [ ] Xbox style (with the Xbox disc imported): burning cars and explosions show white-hot,
      animated flames; switching to the PC style in the menu brings back the orange ones.

- [ ] Cars have their shiny reflections (MatFX environment map) like the normal build, on both
      backends; nothing looks inside out or too bright.
- [ ] Peds walk and animate normally (skinning); no stretched limbs.
- [ ] Display Setup > Vehicle pipeline NEO, rim light, lightmaps, road gloss on: cars get the
      Xbox-style reflections and specular highlights (compare with the normal build), peds a rim
      light, roads a gloss; nothing flickers when turning the camera.
- [ ] Menus, HUD, text, radar look like the normal build.
- [ ] Display Setup > Antialiasing: step through all 9 levels in game (SMAA smooths edges without blurring HUD text as much as FXAA); the picture changes at once,
      edges get smoother the higher the level, frame rate drops at SSAA; the level is kept after a
      restart.
- [ ] Graphics > Visual Style: left/right cycles PC, PS2, XBOX, MOBILE; each changes colour
      filter, PS2 alpha test and (with game/neo) vehicle pipeline, rim light, lightmaps, road gloss
      at once (those rows follow); changing one of them by hand shows CUSTOM; kept after a restart.
- [ ] Options > Display Setup > Colour Filter: each of None, Simple, PS2/XBOX, Mobile, PC
      changes the look at once and is kept after a restart (both builds).

## HUD/menu upscaling (docs/HUD_UPSCALING.md)

- [ ] After `scripts/upscale_hud.py`, play at your monitor's resolution (1440p/4K): HUD text,
      radar, menus and the map screen look sharper; no halos around letters, no seams between
      radar tiles, nothing upside down; menu background acceptable (or `--clean` and compare).

## Task 7 leftovers

- [ ] Change the screen resolution in the in-game Display Setup menu (not only at start-up).

## Task 15: game text now held in Rust

- [ ] Options > Language: switch through the languages; menus and in-game text change and look
      right (accents in French/German/Spanish/Italian), then switch back.
- [ ] Start a mission (mission text loads): objectives and subtitles show and go away on time,
      no "... missing"; the mission title and "MISSION PASSED" big messages appear.
- [ ] Pause menu > Brief: lists the last objectives / subtitles, newest first.
- [ ] Pager messages (e.g. when a phone mission becomes available): the text scrolls through the
      pager at the top, restarts after cutscenes, and the next queued message follows.

## Task 15: save files (file check now in Rust)

- [ ] Load Game menu lists your saves with names and dates; a damaged save shows as corrupted.
- [ ] Save in a safehouse, then load that slot.

## Task 2: N150 frame times (needs the N150 box)

- [ ] Record frame times at the fixed spots (the `spots` test places are a good set).

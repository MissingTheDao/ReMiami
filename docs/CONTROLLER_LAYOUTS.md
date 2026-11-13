# Controller layouts

Controller Setup > CONTROLLER LAYOUT picks the pad bindings: **Xbox Standard** (the default for
new setups and "Restore defaults"), **Xbox Classic**, **Original PC** (re3's old pad layout), or
**Custom** once the pad bindings match none of them. The tables are in
`cpp/re3-miami/src/core/ControllerConfig.cpp` (`padLayouts`); they are our own, written from what
the Xbox release's two presets assign (its `neo/ControlMapping1.txt` = STANDARD,
`ControlMapping2.txt` = CLASSIC; research notes in `Claude outputs/xbox-research`). The sticks
(movement, camera, car look in Standard) aren't bindings.

| Action | Xbox Standard | Xbox Classic | Original PC |
|---|---|---|---|
| Fire | RT | B | B |
| Target | LT | RT | RB |
| Sprint / Jump | A / X | A / X | A / X |
| Enter/exit vehicle | Y | Y | Y |
| Next / previous weapon | D-pad right / left | D-pad right / left | RT / LT |
| Action (phone, pick up) | D-pad up | D-pad up | LB |
| Crouch / look behind | L3 / R3 | L3 / R3 | L3 / R3 |
| Change camera | LB (Xbox White) | LB | Back |
| Camera behind player | RB (Xbox Black) | LT | - |
| Next / previous target | D-pad right / left | LT / - | - |
| Sniper zoom in / out | X / A | X / A | X / A |
| Accelerate / brake | RT / LT (analog) | A / X | A / X |
| Handbrake (Hunter rockets) | B | RT | RB |
| Vehicle fire | R3 | B | B |
| Horn | L3 | L3 | L3 |
| Sub-mission (hydraulics) | X | R3 | R3 |
| Radio station | RB (Xbox Black) | RB | LB |
| Car look left / right / behind (and drive-by) | right stick (pulled back = behind) | D-pad left / right / down | LT / RT / both |
| Movement | left stick | left stick | D-pad and left stick |

## Checked against the disc

All 25 actions that exist in both games match the Xbox disc's two presets exactly (compared
mechanically on 2026-11-10: the disc files' `@alias` lines resolved, each Xbox control turned into
the game's button number, then compared action by action). Bikes, boats, helicopters and planes
share the car's bindings on PC, and the disc gives them the same buttons as cars, so they match too.

Xbox controls with no PC action, done in the Xbox UI style with a pad on the Standard or Classic
layout by reading the stick or the raw button (core/Pad.cpp XboxFlyLayout, XboxRawJustDown):

- holster weapon (D-pad down, not while aiming at someone)
- car look and drive-by on the right stick (Standard) / D-pad down = look behind (Classic)
- flying: pitch and roll on the left stick only, heli yaw at full rate on the right stick's X
  (Standard) or the triggers (Classic), the left stick's X overriding it; look behind with the right
  stick in the air; no stabilise on full-size helis (RC helis: L3); Hunter rockets repeat while
  held
- hydraulics on the sub-mission button (X / R3) instead of the horn
- cutscene skip with A or B
- the siren: a tap shorter than 300 ms toggles it, holding longer sounds the horn
- tank and fire truck fire on LT in Classic
- triggers count as pressed from their first steps (the PC: 10 %)
- pad cheats: Vice City's console sequences (PS2 and Xbox alike), read from the buttons
  themselves in the Xbox style (LT, White = LB, RT, Black = RB for the PS2's L1, L2, R1, R2)

Still missing (research: xbox_controls_full.md §D.2):

- radio next/previous song and soundtrack (D-pad)
- digital steering on the D-pad (Standard)
- bike wheelie / first-person view (RB)

## Feel (from the Xbox executable)

- Sticks: per-axis dead zone, rescaled to full range; 0.25, 0.10 while aiming in first person,
  0.60 until a stick has once gone past 60 % on both axes (drift guard). Was a hard 0.3 cut.
- Accelerate/brake on a trigger are analog (0-255 into the pressure-sensitive Cross/Square the
  game already reads, as the PS2's buttons).
- Rumble: heavy motor shake strength * 64, light motor * 256.

Mission scripts read buttons through the same action-to-PS2-state layer. The Xbox remapped some
per script (`ScriptControls.txt`); where that differs from what reMiami's layer gives, the Xbox style
uses the Xbox's buttons (control/Script5.cpp XboxScriptButton): the phone answers with LT, the
shooting-range and "shit" menus go back with B, Pizza Boy throws with the right stick (Standard)
or the D-pad (Classic), race selection cancels with B, and Ammu-Nation, the hardware shops and race
selection browse with the D-pad. Text waits skip with A itself. (Not yet checked in those missions.)

# Changelog

What players notice, newest first. Developer details are in the commit messages.

## Unreleased

- **PS2 style:** cutscenes with the PS2's own animations and camera moves where the PS2 data
  differs; the PS2's map data (objects, placements, interiors, draw distances, and collision where
  the PS2 rebuilt a model) at the next start after choosing the style.
- **Xbox style:** the Xbox's map data likewise; help lines in the Xbox's own wording with button
  names; white-hot fire.
- **Behaviour (SilentPatch fixes):**
  - Mouse: vertical and horizontal sensitivity are now equal, so mouse aiming feels different if
    you were used to the old setting; the saved sensitivity is kept.
  - Tear gas hurting Tommy and mission characters now happens in the **PS2 style only**. PC- and
    Xbox-style players had it since 2026-11-11 and lose it again (the original PC behaviour).
  - Muggers actually rob their victims (every style).
  - Busted with the free-arrest perk keeps your weapons; only real aiming alarms shopkeepers with
    Classic controls; fist shake with brass knuckles.
- **Looped sounds:** sounds whose loop region starts at the very beginning now loop exactly that
  region instead of the whole sample (e.g. the helicopter rotor loop no longer repeats a few
  samples of tail each cycle), so some looped sounds sound slightly different, as intended.
- **Large texture packs:** packs with more than 65,535 files or over 4 GB work (ZIP64).

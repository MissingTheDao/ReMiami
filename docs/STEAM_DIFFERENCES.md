# Steam differences

reMiami's C++ base follows the original 2002 PC release. Players run the Steam build, so where the
Steam build behaves differently, reMiami should follow it (roadmap R8, decided 2026-11-14: the
Steam build is the behaviour reference for each Rust port step).

How this list is kept:
- Each Rust port step reads the Steam build's behaviour for that module locally and notes the
  differences here, in our own words: what the player sees, where in reMiami it lives, whether
  reMiami follows the Steam build yet.
- Behaviour notes only. No decompiled code, addresses or Rockstar data in this file or anywhere
  in the repo.

| Area | Difference (Steam build vs 2002 release) | reMiami | Since |
|---|---|---|---|
| PC colour filter and trails | Same in both builds: trails (the frame fed back each frame) are off by default and have their own menu option; with trails off the filter draws nothing, and the lighting takes the time cycle's blur colours only while trails are on. Confidence: high (draw states and branches compared). | Follows it since 2026-11-16 (the PC filter's trails used to be forced on); they follow the menu's trails option | 2026-11-16 |
| Car-light quads (brake, tail, reverse) | Flat untextured quads over the light panels, alpha-blended; faint (alpha 31 of 255) within 15 m of the camera, rising to full between 15 and 30 m. Confidence: high. | Same in the PC and PS2 styles | 2026-11-16 |

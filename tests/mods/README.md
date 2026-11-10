Texture-mod fixtures for `scripts/test.sh texmods` (copied to `mods/` in the test game folder):

- `textures/hud/fist.png`: 32x64 (original is 64x64), top half red, bottom half blue: checks that
  a replacement of a different size loads and is not upside down (weapon icon, top right).
- `textures/hud/radar_save.dds`: 16x16 DXT1, solid green (save-house radar icon): stays a BC
  texture (its blocks are uploaded as they are).
- `textures/hi_cuthtlalp/hotel_floortile2_64.png`: 128x128 RGB (original 64x64), quadrants red,
  green / blue, yellow: the hotel floor under the player (mipmapped world texture).
- `textures/hud/siterocket.png`: not an image: must be rejected and the original kept.

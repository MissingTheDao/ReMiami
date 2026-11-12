# Modding

## Texture mods

Replacement textures are loose image files; the game archives are never modified. Put them in
`mods/textures/` inside your game folder (next to `models/`):

```
mods/textures/<dictionary>/<texture>.png    replaces <texture> in that dictionary only
mods/textures/<texture>.png                 replaces <texture> wherever it appears
```

- **Formats:** PNG (any colour type, with or without alpha) and DDS (DXT1, DXT3, DXT5, or
  uncompressed 24/32-bit). If both exist for one texture, PNG wins.
- **Size:** any size up to 8192x8192, independent of the original. Higher-resolution
  replacements are the point; non-square images are allowed. Mipmaps are generated
  automatically where the original texture used them.
- **DXT DDS stays compressed** on the GPU (4-8x less video memory than a PNG of the same size)
  when its width and height are multiples of 4 and, for textures the game draws with mipmaps
  (world, cars), the file contains its mip levels. Otherwise it is decoded like a PNG and the
  mipmaps are generated.
- **Names** are case-insensitive. `<texture>` is the texture's name inside the dictionary
  (without extension). `<dictionary>` is:
  - for models, the TXD named in the IDE files (e.g. `infernus` for the Infernus, `hotelroomint`);
  - for loose dictionaries, the file name: `hud`, `fonts`, `fronten1`, `particle`, `wheels`,
    `generic` ...
  Folders above `<dictionary>` are ignored, so you can group mods
  (`mods/textures/cars/infernus/infernus86body128a.png`).
- **Errors:** a file that can't be read or decoded is reported once in the console
  (`texmods: <path>: <reason>; keeping the original ...`) and the original texture stays.

**Packs:** a set of replacements can stay in one zip file in `mods/packs/` (e.g.
`mods/packs/pc-hud-remake.zip`). Inside, the same layout: `textures/<dictionary>/<texture>.png`
(or `<dictionary>/<texture>.png`; files directly in `textures/` replace the texture everywhere).
Loose files in `mods/textures/` win over packs, so a single texture of a pack can be overridden;
between packs, the first in name order wins. A pack whose name starts with `pc-`, `ps2-` or `xbox-` applies
only in that UI style (Graphics menu UI STYLE), any other pack in every style; the current
style's packs come before the general ones. Changing the UI style reloads the HUD and fonts; the
menu's own art changes the next time the menu opens. Stored and deflated zip entries are supported (no
zip64 or encryption). The console names each pack and how many textures it provides.

To find texture names, list a dictionary (from the `rust/` folder):

```bash
cargo run -p revc-formats --example txdlist -- ../game/models/hud.txd
cargo run -p revc-formats --example txdlist -- ../game/models/gta3.img:infernus.txd
```

The player's skin is not read from a dictionary but from `models/generic/player.bmp` (or
`skins/<name>.bmp` for a skin picked in the menu); it takes `textures/player/player.png`, or
`textures/skins/<name>.png` for a picked skin.

The mods folder is indexed once at start-up; restart the game after adding files.

How it works: when a dictionary loads (streamed, from `txd.img` or a loose file), the C++ side
(`skel/host/texmods.cpp`) asks the Rust host (`rust/revc/src/texmods.rs`) for each texture and swaps
in a new raster. Covered by `scripts/test.sh texmods`.

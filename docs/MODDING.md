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
- **Names** are case-insensitive. `<texture>` is the texture's name inside the dictionary
  (without extension). `<dictionary>` is:
  - for models, the TXD named in the IDE files (e.g. `infernus` for the Infernus, `hotelroomint`);
  - for loose dictionaries, the file name: `hud`, `fonts`, `fronten1`, `particle`, `wheels`,
    `generic` ...
  Folders above `<dictionary>` are ignored, so you can group mods
  (`mods/textures/cars/infernus/infernus86body128a.png`).
- **Errors:** a file that can't be read or decoded is reported once in the console
  (`texmods: <path>: <reason>; keeping the original ...`) and the original texture stays.

To find texture names, list a dictionary (from the `rust/` folder):

```bash
cargo run -p revc-formats --example txdlist -- ../game/models/hud.txd
cargo run -p revc-formats --example txdlist -- ../game/models/gta3.img:infernus.txd
```

The mods folder is indexed once at start-up; restart the game after adding files.

How it works: when a dictionary loads (streamed, from `txd.img` or a loose file), the C++ side
(`skel/host/texmods.cpp`) asks the Rust host (`rust/revc/src/texmods.rs`) for each texture and swaps
in a new raster. Covered by `scripts/test.sh texmods`.

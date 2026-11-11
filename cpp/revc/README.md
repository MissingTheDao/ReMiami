# cpp/revc: revc's part of the C++ game

The game still builds from the reVC C++ source (re3, "miami" branch, commit `dc21472`,
2021-09-02) while it is ported to Rust. That source is not in this repository. This folder holds
only what revc adds to it:

| Path | What |
|---|---|
| `reVC.sha256` | The 518 reVC files the build needs, with their SHA-256 (`sha256sum -c` format) |
| `patches/*.patch` | revc's changes to those files, one unified diff per source folder |
| `overlay/` | Files that are revc's own (menu skins, host glue, test hooks, ...) and librw |

`scripts/revc-source.sh prepare <your copy>` checks your copy against `reVC.sha256` and writes
`cpp/re3-miami/` (git-ignored) from it, the patches and the overlay. After editing
`cpp/re3-miami/`, `scripts/revc-source.sh export` writes the edits back here. See
[docs/BUILDING.md](../../docs/BUILDING.md).

The patches show a few unchanged reVC lines around each change, as every diff does. As the port
moves code to Rust, files leave the manifest, and the reVC copy is needed for less and less.

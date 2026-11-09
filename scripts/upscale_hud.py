#!/usr/bin/env python3
"""Upscale the HUD, fonts, menu art and radar map with Real-ESRGAN into texture mods.

usage: scripts/upscale_hud.py [--game DIR] [--only SET ...] [--clean] [--force]

Reads the game's own textures (your copy of the game), upscales them with
tools/realesrgan (Real-ESRGAN ncnn-vulkan, see docs/HUD_UPSCALING.md) and writes PNG texture mods
to <game>/mods/textures/<txd slot>/<texture>.png, which the game loads instead of the originals.
The results are yours: they stay in game/mods, are never committed or shipped.

Sets (--only picks some): hud, fonts, frontend1, frontend2, radar.
- Art is upscaled 4x. Colour and alpha are upscaled separately; colour is first bled into fully
  transparent pixels so edges get no dark halo; textures with on/off alpha keep on/off alpha.
- Fonts (white glyphs in the alpha channel) are upscaled 2x from the alpha alone and sharpened
  a little; 4x would shimmer without mipmaps.
- HUD textures under 32 pixels (radar blips) are left alone: too small for the upscaler.
- Tiled sets are stitched before upscaling and cut again, so no seams appear: the 3x3 menu map
  (frontend2 mapTop/Mid/Bot01-03) and the 8x8 radar map (radar00-63, tile x + 8 y).
Files written are listed in <game>/mods/textures/upscaled.json; --clean removes exactly those.
Existing mods not written by this script are never overwritten (unless --force).
"""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ESRGAN = ROOT / "tools/realesrgan/realesrgan-ncnn-vulkan"
MODEL = "realesrgan-x4plus-anime"
EXPORTER = ROOT / "rust/target/release/examples/txdexport"
MANIFEST = "upscaled.json"
SETS = ["hud", "fonts", "frontend1", "frontend2", "radar"]


# --- Image helpers (drafted by the local model, reviewed and fixed) ---------------------------

def bleed_rgb(rgba: np.ndarray, iterations: int = 8) -> np.ndarray:
    """RGB of fully transparent pixels filled from their filled 4-neighbours (mean), repeated
    `iterations` times; pixels never reached keep their RGB; alpha unchanged."""
    rgba = np.copy(rgba)
    h, w, _ = rgba.shape
    filled = rgba[:, :, 3] > 0
    for _ in range(iterations):
        rgb = np.pad(rgba[:, :, :3], ((1, 1), (1, 1), (0, 0)))
        fp = np.pad(filled.astype(np.uint8), ((1, 1), (1, 1)))
        neigh = [(rgb[0:h, 1:w + 1], fp[0:h, 1:w + 1]), (rgb[2:h + 2, 1:w + 1], fp[2:h + 2, 1:w + 1]),
                 (rgb[1:h + 1, 0:w], fp[1:h + 1, 0:w]), (rgb[1:h + 1, 2:w + 2], fp[1:h + 1, 2:w + 2])]
        count = sum(f for _, f in neigh).astype(np.float64)
        total = sum(c.astype(np.float64) * f[:, :, None] for c, f in neigh)
        update = ~filled & (count > 0)
        if not update.any():
            break
        rgba[update, :3] = np.round(total[update] / count[update][:, None]).astype(np.uint8)
        filled |= update
    return rgba


def smoothstep_alpha(alpha: np.ndarray, lo: float, hi: float) -> np.ndarray:
    """alpha/255 through smoothstep(lo, hi), back to uint8."""
    t = np.clip((alpha.astype(np.float64) / 255.0 - lo) / (hi - lo), 0.0, 1.0)
    return np.round(t * t * (3.0 - 2.0 * t) * 255.0).astype(np.uint8)


# --- Upscaling --------------------------------------------------------------------------------

def esrgan(rgb: np.ndarray) -> np.ndarray:
    """HxWx3 uint8 -> 4Hx4Wx3 with the Real-ESRGAN anime model."""
    with tempfile.TemporaryDirectory() as tmp:
        src, dst = Path(tmp) / "in.png", Path(tmp) / "out.png"
        Image.fromarray(rgb, "RGB").save(src)
        r = subprocess.run([str(ESRGAN), "-i", str(src), "-o", str(dst), "-n", MODEL, "-s", "4", "-f", "png"],
                           cwd=ESRGAN.parent, capture_output=True, text=True)
        if r.returncode != 0 or not dst.exists():
            sys.exit(f"realesrgan failed: {r.stderr.strip()[-500:]}")
        return np.asarray(Image.open(dst).convert("RGB"))


def upscale_art(rgba: np.ndarray) -> np.ndarray:
    """4x, colour and alpha separately."""
    alpha = rgba[:, :, 3]
    rgb = esrgan(bleed_rgb(rgba, 16)[:, :, :3])
    if alpha.min() == 255:
        a = np.full(rgb.shape[:2], 255, np.uint8)
    else:
        a = esrgan(np.repeat(alpha[:, :, None], 3, axis=2)).mean(axis=2).round().astype(np.uint8)
        if np.isin(alpha, (0, 255)).all():
            a = np.where(a >= 128, 255, 0).astype(np.uint8)
    return np.dstack([rgb, a])


def upscale_font(rgba: np.ndarray) -> np.ndarray:
    """2x from the alpha alone (font sheets are white with the glyphs in alpha)."""
    h, w = rgba.shape[:2]
    a4 = esrgan(np.repeat(rgba[:, :, 3:4], 3, axis=2)).mean(axis=2).astype(np.uint8)
    a2 = np.asarray(Image.fromarray(a4, "L").resize((w * 2, h * 2), Image.LANCZOS))
    a2 = smoothstep_alpha(a2, 0.1, 0.9)
    rgb = np.full((h * 2, w * 2, 3), 255, np.uint8)
    return np.dstack([rgb, a2])


def stitch(tiles, cols, rows):
    """tiles: row-major list of equal-size HxWx4 arrays -> one image."""
    h, w = tiles[0].shape[:2]
    out = np.zeros((h * rows, w * cols, 4), np.uint8)
    for i, t in enumerate(tiles):
        y, x = divmod(i, cols)
        out[y * h:(y + 1) * h, x * w:(x + 1) * w] = t
    return out


def cut(img, cols, rows):
    h, w = img.shape[0] // rows, img.shape[1] // cols
    return [img[y * h:(y + 1) * h, x * w:(x + 1) * w] for y in range(rows) for x in range(cols)]


# --- Game data --------------------------------------------------------------------------------

def export(src: str, outdir: Path) -> dict:
    """All textures of a TXD (file or archive.img:name.txd) as RGBA arrays by name."""
    r = subprocess.run([str(EXPORTER), src, str(outdir)], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"export of {src} failed: {r.stderr.strip()[-500:]}")
    return {p.stem: np.asarray(Image.open(p).convert("RGBA")) for p in sorted(outdir.glob("*.png"))}


class Output:
    def __init__(self, root: Path, force: bool):
        self.root, self.force = root, force
        path = root / MANIFEST
        self.previous = set(json.loads(path.read_text())) if path.exists() else set()
        self.written = set()

    def write(self, slot: str, name: str, rgba: np.ndarray):
        rel = f"{slot}/{name}.png"
        path = self.root / rel
        if path.exists() and rel not in self.previous and not self.force:
            print(f"  keeping {rel}: not made by this script (--force overwrites)")
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray(rgba, "RGBA").save(path, optimize=False)
        self.written.add(rel)

    def save_manifest(self):
        (self.root / MANIFEST).write_text(json.dumps(sorted(self.previous | self.written), indent=1))


def run_set(name, game, work, out):
    models = game / "models"
    if name == "hud":
        tex = export(str(models / "hud.txd"), work / "hud")
        for n, t in tex.items():
            if min(t.shape[:2]) < 32:
                continue
            out.write("hud", n, upscale_art(t))
    elif name == "fonts":
        for n, t in export(str(models / "fonts.txd"), work / "fonts").items():
            out.write("fonts", n, upscale_font(t))
    elif name == "frontend1":
        for n, t in export(str(models / "fronten1.txd"), work / "frontend1").items():
            out.write("frontend1", n, upscale_art(t))
    elif name == "frontend2":
        tex = export(str(models / "fronten2.txd"), work / "frontend2")
        names = [f"map{r}{c:02d}" for r in ("Top", "Mid", "Bot") for c in (1, 2, 3)]
        if all(n in tex for n in names):
            for n, t in zip(names, cut(upscale_art(stitch([tex[n] for n in names], 3, 3)), 3, 3)):
                out.write("frontend2", n, t)
        for n, t in tex.items():
            if n not in names:
                out.write("frontend2", n, upscale_art(t))
    elif name == "radar":
        img = models / "gta3.img"
        tiles = []
        for i in range(64):
            tex = export(f"{img}:radar{i:02d}.txd", work / f"radar{i:02d}")
            tiles.append(tex[f"radar{i:02d}"])
        for i, t in enumerate(cut(upscale_art(stitch(tiles, 8, 8)), 8, 8)):
            out.write(f"radar{i:02d}", f"radar{i:02d}", t)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--game", type=Path, default=ROOT / "game")
    ap.add_argument("--only", nargs="+", choices=SETS)
    ap.add_argument("--clean", action="store_true", help="remove the files this script wrote")
    ap.add_argument("--force", action="store_true", help="overwrite mods not made by this script")
    args = ap.parse_args()
    root = args.game / "mods" / "textures"
    if args.clean:
        path = root / MANIFEST
        listed = json.loads(path.read_text()) if path.exists() else []
        for rel in listed:
            (root / rel).unlink(missing_ok=True)
        path.unlink(missing_ok=True)
        print(f"removed {len(listed)} upscaled textures")
        return
    if not ESRGAN.exists():
        sys.exit(f"{ESRGAN} missing: see docs/HUD_UPSCALING.md")
    if not (args.game / "models" / "hud.txd").exists():
        sys.exit(f"no game data in {args.game}")
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "revc-formats", "--example", "txdexport"],
                   cwd=ROOT / "rust", check=True)
    out = Output(root, args.force)
    with tempfile.TemporaryDirectory() as tmp:
        for s in args.only or SETS:
            print(f"== {s}")
            run_set(s, args.game, Path(tmp), out)
            out.save_manifest()
    print(f"wrote {len(out.written)} textures to {root}")


if __name__ == "__main__":
    main()

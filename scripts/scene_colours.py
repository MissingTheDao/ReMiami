#!/usr/bin/env python3
"""Region colours of scene screenshots (plan task 13): mean RGB and HSV value/saturation of the
sky, road and building regions used for the console captures (the Moonlite spot, facing north).
usage: scene_colours.py image.png [...]   (any size: regions are fractions of the frame)"""
import colorsys, sys
from PIL import Image

REGIONS = {'sky': (.40, .03, .56, .13), 'road': (.58, .78, .88, .93), 'building': (.03, .30, .14, .48)}


def regions(path):
    im = Image.open(path).convert('RGB').resize((320, 240), Image.BILINEAR)
    out = {}
    for name, (x0, y0, x1, y1) in REGIONS.items():
        px = list(im.crop((int(x0 * 320), int(y0 * 240), int(x1 * 320), int(y1 * 240))).getdata())
        rgb = [sum(p[i] for p in px) / len(px) for i in range(3)]
        hsv = [colorsys.rgb_to_hsv(*(c / 255 for c in p)) for p in px]
        s = sum(h[1] for h in hsv) / len(hsv)
        v = sum(h[2] for h in hsv) / len(hsv)
        out[name] = (rgb, s, v)
    return out


if __name__ == '__main__':
    for path in sys.argv[1:]:
        print(path.rsplit('/', 1)[-1])
        for name, (rgb, s, v) in regions(path).items():
            print(f'  {name:9s} rgb {rgb[0]:5.1f} {rgb[1]:5.1f} {rgb[2]:5.1f}  S {s:.2f}  V {v:.2f}')

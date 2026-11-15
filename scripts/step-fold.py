#!/usr/bin/env python3
"""Step-phase fold of a frame grab (R6): how much of the frame-to-frame image change falls on one phase
of the 30 Hz game step. A once-per-step jump in what is drawn shows as one phase far above the mean.

Input: a raw file of 192x108 8-bit gray frames (REVC_GRAB_RAW, host/mod.rs) or a video (--video, decoded
with ffmpeg to the same size). Prints the peak phase / mean ratio of the fold at the step period and of a
control fold at a wrong period (11.5 frames at 320 fps), per region.

The default region leaves out the HUD (rows 20-62 % of the height: below the help box, clock and radio
name, above the radar and subtitles). A band that contains HUD text measures the HUD's own once-per-step
changes, so a hypothesis is not "excluded" from a band that includes the HUD (2026-11-15).

usage: step-fold.py FILE [--video] [--fps 320] [--from N] [--to N] [--rows 0.20:0.62] [--cols 0:1] [--max R]
  --max R: exit 1 if the region's peak ratio exceeds R.
"""
import argparse, subprocess, sys
import numpy as np

W, H = 192, 108

def load(path, video):
    if video:
        raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', path, '-vf', f'scale={W}:{H}:flags=area,format=gray',
                              '-f', 'rawvideo', '-'], check=True, stdout=subprocess.PIPE).stdout
        return np.frombuffer(raw, np.uint8).reshape(-1, H, W)
    return np.fromfile(path, np.uint8).reshape(-1, H, W)

def fold(d, period, k0, bins=32):
    ph = (((np.arange(len(d)) + k0) % period) / period * bins).astype(int)
    m = np.array([d[ph == b].mean() for b in range(bins) if np.any(ph == b)])
    return m.max() / m.mean()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('file')
    ap.add_argument('--video', action='store_true')
    ap.add_argument('--fps', type=float, default=320.0)
    ap.add_argument('--from', dest='start', type=int, default=0)
    ap.add_argument('--to', dest='end', type=int, default=None)
    ap.add_argument('--rows', default='0.20:0.62')
    ap.add_argument('--cols', default='0:1')
    ap.add_argument('--max', type=float, default=None)
    a = ap.parse_args()
    frames = load(a.file, a.video)
    end = len(frames) if a.end is None else min(a.end, len(frames))
    if end - a.start < 64:
        print(f'step-fold: only {end - a.start} frames in range', file=sys.stderr)
        return 2
    r0, r1 = (float(x) for x in a.rows.split(':'))
    rows = slice(int(r0 * H), int(r1 * H))
    c0, c1 = (float(x) for x in a.cols.split(':'))
    cols = slice(int(c0 * W), int(c1 * W))
    seg = frames[a.start:end, rows, cols].astype(np.int16)
    d = np.abs(np.diff(seg, axis=0)).mean(axis=(1, 2))
    period = a.fps / 30.0
    peak, ctrl = fold(d, period, a.start), fold(d, period * 1.078, a.start)
    print(f'step-fold: {len(d)} frame changes, level {d.mean():.2f}, peak {peak:.2f} (control {ctrl:.2f}), rows {a.rows} cols {a.cols}')
    if a.max is not None and peak > a.max:
        print(f'step-fold: FAIL peak {peak:.2f} > {a.max}')
        return 1
    return 0

if __name__ == '__main__':
    sys.exit(main())

"""Per-step L/R gains of the openal-pan.c recording (relative to the mono test tone)."""
import math, struct, sys
b = open(sys.argv[1], 'rb').read(); o = 12
while o + 8 <= len(b):
    cid = b[o:o+4]; sz = struct.unpack('<I', b[o+4:o+8])[0]; body = o + 8
    if cid == b'fmt ': fmt = b[body:body+sz]
    if cid == b'data': data = b[body:] if sz in (0, 0xffffffff) or body + sz > len(b) else b[body:body+sz]
    o = body + sz + (sz & 1)
rate = struct.unpack('<I', fmt[4:8])[0]
v = struct.unpack('<%df' % (len(data) // 4), data[:len(data) // 4 * 4]); L, R = v[0::2], v[1::2]
blk = rate // 100
act = [sum(L[j]*L[j] + R[j]*R[j] for j in range(i, i + blk)) > 1e-6 for i in range(0, len(L) - blk, blk)]
segs, start = [], None
for k, a in enumerate(act):
    if a and start is None: start = k
    if not a and start is not None:
        if k - start > 10: segs.append((start, k))
        start = None
steps = [l.split() for l in open(sys.argv[2]).read().splitlines()[1:]]
assert len(segs) == len(steps), (len(segs), len(steps))
ref = 0.5 / math.sqrt(2)
print('# label x y z buffer gainL gainR   (OpenAL Soft, listener at=+Y up=+Z, inverse distance clamped, ref 1)')
for (a, e), st in zip(segs, steps):
    i0, i1 = (a + 3) * blk, (e - 3) * blk
    g = [math.sqrt(sum(x * x for x in ch[i0:i1]) / (i1 - i0)) / ref for ch in (L, R)]
    print(f'{st[1]} {st[2]} {st[3]} {st[4]} {st[5]} {g[0]:.4f} {g[1]:.4f}')

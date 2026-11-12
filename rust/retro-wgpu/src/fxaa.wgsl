// FXAA (fast approximate antialiasing) on a finished RGBA8 image.
//
// Algorithm: Timothy Lottes, "FXAA" (NVIDIA whitepaper, 2009; FXAA 3.11 "quality" structure:
// local contrast test, subpixel blend, edge orientation, search along the edge, blend towards
// the nearer edge end). This WGSL was written for retro-wgpu from that published description;
// no NVIDIA source code was copied.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: V;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return o;
}

fn lum(uv: vec2<f32>) -> f32 {
    let c = textureSampleLevel(src, smp, uv, 0.0).rgb;
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

const EDGE_MIN: f32 = 0.0312;
const EDGE_REL: f32 = 0.125;
const SUBPIX: f32 = 0.75;
const STEPS: i32 = 10;

@fragment
fn fs(i: V) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    let uv = i.uv;
    let m = textureSampleLevel(src, smp, uv, 0.0);
    let lm = dot(m.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let ln = lum(uv + vec2<f32>(0.0, -px.y));
    let ls = lum(uv + vec2<f32>(0.0, px.y));
    let le = lum(uv + vec2<f32>(px.x, 0.0));
    let lw = lum(uv + vec2<f32>(-px.x, 0.0));
    let hi = max(lm, max(max(ln, ls), max(le, lw)));
    let lo = min(lm, min(min(ln, ls), min(le, lw)));
    let range = hi - lo;
    if (range < max(EDGE_MIN, hi * EDGE_REL)) {
        return m;
    }
    let lne = lum(uv + vec2<f32>(px.x, -px.y));
    let lnw = lum(uv + vec2<f32>(-px.x, -px.y));
    let lse = lum(uv + vec2<f32>(px.x, px.y));
    let lsw = lum(uv + vec2<f32>(-px.x, px.y));

    // Subpixel aliasing: how much the centre differs from its neighbourhood.
    let avg = (2.0 * (ln + ls + le + lw) + lne + lnw + lse + lsw) / 12.0;
    let sub0 = clamp(abs(avg - lm) / range, 0.0, 1.0);
    let sub1 = smoothstep(0.0, 1.0, sub0);
    let sub = sub1 * sub1 * SUBPIX;

    // Horizontal edge (luma changes along y) or vertical.
    let gh = abs(ln + ls - 2.0 * lm) * 2.0 + abs(lne + lse - 2.0 * le) + abs(lnw + lsw - 2.0 * lw);
    let gv = abs(le + lw - 2.0 * lm) * 2.0 + abs(lne + lnw - 2.0 * ln) + abs(lse + lsw - 2.0 * ls);
    let horizontal = gh >= gv;
    let l1 = select(lw, ln, horizontal);
    let l2 = select(le, ls, horizontal);
    let g1 = abs(l1 - lm);
    let g2 = abs(l2 - lm);
    let toward1 = g1 >= g2;
    let grad = 0.25 * max(g1, g2);
    var step = select(px.x, px.y, horizontal);
    var edge_luma = 0.5 * (lm + l2);
    if (toward1) {
        step = -step;
        edge_luma = 0.5 * (lm + l1);
    }
    // Start half a pixel towards the neighbour, then walk along the edge both ways.
    var start = uv;
    let along = select(vec2<f32>(0.0, px.y), vec2<f32>(px.x, 0.0), horizontal);
    if (horizontal) {
        start.y += step * 0.5;
    } else {
        start.x += step * 0.5;
    }
    var p1 = start - along;
    var p2 = start + along;
    var d1 = lum(p1) - edge_luma;
    var d2 = lum(p2) - edge_luma;
    var done1 = abs(d1) >= grad;
    var done2 = abs(d2) >= grad;
    for (var k = 0; k < STEPS; k++) {
        if (done1 && done2) {
            break;
        }
        if (!done1) {
            p1 -= along * 1.5;
            d1 = lum(p1) - edge_luma;
            done1 = abs(d1) >= grad;
        }
        if (!done2) {
            p2 += along * 1.5;
            d2 = lum(p2) - edge_luma;
            done2 = abs(d2) >= grad;
        }
    }
    let dist1 = select(uv.y - p1.y, uv.x - p1.x, horizontal);
    let dist2 = select(p2.y - uv.y, p2.x - uv.x, horizontal);
    let nearer1 = dist1 < dist2;
    let dist = min(dist1, dist2);
    let span = dist1 + dist2;
    // Only blend when the centre is on the side whose end-luma agrees with it.
    let centre_smaller = lm < edge_luma;
    let end_ok = select((d2 < 0.0) != centre_smaller, (d1 < 0.0) != centre_smaller, nearer1);
    let edge_off = select(0.0, 0.5 - dist / span, end_ok);
    let off = max(edge_off, sub);
    var fuv = uv;
    if (horizontal) {
        fuv.y += off * step;
    } else {
        fuv.x += off * step;
    }
    return vec4<f32>(textureSampleLevel(src, smp, fuv, 0.0).rgb, m.a);
}

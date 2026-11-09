// SMAA 1x (Enhanced Subpixel Morphological Antialiasing), a WGSL port of SMAA.hlsl from
// https://github.com/iryoku/smaa (commit 71c806a8), preset HIGH, luma edge detection.
//
// Copyright (C) 2013 Jorge Jimenez, Jose I. Echevarria, Belen Masia, Fernando Navarro,
// Diego Gutierrez. MIT licence: rust/revc-rw/assets/smaa/LICENSE.txt.
//
// Three passes, each a full-screen triangle:
//   edges:   colour (tex_a) -> edges (RG8), discarding pixels without an edge (target cleared to 0)
//   weights: edges (tex_a), area (tex_b), search (tex_c) -> blending weights (RGBA8, cleared to 0)
//   blend:   colour (tex_a), weights (tex_b) -> antialiased colour
// Texture coordinates are D3D's (y down), as in WGSL, so nothing is flipped.

const THRESHOLD: f32 = 0.1;
const MAX_SEARCH_STEPS: f32 = 16.0;
const MAX_SEARCH_STEPS_DIAG: f32 = 8.0;
const CORNER_ROUNDING_NORM: f32 = 0.25;
const LOCAL_CONTRAST_ADAPTATION_FACTOR: f32 = 2.0;
const AREATEX_MAX_DISTANCE: f32 = 16.0;
const AREATEX_MAX_DISTANCE_DIAG: f32 = 20.0;
const AREATEX_PIXEL_SIZE: vec2<f32> = vec2<f32>(1.0 / 160.0, 1.0 / 560.0);
const AREATEX_SUBTEX_SIZE: f32 = 1.0 / 7.0;
const SEARCHTEX_SIZE: vec2<f32> = vec2<f32>(66.0, 33.0);
const SEARCHTEX_PACKED_SIZE: vec2<f32> = vec2<f32>(64.0, 16.0);

@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var tex_b: texture_2d<f32>;
@group(0) @binding(2) var tex_c: texture_2d<f32>;
@group(0) @binding(3) var lin: sampler;
@group(0) @binding(4) var pnt: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: V;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return o;
}

// SMAA_RT_METRICS of the image in tex_a: (1/w, 1/h, w, h).
fn metrics() -> vec4<f32> {
    let d = vec2<f32>(textureDimensions(tex_a));
    return vec4<f32>(1.0 / d, d);
}

// HLSL round (halves away from zero); WGSL's rounds halves to even.
fn hround(x: f32) -> f32 {
    return sign(x) * floor(abs(x) + 0.5);
}
fn hround2(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(hround(v.x), hround(v.y));
}
fn hround4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(hround(v.x), hround(v.y), hround(v.z), hround(v.w));
}

fn edge_lin(c: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(tex_a, lin, c, 0.0).rg;
}

// ---- Pass 1: luma edge detection -----------------------------------------------------------

fn luma_at(c: vec2<f32>) -> f32 {
    return dot(textureSampleLevel(tex_a, pnt, c, 0.0).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn edges(i: V) -> @location(0) vec4<f32> {
    let m = metrics();
    let uv = i.uv;
    let o0 = m.xyxy * vec4<f32>(-1.0, 0.0, 0.0, -1.0) + uv.xyxy;
    let o1 = m.xyxy * vec4<f32>(1.0, 0.0, 0.0, 1.0) + uv.xyxy;
    let o2 = m.xyxy * vec4<f32>(-2.0, 0.0, 0.0, -2.0) + uv.xyxy;

    let l = luma_at(uv);
    let l_left = luma_at(o0.xy);
    let l_top = luma_at(o0.zw);
    let d_lt = abs(l - vec2<f32>(l_left, l_top));
    var e = step(vec2<f32>(THRESHOLD), d_lt);
    if (dot(e, vec2<f32>(1.0)) == 0.0) {
        discard;
    }
    let l_right = luma_at(o1.xy);
    let l_bottom = luma_at(o1.zw);
    var max_delta = max(d_lt, abs(l - vec2<f32>(l_right, l_bottom)));
    let l_leftleft = luma_at(o2.xy);
    let l_toptop = luma_at(o2.zw);
    max_delta = max(max_delta, abs(vec2<f32>(l_left, l_top) - vec2<f32>(l_leftleft, l_toptop)));
    let final_delta = max(max_delta.x, max_delta.y);
    e = e * step(vec2<f32>(final_delta), LOCAL_CONTRAST_ADAPTATION_FACTOR * d_lt);
    return vec4<f32>(e, 0.0, 0.0);
}

// ---- Pass 2: blending weights (tex_a edges, tex_b area, tex_c search) -----------------------

fn decode_diag2(e_in: vec2<f32>) -> vec2<f32> {
    var e = e_in;
    e.x = e.x * abs(5.0 * e.x - 5.0 * 0.75);
    return hround2(e);
}

fn decode_diag4(e_in: vec4<f32>) -> vec4<f32> {
    var e = e_in;
    e.x = e.x * abs(5.0 * e.x - 5.0 * 0.75);
    e.z = e.z * abs(5.0 * e.z - 5.0 * 0.75);
    return hround4(e);
}

// Returns (steps, end found flag) in xy; the last edge value through `e`.
fn search_diag1(texcoord: vec2<f32>, dir: vec2<f32>, e: ptr<function, vec2<f32>>) -> vec2<f32> {
    let m = metrics();
    var coord = vec4<f32>(texcoord, -1.0, 1.0);
    let t = vec3<f32>(m.xy, 1.0);
    while (coord.z < MAX_SEARCH_STEPS_DIAG - 1.0 && coord.w > 0.9) {
        coord = vec4<f32>(t * vec3<f32>(dir, 1.0) + coord.xyz, coord.w);
        *e = edge_lin(coord.xy);
        coord.w = dot(*e, vec2<f32>(0.5));
    }
    return coord.zw;
}

fn search_diag2(texcoord: vec2<f32>, dir: vec2<f32>, e: ptr<function, vec2<f32>>) -> vec2<f32> {
    let m = metrics();
    var coord = vec4<f32>(texcoord, -1.0, 1.0);
    coord.x += 0.25 * m.x;
    let t = vec3<f32>(m.xy, 1.0);
    while (coord.z < MAX_SEARCH_STEPS_DIAG - 1.0 && coord.w > 0.9) {
        coord = vec4<f32>(t * vec3<f32>(dir, 1.0) + coord.xyz, coord.w);
        *e = decode_diag2(edge_lin(coord.xy));
        coord.w = dot(*e, vec2<f32>(0.5));
    }
    return coord.zw;
}

fn area_diag(dist: vec2<f32>, e: vec2<f32>, offset: f32) -> vec2<f32> {
    var tc = vec2<f32>(AREATEX_MAX_DISTANCE_DIAG) * e + dist;
    tc = AREATEX_PIXEL_SIZE * tc + 0.5 * AREATEX_PIXEL_SIZE;
    tc.x += 0.5;
    tc.y += AREATEX_SUBTEX_SIZE * offset;
    return textureSampleLevel(tex_b, lin, tc, 0.0).rg;
}

fn calculate_diag_weights(texcoord: vec2<f32>, e: vec2<f32>) -> vec2<f32> {
    let m = metrics();
    var weights = vec2<f32>(0.0);
    var d = vec4<f32>(0.0);
    var end = vec2<f32>(0.0);
    if (e.x > 0.0) {
        let r = search_diag1(texcoord, vec2<f32>(-1.0, 1.0), &end);
        d.x = r.x;
        d.z = r.y;
        d.x += f32(end.y > 0.9);
    }
    let r2 = search_diag1(texcoord, vec2<f32>(1.0, -1.0), &end);
    d.y = r2.x;
    d.w = r2.y;

    if (d.x + d.y > 2.0) {
        let coords = vec4<f32>(-d.x + 0.25, d.x, d.y, -d.y - 0.25) * m.xyxy + texcoord.xyxy;
        var c: vec4<f32>;
        let cxy = textureSampleLevel(tex_a, lin, coords.xy, 0.0, vec2<i32>(-1, 0)).rg;
        let czw = textureSampleLevel(tex_a, lin, coords.zw, 0.0, vec2<i32>(1, 0)).rg;
        let dec = decode_diag4(vec4<f32>(cxy, czw));
        c = vec4<f32>(dec.y, dec.x, dec.w, dec.z); // c.yxwz = decode(c.xyzw)
        var cc = vec2<f32>(2.0) * c.xz + c.yw;
        cc = select(cc, vec2<f32>(0.0), step(vec2<f32>(0.9), d.zw) > vec2<f32>(0.5));
        weights += area_diag(d.xy, cc, 0.0);
    }

    let r3 = search_diag2(texcoord, vec2<f32>(-1.0, -1.0), &end);
    d.x = r3.x;
    d.z = r3.y;
    if (textureSampleLevel(tex_a, lin, texcoord, 0.0, vec2<i32>(1, 0)).r > 0.0) {
        let r4 = search_diag2(texcoord, vec2<f32>(1.0, 1.0), &end);
        d.y = r4.x;
        d.w = r4.y;
        d.y += f32(end.y > 0.9);
    } else {
        d.y = 0.0;
        d.w = 0.0;
    }

    if (d.x + d.y > 2.0) {
        let coords = vec4<f32>(-d.x, -d.x, d.y, d.y) * m.xyxy + texcoord.xyxy;
        var c: vec4<f32>;
        c.x = textureSampleLevel(tex_a, lin, coords.xy, 0.0, vec2<i32>(-1, 0)).g;
        c.y = textureSampleLevel(tex_a, lin, coords.xy, 0.0, vec2<i32>(0, -1)).r;
        let zw = textureSampleLevel(tex_a, lin, coords.zw, 0.0, vec2<i32>(1, 0)).gr;
        c.z = zw.x;
        c.w = zw.y;
        var cc = vec2<f32>(2.0) * c.xz + c.yw;
        cc = select(cc, vec2<f32>(0.0), step(vec2<f32>(0.9), d.zw) > vec2<f32>(0.5));
        weights += area_diag(d.xy, cc, 0.0).yx;
    }
    return weights;
}

fn search_length(e: vec2<f32>, offset: f32) -> f32 {
    var scale = SEARCHTEX_SIZE * vec2<f32>(0.5, -1.0);
    var bias = SEARCHTEX_SIZE * vec2<f32>(offset, 1.0);
    scale += vec2<f32>(-1.0, 1.0);
    bias += vec2<f32>(0.5, -0.5);
    scale *= 1.0 / SEARCHTEX_PACKED_SIZE;
    bias *= 1.0 / SEARCHTEX_PACKED_SIZE;
    return textureSampleLevel(tex_c, lin, scale * e + bias, 0.0).r;
}

fn search_x_left(texcoord_in: vec2<f32>, end: f32) -> f32 {
    let m = metrics();
    var texcoord = texcoord_in;
    var e = vec2<f32>(0.0, 1.0);
    while (texcoord.x > end && e.g > 0.8281 && e.r == 0.0) {
        e = edge_lin(texcoord);
        texcoord = -vec2<f32>(2.0, 0.0) * m.xy + texcoord;
    }
    let offset = -(255.0 / 127.0) * search_length(e, 0.0) + 3.25;
    return m.x * offset + texcoord.x;
}

fn search_x_right(texcoord_in: vec2<f32>, end: f32) -> f32 {
    let m = metrics();
    var texcoord = texcoord_in;
    var e = vec2<f32>(0.0, 1.0);
    while (texcoord.x < end && e.g > 0.8281 && e.r == 0.0) {
        e = edge_lin(texcoord);
        texcoord = vec2<f32>(2.0, 0.0) * m.xy + texcoord;
    }
    let offset = -(255.0 / 127.0) * search_length(e, 0.5) + 3.25;
    return -m.x * offset + texcoord.x;
}

fn search_y_up(texcoord_in: vec2<f32>, end: f32) -> f32 {
    let m = metrics();
    var texcoord = texcoord_in;
    var e = vec2<f32>(1.0, 0.0);
    while (texcoord.y > end && e.r > 0.8281 && e.g == 0.0) {
        e = edge_lin(texcoord);
        texcoord = -vec2<f32>(0.0, 2.0) * m.xy + texcoord;
    }
    let offset = -(255.0 / 127.0) * search_length(e.gr, 0.0) + 3.25;
    return m.y * offset + texcoord.y;
}

fn search_y_down(texcoord_in: vec2<f32>, end: f32) -> f32 {
    let m = metrics();
    var texcoord = texcoord_in;
    var e = vec2<f32>(1.0, 0.0);
    while (texcoord.y < end && e.r > 0.8281 && e.g == 0.0) {
        e = edge_lin(texcoord);
        texcoord = vec2<f32>(0.0, 2.0) * m.xy + texcoord;
    }
    let offset = -(255.0 / 127.0) * search_length(e.gr, 0.5) + 3.25;
    return -m.y * offset + texcoord.y;
}

fn area(dist: vec2<f32>, e1: f32, e2: f32, offset: f32) -> vec2<f32> {
    var tc = vec2<f32>(AREATEX_MAX_DISTANCE) * hround2(4.0 * vec2<f32>(e1, e2)) + dist;
    tc = AREATEX_PIXEL_SIZE * tc + 0.5 * AREATEX_PIXEL_SIZE;
    tc.y = AREATEX_SUBTEX_SIZE * offset + tc.y;
    return textureSampleLevel(tex_b, lin, tc, 0.0).rg;
}

fn corner_horizontal(weights: vec2<f32>, texcoord: vec4<f32>, d: vec2<f32>) -> vec2<f32> {
    let left_right = step(d.xy, d.yx);
    var rounding = (1.0 - CORNER_ROUNDING_NORM) * left_right;
    rounding /= left_right.x + left_right.y;
    var factor = vec2<f32>(1.0);
    factor.x -= rounding.x * textureSampleLevel(tex_a, lin, texcoord.xy, 0.0, vec2<i32>(0, 1)).r;
    factor.x -= rounding.y * textureSampleLevel(tex_a, lin, texcoord.zw, 0.0, vec2<i32>(1, 1)).r;
    factor.y -= rounding.x * textureSampleLevel(tex_a, lin, texcoord.xy, 0.0, vec2<i32>(0, -2)).r;
    factor.y -= rounding.y * textureSampleLevel(tex_a, lin, texcoord.zw, 0.0, vec2<i32>(1, -2)).r;
    return weights * clamp(factor, vec2<f32>(0.0), vec2<f32>(1.0));
}

fn corner_vertical(weights: vec2<f32>, texcoord: vec4<f32>, d: vec2<f32>) -> vec2<f32> {
    let left_right = step(d.xy, d.yx);
    var rounding = (1.0 - CORNER_ROUNDING_NORM) * left_right;
    rounding /= left_right.x + left_right.y;
    var factor = vec2<f32>(1.0);
    factor.x -= rounding.x * textureSampleLevel(tex_a, lin, texcoord.xy, 0.0, vec2<i32>(1, 0)).g;
    factor.x -= rounding.y * textureSampleLevel(tex_a, lin, texcoord.zw, 0.0, vec2<i32>(1, 1)).g;
    factor.y -= rounding.x * textureSampleLevel(tex_a, lin, texcoord.xy, 0.0, vec2<i32>(-2, 0)).g;
    factor.y -= rounding.y * textureSampleLevel(tex_a, lin, texcoord.zw, 0.0, vec2<i32>(-2, 1)).g;
    return weights * clamp(factor, vec2<f32>(0.0), vec2<f32>(1.0));
}

@fragment
fn weights(i: V) -> @location(0) vec4<f32> {
    let m = metrics();
    let texcoord = i.uv;
    let pixcoord = texcoord * m.zw;
    let off0 = m.xyxy * vec4<f32>(-0.25, -0.125, 1.25, -0.125) + texcoord.xyxy;
    let off1 = m.xyxy * vec4<f32>(-0.125, -0.25, -0.125, 1.25) + texcoord.xyxy;
    let off2 = m.xxyy * vec4<f32>(-2.0, 2.0, -2.0, 2.0) * MAX_SEARCH_STEPS + vec4<f32>(off0.xz, off1.yw);

    var w = vec4<f32>(0.0);
    var e = textureSampleLevel(tex_a, lin, texcoord, 0.0).rg;

    if (e.g > 0.0) { // edge at north
        let diag = calculate_diag_weights(texcoord, e);
        w = vec4<f32>(diag, w.zw);
        if (w.x == -w.y) { // no diagonal: horizontal processing
            var d: vec2<f32>;
            var coords: vec3<f32>;
            coords.x = search_x_left(off0.xy, off2.x);
            coords.y = off1.y;
            d.x = coords.x;
            let e1 = textureSampleLevel(tex_a, lin, coords.xy, 0.0).r;
            coords.z = search_x_right(off0.zw, off2.y);
            d.y = coords.z;
            d = abs(hround2(m.zz * d - pixcoord.xx));
            let sqrt_d = sqrt(d);
            let e2 = textureSampleLevel(tex_a, lin, coords.zy, 0.0, vec2<i32>(1, 0)).r;
            var rg = area(sqrt_d, e1, e2, 0.0);
            coords.y = texcoord.y;
            rg = corner_horizontal(rg, coords.xyzy, d);
            w = vec4<f32>(rg, w.zw);
        } else {
            e.r = 0.0; // skip vertical processing
        }
    }

    if (e.r > 0.0) { // edge at west
        var d: vec2<f32>;
        var coords: vec3<f32>;
        coords.y = search_y_up(off1.xy, off2.z);
        coords.x = off0.x;
        d.x = coords.y;
        let e1 = textureSampleLevel(tex_a, lin, coords.xy, 0.0).g;
        coords.z = search_y_down(off1.zw, off2.w);
        d.y = coords.z;
        d = abs(hround2(m.ww * d - pixcoord.yy));
        let sqrt_d = sqrt(d);
        let e2 = textureSampleLevel(tex_a, lin, coords.xz, 0.0, vec2<i32>(0, 1)).g;
        var ba = area(sqrt_d, e1, e2, 0.0);
        coords.x = texcoord.x;
        ba = corner_vertical(ba, coords.xyxz, d);
        w = vec4<f32>(w.xy, ba);
    }
    return w;
}

// ---- Pass 3: neighbourhood blending (tex_a colour, tex_b weights) ---------------------------

@fragment
fn blend(i: V) -> @location(0) vec4<f32> {
    let m = metrics();
    let texcoord = i.uv;
    let offset = m.xyxy * vec4<f32>(1.0, 0.0, 0.0, 1.0) + texcoord.xyxy;
    var a: vec4<f32>;
    a.x = textureSampleLevel(tex_b, lin, offset.xy, 0.0).a; // right
    a.y = textureSampleLevel(tex_b, lin, offset.zw, 0.0).g; // top
    let bl = textureSampleLevel(tex_b, lin, texcoord, 0.0);
    a.w = bl.x; // bottom
    a.z = bl.z; // left

    if (dot(a, vec4<f32>(1.0)) < 1e-5) {
        return vec4<f32>(textureSampleLevel(tex_a, lin, texcoord, 0.0).rgb, 1.0);
    }
    let h = max(a.x, a.z) > max(a.y, a.w);
    var blending_offset = vec4<f32>(0.0, a.y, 0.0, a.w);
    var blending_weight = a.yw;
    if (h) {
        blending_offset = vec4<f32>(a.x, 0.0, a.z, 0.0);
        blending_weight = a.xz;
    }
    blending_weight /= dot(blending_weight, vec2<f32>(1.0));
    let blending_coord = blending_offset * vec4<f32>(m.xy, -m.xy) + texcoord.xyxy;
    var color = blending_weight.x * textureSampleLevel(tex_a, lin, blending_coord.xy, 0.0);
    color += blending_weight.y * textureSampleLevel(tex_a, lin, blending_coord.zw, 0.0);
    return vec4<f32>(color.rgb, 1.0);
}

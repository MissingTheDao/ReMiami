//! The draw shaders: a fixed WGSL core (transform, per-vertex lighting, texture coordinate
//! generation, fog, four combiner stages, alpha test) plus a vertex entry point generated per
//! vertex layout. All behaviour switches are uniforms, so pipelines only differ in fixed-function
//! state and vertex layout.
//!
//! New code for retro-wgpu (the equations are the textbook fixed-function ones: Gouraud
//! directional lighting, GL sphere-map texgen, linear/exp/exp2 fog, D3D-style stage ops).

use crate::layout::{AttrFormat, Semantic, VertexLayout};
use crate::state::*;

/// Byte size of the per-draw uniform block (see `Draw` in CORE).
pub(crate) const DRAW_BLOCK: usize = 720;
/// Byte size of the per-view uniform block.
pub(crate) const VIEW_BLOCK: usize = 144;
pub const MAX_BONES: usize = 64;
pub(crate) const BONES_BLOCK: usize = MAX_BONES * 64;

const CORE: &str = r#"
struct ViewU { view: mat4x4<f32>, proj: mat4x4<f32>, screen: vec4<f32> };
struct DrawU {
    world: mat4x4<f32>,
    tex_matrix: array<mat4x4<f32>, 4>,
    mat_diffuse: vec4<f32>,
    mat_ambient: vec4<f32>,
    mat_emissive: vec4<f32>,
    ambient: vec4<f32>,
    light_dir: array<vec4<f32>, 4>,
    light_color: array<vec4<f32>, 4>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    stage_const: array<vec4<f32>, 4>,
    // x: 1 lighting, 2 normalise, 4/8/16 diffuse/ambient/emissive from the vertex colour;
    // y: light count; z: alpha compare (0 never .. 7 always); w: active stages.
    flags: vec4<u32>,
    alpha_ref: vec4<f32>,
    stage_ops: array<vec4<u32>, 4>,
    stage_coords: vec4<u32>,
};
@group(0) @binding(0) var<uniform> v: ViewU;
@group(1) @binding(0) var<uniform> d: DrawU;
@group(2) @binding(0) var t0: texture_2d<f32>;
@group(2) @binding(1) var s0: sampler;
@group(2) @binding(2) var t1: texture_2d<f32>;
@group(2) @binding(3) var s1: sampler;
@group(2) @binding(4) var t2: texture_2d<f32>;
@group(2) @binding(5) var s2: sampler;
@group(2) @binding(6) var t3: texture_2d<f32>;
@group(2) @binding(7) var s3: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv01: vec4<f32>,
    @location(2) uv23: vec4<f32>,
    @location(3) fog: f32,
};

fn light(n: vec3<f32>, vcol: vec4<f32>) -> vec4<f32> {
    let f = d.flags.x;
    let diff = select(d.mat_diffuse, vcol, (f & 4u) != 0u && HAS_COLOR);
    if ((f & 1u) == 0u) {
        return diff;
    }
    let amb = select(d.mat_ambient, vcol, (f & 8u) != 0u && HAS_COLOR);
    let emi = select(d.mat_emissive, vcol, (f & 16u) != 0u && HAS_COLOR);
    var c = emi.rgb + d.ambient.rgb * amb.rgb;
    for (var k = 0u; k < min(d.flags.y, 4u); k++) {
        c += max(dot(n, -d.light_dir[k].xyz), 0.0) * d.light_color[k].rgb * diff.rgb;
    }
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), diff.a);
}

fn fog_factor(z: f32) -> f32 {
    let mode = u32(d.fog.w);
    if (mode == 1u) {
        return clamp((d.fog.y - z) / (d.fog.y - d.fog.x), 0.0, 1.0);
    }
    if (mode == 2u) {
        return clamp(exp(-d.fog.z * z), 0.0, 1.0);
    }
    if (mode == 3u) {
        let e = d.fog.z * z;
        return clamp(exp(-e * e), 0.0, 1.0);
    }
    return 1.0;
}

fn gen_coords(s: u32, vp: vec3<f32>, vn: vec3<f32>, uv: array<vec2<f32>, 4>) -> vec2<f32> {
    var uvs = uv;
    let src = d.stage_coords[s];
    var c = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (src < 4u) {
        c = vec4<f32>(uvs[src], 0.0, 1.0);
    } else {
        let e = select(vec3<f32>(0.0, 0.0, -1.0), normalize(vp), dot(vp, vp) > 0.0);
        let r = reflect(e, vn);
        if (src == 4u) {
            let m = 2.0 * sqrt(r.x * r.x + r.y * r.y + (r.z + 1.0) * (r.z + 1.0));
            c = vec4<f32>(r.x / max(m, 1e-6) + 0.5, r.y / max(m, 1e-6) + 0.5, 0.0, 1.0);
        } else if (src == 5u) {
            c = vec4<f32>(r, 1.0);
        } else if (src == 6u) {
            c = vec4<f32>(vn, 1.0);
        } else {
            c = vec4<f32>(vp, 1.0);
        }
    }
    return (d.tex_matrix[s] * c).xy;
}

fn finish(p: vec4<f32>, n: vec3<f32>, vcol: vec4<f32>, uv: array<vec2<f32>, 4>) -> VOut {
    var o: VOut;
    let wp = d.world * p;
    let vp = v.view * wp;
    o.pos = v.proj * vp;
    var wn = (d.world * vec4<f32>(n, 0.0)).xyz;
    if ((d.flags.x & 2u) != 0u && dot(wn, wn) > 0.0) {
        wn = normalize(wn);
    }
    let vn3 = (v.view * vec4<f32>(wn, 0.0)).xyz;
    let vn = select(vn3, normalize(vn3), dot(vn3, vn3) > 0.0);
    o.color = light(wn, vcol);
    o.uv01 = vec4<f32>(gen_coords(0u, vp.xyz, vn, uv), gen_coords(1u, vp.xyz, vn, uv));
    o.uv23 = vec4<f32>(gen_coords(2u, vp.xyz, vn, uv), gen_coords(3u, vp.xyz, vn, uv));
    o.fog = fog_factor(abs(vp.z));
    return o;
}

// Screen-space vertices (x, y pixels, z 0..1, rhw): perspective-correct through w = 1/rhw.
fn finish_t(p: vec4<f32>, vcol: vec4<f32>, uv: array<vec2<f32>, 4>) -> VOut {
    var o: VOut;
    let w = select(1.0 / p.w, 1.0, p.w == 0.0);
    let ndc = vec2<f32>((p.x + v.screen.z) / v.screen.x * 2.0 - 1.0, 1.0 - (p.y + v.screen.z) / v.screen.y * 2.0);
    o.pos = vec4<f32>(ndc * w, p.z * w, w);
    o.color = select(d.mat_diffuse, vcol, (d.flags.x & 4u) != 0u && HAS_COLOR);
    let z = vec3<f32>(0.0);
    o.uv01 = vec4<f32>(gen_coords(0u, z, z, uv), gen_coords(1u, z, z, uv));
    o.uv23 = vec4<f32>(gen_coords(2u, z, z, uv), gen_coords(3u, z, z, uv));
    o.fog = fog_factor(w);
    return o;
}

fn arg(code: u32, cur: vec4<f32>, diff: vec4<f32>, tex: vec4<f32>, k: vec4<f32>) -> vec4<f32> {
    var x = cur;
    let s = code & 15u;
    if (s == 1u) {
        x = diff;
    } else if (s == 2u) {
        x = tex;
    } else if (s == 3u) {
        x = k;
    }
    if ((code & 32u) != 0u) {
        x = vec4<f32>(x.a);
    }
    if ((code & 16u) != 0u) {
        x = vec4<f32>(1.0) - x;
    }
    return x;
}

fn combine(o: u32, a1: vec4<f32>, a2: vec4<f32>, cur: vec4<f32>, diff: vec4<f32>, tex: vec4<f32>, k: vec4<f32>) -> vec4<f32> {
    let one = vec4<f32>(1.0);
    let zero = vec4<f32>(0.0);
    switch o {
        case 1u: { return a1; }
        case 2u: { return a2; }
        case 3u: { return a1 * a2; }
        case 4u: { return clamp(a1 * a2 * 2.0, zero, one); }
        case 5u: { return clamp(a1 * a2 * 4.0, zero, one); }
        case 6u: { return clamp(a1 + a2, zero, one); }
        case 7u: { return clamp(a1 + a2 - vec4<f32>(0.5), zero, one); }
        case 8u: { return clamp(a1 - a2, zero, one); }
        case 9u: { return mix(a2, a1, tex.a); }
        case 10u: { return mix(a2, a1, diff.a); }
        case 11u: { return mix(a2, a1, k.a); }
        case 12u: { return mix(a2, a1, cur.a); }
        case 13u: { return clamp(a1 * a2 + cur, zero, one); }
        default: { return cur; }
    }
}

fn stage(s: u32, cur: vec4<f32>, diff: vec4<f32>, tex: vec4<f32>) -> vec4<f32> {
    let o = d.stage_ops[s];
    let k = d.stage_const[s];
    let c = combine(o.x, arg(o.y & 255u, cur, diff, tex, k), arg(o.y >> 8u, cur, diff, tex, k), cur, diff, tex, k);
    let a = combine(o.z, arg(o.w & 255u, cur, diff, tex, k), arg(o.w >> 8u, cur, diff, tex, k), cur, diff, tex, k);
    return vec4<f32>(c.rgb, a.a);
}

fn alpha_pass(a: f32) -> bool {
    // 8-bit comparison, as the hardware these games ran on did it.
    let x = round(clamp(a, 0.0, 1.0) * 255.0);
    let r = d.alpha_ref.x;
    switch d.flags.z {
        case 0u: { return false; }
        case 1u: { return x < r; }
        case 2u: { return x == r; }
        case 3u: { return x <= r; }
        case 4u: { return x > r; }
        case 5u: { return x != r; }
        case 6u: { return x >= r; }
        default: { return true; }
    }
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let x0 = textureSample(t0, s0, i.uv01.xy);
    let x1 = textureSample(t1, s1, i.uv01.zw);
    let x2 = textureSample(t2, s2, i.uv23.xy);
    let x3 = textureSample(t3, s3, i.uv23.zw);
    let diff = i.color;
    var cur = diff;
    let n = d.flags.w;
    if (n > 0u) { cur = stage(0u, cur, diff, x0); }
    if (n > 1u) { cur = stage(1u, cur, diff, x1); }
    if (n > 2u) { cur = stage(2u, cur, diff, x2); }
    if (n > 3u) { cur = stage(3u, cur, diff, x3); }
    if (!alpha_pass(cur.a)) {
        discard;
    }
    return vec4<f32>(mix(d.fog_color.rgb, cur.rgb, i.fog), cur.a);
}
"#;

const BONES: &str = r#"
struct Bones { m: array<mat4x4<f32>, 64> };
@group(3) @binding(0) var<uniform> bones: Bones;
"#;

fn wgsl_type(f: AttrFormat) -> &'static str {
    match f.components() {
        _ if f.is_int() => "vec4<u32>",
        2 => "vec2<f32>",
        3 => "vec3<f32>",
        _ => "vec4<f32>",
    }
}

/// `x` (an input of format `f`) as a vec4 with w = `w`.
fn as_vec4(x: &str, f: AttrFormat, w: &str) -> String {
    match f.components() {
        2 => format!("vec4<f32>({x}, 0.0, {w})"),
        3 => format!("vec4<f32>({x}, {w})"),
        _ => x.to_string(),
    }
}

/// The full WGSL for a layout (core + generated vertex entry point).
pub(crate) fn source(layout: &VertexLayout) -> String {
    let mut s = String::new();
    s += &format!("const HAS_COLOR: bool = {};\n", layout.has(Semantic::Color));
    s += CORE;
    if layout.skinned() {
        s += BONES;
    }
    s += "struct VIn {\n";
    for a in &layout.attributes {
        let name = match a.semantic {
            Semantic::Position | Semantic::PositionT => "pos".to_string(),
            Semantic::Normal => "normal".into(),
            Semantic::Color => "color".into(),
            Semantic::TexCoord(n) => format!("uv{n}"),
            Semantic::BlendWeights => "weights".into(),
            Semantic::BlendIndices => "indices".into(),
        };
        s += &format!("    @location({}) {name}: {},\n", VertexLayout::location(a.semantic), wgsl_type(a.format));
    }
    s += "};\n@vertex\nfn vs(i: VIn) -> VOut {\n";
    let uv = |n: u8| match layout.get(Semantic::TexCoord(n)) {
        Some(_) => format!("i.uv{n}.xy"),
        None => "vec2<f32>(0.0)".into(),
    };
    s += &format!("    let uv = array<vec2<f32>, 4>({}, {}, {}, {});\n", uv(0), uv(1), uv(2), uv(3));
    s += &match layout.get(Semantic::Color) {
        Some(a) if a.format == AttrFormat::Unorm8x4Bgra => "    let col = i.color.zyxw;\n".to_string(),
        Some(a) => format!("    let col = {};\n", as_vec4("i.color", a.format, "1.0")),
        None => "    let col = vec4<f32>(1.0);\n".into(),
    };
    if let Some(a) = layout.get(Semantic::PositionT) {
        let _ = a;
        s += "    return finish_t(i.pos, col, uv);\n}\n";
        return s;
    }
    let p = layout.get(Semantic::Position).unwrap();
    s += &format!("    var p = {};\n", if p.format.components() == 4 { "vec4<f32>(i.pos.xyz, 1.0)".to_string() } else { as_vec4("i.pos", p.format, "1.0") });
    s += &match layout.get(Semantic::Normal) {
        Some(a) => format!("    var n = {}.xyz;\n", as_vec4("i.normal", a.format, "0.0")),
        None => "    var n = vec3<f32>(0.0);\n".into(),
    };
    if layout.skinned() {
        let w = layout.get(Semantic::BlendWeights).unwrap();
        s += &format!("    let w = {};\n", as_vec4("i.weights", w.format, "0.0"));
        s += "    var sp = vec4<f32>(0.0);\n    var sn = vec3<f32>(0.0);\n";
        s += "    for (var k = 0u; k < 4u; k++) {\n        let m = bones.m[min(i.indices[k], 63u)];\n        sp += (m * p) * w[k];\n        sn += (m * vec4<f32>(n, 0.0)).xyz * w[k];\n    }\n";
        s += "    p = sp;\n    n = sn;\n";
    }
    s += "    return finish(p, n, col, uv);\n}\n";
    s
}

fn compare_code(c: CompareFunction) -> u32 {
    match c {
        CompareFunction::Never => 0,
        CompareFunction::Less => 1,
        CompareFunction::Equal => 2,
        CompareFunction::LessEqual => 3,
        CompareFunction::Greater => 4,
        CompareFunction::NotEqual => 5,
        CompareFunction::GreaterEqual => 6,
        CompareFunction::Always => 7,
    }
}

fn op_code(o: Op) -> u32 {
    match o {
        Op::Disable => 0,
        Op::Arg1 => 1,
        Op::Arg2 => 2,
        Op::Modulate => 3,
        Op::Modulate2x => 4,
        Op::Modulate4x => 5,
        Op::Add => 6,
        Op::AddSigned => 7,
        Op::Subtract => 8,
        Op::BlendTextureAlpha => 9,
        Op::BlendDiffuseAlpha => 10,
        Op::BlendConstantAlpha => 11,
        Op::BlendCurrentAlpha => 12,
        Op::MultiplyAdd => 13,
    }
}

fn arg_code(a: Arg) -> u32 {
    (match a.source {
        Source::Current => 0,
        Source::Diffuse => 1,
        Source::Texture => 2,
        Source::Constant => 3,
    }) | if a.invert { 16 } else { 0 }
        | if a.alpha { 32 } else { 0 }
}

/// The per-draw uniform block, as 32-bit words.
pub(crate) type DrawBlock = [u32; DRAW_BLOCK / 4];

pub(crate) fn draw_block(state: &RenderState, p: &DrawParams) -> DrawBlock {
    let mut b = [0u32; DRAW_BLOCK / 4];
    let mut put = |at: usize, v: &[f32]| {
        for (i, x) in v.iter().enumerate() {
            b[at / 4 + i] = x.to_bits();
        }
    };
    put(0, &p.world);
    for (s, st) in state.stages.iter().enumerate() {
        put(64 + s * 64, &st.matrix);
        put(544 + s * 16, &st.constant);
    }
    let m = &p.material;
    put(320, &m.diffuse);
    put(336, &m.ambient);
    put(352, &m.emissive);
    let l = &p.lighting;
    put(368, &[l.ambient[0], l.ambient[1], l.ambient[2], 0.0]);
    let count = l.count.min(MAX_LIGHTS);
    for (k, light) in l.lights.iter().take(count).enumerate() {
        let d = light.direction;
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let d = if len > 0.0 { [d[0] / len, d[1] / len, d[2] / len] } else { d };
        put(384 + k * 16, &[d[0], d[1], d[2], 0.0]);
        put(448 + k * 16, &[light.color[0], light.color[1], light.color[2], 0.0]);
    }
    put(512, &state.fog.color);
    let fog = match state.fog.mode {
        FogMode::Off => [0.0, 1.0, 0.0, 0.0],
        FogMode::Linear { start, end } => [start, if end == start { start + 1e-6 } else { end }, 0.0, 1.0],
        FogMode::Exp { density } => [0.0, 1.0, density, 2.0],
        FogMode::Exp2 { density } => [0.0, 1.0, density, 3.0],
    };
    put(528, &fog);
    put(624, &[(state.alpha_test.reference.clamp(0.0, 1.0) * 255.0).round(), 0.0, 0.0, 0.0]);
    let active = state.stages.iter().position(|s| s.color_op == Op::Disable).unwrap_or(STAGES) as u32;
    let flags = (l.enabled as u32) | (l.normalize as u32) << 1 | (m.diffuse_from_vertex as u32) << 2 | (m.ambient_from_vertex as u32) << 3 | (m.emissive_from_vertex as u32) << 4;
    b[608 / 4] = flags;
    b[612 / 4] = count as u32;
    b[616 / 4] = compare_code(state.alpha_test.compare);
    b[620 / 4] = active;
    for (s, st) in state.stages.iter().enumerate() {
        let at = (640 + s * 16) / 4;
        b[at] = op_code(st.color_op);
        b[at + 1] = arg_code(st.color_args[0]) | arg_code(st.color_args[1]) << 8;
        b[at + 2] = op_code(st.alpha_op);
        b[at + 3] = arg_code(st.alpha_args[0]) | arg_code(st.alpha_args[1]) << 8;
        b[704 / 4 + s] = match st.coords {
            TexCoordSource::Uv(n) => n.min(3) as u32,
            TexCoordSource::SphereMap => 4,
            TexCoordSource::Reflection => 5,
            TexCoordSource::Normal => 6,
            TexCoordSource::Position => 7,
        };
    }
    b
}

pub(crate) fn view_block(v: &View) -> [u32; VIEW_BLOCK / 4] {
    let mut b = [0u32; VIEW_BLOCK / 4];
    for (i, x) in v.view.iter().chain(v.proj.iter()).chain([v.screen[0], v.screen[1], v.pixel_offset, 0.0].iter()).enumerate() {
        b[i] = x.to_bits();
    }
    b
}

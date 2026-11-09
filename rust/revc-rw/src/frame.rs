//! Immediate-mode drawing for librw's WGPU platform (renderer port stage 1): the C++ device
//! forwards camera clears, texture uploads and Im2D primitives here; they are recorded per frame
//! and encoded into render passes when the frame is shown.
//!
//! Semantics follow librw's GL3 device so both renderers draw the same picture:
//! - Im2D vertices are screen pixels; `xform` maps them to clip space and the position is
//!   multiplied by w (camera z) for perspective-correct interpolation (gl3 im2d.vert).
//! - fog factor `clamp((w - fogEnd) / (fogStart - fogEnd), fogDisable, 1)`, colour mixed towards
//!   the fog colour; alpha test as a [low, high) window (gl3 header.frag / flushCache).
//! - blending is on when vertex alpha or texture alpha is (decided by the caller), with the RW
//!   blend factors for both colour and alpha (glBlendFunc).
//! - a z test that is off while z writes are on becomes "always" (gl3 setDepthTest).
//! - samplers follow the texture stage's filter and addressing; non-mip filters sample level 0.
//!
//! Rasters are RGBA8, top row first, so texture coordinates are used as they are.

use crate::Gpu;
use crate::prim::{expand_primitive, generate_mips, is_line_primitive};
use std::collections::HashMap;

/// librw `wgpu::Im2DVertex` (same layout as gl3's).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex2D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub u: f32,
    pub v: f32,
}
const _: () = assert!(std::mem::size_of::<Vertex2D>() == 28);

/// An Im2D vertex with a second texture coordinate set (re3's screen droplets, Im2DVertexUV2).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex2DUV2 {
    pub base: Vertex2D,
    pub u2: f32,
    pub v2: f32,
}
const _: () = assert!(std::mem::size_of::<Vertex2DUV2>() == 36);

/// librw `wgpu::Im3DVertex` (same layout as gl3's).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub u: f32,
    pub v: f32,
}
const _: () = assert!(std::mem::size_of::<Vertex3D>() == 24);

/// A model vertex as the device instances geometry (gl3 defaultInstanceCB attributes; missing
/// ones are filled with GL's defaults: normal 0, colour (0, 0, 0, 1), texcoords 0).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
    /// Second texture coordinate set (Neo world lightmaps); 0 without one.
    pub uv2: [f32; 2],
}
const _: () = assert!(std::mem::size_of::<MeshVertex>() == 44);

/// A skinned model vertex: up to four bones per vertex (gl3 skinInstanceCB: RW skin weights and
/// bone indices as stored in the geometry's skin).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkinVertex {
    pub base: MeshVertex,
    pub weights: [f32; 4],
    pub indices: [u8; 4],
}
const _: () = assert!(std::mem::size_of::<SkinVertex>() == 64);

/// Bone matrices per skinned draw (gl3 u_boneMatrices).
pub const MAX_BONES: usize = 64;
const BONE_FLOATS: usize = MAX_BONES * 16;

/// Per-draw model data for the lit pipeline (gl3 Object uniforms + material).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelState {
    /// Column-major world matrix.
    pub world: [f32; 16],
    /// Material colour (white unless the geometry modulates), 0..1.
    pub mat_color: [f32; 4],
    /// Ambient, specular, diffuse, extra.
    pub surf_props: [f32; 4],
    /// Sum of ambient lights, 0..1.
    pub ambient: [f32; 4],
    /// 8 lights x (params: type 0 end / 1 directional / 2 point / 3 spot, radius, minusCosAngle,
    /// hardSpot; position; direction; colour).
    pub lights: [[f32; 4]; 32],
    /// MatFX environment map (only read by environment-mapped draws, gl3 matfx_env): normal to
    /// env texture coordinates, the lit colour's lower clamp (0 = PC style lit env map, 1 = env
    /// map at full strength), the env colour, and (coefficient, 1 - frame-buffer alpha flag).
    pub tex_matrix: [f32; 16],
    pub color_clamp: [f32; 4],
    pub env_color: [f32; 4],
    pub fx_params: [f32; 4],
    /// Parameters of the re3 Neo shaders (MESH_NEO_*), as their render callbacks upload them.
    pub extra: [[f32; 4]; 12],
}

/// DrawState::shader for meshes: re3's Neo pipelines (EXTENDED_PIPELINES, the Xbox look).
/// Vehicle: extra = eye, (fresnel, light strength, shininess, specularity), 5 spec light
/// directions (w = power), 5 spec colours; tex1 = env map.
pub const MESH_NEO_VEHICLE: u32 = 10;
/// World lightmaps: extra[0] = light map factors (rgb blend, a = material alpha); tex1 = the
/// MatFX dual texture (white without one); second UV set.
pub const MESH_NEO_WORLD: u32 = 11;
/// Road gloss (additive pass with the gloss texture): extra = eye, (gloss mult, 0, 0, 0).
pub const MESH_NEO_GLOSS: u32 = 12;
/// Rim light (peds, also skinned): extra = view vector, ramp start, ramp end, (offset, scale,
/// scaling, 0).
pub const MESH_NEO_RIM: u32 = 13;

impl ModelState {
    /// A model state without lights or environment map.
    pub fn new(world: [f32; 16]) -> ModelState {
        ModelState {
            world,
            mat_color: [1.0; 4],
            surf_props: [1.0, 0.0, 1.0, 0.0],
            ambient: [0.0; 4],
            lights: [[0.0; 4]; 32],
            tex_matrix: [0.0; 16],
            color_clamp: [0.0; 4],
            env_color: [0.0; 4],
            fx_params: [0.0; 4],
            extra: [[0.0; 4]; 12],
        }
    }
}

/// The environment texture of a MatFX draw, with its texture's filter and addressing (the
/// device passes it as is: `RevcEnvMap` in wgpupipe.cpp).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvMap {
    pub texture: u32,
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
}

/// What a mesh draw adds to the plain lit pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeshExtras<'a> {
    /// Bone matrices of a skinned geometry.
    pub bones: Option<&'a [Mat4]>,
    /// MatFX environment map pass.
    pub env: Option<EnvMap>,
}
const MODEL_FLOATS: usize = std::mem::size_of::<ModelState>() / 4;

/// Column-major 4x4 matrix (RW RawMatrix / GL layout: right, up, at, pos columns).
pub type Mat4 = [f32; 16];

/// a * b, column-major.
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

/// Device state for one draw, filled by the C++ device (`RevcDrawState` in wgpudevice.cpp).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawState {
    /// Texture id from `upload_texture`; 0 = untextured (white).
    pub texture: u32,
    /// rw::Texture::FilterMode (1 nearest .. 6 linear-mip-linear).
    pub filter: u32,
    /// rw::Texture::Addressing (1 wrap, 2 mirror, 3 clamp, 4 border).
    pub address_u: u32,
    pub address_v: u32,
    /// Blending on (vertex alpha or texture alpha), with rw::BlendFunction factors.
    pub blend: u32,
    pub src_blend: u32,
    pub dst_blend: u32,
    pub z_test: u32,
    pub z_write: u32,
    /// rw::CullMode (1 none, 2 back, 3 front).
    pub cull: u32,
    /// rw::AlphaTestFunc as the shader sees it (0 always, 1 >= ref, 2 < ref) and ref in 0..1.
    pub alpha_func: u32,
    pub alpha_ref: f32,
    pub fog: u32,
    pub fog_start: f32,
    pub fog_end: f32,
    /// RGBA bytes, red in the low byte (RWRGBAINT).
    pub fog_color: u32,
    /// Screen pixels to clip space: x * xform[0] + xform[2], y * xform[1] + xform[3].
    pub xform: [f32; 4],
    /// Im2D override shader (re3 CPostFX): 0 none, 1 PS2/Xbox colour filter (params[0..4] blur
    /// colour), 2 mobile contrast (params[0..3] multiply, params[4..7] add).
    pub shader: u32,
    pub shader_params: [f32; 8],
    /// Anisotropic filtering (1 = off), used with trilinear filtering.
    pub max_anisotropy: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Im2D,
    Im3D,
    Mesh,
    Skin,
    MeshEnv,
    /// Im2D with two texture coordinate sets and a second texture (screen droplets).
    Im2DUV2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeshDraw {
    geometry: u32,
    model: u32,
    /// Bone block for skinned draws.
    bones: u32,
    /// Environment texture and sampler for MatFX draws.
    env: Option<(u32, SamplerKey)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PipeKey {
    kind: Kind,
    /// Im2D override shader (DrawState::shader).
    shader: u32,
    lines: bool,
    blend: Option<(u32, u32)>,
    z_test: bool,
    z_write: bool,
    cull: u32,
    /// MSAA sample count of the target (the scene's; texture targets have 1).
    samples: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SamplerKey {
    filter: u32,
    address_u: u32,
    address_v: u32,
    anisotropy: u16,
}

/// xform, alpha window, fog, fog colour, the Im3D model-view-projection matrix, then the
/// override shader's parameters.
type Uniforms = [f32; 40];

#[derive(Clone, Copy, Debug)]
enum Op {
    Clear { color: Option<[f64; 4]>, depth: bool },
    /// Immediate geometry (first/count vertices) or, with `mesh` = (geometry, model uniform), an
    /// index range of a stored geometry.
    /// For skinned meshes the third value is the bone-matrix block.
    Draw { pipe: PipeKey, texture: u32, sampler: SamplerKey, uniform: u32, first: u32, count: u32, mesh: Option<MeshDraw> },
    /// The frame so far copied into a texture at (x, y) (librw rasterRenderFast of the camera).
    CopyFrame { texture: u32, x: u32, y: u32 },
    /// Following clears and draws go to this texture (a camera rendering into a texture, like
    /// the Neo env map camera), or to the scene with 0.
    Target { texture: u32 },
}

/// The frame is drawn into an offscreen texture of this format, then shown (blitted) on the
/// output; frame copies (post effects) and screenshots read it.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var scene: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(scene, vec2<i32>(pos.xy), 0);
}
"#;

struct Geom {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    levels: u32,
}

/// Pixel data of a texture upload: RGBA8, or DXT1/3/5 blocks as they are in the TXD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexData {
    Rgba8,
    Dxt1,
    Dxt3,
    Dxt5,
}

impl TexData {
    /// librw WGPU raster `dxt` field: 0, 1, 3 or 5.
    pub fn from_dxt(dxt: u32) -> TexData {
        match dxt {
            1 => TexData::Dxt1,
            3 => TexData::Dxt3,
            5 => TexData::Dxt5,
            _ => TexData::Rgba8,
        }
    }

    fn kind(self) -> u8 {
        match self {
            TexData::Rgba8 => 0,
            TexData::Dxt1 => 1,
            TexData::Dxt3 => 3,
            TexData::Dxt5 => 5,
        }
    }

    fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            TexData::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            TexData::Dxt1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            TexData::Dxt3 => wgpu::TextureFormat::Bc2RgbaUnorm,
            TexData::Dxt5 => wgpu::TextureFormat::Bc3RgbaUnorm,
        }
    }
}

const SHADER: &str = r#"
struct U { xform: vec4<f32>, alpha: vec4<f32>, fog: vec4<f32>, fog_color: vec4<f32>, mvp: mat4x4<f32>, p0: vec4<f32>, p1: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fog: f32,
};

@vertex
fn vs_2d(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    let w = pos.w;
    let xy = pos.xy * u.xform.xy + u.xform.zw;
    o.pos = vec4<f32>(vec3<f32>(xy, pos.z) * w, w);
    o.fog = clamp((w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    o.color = color;
    o.uv = uv;
    return o;
}

// gl3 im3d.vert: projection * view * world, fog from clip w (camera-space depth).
@vertex
fn vs_3d(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = u.mvp * vec4<f32>(pos, 1.0);
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    o.color = color;
    o.uv = uv;
    return o;
}

// gl3 default.vert: prelit colour + ambient * surfAmbient + dynamic lights * surfDiffuse,
// clamped, times the material colour. Normals are transformed by the world matrix unnormalised.
struct M {
    world: mat4x4<f32>,
    mat_color: vec4<f32>,
    surf: vec4<f32>,
    ambient: vec4<f32>,
    lights: array<vec4<f32>, 32>,
    tex_matrix: mat4x4<f32>,
    color_clamp: vec4<f32>,
    env_color: vec4<f32>,
    fx: vec4<f32>,
    extra: array<vec4<f32>, 12>,
};
@group(2) @binding(0) var<uniform> m: M;

// `mask`: light types the shader handles (1 directional, 2 point, 4 spot), as gl3's
// DIRECTIONALS / POINTLIGHTS / SPOTLIGHTS shader defines.
fn dynamic_light(v: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    return dynamic_light_mask(v, n, 7u);
}

fn dynamic_light_mask(v: vec3<f32>, n: vec3<f32>, mask: u32) -> vec3<f32> {
    var color = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let params = m.lights[i * 4];
        let pos = m.lights[i * 4 + 1].xyz;
        let dir = m.lights[i * 4 + 2].xyz;
        let col = m.lights[i * 4 + 3].rgb;
        if (params.x == 0.0) {
            break;
        }
        if (params.x == 1.0) {
            if ((mask & 1u) != 0u) {
                color += max(0.0, dot(n, -dir)) * col;
            }
        } else if (params.x == 2.0 && (mask & 2u) != 0u) {
            let d = v - pos;
            let dist = length(d);
            let atten = max(0.0, 1.0 - dist / params.y);
            color += max(0.0, dot(n, -normalize(d))) * col * atten;
        } else if (params.x == 3.0 && (mask & 4u) != 0u) {
            let d = v - pos;
            let dist = length(d);
            let atten = max(0.0, 1.0 - dist / params.y);
            let dn = d / dist;
            var l = max(0.0, dot(n, -dn));
            let pcos = dot(dn, dir);
            let ccos = -params.z;
            let falloff = (pcos - ccos) / (1.0 - ccos);
            if (falloff < 0.0) {
                l = 0.0;
            }
            l *= max(falloff, params.w);
            // gl3 returns here (dropping the other lights); D3D adds spots up like the rest.
            color += l * col * atten;
        }
    }
    return color;
}

@vertex
fn vs_mesh(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> Out {
    return lit(pos, normal, color, uv);
}

// gl3 skin.vert: position and normal blended over up to four bones, then lit like vs_mesh
// (fog from clip w like the default pipeline; gl3's skin shader uses clip z there).
struct Bones { m: array<mat4x4<f32>, 64> };
@group(3) @binding(0) var<uniform> bones: Bones;

@vertex
fn vs_skin(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>, @location(5) weights: vec4<f32>, @location(6) indices: vec4<u32>) -> Out {
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let b = bones.m[min(indices[i], 63u)];
        p += (b * vec4<f32>(pos, 1.0)).xyz * weights[i];
        n += (b * vec4<f32>(normal, 0.0)).xyz * weights[i];
    }
    return lit(p, n, color, uv);
}

fn lit(pos: vec3<f32>, normal: vec3<f32>, color: vec4<f32>, uv: vec2<f32>) -> Out {
    var o: Out;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    let n = (m.world * vec4<f32>(normal, 0.0)).xyz;
    var c = color;
    c = vec4<f32>(c.rgb + m.ambient.rgb * m.surf.x, c.a);
    c = vec4<f32>(c.rgb + dynamic_light(v.xyz, n) * m.surf.z, c.a);
    o.color = clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * m.mat_color;
    o.uv = uv;
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

// gl3 matfx_env.vert/.frag: base pass and environment map in one, colour premultiplied by
// the base alpha (drawn with source blend ONE).
struct OutEnv {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fog: f32,
    @location(3) env_color: vec4<f32>,
    @location(4) uv1: vec2<f32>,
};
@group(3) @binding(0) var env_tex: texture_2d<f32>;
@group(3) @binding(1) var env_samp: sampler;

@vertex
fn vs_env(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> OutEnv {
    var o: OutEnv;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    let n = (m.world * vec4<f32>(normal, 0.0)).xyz;
    o.uv = uv;
    o.uv1 = (m.tex_matrix * vec4<f32>(n, 1.0)).xy;
    var c = color;
    c = vec4<f32>(c.rgb + m.ambient.rgb * m.surf.x, c.a);
    c = vec4<f32>(c.rgb + dynamic_light(v.xyz, n) * m.surf.z, c.a);
    c = clamp(c, vec4<f32>(0.0), vec4<f32>(1.0));
    o.env_color = max(c, m.color_clamp) * m.env_color;
    o.color = c * m.mat_color;
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

// re3 Neo shaders (extras/shaders/neo*.vert/.frag, default_UV2.vert, neoWorldVC.frag).
fn neo_lit(v: vec4<f32>, n: vec3<f32>, color: vec4<f32>, mask: u32, light_strength: f32) -> vec4<f32> {
    var c = color;
    c = vec4<f32>(c.rgb + m.ambient.rgb * m.surf.x, c.a);
    c = vec4<f32>(c.rgb + dynamic_light_mask(v.xyz, n, mask) * m.surf.z * light_strength, c.a);
    return clamp(c, vec4<f32>(0.0), vec4<f32>(1.0));
}

@vertex
fn vs_neo_vehicle(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> OutEnv {
    var o: OutEnv;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    let n = (m.world * vec4<f32>(normal, 0.0)).xyz;
    let refl = m.extra[1];
    let view = normalize(m.extra[0].xyz - v.xyz);
    o.uv = uv;
    o.color = neo_lit(v, n, color, 1u, refl.y) * m.mat_color;
    let uv2 = n * dot(view, n) * 2.0 - view;
    o.uv1 = uv2.xy * 0.5 + 0.5;
    let b = 1.0 - clamp(dot(view, n), 0.0, 1.0);
    var rc = vec4<f32>(0.0, 0.0, 0.0, mix(b * b * b * b * b, 1.0, refl.x) * refl.z);
    for (var k = 0; k < 5; k++) {
        let d = m.extra[2 + k];
        let col = m.extra[7 + k].rgb;
        rc = vec4<f32>(rc.rgb + pow(clamp(dot(n, normalize(view - d.xyz)), 0.0, 1.0), d.w) * col * refl.w * refl.y, rc.a);
    }
    o.env_color = rc;
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

@fragment
fn fs_neo_vehicle(i: OutEnv) -> @location(0) vec4<f32> {
    var p1 = i.color * textureSample(tex, samp, i.uv);
    let env = textureSample(env_tex, env_samp, i.uv1).rgb;
    p1 = vec4<f32>(mix(p1.rgb, env, i.env_color.a), p1.a);
    p1 = vec4<f32>(mix(u.fog_color.rgb, p1.rgb, i.fog), p1.a);
    let p2 = i.env_color.rgb * i.fog;
    let c = vec4<f32>(p1.rgb * p1.a + p2, p1.a);
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}

// default_UV2.vert is built without light defines: ambient only.
@vertex
fn vs_neo_world(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>, @location(4) uv2: vec2<f32>) -> OutEnv {
    var o: OutEnv;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    let n = (m.world * vec4<f32>(normal, 0.0)).xyz;
    o.uv = uv;
    o.uv1 = uv2;
    o.color = neo_lit(v, n, color, 0u, 1.0) * m.mat_color;
    o.env_color = vec4<f32>(0.0);
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

@fragment
fn fs_neo_world(i: OutEnv) -> @location(0) vec4<f32> {
    let lm = m.extra[0];
    let t0 = textureSample(tex, samp, i.uv);
    let t1 = textureSample(env_tex, env_samp, i.uv1);
    var c = t0 * i.color * (1.0 + lm * (t1 - 1.0));
    c = vec4<f32>(mix(u.fog_color.rgb, c.rgb, i.fog), i.color.a * t0.a * lm.a);
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}

// Gloss: the light direction is the first light's (u_lightDirection[0]); the normal is up.
@vertex
fn vs_neo_gloss(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> OutEnv {
    var o: OutEnv;
    let v = m.world * vec4<f32>(pos, 1.0);
    o.pos = u.mvp * v;
    o.uv = uv;
    let view = normalize(m.extra[0].xyz - v.xyz);
    let light = normalize(view - m.lights[2].xyz);
    o.env_color = vec4<f32>(0.5 * (1.0 + light), 0.0);
    o.color = vec4<f32>(1.0);
    o.uv1 = vec2<f32>(0.0);
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

@fragment
fn fs_neo_gloss(i: OutEnv) -> @location(0) vec4<f32> {
    var c = textureSample(tex, samp, i.uv);
    let s = dot(vec3<f32>(0.0, 0.0, 1.0), 2.0 * i.env_color.xyz - 1.0);
    let s2 = s * s;
    let s4 = s2 * s2;
    c = s4 * s4 * c * i.fog * m.extra[1].x;
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}

fn rim(v: vec4<f32>, n: vec3<f32>, color: vec4<f32>, uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = u.mvp * v;
    o.uv = uv;
    var c = color;
    c = vec4<f32>(c.rgb + m.ambient.rgb * m.surf.x, c.a);
    c = vec4<f32>(c.rgb + dynamic_light_mask(v.xyz, n, 1u) * m.surf.z, c.a);
    let rd = m.extra[3];
    let f = rd.x - rd.y * dot(n, m.extra[0].xyz);
    let rimlight = clamp(mix(m.extra[2], m.extra[1], f) * rd.z, vec4<f32>(0.0), vec4<f32>(1.0));
    c = vec4<f32>(c.rgb + rimlight.rgb, c.a);
    o.color = clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * m.mat_color;
    o.fog = clamp((o.pos.w - u.fog.y) * u.fog.z, u.fog.w, 1.0);
    return o;
}

@vertex
fn vs_neo_rim(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>) -> Out {
    return rim(m.world * vec4<f32>(pos, 1.0), (m.world * vec4<f32>(normal, 0.0)).xyz, color, uv);
}

@vertex
fn vs_neo_rim_skin(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32>, @location(5) weights: vec4<f32>, @location(6) indices: vec4<u32>) -> Out {
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let b = bones.m[min(indices[i], 63u)];
        p += (b * vec4<f32>(pos, 1.0)).xyz * weights[i];
        n += (b * vec4<f32>(normal, 0.0)).xyz * weights[i];
    }
    return rim(m.world * vec4<f32>(p, 1.0), (m.world * vec4<f32>(n, 0.0)).xyz, color, uv);
}

@fragment
fn fs_env(i: OutEnv) -> @location(0) vec4<f32> {
    var p1 = i.color * textureSample(tex, samp, i.uv);
    var p2 = i.env_color * m.fx.x * textureSample(env_tex, env_samp, i.uv1);
    p1 = vec4<f32>(mix(u.fog_color.rgb, p1.rgb, i.fog), p1.a);
    p2 = vec4<f32>(p2.rgb * i.fog, p2.a);
    let fba = max(p1.a, m.fx.y);
    let c = vec4<f32>(p1.rgb * p1.a + p2.rgb * fba, p1.a);
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}

// re3 colourfilterVC.frag (PS2/Xbox colour filter) and contrast.frag (mobile), on the copy of
// the frame; no fog or alpha test, opaque.
@fragment
fn fs_colourfilter(i: Out) -> @location(0) vec4<f32> {
    let blur = u.p0;
    let a = blur.a;
    let doublec = clamp(blur * 2.0, vec4<f32>(0.0), vec4<f32>(1.0));
    let dst = textureSample(tex, samp, i.uv);
    var prev = dst;
    for (var k = 0; k < 5; k++) {
        var tmp = dst * (1.0 - a) + prev * doublec * a;
        tmp += prev * blur;
        tmp += prev * blur;
        prev = clamp(tmp, vec4<f32>(0.0), vec4<f32>(1.0));
    }
    return vec4<f32>(prev.rgb, 1.0);
}

@fragment
fn fs_contrast(i: Out) -> @location(0) vec4<f32> {
    let dst = textureSample(tex, samp, i.uv);
    return vec4<f32>(dst.rgb * u.p0.rgb + u.p1.rgb, 1.0);
}

@fragment
fn fs_simple(i: Out) -> @location(0) vec4<f32> {
    var c = i.color * textureSample(tex, samp, i.uv);
    c = vec4<f32>(mix(u.fog_color.rgb, c.rgb, i.fog), c.a);
    if (c.a < u.alpha.x || c.a >= u.alpha.y) {
        discard;
    }
    return c;
}
"#;

pub struct Renderer {
    /// Pipelines draw into the scene texture (SCENE_FORMAT).
    format: wgpu::TextureFormat,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    scene: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
    /// MSAA sample count of the scene (1 = off) and its multisampled colour target, resolved
    /// into the scene texture at the end of every pass.
    samples: u32,
    msaa: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
    shader: wgpu::ShaderModule,
    texture_layout: wgpu::BindGroupLayout,
    uniform_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    mesh_layout: wgpu::PipelineLayout,
    model_layout: wgpu::BindGroupLayout,
    model_stride: u64,
    model_buf: wgpu::Buffer,
    model_capacity: usize,
    model_bind: wgpu::BindGroup,
    models: Vec<ModelState>,
    skin_layout: wgpu::PipelineLayout,
    env_layout: wgpu::PipelineLayout,
    bones_layout: wgpu::BindGroupLayout,
    bones_stride: u64,
    bones_buf: wgpu::Buffer,
    bones_capacity: usize,
    bones_bind: wgpu::BindGroup,
    /// BONE_FLOATS per skinned draw.
    bones: Vec<f32>,
    geometries: HashMap<u32, Geom>,
    next_geometry: u32,
    /// Textures and geometries destroyed while a recorded frame may still use them.
    dead_textures: Vec<u32>,
    dead_geometries: Vec<u32>,
    retired: Vec<Geom>,
    pipelines: HashMap<PipeKey, wgpu::RenderPipeline>,
    samplers: HashMap<SamplerKey, wgpu::Sampler>,
    textures: HashMap<u32, Tex>,
    next_texture: u32,
    white: Tex,
    bind_groups: HashMap<(u32, SamplerKey), wgpu::BindGroup>,
    uniform_stride: u64,
    uniform_buf: wgpu::Buffer,
    uniform_capacity: usize,
    uniform_bind: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    vertex3_buf: wgpu::Buffer,
    /// Depth buffers by size (the scene's and those of texture targets).
    depths: HashMap<(u32, u32, u32), (wgpu::Texture, wgpu::TextureView)>,
    /// Render target being recorded (0 = scene).
    recording_target: u32,
    ops: Vec<Op>,
    verts: Vec<Vertex2D>,
    verts3: Vec<Vertex3D>,
    verts_uv2: Vec<Vertex2DUV2>,
    vertex_uv2_buf: wgpu::Buffer,
    shader_uv2: wgpu::ShaderModule,
    uv2_layout: wgpu::PipelineLayout,
    uniforms: Vec<Uniforms>,
    scratch_in: Vec<u32>,
    scratch_out: Vec<u32>,
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ATTRS_2D: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Unorm8x4, 2 => Float32x2];
const ATTRS_3D: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2];
const ATTRS_2D_UV2: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Unorm8x4, 2 => Float32x2, 3 => Float32x2];

// re3 im2d_UV2.vert + screenDroplet.frag: the droplet mask (tex) times the frame copy (tex1) at
// the second coordinates, times the vertex colour; no perspective (w = 1), no fog, no alpha test.
const UV2_SHADER: &str = r#"
struct U { xform: vec4<f32>, alpha: vec4<f32>, fog: vec4<f32>, fog_color: vec4<f32>, mvp: mat4x4<f32>, p0: vec4<f32>, p1: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var tex1: texture_2d<f32>;
@group(2) @binding(1) var samp1: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) uv1: vec2<f32>,
};

@vertex
fn vs(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>, @location(3) uv2: vec2<f32>) -> Out {
    var o: Out;
    o.pos = vec4<f32>(pos.xy * u.xform.xy + u.xform.zw, pos.z, 1.0);
    o.color = color;
    o.uv = uv;
    o.uv1 = uv2;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    return i.color * textureSample(tex, samp, i.uv) * textureSample(tex1, samp1, i.uv1);
}
"#;

const ATTRS_MESH: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32x2, 4 => Float32x2];
const ATTRS_SKIN: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32x2, 4 => Float32x2, 5 => Float32x4, 6 => Uint8x4];

fn blend_factor(f: u32) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as B;
    match f {
        1 => B::Zero,
        2 => B::One,
        3 => B::Src,
        4 => B::OneMinusSrc,
        5 => B::SrcAlpha,
        6 => B::OneMinusSrcAlpha,
        7 => B::DstAlpha,
        8 => B::OneMinusDstAlpha,
        9 => B::Dst,
        10 => B::OneMinusDst,
        11 => B::SrcAlphaSaturated,
        _ => B::One,
    }
}

fn address_mode(a: u32) -> wgpu::AddressMode {
    match a {
        2 => wgpu::AddressMode::MirrorRepeat,
        // Border colour addressing isn't core wgpu; the game doesn't rely on the border colour.
        3 | 4 => wgpu::AddressMode::ClampToEdge,
        _ => wgpu::AddressMode::Repeat,
    }
}

fn new_texture(gpu: &Gpu, format: wgpu::TextureFormat, width: u32, height: u32, levels: u32) -> Tex {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raster"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // Uncompressed textures can also be render targets (camera textures).
        usage: if format.is_compressed() {
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST
        } else {
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT
        },
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Tex { texture, view, format, width, height, levels }
}

fn write_levels(gpu: &Gpu, t: &Tex, levels: &[&[u8]]) {
    for (level, data) in levels.iter().enumerate() {
        let (w, h) = ((t.width >> level).max(1), (t.height >> level).max(1));
        // Block formats are copied in whole 4x4 blocks (the level's physical size).
        let (bw, bh, bytes) = match t.format.block_copy_size(None) {
            Some(b) if t.format.is_compressed() => (w.div_ceil(4), h.div_ceil(4), b),
            _ => (w, h, 4),
        };
        let size = (bw * bh * bytes) as usize;
        if data.len() < size {
            continue;
        }
        let (dim_w, dim_h) = if t.format.is_compressed() { (bw * 4, bh * 4) } else { (w, h) };
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &data[..size],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bw * bytes), rows_per_image: Some(bh) },
            wgpu::Extent3d { width: dim_w, height: dim_h, depth_or_array_layers: 1 },
        );
    }
}

impl Renderer {
    /// For output targets of `output` format (the window's swapchain format).
    pub fn new(gpu: &Gpu, output: wgpu::TextureFormat) -> Renderer {
        Self::with_samples(gpu, output, 1)
    }

    /// The largest MSAA sample count up to `wanted` that the scene's colour and depth formats
    /// support on this device (1, 2, 4, 8, ...).
    pub fn supported_samples(gpu: &Gpu, wanted: u32) -> u32 {
        let colour = gpu.adapter.get_texture_format_features(SCENE_FORMAT).flags;
        let depth = gpu.adapter.get_texture_format_features(DEPTH_FORMAT).flags;
        let mut n = wanted.clamp(1, 16).next_power_of_two();
        if n > wanted.max(1) {
            n /= 2;
        }
        while n > 1 && !(colour.sample_count_supported(n) && depth.sample_count_supported(n)) {
            n /= 2;
        }
        n
    }

    /// As `new`, drawing the scene with `samples`-times MSAA (clamped to what the device does).
    pub fn with_samples(gpu: &Gpu, output: wgpu::TextureFormat, samples: u32) -> Renderer {
        let samples = Self::supported_samples(gpu, samples);
        let d = &gpu.device;
        let format = SCENE_FORMAT;
        let blit_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            }],
        });
        let blit_shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("blit"), source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()) });
        let blit_pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("blit"), bind_group_layouts: &[Some(&blit_layout)], immediate_size: 0 });
        let blit_pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&blit_pl),
            vertex: wgpu::VertexState { module: &blit_shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &blit_shader, entry_point: Some("fs"), targets: &[Some(output.into())], compilation_options: Default::default() }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("im2d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let uniform_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let texture_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("im2d"), bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)], immediate_size: 0 });
        let model_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let mesh_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let env_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh env"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&model_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let align = d.limits().min_uniform_buffer_offset_alignment as u64;
        let model_stride = (std::mem::size_of::<ModelState>() as u64).next_multiple_of(align);
        let (model_buf, model_bind) = Self::make_dyn_uniforms(gpu, &model_layout, model_stride, 64, std::mem::size_of::<ModelState>() as u64);
        let bones_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bone matrices"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let skin_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&model_layout), Some(&bones_layout)],
            immediate_size: 0,
        });
        let bones_stride = ((BONE_FLOATS * 4) as u64).next_multiple_of(align);
        let (bones_buf, bones_bind) = Self::make_dyn_uniforms(gpu, &bones_layout, bones_stride, 16, (BONE_FLOATS * 4) as u64);
        let uniform_stride = (d.limits().min_uniform_buffer_offset_alignment as u64).max(std::mem::size_of::<Uniforms>() as u64);
        let (uniform_buf, uniform_bind) = Self::make_uniforms(gpu, &uniform_layout, uniform_stride, 256);
        let white = new_texture(gpu, wgpu::TextureFormat::Rgba8Unorm, 1, 1, 1);
        write_levels(gpu, &white, &[&[255, 255, 255, 255]]);
        let vertex_buf = Self::make_vertices(gpu, 4096 * std::mem::size_of::<Vertex2D>());
        let vertex3_buf = Self::make_vertices(gpu, 4096 * std::mem::size_of::<Vertex3D>());
        let vertex_uv2_buf = Self::make_vertices(gpu, 1024 * std::mem::size_of::<Vertex2DUV2>());
        let shader_uv2 = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("im2d uv2"), source: wgpu::ShaderSource::Wgsl(UV2_SHADER.into()) });
        let uv2_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("im2d uv2"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        Renderer {
            format,
            blit_pipeline,
            blit_layout,
            scene: None,
            samples,
            msaa: None,
            shader,
            texture_layout,
            uniform_layout,
            pipeline_layout,
            mesh_layout,
            model_layout,
            model_stride,
            model_buf,
            model_capacity: 64,
            model_bind,
            models: Vec::new(),
            skin_layout,
            env_layout,
            bones_layout,
            bones_stride,
            bones_buf,
            bones_capacity: 16,
            bones_bind,
            bones: Vec::new(),
            geometries: HashMap::new(),
            next_geometry: 1,
            dead_textures: Vec::new(),
            dead_geometries: Vec::new(),
            retired: Vec::new(),
            pipelines: HashMap::new(),
            samplers: HashMap::new(),
            textures: HashMap::new(),
            next_texture: 1,
            white,
            bind_groups: HashMap::new(),
            uniform_stride,
            uniform_buf,
            uniform_capacity: 256,
            uniform_bind,
            vertex_buf,
            vertex3_buf,
            depths: HashMap::new(),
            recording_target: 0,
            ops: Vec::new(),
            verts: Vec::new(),
            verts3: Vec::new(),
            verts_uv2: Vec::new(),
            vertex_uv2_buf,
            shader_uv2,
            uv2_layout,
            uniforms: Vec::new(),
            scratch_in: Vec::new(),
            scratch_out: Vec::new(),
        }
    }

    fn make_uniforms(gpu: &Gpu, layout: &wgpu::BindGroupLayout, stride: u64, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw uniforms"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw uniforms"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64) }),
            }],
        });
        (buf, bind)
    }

    /// A uniform buffer of `capacity` blocks `stride` apart, bound with a dynamic offset and
    /// `binding` bytes visible per block.
    fn make_dyn_uniforms(gpu: &Gpu, layout: &wgpu::BindGroupLayout, stride: u64, capacity: usize, binding: u64) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("per-draw uniforms"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("per-draw uniforms"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: wgpu::BufferSize::new(binding) }) }],
        });
        (buf, bind)
    }

    /// Creates or replaces a geometry (vertices + triangle-list indices); returns its id.
    pub fn upload_geometry(&mut self, gpu: &Gpu, id: u32, vertices: &[MeshVertex], indices: &[u16]) -> u32 {
        // SAFETY: MeshVertex is repr(C) plain data without padding (36 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(gpu, id, vbytes, indices)
    }

    /// A skinned geometry (drawn with bone matrices by `draw_mesh`).
    pub fn upload_skinned_geometry(&mut self, gpu: &Gpu, id: u32, vertices: &[SkinVertex], indices: &[u16]) -> u32 {
        // SAFETY: SkinVertex is repr(C) plain data without padding (56 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(gpu, id, vbytes, indices)
    }

    fn upload_raw(&mut self, gpu: &Gpu, id: u32, vbytes: &[u8], indices: &[u16]) -> u32 {
        let id = if id != 0 && self.geometries.contains_key(&id) {
            id
        } else {
            let id = self.next_geometry;
            self.next_geometry += 1;
            id
        };
        let mut ibytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        // Buffer copies come in multiples of 4 bytes.
        ibytes.resize(ibytes.len().next_multiple_of(4).max(4), 0);
        let mut vb = vbytes.to_vec();
        vb.resize(vb.len().next_multiple_of(4).max(4), 0);
        use wgpu::util::DeviceExt;
        let geom = Geom {
            vertices: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("geometry vertices"), contents: &vb, usage: wgpu::BufferUsages::VERTEX }),
            indices: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("geometry indices"), contents: &ibytes, usage: wgpu::BufferUsages::INDEX }),
        };
        // A replaced geometry may still be drawn by the frame being recorded: keep the old
        // buffers alive until the frame is done.
        if let Some(old) = self.geometries.insert(id, geom) {
            self.retired.push(old);
        }
        id
    }

    pub fn destroy_geometry(&mut self, id: u32) {
        self.dead_geometries.push(id);
    }

    fn make_vertices(gpu: &Gpu, bytes: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("immediate vertices"),
            size: bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Creates or replaces a texture from mip levels (top row first, each half the previous
    /// size), RGBA8 or DXT blocks. DXT stays compressed when the device samples BC formats and
    /// the size is whole blocks, else it is decoded here. `autogen` builds the mip chain from
    /// level 0 (RGBA8 rasters with AUTOMIPMAP). Returns the texture's id (`id` itself when it
    /// already existed).
    // One argument per field of the device's upload call (revc_rw_texture_upload).
    #[allow(clippy::too_many_arguments)]
    pub fn upload_texture(&mut self, gpu: &Gpu, id: u32, width: u32, height: u32, data: TexData, levels: &[&[u8]], autogen: bool) -> u32 {
        let (width, height) = (width.max(1), height.max(1));
        let bc = gpu.device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) && width % 4 == 0 && height % 4 == 0;
        let decoded: Vec<Vec<u8>>;
        let decoded_refs: Vec<&[u8]>;
        let (data, levels): (TexData, &[&[u8]]) = if data != TexData::Rgba8 && !bc {
            decoded = levels.iter().enumerate().map_while(|(i, l)| revc_formats::dds::decode_dxt(data.kind(), (width >> i).max(1), (height >> i).max(1), l)).collect();
            decoded_refs = decoded.iter().map(|l| l.as_slice()).collect();
            (TexData::Rgba8, &decoded_refs)
        } else {
            (data, levels)
        };
        let autogen = autogen && data == TexData::Rgba8;
        let generated: Vec<Vec<u8>>;
        let generated_refs: Vec<&[u8]>;
        let levels: &[&[u8]] = if autogen && !levels.is_empty() && levels[0].len() >= (width * height * 4) as usize {
            generated = generate_mips(width, height, &levels[0][..(width * height * 4) as usize]);
            generated_refs = generated.iter().map(|l| l.as_slice()).collect();
            &generated_refs
        } else {
            levels
        };
        let count = levels.len().max(1) as u32;
        let id = if id != 0 && self.textures.contains_key(&id) {
            id
        } else {
            let id = self.next_texture;
            self.next_texture += 1;
            id
        };
        let format = data.wgpu();
        let reuse = self.textures.get(&id).is_some_and(|t| t.format == format && t.width == width && t.height == height && t.levels == count);
        if !reuse {
            self.textures.insert(id, new_texture(gpu, format, width, height, count));
            self.bind_groups.retain(|k, _| k.0 != id);
        }
        write_levels(gpu, &self.textures[&id], levels);
        id
    }

    pub fn destroy_texture(&mut self, id: u32) {
        self.dead_textures.push(id);
    }

    /// Camera clear: colour (when given) and/or depth.
    pub fn clear(&mut self, color: Option<[f64; 4]>, depth: bool) {
        self.ops.push(Op::Clear { color, depth });
    }

    fn expand(&mut self, prim: u32, num_verts: usize, indices: Option<&[u16]>) {
        self.scratch_in.clear();
        match indices {
            Some(ix) => self.scratch_in.extend(ix.iter().map(|&i| i as u32)),
            None => self.scratch_in.extend(0..num_verts as u32),
        }
        self.scratch_out.clear();
        expand_primitive(prim, &self.scratch_in, &mut self.scratch_out);
    }

    /// An Im2D primitive (rw::PrimitiveType `prim`), optionally indexed.
    pub fn draw_2d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex2D], indices: Option<&[u16]>) {
        self.expand(prim, verts.len(), indices);
        let first = self.verts.len() as u32;
        self.verts.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts.len() as u32 - first;
        self.record(state, prim, Kind::Im2D, first, count, &[0.0; 16], None);
    }

    /// An Im3D primitive: `mvp` = projection * view * world (column-major).
    pub fn draw_3d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex3D], indices: Option<&[u16]>, mvp: &Mat4) {
        self.expand(prim, verts.len(), indices);
        let first = self.verts3.len() as u32;
        self.verts3.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts3.len() as u32 - first;
        self.record(state, prim, Kind::Im3D, first, count, mvp, None);
    }

    /// An Im2D primitive with two coordinate sets, textured with the bound raster and `tex1`
    /// (re3's screen droplets: drop mask times the frame copy).
    pub fn draw_2d_uv2(&mut self, state: &DrawState, prim: u32, verts: &[Vertex2DUV2], indices: Option<&[u16]>, tex1: &EnvMap) {
        self.expand(prim, verts.len(), indices);
        let first = self.verts_uv2.len() as u32;
        self.verts_uv2.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.verts_uv2.len() as u32 - first;
        let t = if self.textures.contains_key(&tex1.texture) { tex1.texture } else { 0 };
        let env = Some((t, SamplerKey { filter: tex1.filter, address_u: tex1.address_u, address_v: tex1.address_v, anisotropy: 1 }));
        self.record(state, prim, Kind::Im2DUV2, first, count, &[0.0; 16], Some(MeshDraw { geometry: 0, model: 0, bones: 0, env }));
    }

    /// Triangles `first..first + count` of the index list of a stored geometry, lit with
    /// `model`; `view_proj` = projection * view. A skinned geometry gets its bones (at most
    /// MAX_BONES; missing ones are identity); a MatFX material its environment map.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_mesh(&mut self, state: &DrawState, geometry: u32, first: u32, count: u32, view_proj: &Mat4, model: &ModelState, extras: &MeshExtras) {
        let bones = extras.bones;
        if !self.geometries.contains_key(&geometry) {
            return;
        }
        if self.models.last() != Some(model) {
            self.models.push(*model);
        }
        let mi = self.models.len() as u32 - 1;
        let (kind, bi) = match bones {
            Some(b) => {
                let mut block = [0.0f32; BONE_FLOATS];
                for i in 0..MAX_BONES {
                    let m = b.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                    block[i * 16..][..16].copy_from_slice(&m);
                }
                let n = self.bones.len();
                if n < BONE_FLOATS || self.bones[n - BONE_FLOATS..] != block {
                    self.bones.extend_from_slice(&block);
                }
                (Kind::Skin, (self.bones.len() / BONE_FLOATS - 1) as u32)
            }
            None => (Kind::Mesh, u32::MAX),
        };
        let env = extras.env.map(|e| {
            let t = if self.textures.contains_key(&e.texture) { e.texture } else { 0 };
            (t, SamplerKey { filter: e.filter, address_u: e.address_u, address_v: e.address_v, anisotropy: 1 })
        });
        // Skinned atomics never take the MatFX path (as in gl3, whose skin pipeline has no env pass).
        let kind = if env.is_some() && kind == Kind::Mesh { Kind::MeshEnv } else { kind };
        self.record(state, 3, kind, first, count, view_proj, Some(MeshDraw { geometry, model: mi, bones: bi, env }));
    }

    #[allow(clippy::too_many_arguments)]
    fn record(&mut self, state: &DrawState, prim: u32, kind: Kind, first: u32, count: u32, mvp: &Mat4, mesh: Option<MeshDraw>) {
        if count == 0 {
            return;
        }

        let alpha = match state.alpha_func {
            1 => [state.alpha_ref, 1000.0],
            2 => [-1000.0, state.alpha_ref],
            _ => [-1000.0, 1000.0],
        };
        let c = state.fog_color;
        let u: Uniforms = [
            state.xform[0],
            state.xform[1],
            state.xform[2],
            state.xform[3],
            alpha[0],
            alpha[1],
            0.0,
            0.0,
            state.fog_start,
            state.fog_end,
            1.0 / (state.fog_start - state.fog_end),
            if state.fog != 0 { 0.0 } else { 1.0 },
            (c & 0xFF) as f32 / 255.0,
            (c >> 8 & 0xFF) as f32 / 255.0,
            (c >> 16 & 0xFF) as f32 / 255.0,
            (c >> 24 & 0xFF) as f32 / 255.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
                    0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
];
        // GL3 computes the range unguarded; keep the uniform finite either way.
        let mut u = u;
        if !u[10].is_finite() {
            u[10] = 0.0;
        }
        u[16..32].copy_from_slice(mvp);
        if kind == Kind::Im2D {
            u[32..40].copy_from_slice(&state.shader_params);
        }
        if self.uniforms.last() != Some(&u) {
            self.uniforms.push(u);
        }
        let uniform = self.uniforms.len() as u32 - 1;

        let pipe = PipeKey {
            samples: if self.recording_target == 0 { self.samples } else { 1 },
            kind,
            shader: if kind == Kind::Im3D { 0 } else { state.shader },
            lines: is_line_primitive(prim),
            blend: (state.blend != 0).then_some((state.src_blend, state.dst_blend)),
            z_test: state.z_test != 0,
            z_write: state.z_write != 0,
            cull: state.cull,
        };
        let texture = if self.textures.contains_key(&state.texture) { state.texture } else { 0 };
        let sampler = SamplerKey { filter: state.filter, address_u: state.address_u, address_v: state.address_v, anisotropy: state.max_anisotropy.clamp(1, 16) as u16 };
        if let Some(Op::Draw { pipe: p, texture: t, sampler: s, uniform: un, first: f, count: n, mesh: me }) = self.ops.last_mut()
            && *me == mesh
            && *p == pipe
            && *t == texture
            && *s == sampler
            && *un == uniform
            && *f + *n == first
        {
            *n += count;
            return;
        }
        self.ops.push(Op::Draw { pipe, texture, sampler, uniform, first, count, mesh });
    }

    fn ensure_pipeline(&mut self, gpu: &Gpu, key: PipeKey) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let blend = key.blend.map(|(s, d)| {
            let c = wgpu::BlendComponent { src_factor: blend_factor(s), dst_factor: blend_factor(d), operation: wgpu::BlendOperation::Add };
            wgpu::BlendState { color: c, alpha: c }
        });
        let p = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(match key.kind {
                Kind::Im2D => "im2d",
                Kind::Im3D => "im3d",
                Kind::Mesh => "mesh",
                Kind::Skin => "skin",
                Kind::MeshEnv => "mesh env",
                Kind::Im2DUV2 => "im2d uv2",
            }),
            layout: Some(match key.kind {
                Kind::Mesh => &self.mesh_layout,
                Kind::Skin => &self.skin_layout,
                Kind::MeshEnv => &self.env_layout,
                Kind::Im2DUV2 => &self.uv2_layout,
                _ => &self.pipeline_layout,
            }),
            vertex: wgpu::VertexState {
                module: if key.kind == Kind::Im2DUV2 { &self.shader_uv2 } else { &self.shader },
                entry_point: Some(match key.kind {
                    Kind::Im2D => "vs_2d",
                    Kind::Im3D => "vs_3d",
                    Kind::Mesh => "vs_mesh",
                    Kind::Skin => "vs_skin",
                    Kind::MeshEnv => "vs_env",
                    Kind::Im2DUV2 => "vs",
                })
                .map(|e| match (key.kind, key.shader) {
                    (Kind::MeshEnv, MESH_NEO_VEHICLE) => "vs_neo_vehicle",
                    (Kind::MeshEnv, MESH_NEO_WORLD) => "vs_neo_world",
                    (Kind::Mesh, MESH_NEO_GLOSS) => "vs_neo_gloss",
                    (Kind::Mesh, MESH_NEO_RIM) => "vs_neo_rim",
                    (Kind::Skin, MESH_NEO_RIM) => "vs_neo_rim_skin",
                    _ => e,
                }),
                buffers: &[Some(match key.kind {
                    Kind::Im2D => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex2D>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_2D },
                    Kind::Im3D => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex3D>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_3D },
                    Kind::Im2DUV2 => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex2DUV2>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_2D_UV2 },
                    Kind::Mesh | Kind::MeshEnv => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<MeshVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_MESH },
                    Kind::Skin => wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<SkinVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS_SKIN },
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: if key.kind == Kind::Im2DUV2 { &self.shader_uv2 } else { &self.shader },
                entry_point: Some(match (key.kind, key.shader) {
                    (Kind::MeshEnv, MESH_NEO_VEHICLE) => "fs_neo_vehicle",
                    (Kind::MeshEnv, MESH_NEO_WORLD) => "fs_neo_world",
                    (Kind::Mesh, MESH_NEO_GLOSS) => "fs_neo_gloss",
                    (Kind::MeshEnv, _) => "fs_env",
                    (Kind::Im2DUV2, _) => "fs",
                    (Kind::Im2D, 1) => "fs_colourfilter",
                    (Kind::Im2D, 2) => "fs_contrast",
                    _ => "fs_simple",
                }),
                // The frame has no alpha channel (D3D's X8R8G8B8 back buffer, GL3's window): alpha
                // stays 1, so destination alpha and copies of the frame read opaque.
                targets: &[Some(wgpu::ColorTargetState { format: self.format, blend, write_mask: wgpu::ColorWrites::COLOR })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: if key.lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: match key.cull {
                    2 => Some(wgpu::Face::Back),
                    3 => Some(wgpu::Face::Front),
                    _ => None,
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(key.z_write),
                depth_compare: Some(if key.z_test { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: key.samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, p);
    }

    fn ensure_bind_group(&mut self, gpu: &Gpu, texture: u32, key: SamplerKey) {
        if self.bind_groups.contains_key(&(texture, key)) {
            return;
        }
        let sampler = self.samplers.entry(key).or_insert_with(|| {
            use wgpu::FilterMode::{Linear as L, Nearest as N};
            use wgpu::MipmapFilterMode as M;
            // gl3 filterConvMap_NoMIP / _MIP: (min, mag, mip filter, uses mips)
            let (min, mag, mip, mips) = match key.filter {
                1 => (N, N, M::Nearest, false),
                3 => (N, N, M::Nearest, true),
                4 => (L, L, M::Nearest, true),
                5 => (N, N, M::Linear, true),
                6 => (L, L, M::Linear, true),
                _ => (L, L, M::Nearest, false),
            };
            // Anisotropic filtering needs all-linear filtering in wgpu; GL applies it to any
            // mipmapped linear filter, so those become trilinear while it is on.
            let aniso = if mips && min == L && key.anisotropy > 1 { key.anisotropy } else { 1 };
            let mip = if aniso > 1 { M::Linear } else { mip };
            gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("raster"),
                address_mode_u: address_mode(key.address_u),
                address_mode_v: address_mode(key.address_v),
                mag_filter: mag,
                min_filter: min,
                mipmap_filter: mip,
                lod_max_clamp: if mips { 32.0 } else { 0.0 },
                anisotropy_clamp: aniso,
                ..Default::default()
            })
        });
        let view = self.textures.get(&texture).map_or(&self.white.view, |t| &t.view);
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture"),
            layout: &self.texture_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
        });
        self.bind_groups.insert((texture, key), bind);
    }

    /// Following clears and draws render into texture `texture` (level 0), or into the scene
    /// with 0.
    pub fn set_target(&mut self, texture: u32) {
        if texture != self.recording_target {
            self.ops.push(Op::Target { texture });
            self.recording_target = texture;
        }
    }

    /// The frame so far, copied into texture `texture` at (x, y) when the frame is drawn.
    pub fn copy_frame(&mut self, texture: u32, x: u32, y: u32) {
        self.ops.push(Op::CopyFrame { texture, x, y });
    }

    /// The last encoded frame (SCENE_FORMAT, COPY_SRC), for screenshots.
    pub fn scene_texture(&self) -> Option<&wgpu::Texture> {
        self.scene.as_ref().map(|s| &s.0)
    }

    /// Encodes the recorded frame into the scene texture and shows it on `target` (`size` pixels,
    /// the output format given to `new`). The recording stays until `end_frame`.
    pub fn encode(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: (u32, u32)) {
        if self.scene.as_ref().is_none_or(|s| s.2 != size) {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit"),
                layout: &self.blit_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.create_view(&Default::default())) }],
            });
            self.scene = Some((t, bind, size));
        }
        let scene_view = self.scene.as_ref().unwrap().0.create_view(&Default::default());
        let ops = std::mem::take(&mut self.ops);
        let verts = std::mem::take(&mut self.verts);
        let verts3 = std::mem::take(&mut self.verts3);
        let verts_uv2 = std::mem::take(&mut self.verts_uv2);
        let uniforms = std::mem::take(&mut self.uniforms);
        let models = std::mem::take(&mut self.models);
        if models.len() > self.model_capacity {
            let cap = models.len().next_power_of_two();
            (self.model_buf, self.model_bind) = Self::make_dyn_uniforms(gpu, &self.model_layout, self.model_stride, cap, std::mem::size_of::<ModelState>() as u64);
            self.model_capacity = cap;
        }
        if !models.is_empty() {
            let mut bytes = vec![0u8; models.len() * self.model_stride as usize];
            for (i, mdl) in models.iter().enumerate() {
                // SAFETY: ModelState is repr(C) f32 data.
                let src = unsafe { std::slice::from_raw_parts((mdl as *const ModelState).cast::<u8>(), MODEL_FLOATS * 4) };
                bytes[i * self.model_stride as usize..][..src.len()].copy_from_slice(src);
            }
            gpu.queue.write_buffer(&self.model_buf, 0, &bytes);
        }
        let bones = std::mem::take(&mut self.bones);
        let blocks = bones.len() / BONE_FLOATS;
        if blocks > self.bones_capacity {
            let cap = blocks.next_power_of_two();
            (self.bones_buf, self.bones_bind) = Self::make_dyn_uniforms(gpu, &self.bones_layout, self.bones_stride, cap, (BONE_FLOATS * 4) as u64);
            self.bones_capacity = cap;
        }
        if blocks > 0 {
            let mut bytes = vec![0u8; blocks * self.bones_stride as usize];
            for (i, block) in bones.as_chunks::<BONE_FLOATS>().0.iter().enumerate() {
                let at = i * self.bones_stride as usize;
                for (j, f) in block.iter().enumerate() {
                    bytes[at + j * 4..at + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
                }
            }
            gpu.queue.write_buffer(&self.bones_buf, 0, &bytes);
        }

        // The scene's multisampled colour target.
        if self.samples > 1 && self.msaa.as_ref().is_none_or(|m| m.2 != size) {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene msaa"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: self.samples,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            self.msaa = Some((t, v, size));
        }
        // Depth buffers for the scene and every texture target of the frame.
        let mut sizes = vec![(size.0, size.1, self.samples)];
        for op in &ops {
            if let Op::Target { texture } = *op
                && let Some(t) = self.textures.get(&texture)
            {
                sizes.push((t.width, t.height, 1));
            }
        }
        for sz in sizes {
            self.depths.entry(sz).or_insert_with(|| {
                let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth"),
                    size: wgpu::Extent3d { width: sz.0, height: sz.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: sz.2,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let v = t.create_view(&Default::default());
                (t, v)
            });
        }
        if uniforms.len() > self.uniform_capacity {
            let cap = uniforms.len().next_power_of_two();
            (self.uniform_buf, self.uniform_bind) = Self::make_uniforms(gpu, &self.uniform_layout, self.uniform_stride, cap);
            self.uniform_capacity = cap;
        }
        if !uniforms.is_empty() {
            let mut bytes = vec![0u8; uniforms.len() * self.uniform_stride as usize];
            for (i, u) in uniforms.iter().enumerate() {
                let at = i * self.uniform_stride as usize;
                for (j, f) in u.iter().enumerate() {
                    bytes[at + j * 4..at + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
                }
            }
            gpu.queue.write_buffer(&self.uniform_buf, 0, &bytes);
        }
        // SAFETY: Vertex2D/Vertex3D are repr(C) plain data without padding (sizes asserted above).
        let bytes2 = unsafe { std::slice::from_raw_parts(verts.as_ptr().cast::<u8>(), std::mem::size_of_val(verts.as_slice())) };
        let bytes3 = unsafe { std::slice::from_raw_parts(verts3.as_ptr().cast::<u8>(), std::mem::size_of_val(verts3.as_slice())) };
        if bytes2.len() > self.vertex_buf.size() as usize {
            self.vertex_buf = Self::make_vertices(gpu, bytes2.len().next_power_of_two());
        }
        if bytes3.len() > self.vertex3_buf.size() as usize {
            self.vertex3_buf = Self::make_vertices(gpu, bytes3.len().next_power_of_two());
        }
        if !bytes2.is_empty() {
            gpu.queue.write_buffer(&self.vertex_buf, 0, bytes2);
        }
        // SAFETY: Vertex2DUV2 is repr(C) plain data without padding (36 bytes, asserted above).
        let bytes_uv2 = unsafe { std::slice::from_raw_parts(verts_uv2.as_ptr().cast::<u8>(), std::mem::size_of_val(verts_uv2.as_slice())) };
        if bytes_uv2.len() > self.vertex_uv2_buf.size() as usize {
            self.vertex_uv2_buf = Self::make_vertices(gpu, bytes_uv2.len().next_power_of_two());
        }
        if !bytes_uv2.is_empty() {
            gpu.queue.write_buffer(&self.vertex_uv2_buf, 0, bytes_uv2);
        }
        if !bytes3.is_empty() {
            gpu.queue.write_buffer(&self.vertex3_buf, 0, bytes3);
        }
        for op in &ops {
            if let Op::Draw { pipe, texture, sampler, mesh, .. } = *op {
                self.ensure_pipeline(gpu, pipe);
                self.ensure_bind_group(gpu, texture, sampler);
                if let Some(MeshDraw { env: Some((t, s)), .. }) = mesh {
                    self.ensure_bind_group(gpu, t, s);
                }
            }
        }

        // Attachment views (level 0) of the frame's texture targets.
        let mut target_views: HashMap<u32, (wgpu::TextureView, (u32, u32))> = HashMap::new();
        for op in &ops {
            if let Op::Target { texture } = *op
                && texture != 0
                && let Some(t) = self.textures.get(&texture)
                && !t.format.is_compressed()
            {
                target_views.entry(texture).or_insert_with(|| {
                    (t.texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 0, mip_level_count: Some(1), ..Default::default() }), (t.width, t.height))
                });
            }
        }
        let depths = &self.depths;
        let msaa_view = self.msaa.as_ref().filter(|_| self.samples > 1).map(|m| &m.1);
        let scene_depth = (size.0, size.1, self.samples);
        // `target` 0 is the scene; `first` is the scene's first pass of the frame.
        let begin = |enc: &mut wgpu::CommandEncoder, to: u32, first: bool, color: Option<[f64; 4]>, depth: bool| {
            // The scene with MSAA draws into the multisampled target and resolves into the scene.
            let (view, resolve, depth_view) = match target_views.get(&to) {
                Some((v, sz)) => (v, None, &depths[&(sz.0, sz.1, 1)].1),
                None => match msaa_view {
                    Some(m) => (m, Some(&scene_view), &depths[&scene_depth].1),
                    None => (&scene_view, None, &depths[&scene_depth].1),
                },
            };
            let color_load = match color {
                Some(c) => wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: 1.0 }),
                None if first => wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                None => wgpu::LoadOp::Load,
            };
            let depth_load = if depth || first { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load };
            enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: resolve,
                    depth_slice: None,
                    ops: wgpu::Operations { load: color_load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations { load: depth_load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            })
            .forget_lifetime()
        };

        let mut pass: Option<wgpu::RenderPass<'static>> = None;
        // Which immediate vertex buffer the pass has bound (None after starting a pass or after a
        // geometry's buffers).
        let mut bound: Option<Kind> = None;
        let mut started = false;
        // Current render target (0 = scene) and whether it can be drawn to (a destroyed or
        // compressed target texture swallows its draws).
        let mut cur_target = 0u32;
        let mut target_ok = true;
        for op in &ops {
            match *op {
                Op::Target { texture } => {
                    drop(pass.take());
                    bound = None;
                    cur_target = texture;
                    target_ok = texture == 0 || target_views.contains_key(&texture);
                }
                Op::CopyFrame { texture, x, y } => {
                    drop(pass.take());
                    bound = None;
                    if !started {
                        // Nothing drawn yet: the scene is still last frame's.
                        pass = Some(begin(enc, 0, true, None, false));
                        started = true;
                        drop(pass.take());
                    }
                    let Some(t) = self.textures.get(&texture) else { continue };
                    let (w, h) = (size.0.min(t.width.saturating_sub(x)), size.1.min(t.height.saturating_sub(y)));
                    if t.format != SCENE_FORMAT || w == 0 || h == 0 {
                        continue;
                    }
                    enc.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo { texture: &self.scene.as_ref().unwrap().0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                        wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    );
                }
                Op::Clear { color, depth } => {
                    if !target_ok {
                        continue;
                    }
                    drop(pass.take());
                    pass = Some(begin(enc, cur_target, cur_target == 0 && !started, color, depth));
                    started |= cur_target == 0;
                    bound = None;
                }
                Op::Draw { pipe, texture, sampler, uniform, first, count, mesh } => {
                    if !target_ok {
                        continue;
                    }
                    if pass.is_none() {
                        pass = Some(begin(enc, cur_target, cur_target == 0 && !started, None, false));
                        started |= cur_target == 0;
                        bound = None;
                    }
                    let p = pass.as_mut().unwrap();
                    p.set_pipeline(&self.pipelines[&pipe]);
                    p.set_bind_group(0, &self.uniform_bind, &[(uniform as u64 * self.uniform_stride) as u32]);
                    p.set_bind_group(1, &self.bind_groups[&(texture, sampler)], &[]);
                    if pipe.kind == Kind::Im2DUV2 {
                        if let Some(MeshDraw { env: Some(e), .. }) = mesh {
                            p.set_bind_group(2, &self.bind_groups[&e], &[]);
                        }
                        if bound != Some(pipe.kind) {
                            p.set_vertex_buffer(0, self.vertex_uv2_buf.slice(..));
                            bound = Some(pipe.kind);
                        }
                        p.draw(first..first + count, 0..1);
                        continue;
                    }
                    match mesh {
                        Some(MeshDraw { geometry, model, bones, env }) => {
                            let Some(g) = self.geometries.get(&geometry) else { continue };
                            p.set_bind_group(2, &self.model_bind, &[(model as u64 * self.model_stride) as u32]);
                            if pipe.kind == Kind::Skin {
                                p.set_bind_group(3, &self.bones_bind, &[(bones as u64 * self.bones_stride) as u32]);
                            }
                            if let (Kind::MeshEnv, Some(e)) = (pipe.kind, env) {
                                p.set_bind_group(3, &self.bind_groups[&e], &[]);
                            }
                            p.set_vertex_buffer(0, g.vertices.slice(..));
                            p.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint16);
                            bound = None;
                            p.draw_indexed(first..first + count, 0, 0..1);
                        }
                        None => {
                            if bound != Some(pipe.kind) {
                                p.set_vertex_buffer(0, if pipe.kind == Kind::Im3D { self.vertex3_buf.slice(..) } else { self.vertex_buf.slice(..) });
                                bound = Some(pipe.kind);
                            }
                            p.draw(first..first + count, 0..1);
                        }
                    }
                }
            }
        }
        drop(pass.take());
        if !started {
            pass = Some(begin(enc, 0, true, None, false));
        }
        drop(pass);

        // Show the scene on the output.
        let mut blit = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        blit.set_pipeline(&self.blit_pipeline);
        blit.set_bind_group(0, &self.scene.as_ref().unwrap().1, &[]);
        blit.draw(0..3, 0..1);
        drop(blit);

        self.ops = ops;
        self.verts = verts;
        self.verts3 = verts3;
        self.verts_uv2 = verts_uv2;
        self.uniforms = uniforms;
        self.models = models;
        self.bones = bones;
    }

    /// Drops the recorded frame (keeping the allocations) to start recording the next one.
    pub fn end_frame(&mut self) {
        self.ops.clear();
        self.verts.clear();
        self.verts3.clear();
        self.verts_uv2.clear();
        self.uniforms.clear();
        self.models.clear();
        self.bones.clear();
        self.retired.clear();
        self.recording_target = 0;
        for id in std::mem::take(&mut self.dead_textures) {
            self.textures.remove(&id);
            self.bind_groups.retain(|k, _| k.0 != id);
        }
        for id in std::mem::take(&mut self.dead_geometries) {
            self.geometries.remove(&id);
        }
    }
}

/// A texture copy on its way to the CPU (`readback_finish` after the commands are submitted).
pub struct Readback {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    row: u32,
    bgra: bool,
}

/// Copies `texture` (RGBA8 or BGRA8, `size` pixels, needs COPY_SRC) into a buffer.
pub fn readback_start(gpu: &Gpu, enc: &mut wgpu::CommandEncoder, texture: &wgpu::Texture, size: (u32, u32)) -> Readback {
    let row = (size.0 * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * size.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size.1) } },
        wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
    );
    let bgra = matches!(texture.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    Readback { buffer, width: size.0, height: size.1, row, bgra }
}

/// Waits for the copy and returns tightly packed RGBA8, top row first.
pub fn readback_finish(gpu: &Gpu, r: Readback) -> Result<Vec<u8>, String> {
    let slice = r.buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("poll: {e}"))?;
    let data = slice.get_mapped_range().map_err(|e| format!("readback: {e:?}"))?;
    let mut out = Vec::with_capacity((r.width * r.height * 4) as usize);
    for y in 0..r.height as usize {
        let line = &data[y * r.row as usize..][..r.width as usize * 4];
        if r.bgra {
            out.extend(line.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]));
        } else {
            out.extend_from_slice(line);
        }
    }
    Ok(out)
}

//! The explicit draw state: what a fixed-function era device exposes, expressed in wgpu terms.
//!
//! New code for retro-wgpu. Comparison, blend and stencil enums are wgpu's own types, so a
//! front end maps its API's numbering (D3D, GL, GS, pddi...) onto them in one place.

pub use wgpu::{BlendFactor, BlendOperation, CompareFunction, Face, FrontFace, StencilOperation};

/// Column-major 4x4 matrix (OpenGL/WGSL layout: columns are x, y, z axes and translation).
pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// a * b (column-major).
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

/// One blend equation (colour or alpha): `src * src_factor (op) dst * dst_factor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlendEq {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub op: BlendOperation,
}

impl BlendEq {
    pub const REPLACE: BlendEq = BlendEq { src: BlendFactor::One, dst: BlendFactor::Zero, op: BlendOperation::Add };
    pub const ALPHA: BlendEq = BlendEq { src: BlendFactor::SrcAlpha, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOperation::Add };
    pub const ADD: BlendEq = BlendEq { src: BlendFactor::One, dst: BlendFactor::One, op: BlendOperation::Add };

    fn wgpu(self) -> wgpu::BlendComponent {
        // wgpu requires factors One for min/max.
        let (src, dst) = if matches!(self.op, BlendOperation::Min | BlendOperation::Max) { (BlendFactor::One, BlendFactor::One) } else { (self.src, self.dst) };
        wgpu::BlendComponent { src_factor: src, dst_factor: dst, operation: self.op }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    pub color: BlendEq,
    pub alpha: BlendEq,
}

impl Blend {
    pub const ALPHA: Blend = Blend { color: BlendEq::ALPHA, alpha: BlendEq::ALPHA };
    pub const ADD: Blend = Blend { color: BlendEq::ADD, alpha: BlendEq::ADD };

    /// The same equation for colour and alpha.
    pub fn same(eq: BlendEq) -> Blend {
        Blend { color: eq, alpha: eq }
    }

    pub(crate) fn wgpu(self) -> wgpu::BlendState {
        wgpu::BlendState { color: self.color.wgpu(), alpha: self.alpha.wgpu() }
    }
}

/// Which channels a draw writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColorMask {
    pub rgb: bool,
    pub alpha: bool,
}

impl ColorMask {
    pub const ALL: ColorMask = ColorMask { rgb: true, alpha: true };
    pub const RGB: ColorMask = ColorMask { rgb: true, alpha: false };
    pub const NONE: ColorMask = ColorMask { rgb: false, alpha: false };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DepthState {
    /// `Always` with `write: false` is "depth off".
    pub compare: CompareFunction,
    pub write: bool,
}

impl DepthState {
    pub const OFF: DepthState = DepthState { compare: CompareFunction::Always, write: false };
    pub const LESS_EQUAL: DepthState = DepthState { compare: CompareFunction::LessEqual, write: true };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StencilFace {
    pub compare: CompareFunction,
    /// Stencil test failed.
    pub fail: StencilOperation,
    /// Stencil passed, depth failed.
    pub depth_fail: StencilOperation,
    /// Both passed.
    pub pass: StencilOperation,
}

impl StencilFace {
    pub const KEEP: StencilFace = StencilFace { compare: CompareFunction::Always, fail: StencilOperation::Keep, depth_fail: StencilOperation::Keep, pass: StencilOperation::Keep };
}

/// Stencil test and update. The reference value is dynamic: it lives in [`RenderState`] but
/// doesn't create pipelines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StencilState {
    pub front: StencilFace,
    /// Back faces (two-sided stencil, e.g. shadow volumes in one pass).
    pub back: StencilFace,
    pub read_mask: u8,
    pub write_mask: u8,
}

impl StencilState {
    pub const OFF: StencilState = StencilState { front: StencilFace::KEEP, back: StencilFace::KEEP, read_mask: 0xFF, write_mask: 0 };

    /// The same operations on both faces.
    pub fn both(face: StencilFace, read_mask: u8, write_mask: u8) -> StencilState {
        StencilState { front: face, back: face, read_mask, write_mask }
    }
}

/// Polygon offset: depth += constant * r + slope * max_slope (triangles only).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DepthBias {
    pub constant: i32,
    pub slope: f32,
    pub clamp: f32,
}

/// Alpha test, done in the fragment shader: pixels whose final alpha fails
/// `alpha <compare> reference` are discarded. `Always` = off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaTest {
    pub compare: CompareFunction,
    /// 0..1.
    pub reference: f32,
}

impl AlphaTest {
    pub const OFF: AlphaTest = AlphaTest { compare: CompareFunction::Always, reference: 0.0 };
}

/// Fog by view-space depth (distance along the camera axis), per vertex, mixed in after the
/// texture stages and the alpha test: `colour = mix(fog colour, colour, f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FogMode {
    Off,
    /// f = (end - d) / (end - start), clamped.
    Linear { start: f32, end: f32 },
    /// f = exp(-density * d).
    Exp { density: f32 },
    /// f = exp(-(density * d)^2).
    Exp2 { density: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub mode: FogMode,
    /// RGB used, 0..1.
    pub color: [f32; 4],
}

impl Fog {
    pub const OFF: Fog = Fog { mode: FogMode::Off, color: [0.0; 4] };
}

/// Texture filtering of one stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    /// Point sampling.
    Nearest,
    /// Bilinear.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Address {
    Wrap,
    Mirror,
    Clamp,
    /// Clamp to the border colour: wgpu core has no border colour, so this clamps to the edge.
    Border,
}

/// Sampler state of a texture stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sampler {
    pub min: Filter,
    pub mag: Filter,
    /// None: level 0 only. Some(filter): mipmapping with point or linear blending between levels.
    pub mip: Option<Filter>,
    pub address_u: Address,
    pub address_v: Address,
    /// 1 = off. Anisotropy needs all-linear filtering in wgpu: with > 1, min/mag/mip become
    /// linear for this sampler.
    pub anisotropy: u16,
}

impl Sampler {
    pub const POINT_CLAMP: Sampler = Sampler { min: Filter::Nearest, mag: Filter::Nearest, mip: None, address_u: Address::Clamp, address_v: Address::Clamp, anisotropy: 1 };
    pub const BILINEAR_WRAP: Sampler = Sampler { min: Filter::Linear, mag: Filter::Linear, mip: None, address_u: Address::Wrap, address_v: Address::Wrap, anisotropy: 1 };
    pub const TRILINEAR_WRAP: Sampler = Sampler { min: Filter::Linear, mag: Filter::Linear, mip: Some(Filter::Linear), address_u: Address::Wrap, address_v: Address::Wrap, anisotropy: 1 };
}

impl Default for Sampler {
    fn default() -> Sampler {
        Sampler::BILINEAR_WRAP
    }
}

/// Inputs of a combiner stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The result of the previous stage (the diffuse colour for stage 0).
    Current,
    /// The interpolated vertex colour (after lighting).
    Diffuse,
    /// This stage's texture sample.
    Texture,
    /// This stage's constant colour.
    Constant,
}

/// A combiner argument with D3D-style modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Arg {
    pub source: Source,
    /// 1 - x.
    pub invert: bool,
    /// Use the alpha in all channels.
    pub alpha: bool,
}

impl Arg {
    pub const fn of(source: Source) -> Arg {
        Arg { source, invert: false, alpha: false }
    }
    pub const CURRENT: Arg = Arg::of(Source::Current);
    pub const DIFFUSE: Arg = Arg::of(Source::Diffuse);
    pub const TEXTURE: Arg = Arg::of(Source::Texture);
    pub const CONSTANT: Arg = Arg::of(Source::Constant);
}

/// Combiner operations (a subset of D3D's texture stage ops; enough for PS2 GS TFX modes, GC
/// TEV "basic" set-ups and GL texenv).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// The stage and all after it are off (colour op only; as an alpha op it passes Current).
    Disable,
    Arg1,
    Arg2,
    Modulate,
    Modulate2x,
    Modulate4x,
    Add,
    /// a1 + a2 - 0.5
    AddSigned,
    /// a1 - a2
    Subtract,
    /// mix(a2, a1, texture alpha)
    BlendTextureAlpha,
    /// mix(a2, a1, diffuse alpha)
    BlendDiffuseAlpha,
    /// mix(a2, a1, constant alpha)
    BlendConstantAlpha,
    /// mix(a2, a1, current alpha)
    BlendCurrentAlpha,
    /// a1 * a2 + current (multiply-add)
    MultiplyAdd,
}

/// Where a stage's texture coordinates come from (before its texture matrix).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexCoordSource {
    /// Vertex texture coordinate set 0..3.
    Uv(u8),
    /// Sphere map: view-space normal xy * 0.5 + 0.5.
    SphereMap,
    /// Camera-space reflection vector (xyz into the texture matrix; xy used).
    Reflection,
    /// Camera-space normal.
    Normal,
    /// Camera-space position.
    Position,
}

/// One texture/combiner stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// None: white.
    pub texture: Option<crate::TextureId>,
    pub sampler: Sampler,
    pub color_op: Op,
    pub color_args: [Arg; 2],
    pub alpha_op: Op,
    pub alpha_args: [Arg; 2],
    pub constant: [f32; 4],
    pub coords: TexCoordSource,
    /// Applied to (coords, 1) — identity for plain UVs.
    pub matrix: Mat4,
}

impl Stage {
    /// Stage off (and all after it).
    pub const OFF: Stage = Stage {
        texture: None,
        sampler: Sampler::BILINEAR_WRAP,
        color_op: Op::Disable,
        color_args: [Arg::TEXTURE, Arg::CURRENT],
        alpha_op: Op::Disable,
        alpha_args: [Arg::TEXTURE, Arg::CURRENT],
        constant: [1.0; 4],
        coords: TexCoordSource::Uv(0),
        matrix: IDENTITY,
    };

    /// texture * current for colour and alpha, coordinates from UV set 0.
    pub fn modulate(texture: crate::TextureId, sampler: Sampler) -> Stage {
        Stage { texture: Some(texture), sampler, color_op: Op::Modulate, alpha_op: Op::Modulate, ..Stage::OFF }
    }
}

/// Number of texture stages.
pub const STAGES: usize = 4;

/// Everything about a draw except geometry, transforms and lights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderState {
    pub blend: Option<Blend>,
    /// Blend constant (BlendFactor::Constant).
    pub blend_constant: [f32; 4],
    pub color_mask: ColorMask,
    pub depth: DepthState,
    pub stencil: StencilState,
    pub stencil_reference: u8,
    pub cull: Option<Face>,
    pub front_face: FrontFace,
    pub depth_bias: DepthBias,
    pub alpha_test: AlphaTest,
    pub fog: Fog,
    pub stages: [Stage; STAGES],
}

impl Default for RenderState {
    /// Opaque, depth tested and written, no culling, untextured.
    fn default() -> RenderState {
        RenderState {
            blend: None,
            blend_constant: [0.0; 4],
            color_mask: ColorMask::ALL,
            depth: DepthState::LESS_EQUAL,
            stencil: StencilState::OFF,
            stencil_reference: 0,
            cull: None,
            front_face: FrontFace::Ccw,
            depth_bias: DepthBias::default(),
            alpha_test: AlphaTest::OFF,
            fog: Fog::OFF,
            stages: [Stage::OFF; STAGES],
        }
    }
}

impl RenderState {
    /// For 2D overlays: no depth, alpha blending.
    pub fn overlay() -> RenderState {
        RenderState { blend: Some(Blend::ALPHA), depth: DepthState::OFF, ..RenderState::default() }
    }

    pub fn with_texture(mut self, texture: crate::TextureId, sampler: Sampler) -> RenderState {
        self.stages[0] = Stage::modulate(texture, sampler);
        self
    }
}

/// Material colours for lighting, and where they come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub diffuse: [f32; 4],
    pub ambient: [f32; 4],
    pub emissive: [f32; 4],
    /// Take diffuse/ambient/emissive from the vertex colour instead (D3D's material sources).
    pub diffuse_from_vertex: bool,
    pub ambient_from_vertex: bool,
    pub emissive_from_vertex: bool,
}

impl Default for Material {
    /// White, colour from the vertex where there is one.
    fn default() -> Material {
        Material { diffuse: [1.0; 4], ambient: [1.0; 4], emissive: [0.0; 4], diffuse_from_vertex: true, ambient_from_vertex: true, emissive_from_vertex: false }
    }
}

/// A directional light: `direction` is where the light travels (world space).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DirLight {
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

pub const MAX_LIGHTS: usize = 4;

/// Per-vertex lighting: `emissive + ambient * mat.ambient + sum(max(N.-L, 0) * light * mat.diffuse)`,
/// clamped to 0..1; alpha from the diffuse source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    pub enabled: bool,
    pub ambient: [f32; 3],
    pub lights: [DirLight; MAX_LIGHTS],
    pub count: usize,
    /// Normalise normals after the world transform (scaled models).
    pub normalize: bool,
}

impl Lighting {
    pub const OFF: Lighting = Lighting { enabled: false, ambient: [0.0; 3], lights: [DirLight { direction: [0.0; 3], color: [0.0; 3] }; MAX_LIGHTS], count: 0, normalize: true };
}

impl Default for Lighting {
    fn default() -> Lighting {
        Lighting::OFF
    }
}

/// Per-draw transform and lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawParams {
    pub world: Mat4,
    pub material: Material,
    pub lighting: Lighting,
}

impl Default for DrawParams {
    fn default() -> DrawParams {
        DrawParams { world: IDENTITY, material: Material::default(), lighting: Lighting::OFF }
    }
}

/// The camera: view and projection (to wgpu clip space, depth 0..1), plus what screen-space
/// (pre-transformed) vertices need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub proj: Mat4,
    /// Pixel size of the screen space pre-transformed positions are given in.
    pub screen: [f32; 2],
    /// Added to pre-transformed x/y before the mapping: -0.5 for D3D9-style pixel centres.
    pub pixel_offset: f32,
}

impl View {
    pub fn new(view: Mat4, proj: Mat4) -> View {
        View { view, proj, screen: [640.0, 480.0], pixel_offset: 0.0 }
    }

    /// For screen-space drawing only.
    pub fn screen(width: f32, height: f32) -> View {
        View { view: IDENTITY, proj: IDENTITY, screen: [width, height], pixel_offset: 0.0 }
    }
}

/// Right-handed perspective to wgpu's 0..1 depth range (camera looks down -z).
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    [f / aspect, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, far / (near - far), -1.0, 0.0, 0.0, near * far / (near - far), 0.0]
}

/// Pipeline-relevant part of a state (everything that needs a new wgpu pipeline).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FixedKey {
    pub blend: Option<Blend>,
    pub mask: ColorMask,
    pub depth: DepthState,
    pub stencil: StencilState,
    pub cull: Option<Face>,
    pub front_face: FrontFace,
    pub bias: (i32, u32, u32),
}

impl RenderState {
    pub(crate) fn fixed_key(&self) -> FixedKey {
        FixedKey {
            blend: self.blend,
            mask: self.color_mask,
            depth: self.depth,
            stencil: self.stencil,
            cull: self.cull,
            front_face: self.front_face,
            bias: (self.depth_bias.constant, self.depth_bias.slope.to_bits(), self.depth_bias.clamp.to_bits()),
        }
    }
}

pub(crate) fn wgpu_sampler(s: &Sampler) -> wgpu::SamplerDescriptor<'static> {
    let f = |f: Filter| match f {
        Filter::Nearest => wgpu::FilterMode::Nearest,
        Filter::Linear => wgpu::FilterMode::Linear,
    };
    let a = |a: Address| match a {
        Address::Wrap => wgpu::AddressMode::Repeat,
        Address::Mirror => wgpu::AddressMode::MirrorRepeat,
        Address::Clamp | Address::Border => wgpu::AddressMode::ClampToEdge,
    };
    let aniso = s.anisotropy.clamp(1, 16);
    let (min, mag, mip) = if aniso > 1 { (Filter::Linear, Filter::Linear, Some(Filter::Linear)) } else { (s.min, s.mag, s.mip) };
    wgpu::SamplerDescriptor {
        label: Some("stage"),
        address_mode_u: a(s.address_u),
        address_mode_v: a(s.address_v),
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: f(mag),
        min_filter: f(min),
        mipmap_filter: match mip {
            Some(Filter::Linear) => wgpu::MipmapFilterMode::Linear,
            _ => wgpu::MipmapFilterMode::Nearest,
        },
        lod_min_clamp: 0.0,
        lod_max_clamp: if mip.is_some() { 32.0 } else { 0.0 },
        anisotropy_clamp: aniso,
        ..Default::default()
    }
}

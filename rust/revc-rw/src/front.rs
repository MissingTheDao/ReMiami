//! Front end of the RenderWare renderer: turns the device's calls (camera clears, texture and
//! geometry uploads, Im2D/Im3D primitives, model draws, render targets) into a recorded `Frame`
//! of plain data. It makes no GPU calls, so it can run on the game thread while a back end
//! (`frame::WgpuBackend`, or another one) draws the previous frame elsewhere.
//!
//! Resource ids are allocated here; uploads travel inside the frame and are applied by the back
//! end before the frame's draws (as the immediate uploads did before the split: queue writes
//! land before the frame's command buffer). Destroyed resources stay usable until the frame that
//! destroyed them is drawn.

use crate::prim::{expand_primitive, is_line_primitive};
use std::collections::HashMap as StdHashMap;
use std::collections::HashSet;

/// rustc's Fx hash: the renderer's caches are looked up several times per draw, and std's SipHash
/// showed up in profiles (keys are small, trusted integers).
#[derive(Clone, Copy, Default)]
pub struct FxBuild;

#[derive(Default)]
pub struct FxHasher(u64);

impl FxHasher {
    #[inline]
    fn add(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl std::hash::Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            self.add(u64::from_le_bytes(*c));
        }
        for &b in rest {
            self.add(b as u64);
        }
    }
    #[inline]
    fn write_u8(&mut self, x: u8) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u16(&mut self, x: u16) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u32(&mut self, x: u32) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u64(&mut self, x: u64) {
        self.add(x);
    }
    #[inline]
    fn write_usize(&mut self, x: usize) {
        self.add(x as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

impl std::hash::BuildHasher for FxBuild {
    type Hasher = FxHasher;
    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher(0)
    }
}

pub(crate) type HashMap<K, V> = StdHashMap<K, V, FxBuild>;

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
pub(crate) const BONE_FLOATS: usize = MAX_BONES * 16;

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
pub(crate) const MODEL_FLOATS: usize = std::mem::size_of::<ModelState>() / 4;

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
pub(crate) enum Kind {
    Im2D,
    Im3D,
    Mesh,
    Skin,
    MeshEnv,
    /// Im2D with two texture coordinate sets and a second texture (screen droplets).
    Im2DUV2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MeshDraw {
    pub(crate) geometry: u32,
    pub(crate) model: u32,
    /// Bone block for skinned draws.
    pub(crate) bones: u32,
    /// Environment texture and sampler for MatFX draws.
    pub(crate) env: Option<(u32, SamplerKey)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PipeKey {
    pub(crate) kind: Kind,
    /// Im2D override shader (DrawState::shader).
    pub(crate) shader: u32,
    pub(crate) lines: bool,
    pub(crate) blend: Option<(u32, u32)>,
    pub(crate) z_test: bool,
    pub(crate) z_write: bool,
    pub(crate) cull: u32,
    /// MSAA sample count of the target (the scene's; texture targets have 1).
    pub(crate) samples: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SamplerKey {
    pub(crate) filter: u32,
    pub(crate) address_u: u32,
    pub(crate) address_v: u32,
    pub(crate) anisotropy: u16,
}

/// xform, alpha window, fog, fog colour, the Im3D model-view-projection matrix, then the
/// override shader's parameters.
pub(crate) type Uniforms = [f32; 72];

/// The vertex transform of a draw, kept apart like GL3's u_proj, u_view and u_world so the
/// shaders multiply in GL3's order (clip = proj * view * world * pos) and round the same.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub proj: Mat4,
    pub view: Mat4,
    pub world: Mat4,
}

/// Im2D draws ignore the transform.
pub(crate) const NO_XFORM: Xform = Xform { proj: [0.0; 16], view: [0.0; 16], world: [0.0; 16] };

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

impl From<Mat4> for Xform {
    /// One combined matrix (as projection; view and world identity).
    fn from(m: Mat4) -> Xform {
        Xform { proj: m, view: IDENTITY, world: IDENTITY }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
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
    /// The 3D scene is finished; what follows (HUD, menus) is drawn at window resolution on top
    /// of the scaled and antialiased scene.
    Overlay,
}

/// Antialiasing as one setting, levels ordered by cost (lowest first): what the game's
/// Antialiasing option selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Antialiasing {
    /// MSAA samples (1 = off; clamped to what the device does).
    pub msaa: u32,
    /// Supersampling: the scene is drawn at this many times the window size per axis, then
    /// filtered down (1 = off, 2 = 4 samples per pixel).
    pub supersample: u32,
    /// FXAA post pass on the final image.
    pub fxaa: bool,
    /// SMAA 1x (preset high) on the final image; takes precedence over FXAA.
    pub smaa: bool,
}

impl Antialiasing {
    pub const OFF: Antialiasing = Antialiasing { msaa: 1, supersample: 1, fxaa: false, smaa: false };
    /// Levels 0.. from cheapest to most expensive.
    pub const LEVELS: [(&'static str, Antialiasing); 9] = [
        ("Off", Antialiasing::OFF),
        ("FXAA", Antialiasing { msaa: 1, supersample: 1, fxaa: true, smaa: false }),
        ("SMAA", Antialiasing { msaa: 1, supersample: 1, fxaa: false, smaa: true }),
        ("MSAA 2x", Antialiasing { msaa: 2, supersample: 1, fxaa: false, smaa: false }),
        ("MSAA 4x", Antialiasing { msaa: 4, supersample: 1, fxaa: false, smaa: false }),
        ("MSAA 4x + SMAA", Antialiasing { msaa: 4, supersample: 1, fxaa: false, smaa: true }),
        ("MSAA 8x", Antialiasing { msaa: 8, supersample: 1, fxaa: false, smaa: false }),
        ("SSAA 2x", Antialiasing { msaa: 1, supersample: 2, fxaa: false, smaa: false }),
        ("SSAA 2x + MSAA 4x", Antialiasing { msaa: 4, supersample: 2, fxaa: false, smaa: false }),
    ];

    pub fn level(n: u32) -> Antialiasing {
        Self::LEVELS.get(n as usize).map_or(Self::OFF, |l| l.1)
    }
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

    pub(crate) fn kind(self) -> u8 {
        match self {
            TexData::Rgba8 => 0,
            TexData::Dxt1 => 1,
            TexData::Dxt3 => 3,
            TexData::Dxt5 => 5,
        }
    }
}

/// A resource change carried by a frame, applied by the back end before the frame's draws.
#[derive(Debug)]
pub(crate) enum Upload {
    /// Creates or replaces texture `id` from mip levels (top row first, each half the previous).
    Texture { id: u32, width: u32, height: u32, data: TexData, levels: Vec<Vec<u8>>, autogen: bool },
    /// Creates or replaces geometry `id`: vertex bytes (MeshVertex or SkinVertex) and a
    /// triangle list.
    Geometry { id: u32, vertices: Vec<u8>, indices: Vec<u16> },
}

/// Renderer settings a frame is drawn with (set on the front end, so a frame recorded before a
/// change is drawn as it was recorded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSettings {
    /// MSAA samples of the scene (already clamped to what the device does).
    pub samples: u32,
    /// Supersampling factor (1 or 2).
    pub scale: u32,
    pub fxaa: bool,
    pub smaa: bool,
    /// Render resolution of the 3D scene, percent of the window per axis.
    pub render_scale: u32,
}

impl FrameSettings {
    /// Whether the overlay is drawn separately from the scene (the scene gets filtered).
    pub fn overlay_split(&self) -> bool {
        self.scale > 1 || self.render_scale != 100 || self.fxaa || self.smaa
    }

    /// The scene's size for a `window`: supersampling times, with an overlay, the render scale.
    pub fn scene_size(&self, window: (u32, u32), overlay: bool) -> (u32, u32) {
        let f = self.scale as f64 * if overlay { self.render_scale as f64 / 100.0 } else { 1.0 };
        (((window.0 as f64 * f).round() as u32).max(1), ((window.1 as f64 * f).round() as u32).max(1))
    }
}

/// One recorded frame: commands, their vertex/uniform/model/bone arenas, the resource uploads
/// and destructions that came with it, and its settings. Plain data (Send): it can be handed to
/// a render thread and recycled afterwards with its allocations.
#[derive(Debug)]
pub struct Frame {
    pub(crate) ops: Vec<Op>,
    pub(crate) verts: Vec<Vertex2D>,
    pub(crate) verts3: Vec<Vertex3D>,
    pub(crate) verts_uv2: Vec<Vertex2DUV2>,
    pub(crate) uniforms: Vec<Uniforms>,
    pub(crate) models: Vec<ModelState>,
    /// BONE_FLOATS per skinned draw.
    pub(crate) bones: Vec<f32>,
    pub(crate) uploads: Vec<Upload>,
    /// Destroyed during the frame; removed once it is drawn.
    pub(crate) dead_textures: Vec<u32>,
    pub(crate) dead_geometries: Vec<u32>,
    pub(crate) settings: FrameSettings,
}

impl Frame {
    fn new(settings: FrameSettings) -> Frame {
        Frame {
            ops: Vec::new(),
            verts: Vec::new(),
            verts3: Vec::new(),
            verts_uv2: Vec::new(),
            uniforms: Vec::new(),
            models: Vec::new(),
            bones: Vec::new(),
            uploads: Vec::new(),
            dead_textures: Vec::new(),
            dead_geometries: Vec::new(),
            settings,
        }
    }

    fn clear(&mut self) {
        self.ops.clear();
        self.verts.clear();
        self.verts3.clear();
        self.verts_uv2.clear();
        self.uniforms.clear();
        self.models.clear();
        self.bones.clear();
        self.uploads.clear();
        self.dead_textures.clear();
        self.dead_geometries.clear();
    }

    pub fn settings(&self) -> FrameSettings {
        self.settings
    }

    /// Number of recorded commands (clears, draws, target switches, copies).
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Whether the frame has an overlay marker (end of the 3D scene).
    pub fn has_overlay(&self) -> bool {
        self.ops.iter().any(|op| matches!(op, Op::Overlay))
    }
}

/// Records the device's calls into frames (the front end).
pub struct Recorder {
    frame: Frame,
    /// Recycled frames, allocations kept.
    spare: Vec<Frame>,
    settings: FrameSettings,
    textures: HashSet<u32>,
    geometries: HashSet<u32>,
    next_texture: u32,
    next_geometry: u32,
    /// Render target being recorded (0 = scene).
    recording_target: u32,
    /// Recording past the overlay marker.
    recording_overlay: bool,
    scratch_in: Vec<u32>,
    scratch_out: Vec<u32>,
}

impl Recorder {
    /// A recorder for a scene with `samples`-times MSAA (clamped by the caller to what the
    /// device does).
    pub fn new(samples: u32) -> Recorder {
        let settings = FrameSettings { samples: samples.max(1), scale: 1, fxaa: false, smaa: false, render_scale: 100 };
        Recorder {
            frame: Frame::new(settings),
            spare: Vec::new(),
            settings,
            textures: HashSet::new(),
            geometries: HashSet::new(),
            next_texture: 1,
            next_geometry: 1,
            recording_target: 0,
            recording_overlay: false,
            scratch_in: Vec::new(),
            scratch_out: Vec::new(),
        }
    }

    pub fn settings(&self) -> FrameSettings {
        self.settings
    }

    /// Settings for the frame being recorded if nothing is recorded yet, else for the next
    /// one (a frame is drawn with the settings its commands were recorded for).
    pub fn set_settings(&mut self, settings: FrameSettings) {
        self.settings = FrameSettings { samples: settings.samples.max(1), scale: settings.scale.clamp(1, 2), render_scale: settings.render_scale.clamp(25, 100), ..settings };
        if self.frame.ops.is_empty() {
            self.frame.settings = self.settings;
        }
    }

    /// Whether texture `id` exists (destroyed ones count until their frame is drawn).
    pub fn has_texture(&self, id: u32) -> bool {
        self.textures.contains(&id)
    }

    /// Creates (id 0 or unknown) or replaces texture `id` from mip levels, RGBA8 or DXT blocks;
    /// `autogen` builds the mip chain from level 0 (RGBA8 only). Returns the texture's id.
    pub fn upload_texture(&mut self, id: u32, width: u32, height: u32, data: TexData, levels: &[&[u8]], autogen: bool) -> u32 {
        let id = if id != 0 && self.textures.contains(&id) {
            id
        } else {
            let id = self.next_texture;
            self.next_texture += 1;
            self.textures.insert(id);
            id
        };
        let levels = levels.iter().map(|l| l.to_vec()).collect();
        self.frame.uploads.push(Upload::Texture { id, width: width.max(1), height: height.max(1), data, levels, autogen });
        id
    }

    pub fn destroy_texture(&mut self, id: u32) {
        self.frame.dead_textures.push(id);
    }

    /// Creates or replaces a geometry (vertices + triangle-list indices); returns its id.
    pub fn upload_geometry(&mut self, id: u32, vertices: &[MeshVertex], indices: &[u16]) -> u32 {
        // SAFETY: MeshVertex is repr(C) plain data without padding (44 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(id, vbytes, indices)
    }

    /// A skinned geometry (drawn with bone matrices by `draw_mesh`).
    pub fn upload_skinned_geometry(&mut self, id: u32, vertices: &[SkinVertex], indices: &[u16]) -> u32 {
        // SAFETY: SkinVertex is repr(C) plain data without padding (64 bytes, asserted above).
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        self.upload_raw(id, vbytes, indices)
    }

    fn upload_raw(&mut self, id: u32, vbytes: &[u8], indices: &[u16]) -> u32 {
        let id = if id != 0 && self.geometries.contains(&id) {
            id
        } else {
            let id = self.next_geometry;
            self.next_geometry += 1;
            self.geometries.insert(id);
            id
        };
        self.frame.uploads.push(Upload::Geometry { id, vertices: vbytes.to_vec(), indices: indices.to_vec() });
        id
    }

    pub fn destroy_geometry(&mut self, id: u32) {
        self.frame.dead_geometries.push(id);
    }

    /// Camera clear: colour (when given) and/or depth.
    pub fn clear(&mut self, color: Option<[f64; 4]>, depth: bool) {
        self.frame.ops.push(Op::Clear { color, depth });
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
        let first = self.frame.verts.len() as u32;
        self.frame.verts.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.frame.verts.len() as u32 - first;
        self.record(state, prim, Kind::Im2D, first, count, &NO_XFORM, None);
    }

    /// An Im3D primitive with its projection, view and world matrices (column-major).
    pub fn draw_3d(&mut self, state: &DrawState, prim: u32, verts: &[Vertex3D], indices: Option<&[u16]>, xf: &Xform) {
        self.expand(prim, verts.len(), indices);
        let first = self.frame.verts3.len() as u32;
        self.frame.verts3.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.frame.verts3.len() as u32 - first;
        self.record(state, prim, Kind::Im3D, first, count, xf, None);
    }

    /// An Im2D primitive with two coordinate sets, textured with the bound raster and `tex1`
    /// (re3's screen droplets: drop mask times the frame copy).
    pub fn draw_2d_uv2(&mut self, state: &DrawState, prim: u32, verts: &[Vertex2DUV2], indices: Option<&[u16]>, tex1: &EnvMap) {
        self.expand(prim, verts.len(), indices);
        let first = self.frame.verts_uv2.len() as u32;
        self.frame.verts_uv2.extend(self.scratch_out.iter().filter_map(|&i| verts.get(i as usize)));
        let count = self.frame.verts_uv2.len() as u32 - first;
        let t = if self.textures.contains(&tex1.texture) { tex1.texture } else { 0 };
        let env = Some((t, SamplerKey { filter: tex1.filter, address_u: tex1.address_u, address_v: tex1.address_v, anisotropy: 1 }));
        self.record(state, prim, Kind::Im2DUV2, first, count, &NO_XFORM, Some(MeshDraw { geometry: 0, model: 0, bones: 0, env }));
    }

    /// Triangles `first..first + count` of the index list of a stored geometry, lit with
    /// `model`; `xf` = projection and view (world comes with the model). A skinned geometry gets its bones (at most
    /// MAX_BONES; missing ones are identity); a MatFX material its environment map.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_mesh(&mut self, state: &DrawState, geometry: u32, first: u32, count: u32, xf: &Xform, model: &ModelState, extras: &MeshExtras) {
        let bones = extras.bones;
        if !self.geometries.contains(&geometry) {
            return;
        }
        let f = &mut self.frame;
        if f.models.last() != Some(model) {
            f.models.push(*model);
        }
        let mi = f.models.len() as u32 - 1;
        let (kind, bi) = match bones {
            Some(b) => {
                let mut block = [0.0f32; BONE_FLOATS];
                for i in 0..MAX_BONES {
                    let m = b.get(i).copied().unwrap_or(IDENTITY);
                    block[i * 16..][..16].copy_from_slice(&m);
                }
                let n = f.bones.len();
                if n < BONE_FLOATS || f.bones[n - BONE_FLOATS..] != block {
                    f.bones.extend_from_slice(&block);
                }
                (Kind::Skin, (f.bones.len() / BONE_FLOATS - 1) as u32)
            }
            None => (Kind::Mesh, u32::MAX),
        };
        let env = extras.env.map(|e| {
            let t = if self.textures.contains(&e.texture) { e.texture } else { 0 };
            (t, SamplerKey { filter: e.filter, address_u: e.address_u, address_v: e.address_v, anisotropy: 1 })
        });
        // Skinned atomics never take the MatFX path (as in gl3, whose skin pipeline has no env pass).
        let kind = if env.is_some() && kind == Kind::Mesh { Kind::MeshEnv } else { kind };
        self.record(state, 3, kind, first, count, xf, Some(MeshDraw { geometry, model: mi, bones: bi, env }));
    }

    #[allow(clippy::too_many_arguments)]
    fn record(&mut self, state: &DrawState, prim: u32, kind: Kind, first: u32, count: u32, xf: &Xform, mesh: Option<MeshDraw>) {
        if count == 0 {
            return;
        }

        let alpha = match state.alpha_func {
            1 => [state.alpha_ref, 1000.0],
            2 => [-1000.0, state.alpha_ref],
            _ => [-1000.0, 1000.0],
        };
        let c = state.fog_color;
        let mut u: Uniforms = [0.0; 72];
        u[0..4].copy_from_slice(&state.xform);
        u[4] = alpha[0];
        u[5] = alpha[1];
        u[8] = state.fog_start;
        u[9] = state.fog_end;
        u[10] = 1.0 / (state.fog_start - state.fog_end);
        u[11] = if state.fog != 0 { 0.0 } else { 1.0 };
        u[12] = (c & 0xFF) as f32 / 255.0;
        u[13] = (c >> 8 & 0xFF) as f32 / 255.0;
        u[14] = (c >> 16 & 0xFF) as f32 / 255.0;
        u[15] = (c >> 24 & 0xFF) as f32 / 255.0;
        // GL3 computes the range unguarded; keep the uniform finite either way.
        if !u[10].is_finite() {
            u[10] = 0.0;
        }
        u[16..32].copy_from_slice(&xf.proj);
        u[32..48].copy_from_slice(&xf.view);
        u[48..64].copy_from_slice(&xf.world);
        if kind == Kind::Im2D {
            u[64..72].copy_from_slice(&state.shader_params);
        }
        let f = &mut self.frame;
        if f.uniforms.last() != Some(&u) {
            f.uniforms.push(u);
        }
        let uniform = f.uniforms.len() as u32 - 1;

        let s = f.settings;
        let pipe = PipeKey {
            // Past the overlay marker the scene is finished and the HUD draws single-sampled.
            samples: if self.recording_target == 0 && !(self.recording_overlay && s.overlay_split()) { s.samples } else { 1 },
            kind,
            shader: if kind == Kind::Im3D { 0 } else { state.shader },
            lines: is_line_primitive(prim),
            blend: (state.blend != 0).then_some((state.src_blend, state.dst_blend)),
            z_test: state.z_test != 0,
            z_write: state.z_write != 0,
            cull: state.cull,
        };
        let texture = if self.textures.contains(&state.texture) { state.texture } else { 0 };
        let sampler = SamplerKey { filter: state.filter, address_u: state.address_u, address_v: state.address_v, anisotropy: state.max_anisotropy.clamp(1, 16) as u16 };
        if let Some(Op::Draw { pipe: p, texture: t, sampler: s, uniform: un, first: fi, count: n, mesh: me }) = f.ops.last_mut()
            && *me == mesh
            && *p == pipe
            && *t == texture
            && *s == sampler
            && *un == uniform
            && *fi + *n == first
        {
            *n += count;
            return;
        }
        f.ops.push(Op::Draw { pipe, texture, sampler, uniform, first, count, mesh });
    }

    /// Following clears and draws render into texture `texture` (level 0), or into the scene
    /// with 0.
    pub fn set_target(&mut self, texture: u32) {
        if texture != self.recording_target {
            self.frame.ops.push(Op::Target { texture });
            self.recording_target = texture;
        }
    }

    /// Marks the end of the 3D scene: following draws (HUD, menus) go on top of the scaled and
    /// antialiased scene at window resolution. Once per frame; frames without it are drawn
    /// whole at the antialiasing scale (no render scale).
    pub fn begin_overlay(&mut self) {
        if !self.recording_overlay {
            self.frame.ops.push(Op::Overlay);
            self.recording_overlay = true;
        }
    }

    /// The frame so far, copied into texture `texture` at (x, y) when the frame is drawn.
    pub fn copy_frame(&mut self, texture: u32, x: u32, y: u32) {
        self.frame.ops.push(Op::CopyFrame { texture, x, y });
    }

    /// Ends the frame being recorded and starts the next one (with the current settings).
    pub fn finish(&mut self) -> Frame {
        let mut next = self.spare.pop().unwrap_or_else(|| Frame::new(self.settings));
        next.clear();
        next.settings = self.settings;
        let done = std::mem::replace(&mut self.frame, next);
        for id in &done.dead_textures {
            self.textures.remove(id);
        }
        for id in &done.dead_geometries {
            self.geometries.remove(id);
        }
        self.recording_target = 0;
        self.recording_overlay = false;
        done
    }

    /// Gives a drawn frame back for reuse of its allocations.
    pub fn recycle(&mut self, mut frame: Frame) {
        if self.spare.len() < 2 {
            frame.clear();
            self.spare.push(frame);
        }
    }
}

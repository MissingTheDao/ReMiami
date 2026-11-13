//! retro-wgpu: an engine-neutral renderer core on wgpu for ports and remasters of games from
//! the fixed-function era (PS2/GC/Xbox/PC D3D8-9), with Vulkan, OpenGL and Direct3D 12.
//!
//! - [`gpu`]: backend and device selection.
//! - [`present`]: window surfaces through raw window handles (Wayland, X11, Win32).
//! - [`Renderer`]: per-frame command recorder with an explicit [`RenderState`] (blend, depth,
//!   stencil, cull, polygon offset, alpha test, fog, four texture/combiner stages, samplers),
//!   data-described vertex layouts, retained or immediate geometry, render targets with MSAA,
//!   frame copies; then supersampling, a post chain (FXAA, SMAA, own passes) and the output
//!   stage (internal resolution upscaling, aspect, integer scaling, gamma).
//! - [`texture`]: RGBA8/BGRA8, 16-bit packed, paletted 4/8-bit (PS2 CLUT), BC1-3/BC7.
//! - [`replace`]: high-resolution texture packs keyed by a hash of the original data.
//! - [`readback`]: screenshots.
//! - `slang` (feature "slang"): RetroArch .slangp presets (CRT, Mega Bezel-style overlays)
//!   as the output stage, through librashader.
//!
//! Nothing here is derived from librw or RenderWare; see NOTICE.md for provenance.

mod fxhash;
pub mod gpu;
pub mod layout;
pub mod post;
pub mod present;
pub mod readback;
pub mod renderer;
pub mod replace;
pub(crate) mod shader;
#[cfg(feature = "slang")]
pub mod slang;
pub mod state;
pub mod texture;

pub use gpu::{Backend, Gpu, GpuOptions};
pub use layout::{AttrFormat, Attribute, Semantic, VertexLayout};
pub use post::{Fxaa, OutputSettings, PostPass, ScaleFilter, ScaleMode, Smaa, TARGET_FORMAT};
pub use readback::Image;
pub use renderer::{BufferId, DEPTH_FORMAT, Draw, Indices, LayoutId, OutputStage, ProgramDesc, ProgramId, Renderer, SceneSettings, Target, TextureDesc, TextureId, Topology, VertexWriter, Vertices};
pub use shader::MAX_BONES;
pub use state::*;
pub use replace::{ReplaceConfig, texture_key};
pub use texture::{ClutLayout, Palette, TexFormat};
pub use wgpu;

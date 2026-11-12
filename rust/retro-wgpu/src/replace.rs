//! High-resolution texture replacement (stage 2).

use crate::Palette;
use crate::gpu::Gpu;
use crate::renderer::{Renderer, TextureDesc, TextureId};

/// Placeholder until stage 2.
pub struct Replacer;

pub(crate) fn on_upload(_r: &mut Renderer, _gpu: &Gpu, _id: TextureId, _desc: TextureDesc, _data: &[u8], _palette: Option<&Palette>) -> bool {
    false
}

//! RetroArch slang shader presets as the output stage (feature "slang", stage 3), through
//! librashader's wgpu runtime (MPL-2.0, see NOTICE.md). Presets are supplied by the user; none
//! are shipped.
//!
//! What presets get:
//! - `Original`: the post-processed game image at the game's internal resolution (an offscreen
//!   texture, never the window-sized image), so CRT masks and bezels line up with real pixels;
//! - `OriginalHistory#` and `PassFeedback#`: kept by librashader across frames (one chain per
//!   renderer; `FrameCount` is the renderer's frame counter);
//! - the output: either the whole window (`SlangPlacement::FullOutput`, what bezel presets such
//!   as Mega Bezel expect: they draw the frame and place the game themselves), or the rectangle
//!   the built-in scaler would use (`SlangPlacement::GameRect`: aspect, integer scaling and an
//!   explicit viewport rect, for plain CRT/filter presets).
//!
//! librashader clears the target to transparent black before its last pass, so the area
//! around a `GameRect` image is black.

use crate::gpu::Gpu;
use crate::post::{OutputSettings, output_rect};
use crate::renderer::OutputStage;
use librashader_common::{Size, Viewport};
use librashader_presets::ShaderFeatures;
use librashader_runtime::parameters::FilterChainParameters;
use librashader_runtime_wgpu::options::{FilterChainOptionsWgpu, FrameOptionsWgpu};
use librashader_runtime_wgpu::{FilterChainWgpu, WgpuOutputView};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlangPlacement {
    /// The preset draws over the whole output (bezel presets).
    FullOutput,
    /// The preset draws into the rectangle given by these settings (mode, display aspect,
    /// viewport rect; filter, background and gamma are not used).
    GameRect(OutputSettings),
}

pub struct SlangOutput {
    chain: FilterChainWgpu,
    pub placement: SlangPlacement,
    /// Aspect ratio reported to presets (FrameOptions::aspect_ratio); 0 = from the image.
    pub aspect_ratio: f32,
}

impl SlangOutput {
    /// Loads and compiles a .slangp preset (blocks until its textures are uploaded).
    pub fn load(gpu: &Gpu, preset: &Path) -> Result<SlangOutput, String> {
        let features = ShaderFeatures::ORIGINAL_ASPECT_UNIFORMS | ShaderFeatures::FRAMETIME_UNIFORMS;
        let options = FilterChainOptionsWgpu { force_no_mipmaps: false, enable_cache: false, adapter_info: Some(gpu.info.clone()) };
        let chain = FilterChainWgpu::load_from_path(preset, features, &gpu.device, &gpu.queue, Some(&options)).map_err(|e| format!("{}: {e}", preset.display()))?;
        Ok(SlangOutput { chain, placement: SlangPlacement::FullOutput, aspect_ratio: 0.0 })
    }

    /// Sets a preset parameter (`#pragma parameter`); returns the old value if it exists.
    pub fn set_parameter(&self, name: &str, value: f32) -> Option<f32> {
        self.chain.parameters().set_parameter_value(name, value)
    }

    /// Current parameter values.
    pub fn parameters(&self) -> Vec<(String, f32)> {
        let mut v: Vec<(String, f32)> = self.chain.parameters().parameters().iter().map(|(k, v)| (k.to_string(), *v)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
}

impl OutputStage for SlangOutput {
    fn run(&mut self, _gpu: &Gpu, enc: &mut wgpu::CommandEncoder, image: &wgpu::Texture, target: &wgpu::TextureView, format: wgpu::TextureFormat, size: (u32, u32), frame: u64) {
        let rect = match self.placement {
            SlangPlacement::FullOutput => [0.0, 0.0, size.0 as f32, size.1 as f32],
            SlangPlacement::GameRect(s) => output_rect((image.width(), image.height()), size, &s),
        };
        let x = rect[0].clamp(0.0, size.0 as f32);
        let y = rect[1].clamp(0.0, size.1 as f32);
        let w = (rect[2].min(size.0 as f32 - x)).max(1.0) as u32;
        let h = (rect[3].min(size.1 as f32 - y)).max(1.0) as u32;
        let output = WgpuOutputView::new_from_raw(target, Size { width: size.0, height: size.1 }, format);
        let viewport = Viewport { x, y, mvp: None, output, size: Size { width: w, height: h } };
        let aspect = if self.aspect_ratio > 0.0 { self.aspect_ratio } else { image.width() as f32 / image.height().max(1) as f32 };
        let options = FrameOptionsWgpu { aspect_ratio: aspect, ..Default::default() };
        if let Err(e) = self.chain.frame(image, &viewport, enc, frame as usize, Some(&options)) {
            eprintln!("retro-wgpu: slang frame: {e}");
        }
    }
}

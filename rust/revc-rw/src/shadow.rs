//! Shadow mode: the presenting back end (`WgpuBackend`) shows the game as usual, and the
//! retro-wgpu back end draws the same recorded frames off-screen. Checked frames are read back
//! from both and compared before the output stage: the retro-wgpu path passes when at least
//! 99.5 % of pixels are within 2 levels per channel.

use crate::Gpu;
use crate::frame::{Frame, WgpuBackend, readback_finish, readback_start};
use crate::retro::RetroBackend;

/// Share of pixels that must be within `TOLERANCE` levels.
pub const GATE: f64 = 0.995;
pub const TOLERANCE: u8 = 2;

/// One compared frame.
pub struct Comparison {
    pub frame: u64,
    pub width: u32,
    pub height: u32,
    /// Share of pixels within TOLERANCE in every channel.
    pub within: f64,
    /// Largest difference of any channel.
    pub max_diff: u8,
    /// RGBA8 images (presenting back end, retro-wgpu), kept when the frame fails the gate.
    pub images: Option<(Vec<u8>, Vec<u8>)>,
}

impl Comparison {
    pub fn passed(&self) -> bool {
        self.within >= GATE
    }
}

/// Compares two RGBA8 images of the same size (alpha ignored: the frame is opaque).
pub fn compare(a: &[u8], b: &[u8]) -> (f64, u8) {
    let n = (a.len().min(b.len()) / 4).max(1);
    let mut ok = 0usize;
    let mut max = 0u8;
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let d = (0..3).map(|i| p[i].abs_diff(q[i])).max().unwrap();
        max = max.max(d);
        if d <= TOLERANCE {
            ok += 1;
        }
    }
    (ok as f64 / n as f64, max)
}

pub struct Shadow {
    gpu: retro_wgpu::Gpu,
    back: RetroBackend,
    out: Option<(wgpu::Texture, (u32, u32))>,
    frame: u64,
}

impl Shadow {
    pub fn new(gpu: &Gpu) -> Shadow {
        let rgpu = gpu.retro();
        let back = RetroBackend::new(&rgpu, wgpu::TextureFormat::Rgba8Unorm);
        Shadow { gpu: rgpu, back, out: None, frame: 0 }
    }

    /// Draws `frame` (already drawn and submitted by `legacy`) with retro-wgpu at `size`; with
    /// `check`, compares both images. Call before the frame is recycled.
    pub fn run(&mut self, gpu: &Gpu, legacy: &WgpuBackend, frame: &Frame, size: (u32, u32), check: bool) -> Option<Comparison> {
        self.frame += 1;
        if self.out.as_ref().is_none_or(|o| o.1 != size) {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow output"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.out = Some((t, size));
        }
        let view = self.out.as_ref().unwrap().0.create_view(&Default::default());
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        self.back.encode(&self.gpu, &mut enc, &view, size, frame);
        let reads = if check {
            match (legacy.scene_texture(), self.back.final_image()) {
                (Some(a), Some(b)) if (a.width(), a.height()) == (b.width(), b.height()) => {
                    let sz = (a.width(), a.height());
                    Some((readback_start(gpu, &mut enc, a, sz), readback_start(gpu, &mut enc, b, sz), sz))
                }
                _ => None,
            }
        } else {
            None
        };
        gpu.queue.submit([enc.finish()]);
        self.back.end_frame(frame);
        let (ra, rb, sz) = reads?;
        let (a, b) = (readback_finish(gpu, ra).ok()?, readback_finish(gpu, rb).ok()?);
        let (within, max_diff) = compare(&a, &b);
        let images = (within < GATE).then_some((a, b));
        Some(Comparison { frame: self.frame, width: sz.0, height: sz.1, within, max_diff, images })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compare_counts_pixels_within_tolerance() {
        let a = [10, 10, 10, 255, 0, 0, 0, 255, 100, 100, 100, 255, 7, 7, 7, 0];
        let b = [12, 10, 9, 255, 3, 0, 0, 255, 100, 100, 100, 0, 7, 7, 7, 255];
        let (within, max) = super::compare(&a, &b);
        assert_eq!(max, 3);
        assert!((within - 0.75).abs() < 1e-9);
    }
}

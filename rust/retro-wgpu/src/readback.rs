//! GPU -> CPU copies for screenshots and tests.
//!
//! Provenance: from revc-rw's `readback_start`/`readback_finish` (the user's own engine-neutral
//! code), returning an [`Image`] instead of a bare vector.

use crate::gpu::Gpu;

/// An RGBA8 image, top row first, tightly packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
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

/// Copies mip level 0 of `texture` (RGBA8 or BGRA8, needs COPY_SRC) into a mappable buffer.
pub fn readback_start(gpu: &Gpu, enc: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) -> Readback {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    let bgra = matches!(texture.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    Readback { buffer, width: w, height: h, row, bgra }
}

/// Waits for the copy (after the encoder was submitted) and returns RGBA8, top row first.
pub fn readback_finish(gpu: &Gpu, r: Readback) -> Result<Image, String> {
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
    Ok(Image { width: r.width, height: r.height, rgba: out })
}

/// Reads a texture back right away (own encoder and submit).
pub fn read_texture(gpu: &Gpu, texture: &wgpu::Texture) -> Result<Image, String> {
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    let r = readback_start(gpu, &mut enc, texture);
    gpu.queue.submit([enc.finish()]);
    readback_finish(gpu, r)
}

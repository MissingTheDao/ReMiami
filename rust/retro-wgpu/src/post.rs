//! After the scene: supersampling downsample, a pluggable chain of post passes at the game's
//! internal resolution, and the output stage that places the image in the window (scaling,
//! aspect correction, filtering, output gamma).
//!
//! Provenance: the SMAA pass (smaa.wgsl, lookup textures, three-pass plumbing) is copied from
//! revc-rw, the user's own port of iryoku/SMAA (MIT, notice kept). FXAA (fxaa.wgsl), the blit
//! shader, the scaler and the rectangle maths are new code for retro-wgpu.

use crate::gpu::Gpu;

/// Format of the scene, render targets and every post-chain image.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One pass over a finished image: `input` (sampled, TARGET_FORMAT) drawn into `output` (same
/// size and format).
pub trait PostPass: Send {
    fn name(&self) -> &str;
    fn run(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, input: &wgpu::Texture, output: &wgpu::TextureView);
}

const BLIT: &str = r#"
struct P { gamma: vec4<f32>, uv_rect: vec4<f32> };
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var<uniform> p: P;

struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let q = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: V;
    o.pos = vec4<f32>(q, 0.0, 1.0);
    o.uv = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    return o;
}

@fragment
fn fs(i: V) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(src, smp, p.uv_rect.xy + i.uv * p.uv_rect.zw, 0.0);
    if (p.gamma.y != 0.0) {
        return vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(p.gamma.x)), 1.0);
    }
    return c;
}
"#;

/// Full-screen copies with a choice of sampler and target format (downsample, prescale, output).
pub(crate) struct Blitter {
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pl: wgpu::PipelineLayout,
    pipelines: std::collections::HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    pub nearest: wgpu::Sampler,
    pub linear: wgpu::Sampler,
}

fn tex_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}

fn fullscreen_pipeline(d: &wgpu::Device, label: &str, pl: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, entry: &str, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pl),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some(entry), targets: &[Some(format.into())], compilation_options: Default::default() }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) fn render_texture(gpu: &Gpu, label: &str, size: (u32, u32), format: wgpu::TextureFormat) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

impl Blitter {
    pub fn new(gpu: &Gpu) -> Blitter {
        let d = &gpu.device;
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                tex_entry(0, true),
                sampler_entry(1),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("blit"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let sampler = |f: wgpu::FilterMode| d.create_sampler(&wgpu::SamplerDescriptor { label: Some("blit"), mag_filter: f, min_filter: f, ..Default::default() });
        Blitter { shader, layout, pl, pipelines: Default::default(), nearest: sampler(wgpu::FilterMode::Nearest), linear: sampler(wgpu::FilterMode::Linear) }
    }

    /// Draws `src` (the `uv_rect` part of it: offset, size in 0..1) into `dst`, inside
    /// `viewport` (x, y, w, h) or the whole target; `clear` clears the target first.
    /// `gamma`: output curve exponent applied to rgb (alpha forced to 1), None = plain copy.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::Texture,
        sampler_linear: bool,
        dst: &wgpu::TextureView,
        dst_format: wgpu::TextureFormat,
        viewport: Option<[f32; 4]>,
        uv_rect: [f32; 4],
        clear: Option<wgpu::Color>,
        gamma: Option<f32>,
    ) {
        let d = &gpu.device;
        let pipeline = self.pipelines.entry(dst_format).or_insert_with(|| fullscreen_pipeline(d, "blit", &self.pl, &self.shader, "fs", dst_format));
        let mut p = [0f32; 8];
        if let Some(g) = gamma {
            p[0] = 1.0 / g.max(1e-3);
            p[1] = 1.0;
        }
        p[4..8].copy_from_slice(&uv_rect);
        let bytes: Vec<u8> = p.iter().flat_map(|f| f.to_le_bytes()).collect();
        use wgpu::util::DeviceExt;
        let ub = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blit params"), contents: &bytes, usage: wgpu::BufferUsages::UNIFORM });
        let view = src.create_view(&Default::default());
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if sampler_linear { &self.linear } else { &self.nearest }) },
                wgpu::BindGroupEntry { binding: 2, resource: ub.as_entire_binding() },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        if let Some(v) = viewport {
            if v[2] <= 0.0 || v[3] <= 0.0 {
                return;
            }
            pass.set_viewport(v[0], v[1], v[2], v[3], 0.0, 1.0);
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// FXAA, written for retro-wgpu (see fxaa.wgsl for the algorithm's origin).
pub struct Fxaa {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl Fxaa {
    pub fn new(gpu: &Gpu) -> Fxaa {
        let d = &gpu.device;
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fxaa"), entries: &[tex_entry(0, true), sampler_entry(1)] });
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fxaa"), source: wgpu::ShaderSource::Wgsl(include_str!("fxaa.wgsl").into()) });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fxaa"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = fullscreen_pipeline(d, "fxaa", &pl, &shader, "fs", TARGET_FORMAT);
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor { label: Some("fxaa"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        Fxaa { layout, pipeline, sampler }
    }
}

impl PostPass for Fxaa {
    fn name(&self) -> &str {
        "fxaa"
    }

    fn run(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, input: &wgpu::Texture, output: &wgpu::TextureView) {
        let view = input.create_view(&Default::default());
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fxaa"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fxaa"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: output, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// SMAA 1x (preset high), from revc-rw's port of iryoku/SMAA (MIT; see NOTICE.md).
pub struct Smaa {
    layout: wgpu::BindGroupLayout,
    edges: wgpu::RenderPipeline,
    weights: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    linear: wgpu::Sampler,
    point: wgpu::Sampler,
    area: wgpu::Texture,
    search: wgpu::Texture,
    /// Edges (RG8) and blending weights (RGBA8) at the image size.
    work: Option<(wgpu::Texture, wgpu::Texture, (u32, u32))>,
}

impl Smaa {
    pub fn new(gpu: &Gpu) -> Smaa {
        let d = &gpu.device;
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("smaa"), entries: &[tex_entry(0, true), tex_entry(1, true), tex_entry(2, true), sampler_entry(3), sampler_entry(4)] });
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("smaa"), source: wgpu::ShaderSource::Wgsl(include_str!("smaa.wgsl").into()) });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("smaa"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let lookup = |label: &str, w: u32, h: u32, format: wgpu::TextureFormat, bpt: u32, data: &[u8]| {
            let t = d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * bpt), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            t
        };
        Smaa {
            edges: fullscreen_pipeline(d, "smaa edges", &pl, &shader, "edges", wgpu::TextureFormat::Rg8Unorm),
            weights: fullscreen_pipeline(d, "smaa weights", &pl, &shader, "weights", wgpu::TextureFormat::Rgba8Unorm),
            blend: fullscreen_pipeline(d, "smaa blend", &pl, &shader, "blend", TARGET_FORMAT),
            layout,
            linear: d.create_sampler(&wgpu::SamplerDescriptor { label: Some("smaa linear"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() }),
            point: d.create_sampler(&wgpu::SamplerDescriptor { label: Some("smaa point"), ..Default::default() }),
            area: lookup("smaa area", 160, 560, wgpu::TextureFormat::Rg8Unorm, 2, include_bytes!("../assets/smaa/area_160x560.rg8")),
            search: lookup("smaa search", 64, 16, wgpu::TextureFormat::R8Unorm, 1, include_bytes!("../assets/smaa/search_64x16.r8")),
            work: None,
        }
    }
}

impl PostPass for Smaa {
    fn name(&self) -> &str {
        "smaa"
    }

    fn run(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, input: &wgpu::Texture, output: &wgpu::TextureView) {
        let size = (input.width(), input.height());
        if self.work.as_ref().is_none_or(|w| w.2 != size) {
            self.work = Some((render_texture(gpu, "smaa edges", size, wgpu::TextureFormat::Rg8Unorm), render_texture(gpu, "smaa weights", size, wgpu::TextureFormat::Rgba8Unorm), size));
        }
        let (edges, weights, _) = self.work.as_ref().unwrap();
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let (src_v, edges_v, weights_v, area_v, search_v) = (view(input), view(edges), view(weights), view(&self.area), view(&self.search));
        let bind = |a: &wgpu::TextureView, b: &wgpu::TextureView, c: &wgpu::TextureView| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("smaa"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(a) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(b) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(c) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.linear) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.point) },
                ],
            })
        };
        let pass = |enc: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup, target: &wgpu::TextureView| {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("smaa"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    // Edges and weights start at 0 (the edge pass discards edge-free pixels).
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            p.set_pipeline(pipeline);
            p.set_bind_group(0, group, &[]);
            p.draw(0..3, 0..1);
        };
        pass(enc, &self.edges, &bind(&src_v, &src_v, &src_v), &edges_v);
        pass(enc, &self.weights, &bind(&edges_v, &area_v, &search_v), &weights_v);
        pass(enc, &self.blend, &bind(&src_v, &weights_v, &src_v), output);
    }
}

/// How the game image is fitted into the output rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    /// Fill the rectangle (ignores aspect).
    Stretch,
    /// As large as fits with the display aspect kept.
    Fit,
    /// Integer multiples of the image height (and of the width when the pixels are square);
    /// with a display aspect, the width follows the aspect (non-integer horizontally).
    Integer,
}

/// Texture filtering when scaling to the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleFilter {
    Nearest,
    Bilinear,
    /// Nearest-neighbour prescale by the largest integer factor that fits, then bilinear to the
    /// final size: sharp pixels without uneven widths.
    SharpBilinear,
}

/// The output stage's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputSettings {
    pub mode: ScaleMode,
    pub filter: ScaleFilter,
    /// Aspect ratio the game image is meant to be shown at (e.g. 4.0 / 3.0 for a 640x448 PS2
    /// frame). None: square pixels (image width / height).
    pub display_aspect: Option<f32>,
    /// Rectangle of the output (x, y, w, h in output pixels) the image goes into, e.g. the
    /// screen area of a bezel. None: the whole output.
    pub viewport: Option<[f32; 4]>,
    /// Colour around the image.
    pub background: [f64; 4],
    /// Output curve: rgb^(1/gamma) on the final image (1.0 = none; 1.1 = the original Xbox's
    /// default gamma ramp, for example).
    pub gamma: f32,
}

impl Default for OutputSettings {
    fn default() -> OutputSettings {
        OutputSettings { mode: ScaleMode::Stretch, filter: ScaleFilter::Bilinear, display_aspect: None, viewport: None, background: [0.0, 0.0, 0.0, 1.0], gamma: 1.0 }
    }
}

/// Where an image of `src` pixels lands in an output of `out` pixels: (x, y, w, h).
pub fn output_rect(src: (u32, u32), out: (u32, u32), s: &OutputSettings) -> [f32; 4] {
    let vp = s.viewport.unwrap_or([0.0, 0.0, out.0 as f32, out.1 as f32]);
    let (vw, vh) = (vp[2].max(1.0), vp[3].max(1.0));
    let (sw, sh) = (src.0.max(1) as f32, src.1.max(1) as f32);
    let aspect = s.display_aspect.unwrap_or(sw / sh);
    let (w, h) = match s.mode {
        ScaleMode::Stretch => (vw, vh),
        ScaleMode::Fit => {
            if vw / vh > aspect {
                (vh * aspect, vh)
            } else {
                (vw, vw / aspect)
            }
        }
        ScaleMode::Integer => {
            let square = s.display_aspect.is_none();
            let mut k = (vh / sh).floor().max(1.0);
            if square {
                k = k.min((vw / sw).floor().max(1.0));
                (sw * k, sh * k)
            } else {
                while k > 1.0 && sh * k * aspect > vw {
                    k -= 1.0;
                }
                ((sh * k * aspect).round(), sh * k)
            }
        }
    };
    let x = vp[0] + ((vw - w) / 2.0).floor();
    let y = vp[1] + ((vh - h) / 2.0).floor();
    [x, y, w, h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects() {
        let s = OutputSettings { mode: ScaleMode::Integer, ..Default::default() };
        assert_eq!(output_rect((320, 240), (1920, 1080), &s), [320.0, 60.0, 1280.0, 960.0]);
        let s = OutputSettings { mode: ScaleMode::Integer, display_aspect: Some(4.0 / 3.0), ..Default::default() };
        // 448 lines: 2x vertically (896), width 896 * 4/3 = 1195.
        assert_eq!(output_rect((640, 448), (1920, 1080), &s), [362.0, 92.0, 1195.0, 896.0]);
        let s = OutputSettings { mode: ScaleMode::Fit, display_aspect: Some(4.0 / 3.0), ..Default::default() };
        assert_eq!(output_rect((640, 448), (1920, 1080), &s), [240.0, 0.0, 1440.0, 1080.0]);
        let s = OutputSettings { mode: ScaleMode::Fit, viewport: Some([100.0, 50.0, 400.0, 400.0]), ..Default::default() };
        assert_eq!(output_rect((320, 240), (1920, 1080), &s), [100.0, 100.0, 400.0, 300.0]);
    }
}

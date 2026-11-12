//! Backend and device selection.
//!
//! Provenance: the backend enum, the WebGL2/GLES3-level limits and the logging error handler
//! follow revc-rw's `Gpu` (the user's own engine-neutral code); the options struct is new.

/// The graphics API to render with.
///
/// Every variant exists on every platform so settings files and menus stay portable;
/// [`Backend::available`] says which ones this build can actually use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    Vulkan,
    /// OpenGL (GLES 3 / GL 3.3 class, through EGL or WGL).
    Gl,
    /// Direct3D 12 (Windows only).
    Dx12,
}

impl Backend {
    pub const ALL: [Backend; 3] = [Backend::Vulkan, Backend::Gl, Backend::Dx12];

    /// wgpu's backend bit for this API.
    pub fn wgpu(self) -> wgpu::Backends {
        match self {
            Backend::Vulkan => wgpu::Backends::VULKAN,
            Backend::Gl => wgpu::Backends::GL,
            Backend::Dx12 => wgpu::Backends::DX12,
        }
    }

    /// Whether this build of wgpu has the backend compiled in (DX12 only on Windows).
    pub fn compiled_in(self) -> bool {
        match self {
            Backend::Dx12 => cfg!(windows),
            _ => cfg!(not(target_arch = "wasm32")),
        }
    }

    /// The backends compiled into this build, in order of preference.
    pub fn available() -> Vec<Backend> {
        let order = if cfg!(windows) { [Backend::Dx12, Backend::Vulkan, Backend::Gl] } else { [Backend::Vulkan, Backend::Gl, Backend::Dx12] };
        order.into_iter().filter(|b| b.compiled_in()).collect()
    }

    /// Parses "vulkan"/"vk", "gl"/"opengl"/"gles", "dx12"/"d3d12" (case-insensitive).
    pub fn parse(name: &str) -> Option<Backend> {
        match name.to_ascii_lowercase().as_str() {
            "vulkan" | "vk" => Some(Backend::Vulkan),
            "gl" | "opengl" | "gles" => Some(Backend::Gl),
            "dx12" | "d3d12" => Some(Backend::Dx12),
            _ => None,
        }
    }
}

/// How the device is created.
#[derive(Clone, Debug)]
pub struct GpuOptions {
    /// Device label (shows up in debuggers and validation messages).
    pub label: &'static str,
    pub power: wgpu::PowerPreference,
    /// Keep BC1-3 textures compressed on GPUs that sample them (else they are decoded on the CPU).
    pub bc_textures: bool,
    /// Ask for the adapter's own anisotropy/MSAA format features (2x/8x MSAA).
    pub adapter_format_features: bool,
    /// Log validation errors instead of panicking (wgpu's default panics).
    pub log_errors: bool,
}

impl Default for GpuOptions {
    fn default() -> GpuOptions {
        GpuOptions { label: "retro-wgpu", power: wgpu::PowerPreference::HighPerformance, bc_textures: true, adapter_format_features: true, log_errors: true }
    }
}

/// An adapter, its device and queue.
pub struct Gpu {
    pub backend: Backend,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}

impl Gpu {
    /// A device without a window (tests, offscreen rendering, tools).
    pub fn headless(backend: Backend) -> Result<Gpu, String> {
        Gpu::headless_with(backend, &GpuOptions::default())
    }

    pub fn headless_with(backend: Backend, opts: &GpuOptions) -> Result<Gpu, String> {
        if !backend.compiled_in() {
            return Err(format!("{backend:?} is not available in this build"));
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: backend.wgpu(), ..wgpu::InstanceDescriptor::new_without_display_handle() });
        Gpu::create(instance, backend, None, opts)
    }

    /// Adapter and device on `instance`, able to present to `surface` if given.
    pub fn create(instance: wgpu::Instance, backend: Backend, surface: Option<&wgpu::Surface<'_>>, opts: &GpuOptions) -> Result<Gpu, String> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: opts.power,
            compatible_surface: surface,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no {backend:?} adapter: {e}"))?;
        let info = adapter.get_info();
        // GLES 3 level limits on every backend, so all of them run the same code paths.
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let mut features = wgpu::Features::empty();
        if opts.bc_textures {
            features |= adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        }
        if opts.adapter_format_features {
            features |= adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some(opts.label), required_limits: limits, required_features: features, ..Default::default() }))
            .map_err(|e| format!("{backend:?} device: {e}"))?;
        if opts.log_errors {
            let label = opts.label;
            device.on_uncaptured_error(std::sync::Arc::new(move |e: wgpu::Error| {
                eprintln!("{label}: wgpu error (frame continues): {e}");
            }));
        }
        Ok(Gpu { backend, instance, adapter, device, queue, info })
    }

    /// Whether BC1-3 textures stay compressed on this device.
    pub fn bc_textures(&self) -> bool {
        self.device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    }

    /// The largest MSAA sample count up to `wanted` that `color` and `depth` support together
    /// (1, 2, 4, 8, 16). Without adapter-specific format features only 1 and 4 are allowed.
    pub fn supported_samples(&self, color: wgpu::TextureFormat, depth: wgpu::TextureFormat, wanted: u32) -> u32 {
        let c = self.adapter.get_texture_format_features(color).flags;
        let d = self.adapter.get_texture_format_features(depth).flags;
        let specific = self.device.features().contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        let ok = |n: u32| if specific { c.sample_count_supported(n) && d.sample_count_supported(n) } else { n == 1 || n == 4 };
        let mut n = wanted.clamp(1, 16).next_power_of_two();
        if n > wanted.max(1) {
            n /= 2;
        }
        while n > 1 && !ok(n) {
            n /= 2;
        }
        n
    }

    /// The largest anisotropy (1..=16) samplers may ask for.
    pub fn max_anisotropy(&self) -> u16 {
        if self.adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING) { 16 } else { 1 }
    }
}

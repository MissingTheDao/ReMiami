//! Presenting to a window the host created (SDL3, winit, a game's own Win32 window...).
//!
//! The host passes raw window-system handles through [`WindowHandles`] (raw-window-handle 0.6 as
//! re-exported by wgpu), so no windowing library is a dependency. Wayland, X11 (Xlib/XCB) and
//! Win32 constructors are provided; anything else can be passed as raw handles directly.
//!
//! Provenance: the surface set-up (own instance with the display handle for GL on Wayland,
//! 8-bit non-sRGB format choice, the GL vsync driver switches, reconfigure-and-retry on a stale
//! surface) follows revc-rw's `Presenter` (the user's own engine-neutral code).

use crate::gpu::{Backend, Gpu, GpuOptions};
use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::ptr::NonNull;
use wgpu::rwh;

/// The raw handles of a window and its display.
#[derive(Clone, Copy, Debug)]
pub struct WindowHandles {
    pub display: rwh::RawDisplayHandle,
    pub window: rwh::RawWindowHandle,
}

impl WindowHandles {
    /// Wayland: `wl_display*` and `wl_surface*` (SDL_PROP_WINDOW_WAYLAND_DISPLAY/SURFACE_POINTER).
    pub fn wayland(display: NonNull<c_void>, surface: NonNull<c_void>) -> WindowHandles {
        WindowHandles {
            display: rwh::RawDisplayHandle::Wayland(rwh::WaylandDisplayHandle::new(display)),
            window: rwh::RawWindowHandle::Wayland(rwh::WaylandWindowHandle::new(surface)),
        }
    }

    /// X11 through Xlib: `Display*` (None = open the default), screen and `Window`.
    pub fn xlib(display: Option<NonNull<c_void>>, screen: i32, window: u64) -> WindowHandles {
        WindowHandles {
            display: rwh::RawDisplayHandle::Xlib(rwh::XlibDisplayHandle::new(display, screen)),
            window: rwh::RawWindowHandle::Xlib(rwh::XlibWindowHandle::new(window as _)),
        }
    }

    /// Win32: the `HWND` and optionally its `HINSTANCE` (SDL_PROP_WINDOW_WIN32_HWND_POINTER /
    /// _INSTANCE_POINTER, or a game's own window).
    pub fn win32(hwnd: NonZeroIsize, hinstance: Option<NonZeroIsize>) -> WindowHandles {
        let mut w = rwh::Win32WindowHandle::new(hwnd);
        w.hinstance = hinstance;
        WindowHandles { display: rwh::RawDisplayHandle::Windows(rwh::WindowsDisplayHandle::new()), window: rwh::RawWindowHandle::Win32(w) }
    }

    /// From anything that implements raw-window-handle's traits (winit, sdl3 crate windows...).
    pub fn from_window(w: &(impl rwh::HasWindowHandle + rwh::HasDisplayHandle)) -> Result<WindowHandles, String> {
        Ok(WindowHandles {
            display: w.display_handle().map_err(|e| e.to_string())?.as_raw(),
            window: w.window_handle().map_err(|e| e.to_string())?.as_raw(),
        })
    }
}

/// Owned display handle for the instance descriptor; the host keeps the display alive.
#[derive(Debug)]
struct Display(rwh::RawDisplayHandle);
unsafe impl Send for Display {}
unsafe impl Sync for Display {}
impl rwh::HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        Ok(unsafe { rwh::DisplayHandle::borrow_raw(self.0) })
    }
}

/// wgpu's GL backend presents with FIFO only on Linux, so "vsync off" still waits for vblank;
/// the drivers' own switches (Mesa `vblank_mode`, NVIDIA `__GL_SYNC_TO_VBLANK`) turn the swap
/// interval off. They are read when the driver loads; a value the user set wins.
fn no_gl_vsync() {
    if !cfg!(unix) {
        return;
    }
    for (k, v) in [("vblank_mode", "0"), ("__GL_SYNC_TO_VBLANK", "0")] {
        if std::env::var_os(k).is_none() {
            // SAFETY: called while the renderer starts, before the GL driver or other threads
            // read the environment (documented requirement of `Presenter::new`).
            unsafe { std::env::set_var(k, v) };
        }
    }
}

/// A window surface with its device.
pub struct Presenter {
    pub gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl Presenter {
    /// # Safety
    /// The window and display must outlive the presenter. With `Backend::Gl` and `vsync` off
    /// this sets environment variables, so call it before other threads start.
    pub unsafe fn new(backend: Backend, window: WindowHandles, width: u32, height: u32, vsync: bool, opts: &GpuOptions) -> Result<Presenter, String> {
        if !backend.compiled_in() {
            return Err(format!("{backend:?} is not available in this build"));
        }
        if backend == Backend::Gl && !vsync {
            no_gl_vsync();
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend.wgpu(),
            display: Some(Box::new(Display(window.display))),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle: Some(window.display), raw_window_handle: window.window }) }
            .map_err(|e| format!("{backend:?} surface: {e}"))?;
        let gpu = Gpu::create(instance, backend, Some(&surface), opts)?;
        let mut config = surface.get_default_config(&gpu.adapter, width.max(1), height.max(1)).ok_or_else(|| format!("{backend:?}: surface not supported by the adapter"))?;
        // Old games compute colours in gamma space: a non-sRGB 8-bit surface shows them as is.
        let formats = surface.get_capabilities(&gpu.adapter).formats;
        if let Some(f) = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm].into_iter().find(|f| formats.contains(f)).or_else(|| formats.iter().copied().find(|f| !f.is_srgb())) {
            config.format = f;
        }
        config.present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        surface.configure(&gpu.device, &config);
        Ok(Presenter { gpu, surface, config })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.gpu.device, &self.config);
    }

    /// The next swapchain image; reconfigures and retries once if the surface went stale.
    pub fn acquire(&mut self) -> Result<wgpu::SurfaceTexture, String> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Ok(t),
            _ => {
                self.surface.configure(&self.gpu.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Ok(t),
                    other => Err(format!("no surface texture: {other:?}")),
                }
            }
        }
    }

    /// Encodes the recorded frame of `renderer` onto the next swapchain image and presents it.
    /// With `capture`, also returns the frame (RGBA8, top row first) as it went into the output
    /// stage.
    pub fn present(&mut self, renderer: &mut crate::Renderer, capture: bool) -> Result<Option<crate::Image>, String> {
        let frame = self.acquire()?;
        let size = (frame.texture.width(), frame.texture.height());
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        renderer.encode(&self.gpu, &mut enc, &view, size);
        let rb = if capture { renderer.final_image().map(|t| crate::readback::readback_start(&self.gpu, &mut enc, t)) } else { None };
        renderer.end_frame();
        self.gpu.queue.submit([enc.finish()]);
        let shot = match rb {
            Some(r) => Some(crate::readback::readback_finish(&self.gpu, r)?),
            None => None,
        };
        self.gpu.queue.present(frame);
        Ok(shot)
    }
}

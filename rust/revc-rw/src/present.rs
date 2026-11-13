//! Presenting to a window the host created (SDL3 in the game; no GL context needed): a wgpu
//! surface on the window's native Wayland or X11 handles, on Vulkan or OpenGL. On Wayland the GL
//! backend (EGL) needs the display handle when the instance is created, so the presenter creates
//! its own instance.

use crate::frame::{DrawFrames, Frame, Renderer, readback_finish, readback_start};
use crate::{Backend, Gpu};
use std::ffi::c_void;
use std::ptr::NonNull;
use wgpu::rwh;

/// Native handles of a window, as SDL3 reports them (SDL_PROP_WINDOW_WAYLAND_* / X11_*).
#[derive(Clone, Copy, Debug)]
pub enum NativeWindow {
    Wayland { display: NonNull<c_void>, surface: NonNull<c_void> },
    X11 { display: Option<NonNull<c_void>>, screen: i32, window: u64 },
}

impl NativeWindow {
    fn raw(self) -> (rwh::RawDisplayHandle, rwh::RawWindowHandle) {
        match self {
            NativeWindow::Wayland { display, surface } => (
                rwh::RawDisplayHandle::Wayland(rwh::WaylandDisplayHandle::new(display)),
                rwh::RawWindowHandle::Wayland(rwh::WaylandWindowHandle::new(surface)),
            ),
            NativeWindow::X11 { display, screen, window } => (
                rwh::RawDisplayHandle::Xlib(rwh::XlibDisplayHandle::new(display, screen)),
                rwh::RawWindowHandle::Xlib(rwh::XlibWindowHandle::new(window as _)),
            ),
        }
    }
}

/// The display handle in the form wgpu's instance descriptor takes. The host keeps the display
/// open for as long as the presenter lives.
#[derive(Debug)]
struct Display(rwh::RawDisplayHandle);
unsafe impl Send for Display {}
unsafe impl Sync for Display {}
impl rwh::HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        Ok(unsafe { rwh::DisplayHandle::borrow_raw(self.0) })
    }
}

pub struct Presenter {
    pub gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

/// wgpu's GL backend presents with FIFO only on Linux (wgpu-hal 30: immediate is a TODO there), so
/// the game's VSync off would still wait for vertical blank. The drivers' own switches turn the
/// swap interval off instead: Mesa `vblank_mode=0`, NVIDIA `__GL_SYNC_TO_VBLANK=0`. They are read
/// when the GL driver loads, so on GL a VSync change applies from the next start; a value the user
/// set already wins.
fn no_gl_vsync() {
    for (k, v) in [("vblank_mode", "0"), ("__GL_SYNC_TO_VBLANK", "0")] {
        if std::env::var_os(k).is_none() {
            // SAFETY: called while the renderer starts, before the GL driver (and any thread
            // reading the environment) exists.
            unsafe { std::env::set_var(k, v) };
        }
    }
}

impl Presenter {
    /// # Safety
    /// The window (and its display) must outlive the presenter.
    pub unsafe fn new(backend: Backend, window: NativeWindow, width: u32, height: u32, vsync: bool) -> Result<Presenter, String> {
        let (raw_display, raw_window) = window.raw();
        if backend == Backend::Gl && !vsync {
            no_gl_vsync();
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend.wgpu(),
            display: Some(Box::new(Display(raw_display))),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle: Some(raw_display), raw_window_handle: raw_window })
        }
        .map_err(|e| format!("{backend:?} surface: {e}"))?;
        let gpu = Gpu::create(&instance, backend, Some(&surface))?;
        let mut config = surface.get_default_config(&gpu.adapter, width.max(1), height.max(1)).ok_or_else(|| format!("{backend:?}: surface not supported by the adapter"))?;
        // The game computes colours in gamma space (as the GL3 renderer outputs them): a non-sRGB
        // surface keeps them as they are instead of brightening everything.
        // 8-bit like the GL3 renderer's output (and the screenshot tests), else any non-sRGB format.
        let formats = surface.get_capabilities(&gpu.adapter).formats;
        let pick = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| formats.contains(f))
            .or_else(|| formats.iter().copied().find(|f| !f.is_srgb()));
        if let Some(f) = pick {
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

    /// Waits for vertical blank on present (the game's VSync setting) or not; reconfigures the
    /// surface only when this changes.
    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    /// After the window changed size.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.gpu.device, &self.config);
    }

    /// The next frame's texture; reconfigures and retries once if the surface went stale.
    fn acquire(&mut self) -> Result<wgpu::SurfaceTexture, String> {
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

    /// Draws the frame `renderer` recorded and presents it. With `capture`, also returns the
    /// frame as RGBA8 (top row first, alpha forced opaque like the GL3 capture).
    pub fn present_frame(&mut self, renderer: &mut Renderer, capture: bool) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
        let mut frame = renderer.front.finish();
        let shot = self.present(&mut renderer.back, &mut frame, capture);
        renderer.front.recycle(frame);
        shot
    }

    /// Draws a recorded `frame` with the back end and presents it (the back end's half of
    /// `present_frame`, usable on a render thread).
    pub fn present(&mut self, back: &mut dyn DrawFrames, rec: &mut Frame, capture: bool) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
        let frame = match self.acquire() {
            Ok(f) => f,
            Err(e) => {
                back.finish_frame(rec);
                return Err(e);
            }
        };
        let size = (frame.texture.width(), frame.texture.height());
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        back.draw(&self.gpu, &mut enc, &view, size, rec);
        // Screenshots read the scene texture (RGBA8) the frame was drawn into.
        let readback = if capture { back.shown_image().map(|t| (readback_start(&self.gpu, &mut enc, t, (t.width(), t.height())), (t.width(), t.height()))) } else { None };
        self.gpu.queue.submit([enc.finish()]);
        back.finish_frame(rec);
        let shot = match readback {
            Some((r, shot_size)) => {
                let mut px = readback_finish(&self.gpu, r)?;
                px.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
                // REVC_TEST_SHOT_POSTFX=1: the shot as presented, with the output stage's colour
                // transform applied the same way as the final blit (frame.rs fs_xbox).
                if std::env::var_os("REVC_TEST_SHOT_POSTFX").is_some()
                    && crate::frame::OUTPUT_TRANSFORM.load(std::sync::atomic::Ordering::Relaxed) == 1
                {
                    let lut: Vec<u8> = (0..256).map(|i| ((i as f32 / 255.0).powf(1.0 / 1.1) * 255.0 + 0.5) as u8).collect();
                    px.as_chunks_mut::<4>().0.iter_mut().for_each(|p| for c in &mut p[..3] { *c = lut[*c as usize] });
                }
                Some((shot_size.0, shot_size.1, px))
            }
            None => None,
        };
        self.gpu.queue.present(frame);
        Ok(shot)
    }

    /// One blank frame in `rgba` (linear 0..1), presented.
    pub fn clear_and_present(&mut self, rgba: [f64; 4]) -> Result<(), String> {
        let frame = self.acquire()?;
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        self.gpu.queue.submit([enc.finish()]);
        self.gpu.queue.present(frame);
        Ok(())
    }
}

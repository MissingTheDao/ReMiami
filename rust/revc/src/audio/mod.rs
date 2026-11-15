//! Audio output through SDL3. Mixes the game's sound-effect voices (the C++ `CChannel` class,
//! cpp/re3-miami/src/audio/oal/channel_rust.cpp) and its music/radio/cutscene streams
//! (oal/stream.cpp via the OpenAL-subset in oal/rust_al.h, implemented by `revc_audio::al`).
//!
//! The mixer state is shared between the game thread (the `revc_sfx_*` setters) and SDL's
//! audio thread (the stream callback), behind a mutex. Mixing itself is in the `revc-audio` crate.
//!
//! The audio engine is now behind the `AudioBackend` trait defined in `backend.rs`.
//! `RevcBackend` implements today's revc-audio mixer and streams. A second backend and a
//! shadow mode for the regression harness will be added later.

mod backend;
mod retro;
#[cfg(test)]
mod tonelist;
pub use backend::{AudioBackend, RevcBackend};
pub use retro::RetroBackend;

/// REVC_AUDIO_LAWS=eax3: today's engine follows the original's EAX-path laws measured in audio
/// step 4 (mute beyond max distance, the measured pan curve); default (unset / `classic`): the
/// laws it was calibrated with. The retro back end always uses the measured laws.
pub(crate) fn eax_laws() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("REVC_AUDIO_LAWS").is_ok_and(|v| v.eq_ignore_ascii_case("eax3")))
}

use revc_audio::mixer::OUTPUT_RATE;
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_float, c_int, c_uint, c_void};
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};

/// The audio engine(s): the active back end and an optional shadow. Created on first use: streams
/// can exist before the output device opens.
static BACKEND: Mutex<Option<Dispatch>> = Mutex::new(None);
static STREAM: AtomicPtr<SDL_AudioStream> = AtomicPtr::new(null_mut());

/// When set, every mixed output buffer is appended here (stereo f32 at OUTPUT_RATE).
static RECORDING: Mutex<Option<Vec<f32>>> = Mutex::new(None);

/// When set and a shadow backend exists, the shadow's mixed output buffer is appended here.
static RECORDING_SHADOW: Mutex<Option<Vec<f32>>> = Mutex::new(None);

/// Starts capturing the final mixed output (regression harness, A/B audio comparison).
pub fn start_recording() {
    *RECORDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    *RECORDING_SHADOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
}

/// Writes a 32-bit float stereo WAV file from the given f32 data. Returns the number of frames written.
fn write_wav(path: &std::path::Path, data: &[f32]) -> std::io::Result<usize> {
    let bytes = (data.len() * 4) as u32;
    let mut w = Vec::with_capacity(44 + bytes as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    w.extend_from_slice(&(CHANNELS as u16).to_le_bytes());
    w.extend_from_slice(&OUTPUT_RATE.to_le_bytes());
    w.extend_from_slice(&(OUTPUT_RATE * BYTES_PER_FRAME as u32).to_le_bytes());
    w.extend_from_slice(&(BYTES_PER_FRAME as u16).to_le_bytes());
    w.extend_from_slice(&32u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&bytes.to_le_bytes());
    for s in data {
        w.extend_from_slice(&s.to_ne_bytes());
    }
    std::fs::write(path, w)?;
    Ok(data.len() / CHANNELS)
}

/// Stops capturing and writes a 32-bit float stereo WAV. Returns the number of frames written.
pub fn finish_recording(path: &std::path::Path) -> std::io::Result<usize> {
    let data = RECORDING.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    write_wav(path, &data)
}

/// Writes the shadow recording buffer to a WAV file. Returns the number of frames written.
pub fn finish_recording_shadow(path: &std::path::Path) -> std::io::Result<usize> {
    let data = RECORDING_SHADOW.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    write_wav(path, &data)
}

thread_local! {
    /// Reused output buffer on the audio thread.
    static SCRATCH: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    /// The shadow back end's output for the same callback (only when a shadow exists).
    static SCRATCH_SHADOW: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

const CHANNELS: usize = 2;
const BYTES_PER_FRAME: usize = CHANNELS * std::mem::size_of::<f32>();

/// Dispatches calls to the active backend, and optionally to a shadow backend.
struct Dispatch {
    active: Box<dyn AudioBackend>,
    shadow: Option<Box<dyn AudioBackend>>,
    /// Maps game-visible source IDs to shadow source IDs.
    source_map: HashMap<u32, u32>,
    /// Maps game-visible buffer IDs to shadow buffer IDs.
    buffer_map: HashMap<u32, u32>,
}

impl Dispatch {
    /// Creates a new Dispatch based on environment variables.
    fn new() -> Self {
        let name = std::env::var("REVC_AUDIO_BACKEND").unwrap_or_else(|_| "revc".into());
        let active = Self::backend_from_name(&name).unwrap_or_else(|| {
            eprintln!("revc: audio: back end {name} not known, using revc");
            Box::new(RevcBackend::new())
        });

        // REVC_AUDIO_SHADOW=<name> runs that back end in shadow (1 = revc, the harness check)
        let shadow = match std::env::var("REVC_AUDIO_SHADOW").as_deref() {
            Ok("1") => Some(Box::new(RevcBackend::new()) as Box<dyn AudioBackend>),
            Ok(n) if !n.is_empty() => Self::backend_from_name(n).or_else(|| {
                eprintln!("revc: audio: shadow back end {n} not known, none");
                None
            }),
            _ => None,
        };

        Self {
            active,
            shadow,
            source_map: HashMap::new(),
            buffer_map: HashMap::new(),
        }
    }

    /// Looks up a backend by name. Returns None if unknown.
    fn backend_from_name(name: &str) -> Option<Box<dyn AudioBackend>> {
        match name {
            "revc" => Some(Box::new(RevcBackend::new())),
            "retro" => Some(Box::new(RetroBackend::new())),
            _ => None,
        }
    }

    /// Runs `f` on the active back end, then on the shadow if present. For calls whose ids need no
    /// translation (sound-effect channel indices, the reverb preset).
    fn both(&mut self, f: impl Fn(&mut dyn AudioBackend)) {
        f(self.active.as_mut());
        if let Some(shadow) = self.shadow.as_deref_mut() {
            f(shadow);
        }
    }

    /// Handles `al_gen_sources`: generates IDs for active, maps them for shadow, and returns active IDs.
    fn al_gen_sources(&mut self, n: usize) -> Vec<u32> {
        let active_ids = self.active.al_gen_sources(n);
        let shadow_ids = if let Some(ref mut shadow) = self.shadow {
            shadow.al_gen_sources(n)
        } else {
            Vec::new()
        };

        for (&aid, &sid) in active_ids.iter().zip(shadow_ids.iter()) {
            self.source_map.insert(aid, sid);
        }
        active_ids
    }

    /// Handles `al_delete_sources`: deletes from active, translates IDs and deletes from shadow.
    fn al_delete_sources(&mut self, ids: &[u32]) {
        for &id in ids {
            self.active.al_delete_source(id);
            if let Some(ref mut shadow) = self.shadow {
                if let Some(&sid) = self.source_map.get(&id) {
                    shadow.al_delete_source(sid);
                    self.source_map.remove(&id);
                }
            }
        }
    }

    /// Handles `al_gen_buffers`: generates IDs for active, maps them for shadow, and returns active IDs.
    fn al_gen_buffers(&mut self, n: usize) -> Vec<u32> {
        let active_ids = self.active.al_gen_buffers(n);
        let shadow_ids = if let Some(ref mut shadow) = self.shadow {
            shadow.al_gen_buffers(n)
        } else {
            Vec::new()
        };

        for (&aid, &sid) in active_ids.iter().zip(shadow_ids.iter()) {
            self.buffer_map.insert(aid, sid);
        }
        active_ids
    }

    /// Handles `al_delete_buffers`: deletes from active, translates IDs and deletes from shadow.
    fn al_delete_buffers(&mut self, ids: &[u32]) {
        for &id in ids {
            self.active.al_delete_buffer(id);
            if let Some(ref mut shadow) = self.shadow {
                if let Some(&sid) = self.buffer_map.get(&id) {
                    shadow.al_delete_buffer(sid);
                    self.buffer_map.remove(&id);
                }
            }
        }
    }

    /// Handles `al_buffer_data`: translates buffer ID for shadow.
    fn al_buffer_data(&mut self, id: u32, samples: &[i16], freq: u32) {
        self.active.al_buffer_data(id, samples, freq);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&sid) = self.buffer_map.get(&id) {
                shadow.al_buffer_data(sid, samples, freq);
            }
        }
    }

    /// Handles `al_queue_buffer`: translates src and buf IDs for shadow.
    fn al_queue_buffer(&mut self, src: u32, buf: u32) {
        self.active.al_queue_buffer(src, buf);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                if let Some(&sbid) = self.buffer_map.get(&buf) {
                    shadow.al_queue_buffer(ssid, sbid);
                }
            }
        }
    }

    /// Handles `al_unqueue_buffer`: returns active result, calls shadow (ignores result).
    fn al_unqueue_buffer(&mut self, src: u32) -> Option<u32> {
        let res = self.active.al_unqueue_buffer(src);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                let _ = shadow.al_unqueue_buffer(ssid);
            }
        }
        res
    }

    /// Handles `al_play`: translates src ID for shadow.
    fn al_play(&mut self, src: u32) {
        self.active.al_play(src);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_play(ssid);
            }
        }
    }

    /// Handles `al_pause`: translates src ID for shadow.
    fn al_pause(&mut self, src: u32) {
        self.active.al_pause(src);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_pause(ssid);
            }
        }
    }

    /// Handles `al_stop`: translates src ID for shadow.
    fn al_stop(&mut self, src: u32) {
        self.active.al_stop(src);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_stop(ssid);
            }
        }
    }

    /// Handles `al_set_relative`: translates src ID for shadow.
    fn al_set_relative(&mut self, src: u32, relative: bool) {
        self.active.al_set_relative(src, relative);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_set_relative(ssid, relative);
            }
        }
    }

    /// Handles `al_set_gain`: translates src ID for shadow.
    fn al_set_gain(&mut self, src: u32, gain: f32) {
        self.active.al_set_gain(src, gain);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_set_gain(ssid, gain);
            }
        }
    }

    /// Handles `al_set_pitch`: translates src ID for shadow.
    fn al_set_pitch(&mut self, src: u32, pitch: f32) {
        self.active.al_set_pitch(src, pitch);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_set_pitch(ssid, pitch);
            }
        }
    }

    /// Handles `al_set_rolloff`: translates src ID for shadow.
    fn al_set_rolloff(&mut self, src: u32, rolloff: f32) {
        self.active.al_set_rolloff(src, rolloff);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_set_rolloff(ssid, rolloff);
            }
        }
    }

    /// Handles `al_set_position`: translates src ID for shadow.
    fn al_set_position(&mut self, src: u32, x: f32, y: f32, z: f32) {
        self.active.al_set_position(src, x, y, z);
        if let Some(ref mut shadow) = self.shadow {
            if let Some(&ssid) = self.source_map.get(&src) {
                shadow.al_set_position(ssid, x, y, z);
            }
        }
    }

    /// Renders the active back end into `out`; the shadow, if any, into `shadow_buf` (zeroed, same length).
    fn render(&mut self, out: &mut [f32], shadow_buf: &mut [f32]) {
        self.active.render(out);
        if let Some(ref mut shadow) = self.shadow {
            shadow.render(shadow_buf);
        }
    }

    /// Takes stats from active backend.
    fn take_stats(&mut self) -> Option<revc_audio::mixer::Stats> {
        self.active.take_stats()
    }

    /// Takes stream frames from active backend.
    fn take_stream_frames(&mut self) -> Option<u64> {
        self.active.take_stream_frames()
    }

    /// Takes stream frames from shadow backend.
    fn take_shadow_stream_frames(&mut self) -> Option<u64> {
        self.shadow.as_mut().and_then(|s| s.take_stream_frames())
    }
}

/// SDL asks for `additional` more bytes of stereo f32 at OUTPUT_RATE.
unsafe extern "C" fn feed(_: *mut c_void, stream: *mut SDL_AudioStream, additional: c_int, _total: c_int) {
    if additional <= 0 {
        return;
    }
    let frames = additional as usize / BYTES_PER_FRAME + 1;
    SCRATCH.with(|s| {
        SCRATCH_SHADOW.with(|sh| {
            let mut buf = s.borrow_mut();
            buf.clear();
            buf.resize(frames * CHANNELS, 0.0);
            let mut sbuf = sh.borrow_mut();
            sbuf.clear();
            // Streams first (added), then sound effects (added, then the result is clamped); the
            // shadow the same way into its own buffer, in the same lock.
            let has_shadow = match BACKEND.lock() {
                Ok(mut dispatch) => match dispatch.as_mut() {
                    Some(d) => {
                        if d.shadow.is_some() {
                            sbuf.resize(buf.len(), 0.0);
                        }
                        d.render(&mut buf, &mut sbuf);
                        d.shadow.is_some()
                    }
                    None => false,
                },
                Err(_) => return, // a panic elsewhere poisoned the lock; output silence
            };
            if let Ok(mut rec) = RECORDING.lock() {
                if let Some(rec) = rec.as_mut() {
                    rec.extend_from_slice(&buf);
                }
            }
            if has_shadow {
                if let Ok(mut rec) = RECORDING_SHADOW.lock() {
                    if let Some(rec) = rec.as_mut() {
                        rec.extend_from_slice(&sbuf);
                    }
                }
            }
            unsafe {
                SDL_PutAudioStreamData(stream, buf.as_ptr().cast(), (buf.len() * std::mem::size_of::<f32>()) as c_int);
            }
        })
    });
}

fn with_backend<R>(f: impl FnOnce(&mut Dispatch) -> R) -> R {
    let mut backend = BACKEND.lock().unwrap_or_else(|e| e.into_inner());
    if backend.is_none() {
        *backend = Some(Dispatch::new());
    }
    f(backend.as_mut().unwrap())
}

/// Opens the SDL playback device if it isn't open yet. Both sound effects and streams need it,
/// and streams create their sources before the sound-effect channels are opened.
/// It stays open until `close_device()` (host shutdown).
fn ensure_device() -> bool {
    if !STREAM.load(Ordering::Acquire).is_null() {
        return true;
    }
    unsafe {
        if !SDL_InitSubSystem(SDL_INIT_AUDIO) {
            eprintln!("revc: SDL audio init failed: {}", crate::host::sdl_error());
            return false;
        }
    }
    let spec = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: CHANNELS as c_int, freq: OUTPUT_RATE as c_int };
    let stream = unsafe { SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, Some(feed), null_mut()) };
    if stream.is_null() {
        eprintln!("revc: cannot open audio device: {}", crate::host::sdl_error());
        return false;
    }
    STREAM.store(stream, Ordering::Release);
    unsafe { SDL_ResumeAudioStreamDevice(stream) };
    // what the device actually runs at once opened (SDL converts our fixed 48 kHz stream to it:
    // a different rate means an extra resampling stage); the latency measurements record it
    unsafe {
        let mut dev = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: 0, freq: 0 };
        let mut frames: c_int = 0;
        if SDL_GetAudioDeviceFormat(SDL_GetAudioStreamDevice(stream), &mut dev, &mut frames) {
            eprintln!("revc: audio: requested {OUTPUT_RATE} Hz stereo f32; the device runs at {} Hz, {} channels, {frames} frames per period", dev.freq, dev.channels);
        }
    }
    true
}

/// Stops output (the callback) and closes the device. Called from host shutdown.
pub fn close_device() {
    let stream = STREAM.swap(null_mut(), Ordering::AcqRel);
    if !stream.is_null() {
        unsafe { SDL_DestroyAudioStream(stream) };
    }
}

/// Creates the sound-effect mixer with `voices` voices (and opens the device). 0 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_open(voices: c_uint) -> c_int {
    with_backend(|d| d.both(|b| b.sfx_open(voices as usize)));
    if !ensure_device() {
        with_backend(|d| d.both(|b| b.sfx_close()));
        return 0;
    }
    1
}

/// Drops the sound-effect mixer; the device stays open for streams.
#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_close() {
    with_backend(|d| d.both(|b| b.sfx_close()));
}

/// Mixer counters since the last call (None if audio isn't open). For the regression harness.
pub fn take_stats() -> Option<revc_audio::mixer::Stats> {
    BACKEND.lock().ok()?.as_mut().and_then(|d| d.take_stats())
}

/// Stream source-frames mixed since the last call (None if streams aren't on the Rust mixer).
pub fn take_stream_frames() -> Option<u64> {
    BACKEND.lock().ok()?.as_mut().and_then(|d| d.take_stream_frames())
}

/// Shadow stream source-frames mixed since the last call (None if streams aren't on the Rust mixer or no shadow).
pub fn take_shadow_stream_frames() -> Option<u64> {
    BACKEND.lock().ok()?.as_mut().and_then(|d| d.take_shadow_stream_frames())
}

/// The environment reverb, already converted from the game's EAX3 preset (sampman_oal.cpp).
#[unsafe(no_mangle)]
pub extern "C" fn revc_reverb_set(
    density: c_float,
    diffusion: c_float,
    gain: c_float,
    gain_hf: c_float,
    decay_time: c_float,
    decay_hf_ratio: c_float,
    reflections_gain: c_float,
    reflections_delay: c_float,
    late_gain: c_float,
    late_delay: c_float,
) {
    let p = revc_audio::reverb::ReverbParams {
        density,
        diffusion,
        gain,
        gain_hf,
        decay_time,
        decay_hf_ratio,
        reflections_gain,
        reflections_delay,
        late_gain,
        late_delay,
    };
    with_backend(|d| d.both(|b| b.reverb_set(&p)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_2d(id: c_uint, is_2d: c_int) {
    with_backend(|d| d.both(|b| b.sfx_set_2d(id as usize, is_2d != 0)));
}

/// Copies `samples` mono 16-bit samples from `data` and starts the voice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_sfx_start(id: c_uint, data: *const i16, samples: usize, freq: c_int) {
    if data.is_null() || freq <= 0 {
        return;
    }
    // SAFETY: the caller passes a sample buffer of at least `samples` i16 values.
    let data = unsafe { std::slice::from_raw_parts(data, samples) };
    with_backend(|d| d.both(|b| b.sfx_start(id as usize, data, freq as u32)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_stop(id: c_uint) {
    with_backend(|d| d.both(|b| b.sfx_stop(id as usize)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_reset(id: c_uint) {
    with_backend(|d| d.both(|b| b.sfx_reset(id as usize)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_is_playing(id: c_uint) -> c_int {
    let mut playing = false;
    with_backend(|d| playing = d.active.sfx_is_playing(id as usize));
    playing as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_pitch(id: c_uint, pitch: c_float) {
    with_backend(|d| d.both(|b| b.sfx_set_pitch(id as usize, pitch)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_gain(id: c_uint, gain: c_float) {
    with_backend(|d| d.both(|b| b.sfx_set_gain(id as usize, gain)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_position(id: c_uint, x: c_float, y: c_float, z: c_float) {
    with_backend(|d| d.both(|b| b.sfx_set_position(id as usize, x, y, z)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_distances(id: c_uint, max: c_float, min: c_float) {
    with_backend(|d| d.both(|b| b.sfx_set_distances(id as usize, max, min)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_loop_points(id: c_uint, start: c_int, end: c_int) {
    with_backend(|d| d.both(|b| b.sfx_set_loop_points(id as usize, start, end)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_loop_count(id: c_uint, count: c_int) {
    with_backend(|d| d.both(|b| b.sfx_set_loop_count(id as usize, count)));
}

/// Calls that set a non-zero reverb send (diagnostics for the regression harness).
static NONZERO_SENDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn take_nonzero_sends() -> u64 {
    NONZERO_SENDS.swap(0, Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_reverb_mix(id: c_uint, mix: c_float) {
    if mix > 0.0 {
        NONZERO_SENDS.fetch_add(1, Ordering::Relaxed);
    }
    with_backend(|d| d.both(|b| b.sfx_set_reverb_mix(id as usize, mix)));
}

// ---------------------------------------------------------------- OpenAL subset for streams
// Called from oal/stream.cpp and sampman_oal.cpp through the renames in oal/rust_al.h,
// with the same argument types and enum values as OpenAL.

const AL_SOURCE_RELATIVE: c_int = 0x202;
const AL_PITCH: c_int = 0x1003;
const AL_POSITION: c_int = 0x1004;
const AL_GAIN: c_int = 0x100A;
const AL_SOURCE_STATE: c_int = 0x1010;
const AL_INITIAL: c_int = 0x1011;
const AL_PLAYING: c_int = 0x1012;
const AL_PAUSED: c_int = 0x1013;
const AL_STOPPED: c_int = 0x1014;
const AL_BUFFERS_QUEUED: c_int = 0x1015;
const AL_BUFFERS_PROCESSED: c_int = 0x1016;
const AL_ROLLOFF_FACTOR: c_int = 0x1021;
const AL_SAMPLE_OFFSET: c_int = 0x1025;
const AL_BYTE_OFFSET: c_int = 0x1026;
const AL_FORMAT_MONO16: c_int = 0x1101;

/// SAFETY (all below): pointers come from C++ with the element counts OpenAL would require.
unsafe fn ids<'a>(n: c_int, p: *const c_uint) -> &'a [c_uint] {
    if p.is_null() || n <= 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n as usize) } }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGenSources(n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    ensure_device(); // streams may be on Rust while sound effects stay on OpenAL
    let v = with_backend(|d| d.al_gen_sources(n as usize));
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alDeleteSources(n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_backend(|d| d.al_delete_sources(list));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGenBuffers(n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    let v = with_backend(|d| d.al_gen_buffers(n as usize));
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alDeleteBuffers(n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_backend(|d| d.al_delete_buffers(list));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alIsBuffer(id: c_uint) -> u8 {
    with_backend(|d| d.active.al_is_buffer(id)) as u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alBufferData(id: c_uint, format: c_int, data: *const c_void, size: c_int, freq: c_int) {
    if format != AL_FORMAT_MONO16 || data.is_null() || size < 0 || freq <= 0 {
        return; // streams only ever upload mono 16-bit
    }
    let samples = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), size as usize / 2) };
    with_backend(|d| d.al_buffer_data(id, samples, freq as u32));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alSourceQueueBuffers(src: c_uint, n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_backend(|d| {
        for &buf in list {
            d.al_queue_buffer(src, buf);
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alSourceUnqueueBuffers(src: c_uint, n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    with_backend(|d| {
        for i in 0..n as usize {
            // OpenAL would fail the whole call; the game only asks for processed buffers.
            let id = d.al_unqueue_buffer(src).unwrap_or(0);
            unsafe { *out.add(i) = id };
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcePlay(src: c_uint) {
    with_backend(|d| d.al_play(src));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcePause(src: c_uint) {
    with_backend(|d| d.al_pause(src));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourceStop(src: c_uint) {
    with_backend(|d| d.al_stop(src));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGetSourcei(src: c_uint, param: c_int, value: *mut c_int) {
    if value.is_null() { return; }
    let v = with_backend(|d| match param {
        AL_SOURCE_STATE => match d.active.al_state(src) {
            revc_audio::al::State::Initial => AL_INITIAL,
            revc_audio::al::State::Playing => AL_PLAYING,
            revc_audio::al::State::Paused => AL_PAUSED,
            revc_audio::al::State::Stopped => AL_STOPPED,
        },
        AL_BUFFERS_QUEUED => d.active.al_buffers_queued(src),
        AL_BUFFERS_PROCESSED => d.active.al_buffers_processed(src),
        AL_SAMPLE_OFFSET => d.active.al_sample_offset(src),
        AL_BYTE_OFFSET => d.active.al_byte_offset(src),
        _ => 0,
    });
    unsafe { *value = v };
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcei(src: c_uint, param: c_int, value: c_int) {
    if param == AL_SOURCE_RELATIVE {
        with_backend(|d| d.al_set_relative(src, value != 0));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcef(src: c_uint, param: c_int, value: c_float) {
    with_backend(|d| match param {
        AL_GAIN => d.al_set_gain(src, value),
        AL_PITCH => d.al_set_pitch(src, value),
        AL_ROLLOFF_FACTOR => d.al_set_rolloff(src, value),
        _ => {}
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSource3f(src: c_uint, param: c_int, x: c_float, y: c_float, z: c_float) {
    if param == AL_POSITION {
        with_backend(|d| d.al_set_position(src, x, y, z));
    }
}

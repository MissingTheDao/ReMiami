//! Audio output through SDL3. Mixes the game's sound-effect voices (the C++ `CChannel` class,
//! cpp/re3-miami/src/audio/oal/channel_rust.cpp) and its music/radio/cutscene streams
//! (oal/stream.cpp via the OpenAL-subset in oal/rust_al.h, implemented by `revc_audio::al`).
//!
//! The mixer state is shared between the game thread (the `revc_sfx_*` setters) and SDL's
//! audio thread (the stream callback), behind a mutex. Mixing itself is in the `revc-audio` crate.

use revc_audio::al::{AlStreams, State};
use revc_audio::mixer::{Mixer, OUTPUT_RATE};
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::ffi::{c_float, c_int, c_uint, c_void};
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};

static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);
/// Stream sources/buffers exist independently of the output device (the game creates them
/// before opening sound-effect channels), so they live in their own lock.
static STREAMS: Mutex<Option<AlStreams>> = Mutex::new(None);
/// Last reverb preset from the game. It can arrive before the mixer opens, so it's kept here and
/// applied on open.
static REVERB: Mutex<Option<revc_audio::reverb::ReverbParams>> = Mutex::new(None);
static STREAM: AtomicPtr<SDL_AudioStream> = AtomicPtr::new(null_mut());

/// When set, every mixed output buffer is appended here (stereo f32 at OUTPUT_RATE).
static RECORDING: Mutex<Option<Vec<f32>>> = Mutex::new(None);

/// Starts capturing the final mixed output (regression harness, A/B audio comparison).
pub fn start_recording() {
    *RECORDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
}

/// Stops capturing and writes a 32-bit float stereo WAV. Returns the number of frames written.
pub fn finish_recording(path: &std::path::Path) -> std::io::Result<usize> {
    let data = RECORDING.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
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
    for s in &data {
        w.extend_from_slice(&s.to_ne_bytes());
    }
    std::fs::write(path, w)?;
    Ok(data.len() / CHANNELS)
}

thread_local! {
    /// Reused output buffer on the audio thread.
    static SCRATCH: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

const CHANNELS: usize = 2;
const BYTES_PER_FRAME: usize = CHANNELS * std::mem::size_of::<f32>();

/// SDL asks for `additional` more bytes of stereo f32 at OUTPUT_RATE.
unsafe extern "C" fn feed(_: *mut c_void, stream: *mut SDL_AudioStream, additional: c_int, _total: c_int) {
    if additional <= 0 {
        return;
    }
    let frames = additional as usize / BYTES_PER_FRAME + 1;
    SCRATCH.with(|s| {
        let mut buf = s.borrow_mut();
        buf.clear();
        buf.resize(frames * CHANNELS, 0.0);
        // Streams first (added), then sound effects (added, then the result is clamped).
        if let Ok(mut st) = STREAMS.lock() {
            if let Some(st) = st.as_mut() {
                st.mix(&mut buf);
            }
        }
        match MIXER.lock() {
            Ok(mut m) => {
                if let Some(m) = m.as_mut() {
                    m.mix(&mut buf);
                }
            }
            Err(_) => return, // a panic elsewhere poisoned the lock; output silence
        }
        if let Ok(mut rec) = RECORDING.lock() {
            if let Some(rec) = rec.as_mut() {
                rec.extend_from_slice(&buf);
            }
        }
        unsafe {
            SDL_PutAudioStreamData(stream, buf.as_ptr().cast(), (buf.len() * std::mem::size_of::<f32>()) as c_int);
        }
    });
}

fn with_mixer(f: impl FnOnce(&mut Mixer)) {
    if let Ok(mut m) = MIXER.lock() {
        if let Some(m) = m.as_mut() {
            f(m);
        }
    }
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
    let mut mixer = Mixer::new(voices as usize);
    if let Some(p) = *REVERB.lock().unwrap_or_else(|e| e.into_inner()) {
        mixer.set_reverb_params(&p);
    }
    *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = Some(mixer);
    if !ensure_device() {
        *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return 0;
    }
    1
}

/// Drops the sound-effect mixer; the device stays open for streams.
#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_close() {
    *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Mixer counters since the last call (None if audio isn't open). For the regression harness.
pub fn take_stats() -> Option<revc_audio::mixer::Stats> {
    MIXER.lock().ok()?.as_mut().map(|m| m.take_stats())
}

/// Stream source-frames mixed since the last call (None if streams aren't on the Rust mixer).
pub fn take_stream_frames() -> Option<u64> {
    STREAMS.lock().ok()?.as_mut().map(|s| s.take_frames_mixed())
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
    *REVERB.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    with_mixer(|m| m.set_reverb_params(&p));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_2d(id: c_uint, is_2d: c_int) {
    with_mixer(|m| m.set_2d(id as usize, is_2d != 0));
}

/// Copies `samples` mono 16-bit samples from `data` and starts the voice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_sfx_start(id: c_uint, data: *const i16, samples: usize, freq: c_int) {
    if data.is_null() || freq <= 0 {
        return;
    }
    // SAFETY: the caller passes a sample buffer of at least `samples` i16 values.
    let data = unsafe { std::slice::from_raw_parts(data, samples) };
    with_mixer(|m| m.start(id as usize, data, freq as u32));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_stop(id: c_uint) {
    with_mixer(|m| m.stop(id as usize));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_reset(id: c_uint) {
    with_mixer(|m| m.reset(id as usize));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_is_playing(id: c_uint) -> c_int {
    let mut playing = false;
    with_mixer(|m| playing = m.is_playing(id as usize));
    playing as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_pitch(id: c_uint, pitch: c_float) {
    with_mixer(|m| m.set_pitch(id as usize, pitch));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_gain(id: c_uint, gain: c_float) {
    with_mixer(|m| m.set_gain(id as usize, gain));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_position(id: c_uint, x: c_float, y: c_float, z: c_float) {
    with_mixer(|m| m.set_position(id as usize, x, y, z));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_distances(id: c_uint, max: c_float, min: c_float) {
    with_mixer(|m| m.set_distances(id as usize, max, min));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_loop_points(id: c_uint, start: c_int, end: c_int) {
    with_mixer(|m| m.set_loop_points(id as usize, start, end));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_loop_count(id: c_uint, count: c_int) {
    with_mixer(|m| m.set_loop_count(id as usize, count));
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
    with_mixer(|m| m.set_reverb_mix(id as usize, mix));
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

fn with_streams<R>(f: impl FnOnce(&mut AlStreams) -> R) -> R {
    let mut g = STREAMS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(AlStreams::default))
}

/// SAFETY (all below): pointers come from C++ with the element counts OpenAL would require.
unsafe fn ids<'a>(n: c_int, p: *const c_uint) -> &'a [c_uint] {
    if p.is_null() || n <= 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n as usize) } }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGenSources(n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    ensure_device(); // streams may be on Rust while sound effects stay on OpenAL
    let v = with_streams(|a| a.gen_sources(n as usize));
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alDeleteSources(n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_streams(|a| list.iter().for_each(|&id| a.delete_source(id)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGenBuffers(n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    let v = with_streams(|a| a.gen_buffers(n as usize));
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alDeleteBuffers(n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_streams(|a| list.iter().for_each(|&id| a.delete_buffer(id)));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alIsBuffer(id: c_uint) -> u8 {
    with_streams(|a| a.is_buffer(id)) as u8
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alBufferData(id: c_uint, format: c_int, data: *const c_void, size: c_int, freq: c_int) {
    if format != AL_FORMAT_MONO16 || data.is_null() || size < 0 || freq <= 0 {
        return; // streams only ever upload mono 16-bit
    }
    let samples = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), size as usize / 2) };
    with_streams(|a| a.buffer_data(id, samples, freq as u32));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alSourceQueueBuffers(src: c_uint, n: c_int, p: *const c_uint) {
    let list = unsafe { ids(n, p) };
    with_streams(|a| list.iter().for_each(|&b| a.queue_buffer(src, b)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alSourceUnqueueBuffers(src: c_uint, n: c_int, out: *mut c_uint) {
    if out.is_null() || n <= 0 { return; }
    with_streams(|a| {
        for i in 0..n as usize {
            // OpenAL would fail the whole call; the game only asks for processed buffers.
            let id = a.unqueue_buffer(src).unwrap_or(0);
            unsafe { *out.add(i) = id };
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcePlay(src: c_uint) {
    with_streams(|a| a.play(src));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcePause(src: c_uint) {
    with_streams(|a| a.pause(src));
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourceStop(src: c_uint) {
    with_streams(|a| a.stop(src));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_alGetSourcei(src: c_uint, param: c_int, value: *mut c_int) {
    if value.is_null() { return; }
    let v = with_streams(|a| match param {
        AL_SOURCE_STATE => match a.state(src) {
            State::Initial => AL_INITIAL,
            State::Playing => AL_PLAYING,
            State::Paused => AL_PAUSED,
            State::Stopped => AL_STOPPED,
        },
        AL_BUFFERS_QUEUED => a.buffers_queued(src),
        AL_BUFFERS_PROCESSED => a.buffers_processed(src),
        AL_SAMPLE_OFFSET => a.sample_offset(src),
        AL_BYTE_OFFSET => a.byte_offset(src),
        _ => 0,
    });
    unsafe { *value = v };
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcei(src: c_uint, param: c_int, value: c_int) {
    if param == AL_SOURCE_RELATIVE {
        with_streams(|a| a.set_relative(src, value != 0));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSourcef(src: c_uint, param: c_int, value: c_float) {
    with_streams(|a| match param {
        AL_GAIN => a.set_gain(src, value),
        AL_PITCH => a.set_pitch(src, value),
        AL_ROLLOFF_FACTOR => a.set_rolloff(src, value),
        _ => {}
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_alSource3f(src: c_uint, param: c_int, x: c_float, y: c_float, z: c_float) {
    if param == AL_POSITION {
        with_streams(|a| a.set_position(src, x, y, z));
    }
}

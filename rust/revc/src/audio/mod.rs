//! Audio output through SDL3. For now this mixes the game's sound-effect voices (the C++
//! `CChannel` class, cpp/re3-miami/src/audio/oal/channel_rust.cpp); streams still use OpenAL.
//!
//! The mixer state is shared between the game thread (the `revc_sfx_*` setters) and SDL's
//! audio thread (the stream callback), behind a mutex. Mixing itself is in the `revc-audio` crate.

use revc_audio::mixer::{Mixer, OUTPUT_RATE};
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::ffi::{c_float, c_int, c_uint, c_void};
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};

static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);
static STREAM: AtomicPtr<SDL_AudioStream> = AtomicPtr::new(null_mut());

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
        match MIXER.lock() {
            Ok(mut m) => {
                if let Some(m) = m.as_mut() {
                    m.mix(&mut buf);
                }
            }
            Err(_) => return, // a panic elsewhere poisoned the lock; output silence
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

/// Opens the default playback device with `voices` sound-effect voices. Returns 0 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_open(voices: c_uint) -> c_int {
    revc_sfx_close();
    unsafe {
        if !SDL_InitSubSystem(SDL_INIT_AUDIO) {
            eprintln!("revc: SDL audio init failed: {}", crate::host::sdl_error());
            return 0;
        }
    }
    *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Mixer::new(voices as usize));
    let spec = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: CHANNELS as c_int, freq: OUTPUT_RATE as c_int };
    let stream = unsafe { SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, Some(feed), null_mut()) };
    if stream.is_null() {
        eprintln!("revc: cannot open audio device: {}", crate::host::sdl_error());
        *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return 0;
    }
    STREAM.store(stream, Ordering::Release);
    unsafe { SDL_ResumeAudioStreamDevice(stream) };
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_close() {
    let stream = STREAM.swap(null_mut(), Ordering::AcqRel);
    if !stream.is_null() {
        // Stops the callback before the mixer goes away.
        unsafe { SDL_DestroyAudioStream(stream) };
    }
    *MIXER.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Mixer counters since the last call (None if audio isn't open). For the regression harness.
pub fn take_stats() -> Option<revc_audio::mixer::Stats> {
    MIXER.lock().ok()?.as_mut().map(|m| m.take_stats())
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

#[unsafe(no_mangle)]
pub extern "C" fn revc_sfx_set_reverb_mix(id: c_uint, mix: c_float) {
    with_mixer(|m| m.set_reverb_mix(id as usize, mix));
}

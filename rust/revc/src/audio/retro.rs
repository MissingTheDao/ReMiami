use std::sync::Arc;
use retro_audio::{
    Engine, EngineConfig, Mixer, PlayOptions, Source3D, VoiceParams, Volume,
    SoundBuffer, Channels, PanLaw, Listener, Mode3D, DistanceModel,
};
use revc_audio::reverb::ReverbParams;
use retro_audio::reverb::{EfxReverbParams, MillibelListener};

use super::backend::AudioBackend;
use revc_audio::al::{AlStreams, State};
use revc_audio::mixer::Stats;

pub struct RetroBackend {
    engine: Option<Engine>,
    mixer: Option<Mixer>,
    streams: AlStreams,
    // Per-channel state for retro-audio mapping
    channel_state: Vec<ChannelState>,
    scratch: Vec<f32>,
    // Stats tracking
    starts_count: usize,
    max_active_seen: usize,
    peak_sample: f32,
    last_stats: Option<Stats>,
    last_stream_frames: Option<u64>,
    reverb_mixes: Vec<f32>,
    reverb_params: Option<ReverbParams>,
    /// The game's full listener, converted (the reverb's second parameter path).
    listener: Option<EfxReverbParams>,
}

struct ChannelState {
    voice_id: Option<retro_audio::VoiceId>,
    is_2d: bool,
    gain: f32,
    pitch: f32,
    position: [f32; 3],
    min_dist: f32,
    max_dist: f32,
    loop_start: i32,
    loop_end: i32,
    loop_count: i32,
}

/// The original plays 3D sound effects at this level relative to 2D ones (audio step 4: its
/// EAX path renders game positions, which lie in the lateral-up plane of its listener, at
/// 0.4197 * (1 -/+ pan) per side, i.e. the linear law times 0.8394, -1.52 dB).
pub(crate) const LEVEL_3D: f32 = 0.8394;

/// The volume a channel's voice plays at (3D voices at LEVEL_3D).
fn voice_volume(st: &ChannelState) -> f32 {
    if st.is_2d { st.gain } else { st.gain * LEVEL_3D }
}

/// The reverb send for a channel. retro-audio scales the send by the voice volume; the 3D level
/// is a property of the direct path's pan law only, so it is divided out of the send again.
fn send_level(st: &ChannelState, mix: f32) -> f32 {
    if st.is_2d { mix } else { mix / LEVEL_3D }
}

/// The channel's 3D source (the game's positions are listener-relative; inverse distance clamped).
fn source_3d(st: &ChannelState) -> Source3D {
    Source3D {
        position: st.position,
        min_distance: st.min_dist,
        max_distance: st.max_dist,
        mode: Mode3D::HeadRelative,
        distance_model: DistanceModel::InverseClamped,
        room_rolloff_factor: 0.0,
        // the original's DS3D buffers mute beyond max distance (measured in audio step 4)
        mute_at_max_distance: true,
        // air absorption on the direct path as published (factor 1): since retro-audio 0.2.5 the EFX
        // path applies it as a high shelf at the HF reference, so low and mid frequencies stay
        // (reverb item 3c, measured against the open implementation's shape)
        air_absorption_factor: 1.0,
        ..Default::default()
    }
}

impl RetroBackend {
    pub fn new() -> Self {
        Self {
            engine: None,
            mixer: None,
            streams: AlStreams::default(),
            channel_state: Vec::new(),
            scratch: Vec::new(),
            starts_count: 0,
            max_active_seen: 0,
            peak_sample: 0.0,
            last_stats: None,
            last_stream_frames: None,
            reverb_mixes: Vec::new(),
            reverb_params: None,
            listener: None,
        }
    }

    fn ensure_engine(&mut self) {
        if self.engine.is_none() {
            let cfg = EngineConfig {
                sample_rate: revc_audio::mixer::OUTPUT_RATE,
                max_voices: self.channel_state.len(),
                max_buses: 1,
                voice_stealing: false,
                command_capacity: 65536,
                reverb: true,
                pan_law: PanLaw::DirectSound,
                pan_law_3d: PanLaw::Linear,
            };
            let (eng, mix) = Engine::new(cfg);
            self.engine = Some(eng);
            self.mixer = Some(mix);
            // Set default listener orientation (OpenAL default: looking down -Z, up +Y)
            if let Some(ref mut eng) = self.engine {
                let _ = eng.set_listener(Listener {
                    position: [0.0, 0.0, 0.0],
                    velocity: [0.0, 0.0, 0.0],
                    front: [0.0, 0.0, -1.0],
                    top: [0.0, 1.0, 0.0],
                    distance_factor: 1.0,
                    rolloff_factor: 1.0,
                    doppler_factor: 1.0,
                });
            }
            if let (Some(eng), Some(p)) = (&mut self.engine, &self.listener) {
                let _ = eng.set_environment_efx(p);
            }
        }
    }
}

impl AudioBackend for RetroBackend {
    fn sfx_open(&mut self, voices: usize) {
        self.channel_state.clear();
        self.channel_state.resize_with(voices, || ChannelState {
            voice_id: None,
            is_2d: false,
            gain: 1.0,
            pitch: 1.0,
            position: [0.0, 0.0, 0.0],
            min_dist: 1.0,
            max_dist: 100.0,
            loop_start: 0,
            loop_end: 0,
            loop_count: 1,
        });
        self.reverb_mixes.clear();
        self.reverb_mixes.resize_with(voices, || 0.0);
        self.ensure_engine();
        self.starts_count = 0;
        self.max_active_seen = 0;
        self.peak_sample = 0.0;
        self.last_stats = None;
        self.last_stream_frames = None;
    }

    fn sfx_close(&mut self) {
        self.engine = None;
        self.mixer = None;
    }

    fn sfx_is_open(&self) -> bool {
        self.engine.is_some()
    }

    fn reverb_set(&mut self, p: &ReverbParams) {
        self.reverb_params = Some(*p);
    }

    fn reverb_set_listener(&mut self, l: &MillibelListener) {
        let p = EfxReverbParams::from_millibels(l);
        self.listener = Some(p);
        if let Some(eng) = &mut self.engine {
            let _ = eng.set_environment_efx(&p);
        }
    }

    fn sfx_set_2d(&mut self, id: usize, is_2d: bool) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.is_2d = is_2d;
            if let (Some(eng), Some(vid)) = (&mut self.engine, state.voice_id) {
                let _ = eng.set_3d(vid, if is_2d { None } else { Some(source_3d(state)) });
                let _ = eng.set_volume(vid, Volume::linear(voice_volume(state)));
                let mix = self.reverb_mixes.get(id).copied().unwrap_or(0.0);
                let _ = eng.set_send(vid, send_level(state, mix), 1.0);
            }
        }
    }

    fn sfx_start(&mut self, id: usize, data: &[i16], freq: u32) {
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get_mut(id)) {
            // Stop previous voice on this channel
            if let Some(vid) = state.voice_id {
                let _ = eng.stop(vid);
            }

            // The game's loop region: explicit points when valid, else the whole buffer.
            let len = data.len();
            let (ls, le) = (state.loop_start, state.loop_end);
            let (rs, re) = if ls >= 0 && le > ls && le as usize <= len { (ls as usize, le as usize) } else { (0, len) };
            let count = state.loop_count;
            // retro-audio has no loop count: n >= 2 passes are unrolled into the buffer; 0 loops forever.
            let buffer_data: Vec<i16> = if count >= 2 {
                let mut v = Vec::with_capacity(len + (count as usize - 1) * (re - rs));
                v.extend_from_slice(&data[..re]);
                for _ in 1..count {
                    v.extend_from_slice(&data[rs..re]);
                }
                v.extend_from_slice(&data[re..]);
                v
            } else {
                data.to_vec()
            };
            let buf = Arc::new(SoundBuffer::from_i16(&buffer_data, Channels::Mono, freq));
            let params = VoiceParams {
                volume: voice_volume(state),
                pitch: state.pitch,
                looping: count == 0,
                loop_start: if count == 0 { rs } else { 0 },
                loop_end: if count == 0 { re } else { 0 },
                send: send_level(state, self.reverb_mixes.get(id).copied().unwrap_or(0.0)), // the game's per-source send (no extra HF cut on the send)
                ..Default::default()
            };

            let spatial = if state.is_2d {
                None
            } else {
                Some(source_3d(state))
            };

            let opts = PlayOptions {
                params,
                start_frame: 0,
                paused: false,
                spatial,
            };

            match eng.play(&buf, opts) {
                Ok(vid) => {
                    state.voice_id = Some(vid);
                    self.starts_count += 1;
                }
                Err(_) => {
                    state.voice_id = None;
                }
            }
        }
    }

    fn sfx_stop(&mut self, id: usize) {
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get_mut(id)) {
            if let Some(vid) = state.voice_id {
                let _ = eng.stop(vid);
                state.voice_id = None;
            }
        }
    }

    fn sfx_reset(&mut self, id: usize) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.loop_count = 1;
            state.loop_start = 0;
            state.loop_end = 0;
            // Stop and let next start handle reset
            self.sfx_stop(id);
        }
    }

    fn sfx_is_playing(&mut self, id: usize) -> bool {
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get(id)) {
            if let Some(vid) = state.voice_id {
                eng.is_playing(vid)
            } else {
                false
            }
        } else {
            false
        }
    }

    fn sfx_set_pitch(&mut self, id: usize, pitch: f32) {
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get_mut(id)) {
            state.pitch = pitch;
            if let Some(vid) = state.voice_id {
                let _ = eng.set_pitch(vid, pitch);
            }
        }
    }

    fn sfx_set_gain(&mut self, id: usize, gain: f32) {
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get_mut(id)) {
            state.gain = gain;
            if let Some(vid) = state.voice_id {
                let _ = eng.set_volume(vid, Volume::linear(voice_volume(state)));
            }
        }
    }

    fn sfx_set_position(&mut self, id: usize, x: f32, y: f32, z: f32) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.position = [x, y, z];
            if !state.is_2d {
                if let (Some(eng), Some(vid)) = (&mut self.engine, state.voice_id) {
                                        let _ = eng.set_3d(vid, Some(source_3d(state)));
                }
            }
        }
    }

    fn sfx_set_distances(&mut self, id: usize, max: f32, min: f32) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.min_dist = min;
            state.max_dist = max;
            if !state.is_2d {
                if let (Some(eng), Some(vid)) = (&mut self.engine, state.voice_id) {
                                        let _ = eng.set_3d(vid, Some(source_3d(state)));
                }
            }
        }
    }

    fn sfx_set_loop_points(&mut self, id: usize, start: i32, end: i32) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.loop_start = start;
            state.loop_end = end;
            // Loop changes take effect at the next start
        }
    }

    fn sfx_set_loop_count(&mut self, id: usize, count: i32) {
        if let Some(state) = self.channel_state.get_mut(id) {
            state.loop_count = count;
            // Loop changes take effect at the next start
        }
    }

    fn sfx_set_reverb_mix(&mut self, id: usize, mix: f32) {
        if let Some(mixes) = self.reverb_mixes.get_mut(id) {
            *mixes = mix;
        }
        if let (Some(eng), Some(state)) = (&mut self.engine, self.channel_state.get(id)) {
            if let Some(vid) = state.voice_id {
                let _ = eng.set_send(vid, send_level(state, mix), 1.0);
            }
        }
    }

    fn al_gen_sources(&mut self, n: usize) -> Vec<u32> {
        self.streams.gen_sources(n)
    }

    fn al_delete_source(&mut self, id: u32) {
        self.streams.delete_source(id);
    }

    fn al_gen_buffers(&mut self, n: usize) -> Vec<u32> {
        self.streams.gen_buffers(n)
    }

    fn al_delete_buffer(&mut self, id: u32) {
        self.streams.delete_buffer(id);
    }

    fn al_is_buffer(&mut self, id: u32) -> bool {
        self.streams.is_buffer(id)
    }

    fn al_buffer_data(&mut self, id: u32, samples: &[i16], freq: u32) {
        self.streams.buffer_data(id, samples, freq);
    }

    fn al_queue_buffer(&mut self, src: u32, buf: u32) {
        self.streams.queue_buffer(src, buf);
    }

    fn al_unqueue_buffer(&mut self, src: u32) -> Option<u32> {
        self.streams.unqueue_buffer(src)
    }

    fn al_play(&mut self, src: u32) {
        self.streams.play(src);
    }

    fn al_pause(&mut self, src: u32) {
        self.streams.pause(src);
    }

    fn al_stop(&mut self, src: u32) {
        self.streams.stop(src);
    }

    fn al_state(&mut self, src: u32) -> State {
        self.streams.state(src)
    }

    fn al_buffers_queued(&mut self, src: u32) -> i32 {
        self.streams.buffers_queued(src)
    }

    fn al_buffers_processed(&mut self, src: u32) -> i32 {
        self.streams.buffers_processed(src)
    }

    fn al_sample_offset(&mut self, src: u32) -> i32 {
        self.streams.sample_offset(src)
    }

    fn al_byte_offset(&mut self, src: u32) -> i32 {
        self.streams.byte_offset(src)
    }

    fn al_set_relative(&mut self, src: u32, relative: bool) {
        self.streams.set_relative(src, relative);
    }

    fn al_set_gain(&mut self, src: u32, gain: f32) {
        self.streams.set_gain(src, gain);
    }

    fn al_set_pitch(&mut self, src: u32, pitch: f32) {
        self.streams.set_pitch(src, pitch);
    }

    fn al_set_rolloff(&mut self, src: u32, rolloff: f32) {
        self.streams.set_rolloff(src, rolloff);
    }

    fn al_set_position(&mut self, src: u32, x: f32, y: f32, z: f32) {
        self.streams.set_position(src, x, y, z);
    }

    fn render(&mut self, out: &mut [f32]) {
        // Ensure scratch buffer capacity
        if self.scratch.len() < out.len() {
            self.scratch.resize(out.len(), 0.0);
        }
        // Zero scratch
        for s in self.scratch.iter_mut() {
            *s = 0.0;
        }

        // Mix streams first
        self.streams.mix(out);

        // Mix engine if open
        if let Some(mix) = &mut self.mixer {
            if let Some(eng) = &mut self.engine {
                eng.update();
                let active = eng.active_voices();
                if active > self.max_active_seen {
                    self.max_active_seen = active;
                }
                let n = out.len();
                self.scratch[..n].fill(0.0);
                mix.render(&mut self.scratch[..n], 2); // exactly this callback's frames
                
                // Add scratch to out
                for (o, s) in out.iter_mut().zip(self.scratch.iter()) {
                    *o += s;
                }
            }
        }

        // track the peak (since take_stats) before the final clamp, as the game's mixer does
        for s in out.iter_mut() {
            self.peak_sample = self.peak_sample.max(s.abs());
            *s = s.clamp(-1.0, 1.0);
        }
    }

    fn take_stats(&mut self) -> Option<Stats> {
        if self.engine.is_none() {
            return None;
        }
        let stats = Stats {
            starts: self.starts_count as u64,
            max_active: self.max_active_seen,
            peak: self.peak_sample,
            max_send: 0.0,
        };
        self.starts_count = 0;
        self.max_active_seen = 0;
        self.peak_sample = 0.0;
        self.last_stats = Some(stats);
        Some(stats)
    }

    fn take_stream_frames(&mut self) -> Option<u64> {
        let frames = self.streams.take_frames_mixed();
        self.last_stream_frames = Some(frames);
        Some(frames)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::backend::RevcBackend;
    use std::f32::consts::PI;

    const SAMPLE_RATE: u32 = 48000;
    const BLOCK_SIZE: usize = 512;

    fn render_frames(b: &mut RetroBackend, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0_f32; frames * 2];
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK_SIZE);
            b.render(&mut buf[done * 2..(done + n) * 2]);
            done += n;
        }
        buf
    }

    fn const_buf(len: usize, v: i16) -> Vec<i16> {
        vec![v; len]
    }

    fn sine_buf_440_hz_22050hz(len: usize) -> Vec<i16> {
        let freq = 440.0;
        let sr = 22050.0;
        let amp = 32767.0;
        let mut buf = Vec::with_capacity(len);
        for i in 0..len {
            let phase = (i as f32) * freq / sr;
            let val = (2.0 * PI * phase).sin() * amp;
            buf.push(val as i16);
        }
        buf
    }

    #[test]
    fn two_d_full_gain() {
        let mut b = RetroBackend::new();
        b.sfx_open(4);
        b.sfx_set_2d(0, true);
        b.sfx_set_gain(0, 0.5);
        let data = const_buf(48000, 16384);
        b.sfx_start(0, &data, 48000);

        let buf = render_frames(&mut b, 4800);
        // After 256 frames ramp, check frames 256..512
        let start_idx = 256 * 2;
        let end_idx = 512 * 2;
        let left_means: f32 = buf[start_idx..end_idx].iter().step_by(2).sum::<f32>() / 256.0;
        let right_means: f32 = buf[start_idx..end_idx].iter().skip(1).step_by(2).sum::<f32>() / 256.0;

        let expected = 0.25;
        let tol = 0.02;
        assert!((left_means - expected).abs() < tol, "Left: {} vs {}", left_means, expected);
        assert!((right_means - expected).abs() < tol, "Right: {} vs {}", right_means, expected);
        assert!((left_means - right_means).abs() < tol, "L != R");
    }

    #[test]
    fn three_d_right_is_right() {
        let mut b = RetroBackend::new();
        b.sfx_open(4);
        b.sfx_set_2d(0, false);
        b.sfx_set_distances(0, 100.0, 1.0);
        b.sfx_set_position(0, 2.0, 0.0, 0.0);
        
        let data = const_buf(48000, 16384);
        b.sfx_start(0, &data, 48000);
        
        let buf = render_frames(&mut b, 4800);
        let start_idx = 256 * 2;
        let end_idx = 512 * 2;
        let left_sum: f32 = buf[start_idx..end_idx].iter().step_by(2).sum();
        let right_sum: f32 = buf[start_idx..end_idx].iter().skip(1).step_by(2).sum();
        let left_mean = left_sum / 256.0;
        let right_mean = right_sum / 256.0;

        assert!(right_mean > 3.0 * left_mean, "Right {} not > 3x Left {}", right_mean, left_mean);

        // New start at (-2, 0, 0)
        b.sfx_set_position(0, -2.0, 0.0, 0.0);
        b.sfx_start(0, &data, 48000);
        
        let buf2 = render_frames(&mut b, 4800);
        let start_idx2 = 256 * 2;
        let end_idx2 = 512 * 2;
        let left_sum2: f32 = buf2[start_idx2..end_idx2].iter().step_by(2).sum();
        let right_sum2: f32 = buf2[start_idx2..end_idx2].iter().skip(1).step_by(2).sum();
        let left_mean2 = left_sum2 / 256.0;
        let right_mean2 = right_sum2 / 256.0;

        assert!(left_mean2 > 3.0 * right_mean2, "Left {} not > 3x Right {}", left_mean2, right_mean2);
    }

    #[test]
    fn three_d_front_centred() {
        let mut b = RetroBackend::new();
        b.sfx_open(4);
        b.sfx_set_2d(0, false);
        b.sfx_set_position(0, 0.0, 0.0, -3.0);
        b.sfx_set_gain(0, 1.0);
        
        // Expected level: 0.5 * (1/3) * 0.5 = 0.0833
        // Buffer value 16384 -> 0.5 norm.
        // Inverse clamp: min_dist=1, max_dist=100. Dist=3.
        // Attenuation = min(1, 1/3) = 0.3333
        // Pan law linear: L=R=0.5.
        // Total = 0.5 (buf) * 1.0 (gain) * 0.3333 (dist) * 0.5 (pan) = 0.08333
        
        let data = const_buf(48000, 16384);
        b.sfx_start(0, &data, 48000);
        
        let buf = render_frames(&mut b, 4800);
        let start_idx = 256 * 2;
        let end_idx = 512 * 2;
        let left_sum: f32 = buf[start_idx..end_idx].iter().step_by(2).sum::<f32>() / 256.0;
        let right_sum: f32 = buf[start_idx..end_idx].iter().skip(1).step_by(2).sum::<f32>() / 256.0;
        
        let expected = 0.083333;
        let tol = 0.20; // 20%
        
        assert!((left_sum - expected).abs() / expected < tol, "Left {} vs {}", left_sum, expected);
        assert!((right_sum - expected).abs() / expected < tol, "Right {} vs {}", right_sum, expected);
        assert!((left_sum - right_sum).abs() / expected < 0.05, "L/R diff too large");
    }

    #[test]
    fn loop_count_three() {
        let mut b = RetroBackend::new();
        b.sfx_open(4);
        b.sfx_set_2d(0, true);
        b.sfx_set_gain(0, 1.0);
        
        let buf_data = const_buf(100, 16384);
        b.sfx_set_loop_points(0, 20, 60);
        b.sfx_set_loop_count(0, 3);
        b.sfx_start(0, &buf_data, 48000);

        let buf = render_frames(&mut b, 400);
        // Count frames where |left| > 0.01
        // Expected: 100 + 2 * 40 = 180 frames of sound.
        // Tolerance +- 2.
        
        let mut active_frames = 0;
        for i in 0..400 {
            let left = buf[i * 2];
            if left.abs() > 0.01 {
                active_frames += 1;
            }
        }
        
        assert_eq!(active_frames, 180, "Active frames: {} vs 180", active_frames);
    }

    #[test]
    fn loop_forever_and_stop() {
        let mut b = RetroBackend::new();
        b.sfx_open(4);
        b.sfx_set_2d(0, true);
        b.sfx_set_gain(0, 1.0);
        
        let buf_data = const_buf(100, 16384);
        b.sfx_set_loop_points(0, 0, 100);
        b.sfx_set_loop_count(0, 0); // Forever
        b.sfx_start(0, &buf_data, 48000);

        let buf1 = render_frames(&mut b, 1000);
        
        assert!(b.sfx_is_playing(0), "Should be playing");
        
        // Last 100 frames should be non-silent
        let last_100_start = 900 * 2;
        let last_100_end = 1000 * 2;
        let silent = buf1[last_100_start..last_100_end].iter().all(|&s| s.abs() < 0.01);
        assert!(!silent, "Last 100 frames should not be silent");

        b.sfx_stop(0);
        
        let buf2 = render_frames(&mut b, 512);
        
        // Last 256 frames of the second render should be silent
        let last_256_start = (512 - 256) * 2;
        let last_256_end = 512 * 2;
        let silent2 = buf2[last_256_start..last_256_end].iter().all(|&s| s.abs() < 0.01);
        assert!(silent2, "Last 256 frames after stop should be silent");
    }

    #[test]
    fn stream_same_as_revc() {
        // This test requires comparing RetroBackend and RevcBackend outputs.
        // Since RevcBackend is in the same crate (implied by context or import), we can use it.
        // However, the prompt says "Offline, no SDL: drive a RetroBackend only through the trait methods".
        // It also says "a RevcBackend and a RetroBackend... outputs bit-identical".
        // We need to ensure RevcBackend is accessible. Assuming it's in scope or super.
        
        let mut retro = RetroBackend::new();
        let mut revc = RevcBackend::new();

        // Neither sfx-open initially, but we need to set up the engine.
        // RevcBackend doesn't have sfx_open call in the same way, but let's look at impl.
        // RevcBackend::sfx_open takes voices. RetroBackend::sfx_open takes voices.
        
        retro.sfx_open(2);
        revc.sfx_open(2);

        // Gen 1 source + 1 buffer for streams? 
        // The prompt says: "gen 1 source + 1 buffer, buffer_data with a 4096-sample 440 Hz sine at 22050 Hz, queue, play"
        // This part refers to the 'streams' API (al_*), not the sfx_* API.
        // But wait, Test 6 says "neither sfx-open". This implies we are testing the AlStreams part of the backends.
        // Let's check the backends. Both have `streams: AlStreams`.
        
        // RetroBackend streams are initialized via `AlStreams::default()` in `new()`.
        // RevcBackend streams are `None` and created on demand via `get_or_insert_with`.
        
        // Let's test the AlStreams part.
        // RetroBackend has `streams: AlStreams` directly.
        // RevcBackend has `streams: Option<AlStreams>`.
        
        // We need to drive them through the trait methods.
        
        // RetroBackend
        let mut retro_rb = RetroBackend::new();
        // RetroBackend streams are already default.
        
        // RevcBackend
        let mut revc_rb = RevcBackend::new();

        // Generate 1 source and 1 buffer for both
        let retro_srcs = retro_rb.al_gen_sources(1);
        let retro_bufs = retro_rb.al_gen_buffers(1);
        
        let revc_srcs = revc_rb.al_gen_sources(1);
        let revc_bufs = revc_rb.al_gen_buffers(1);

        // Buffer data: 4096 samples, 440 Hz sine, 22050 Hz
        let sine_data = sine_buf_440_hz_22050hz(4096);
        
        retro_rb.al_buffer_data(retro_bufs[0], &sine_data, 22050);
        revc_rb.al_buffer_data(revc_bufs[0], &sine_data, 22050);

        // Queue
        retro_rb.al_queue_buffer(retro_srcs[0], retro_bufs[0]);
        revc_rb.al_queue_buffer(revc_srcs[0], revc_bufs[0]);

        // Play
        retro_rb.al_play(retro_srcs[0]);
        revc_rb.al_play(revc_srcs[0]);

        // Render 2048 frames
        let mut retro_out = vec![0.0f32; 2048 * 2];
        let mut revc_out = vec![0.0f32; 2048 * 2];
        
        retro_rb.render(&mut retro_out);
        revc_rb.render(&mut revc_out);

        // Compare bit-identical
        assert_eq!(retro_out.len(), revc_out.len());
        for (r, v) in retro_out.iter().zip(revc_out.iter()) {
            assert_eq!(r.to_bits(), v.to_bits(), "Mismatch at sample");
        }
    }
}

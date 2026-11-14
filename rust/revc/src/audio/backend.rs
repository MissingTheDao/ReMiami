use revc_audio::al::{AlStreams, State};
use revc_audio::mixer::{Mixer, Stats};
use revc_audio::reverb::ReverbParams;

/// One audio engine behind the game's audio ABI (revc_sfx_*, revc_al*, revc_reverb_set).
/// Called with the backend lock held, from the game thread (setters) and the audio thread (render).
pub trait AudioBackend: Send {
    // sound effects
    fn sfx_open(&mut self, voices: usize);
    fn sfx_close(&mut self);
    fn sfx_is_open(&self) -> bool;
    fn reverb_set(&mut self, p: &ReverbParams);
    fn sfx_set_2d(&mut self, id: usize, is_2d: bool);
    fn sfx_start(&mut self, id: usize, data: &[i16], freq: u32);
    fn sfx_stop(&mut self, id: usize);
    fn sfx_reset(&mut self, id: usize);
    fn sfx_is_playing(&self, id: usize) -> bool;
    fn sfx_set_pitch(&mut self, id: usize, pitch: f32);
    fn sfx_set_gain(&mut self, id: usize, gain: f32);
    fn sfx_set_position(&mut self, id: usize, x: f32, y: f32, z: f32);
    fn sfx_set_distances(&mut self, id: usize, max: f32, min: f32);
    fn sfx_set_loop_points(&mut self, id: usize, start: i32, end: i32);
    fn sfx_set_loop_count(&mut self, id: usize, count: i32);
    fn sfx_set_reverb_mix(&mut self, id: usize, mix: f32);
    // streams (OpenAL subset)
    fn al_gen_sources(&mut self, n: usize) -> Vec<u32>;
    fn al_delete_source(&mut self, id: u32);
    fn al_gen_buffers(&mut self, n: usize) -> Vec<u32>;
    fn al_delete_buffer(&mut self, id: u32);
    fn al_is_buffer(&mut self, id: u32) -> bool;
    fn al_buffer_data(&mut self, id: u32, samples: &[i16], freq: u32);
    fn al_queue_buffer(&mut self, src: u32, buf: u32);
    fn al_unqueue_buffer(&mut self, src: u32) -> Option<u32>;
    fn al_play(&mut self, src: u32);
    fn al_pause(&mut self, src: u32);
    fn al_stop(&mut self, src: u32);
    fn al_state(&mut self, src: u32) -> State;
    fn al_buffers_queued(&mut self, src: u32) -> i32;
    fn al_buffers_processed(&mut self, src: u32) -> i32;
    fn al_sample_offset(&mut self, src: u32) -> i32;
    fn al_byte_offset(&mut self, src: u32) -> i32;
    fn al_set_relative(&mut self, src: u32, relative: bool);
    fn al_set_gain(&mut self, src: u32, gain: f32);
    fn al_set_pitch(&mut self, src: u32, pitch: f32);
    fn al_set_rolloff(&mut self, src: u32, rolloff: f32);
    fn al_set_position(&mut self, src: u32, x: f32, y: f32, z: f32);
    // output: adds this engine's mix to `out` (interleaved stereo f32 at OUTPUT_RATE, pre-zeroed)
    fn render(&mut self, out: &mut [f32]);
    // regression-harness counters since the last call
    fn take_stats(&mut self) -> Option<Stats>;
    fn take_stream_frames(&mut self) -> Option<u64>;
}

pub struct RevcBackend {
    mixer: Option<Mixer>,
    streams: Option<AlStreams>,
    reverb: Option<ReverbParams>,
}

impl RevcBackend {
    pub fn new() -> Self {
        Self {
            mixer: None,
            streams: None,
            reverb: None,
        }
    }
}

impl AudioBackend for RevcBackend {
    fn sfx_open(&mut self, voices: usize) {
        let mut mixer = Mixer::new(voices);
        if let Some(p) = &self.reverb {
            mixer.set_reverb_params(p);
        }
        self.mixer = Some(mixer);
    }

    fn sfx_close(&mut self) {
        self.mixer = None;
    }

    fn sfx_is_open(&self) -> bool {
        self.mixer.is_some()
    }

    fn reverb_set(&mut self, p: &ReverbParams) {
        self.reverb = Some(*p);
        if let Some(m) = &mut self.mixer {
            m.set_reverb_params(p);
        }
    }

    fn sfx_set_2d(&mut self, id: usize, is_2d: bool) {
        if let Some(m) = &mut self.mixer {
            m.set_2d(id, is_2d);
        }
    }

    fn sfx_start(&mut self, id: usize, data: &[i16], freq: u32) {
        if let Some(m) = &mut self.mixer {
            m.start(id, data, freq);
        }
    }

    fn sfx_stop(&mut self, id: usize) {
        if let Some(m) = &mut self.mixer {
            m.stop(id);
        }
    }

    fn sfx_reset(&mut self, id: usize) {
        if let Some(m) = &mut self.mixer {
            m.reset(id);
        }
    }

    fn sfx_is_playing(&self, id: usize) -> bool {
        self.mixer.as_ref().map(|m| m.is_playing(id)).unwrap_or(false)
    }

    fn sfx_set_pitch(&mut self, id: usize, pitch: f32) {
        if let Some(m) = &mut self.mixer {
            m.set_pitch(id, pitch);
        }
    }

    fn sfx_set_gain(&mut self, id: usize, gain: f32) {
        if let Some(m) = &mut self.mixer {
            m.set_gain(id, gain);
        }
    }

    fn sfx_set_position(&mut self, id: usize, x: f32, y: f32, z: f32) {
        if let Some(m) = &mut self.mixer {
            m.set_position(id, x, y, z);
        }
    }

    fn sfx_set_distances(&mut self, id: usize, max: f32, min: f32) {
        if let Some(m) = &mut self.mixer {
            m.set_distances(id, max, min);
        }
    }

    fn sfx_set_loop_points(&mut self, id: usize, start: i32, end: i32) {
        if let Some(m) = &mut self.mixer {
            m.set_loop_points(id, start, end);
        }
    }

    fn sfx_set_loop_count(&mut self, id: usize, count: i32) {
        if let Some(m) = &mut self.mixer {
            m.set_loop_count(id, count);
        }
    }

    fn sfx_set_reverb_mix(&mut self, id: usize, mix: f32) {
        if let Some(m) = &mut self.mixer {
            m.set_reverb_mix(id, mix);
        }
    }

    fn al_gen_sources(&mut self, n: usize) -> Vec<u32> {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .gen_sources(n)
    }

    fn al_delete_source(&mut self, id: u32) {
        if let Some(s) = &mut self.streams {
            s.delete_source(id);
        }
    }

    fn al_gen_buffers(&mut self, n: usize) -> Vec<u32> {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .gen_buffers(n)
    }

    fn al_delete_buffer(&mut self, id: u32) {
        if let Some(s) = &mut self.streams {
            s.delete_buffer(id);
        }
    }

    fn al_is_buffer(&mut self, id: u32) -> bool {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .is_buffer(id)
    }

    fn al_buffer_data(&mut self, id: u32, samples: &[i16], freq: u32) {
        if let Some(s) = &mut self.streams {
            s.buffer_data(id, samples, freq);
        }
    }

    fn al_queue_buffer(&mut self, src: u32, buf: u32) {
        if let Some(s) = &mut self.streams {
            s.queue_buffer(src, buf);
        }
    }

    fn al_unqueue_buffer(&mut self, src: u32) -> Option<u32> {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .unqueue_buffer(src)
    }

    fn al_play(&mut self, src: u32) {
        if let Some(s) = &mut self.streams {
            s.play(src);
        }
    }

    fn al_pause(&mut self, src: u32) {
        if let Some(s) = &mut self.streams {
            s.pause(src);
        }
    }

    fn al_stop(&mut self, src: u32) {
        if let Some(s) = &mut self.streams {
            s.stop(src);
        }
    }

    fn al_state(&mut self, src: u32) -> State {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .state(src)
    }

    fn al_buffers_queued(&mut self, src: u32) -> i32 {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .buffers_queued(src)
    }

    fn al_buffers_processed(&mut self, src: u32) -> i32 {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .buffers_processed(src)
    }

    fn al_sample_offset(&mut self, src: u32) -> i32 {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .sample_offset(src)
    }

    fn al_byte_offset(&mut self, src: u32) -> i32 {
        self.streams
            .get_or_insert_with(AlStreams::default)
            .byte_offset(src)
    }

    fn al_set_relative(&mut self, src: u32, relative: bool) {
        if let Some(s) = &mut self.streams {
            s.set_relative(src, relative);
        }
    }

    fn al_set_gain(&mut self, src: u32, gain: f32) {
        if let Some(s) = &mut self.streams {
            s.set_gain(src, gain);
        }
    }

    fn al_set_pitch(&mut self, src: u32, pitch: f32) {
        if let Some(s) = &mut self.streams {
            s.set_pitch(src, pitch);
        }
    }

    fn al_set_rolloff(&mut self, src: u32, rolloff: f32) {
        if let Some(s) = &mut self.streams {
            s.set_rolloff(src, rolloff);
        }
    }

    fn al_set_position(&mut self, src: u32, x: f32, y: f32, z: f32) {
        if let Some(s) = &mut self.streams {
            s.set_position(src, x, y, z);
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        if let Some(s) = &mut self.streams {
            s.mix(out);
        }
        if let Some(m) = &mut self.mixer {
            m.mix(out);
        }
    }

    fn take_stats(&mut self) -> Option<Stats> {
        self.mixer.as_mut().map(|m| m.take_stats())
    }

    fn take_stream_frames(&mut self) -> Option<u64> {
        self.streams.as_mut().map(|s| s.take_frames_mixed())
    }
}

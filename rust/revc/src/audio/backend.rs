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
#[cfg(test)]
mod tests {
    //! RevcBackend must render exactly what the engine did before it moved behind the trait
    //! (streams mixed first, then sound effects), and two back ends given the same commands must
    //! render the same samples (the shadow mode's premise).
    use super::*;

    const BLOCK: usize = 512; // stereo frames per render call

    fn sine(freq: f32, rate: u32, n: usize) -> Vec<i16> {
        (0..n).map(|i| ((2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 16384.0) as i16).collect()
    }

    fn reverb() -> ReverbParams {
        ReverbParams { decay_time: 2.0, gain: 0.5, ..Default::default() }
    }

    /// The pre-trait engine: a mixer and the AL streams, driven directly.
    struct Direct {
        mixer: Mixer,
        streams: AlStreams,
    }

    impl Direct {
        fn render(&mut self, out: &mut [f32]) {
            self.streams.mix(out);
            self.mixer.mix(out);
        }
    }

    /// The command script, once for a back end and once for the direct engine; returns the source.
    fn script_backend(b: &mut dyn AudioBackend, s440: &[i16], s1k: &[i16], s300: &[i16]) -> u32 {
        b.sfx_open(8);
        b.reverb_set(&reverb());
        b.sfx_set_2d(0, true);
        b.sfx_start(0, s440, 22050);
        b.sfx_set_2d(3, false);
        b.sfx_start(3, s1k, 22050);
        b.sfx_set_position(3, 1.0, 0.0, 0.0);
        b.sfx_set_distances(3, 10.0, 1.0);
        b.sfx_set_pitch(3, 1.2);
        b.sfx_set_gain(3, 0.8);
        b.sfx_set_reverb_mix(3, 0.5);
        let src = b.al_gen_sources(1)[0];
        for buf in b.al_gen_buffers(3) {
            b.al_buffer_data(buf, s300, 32000);
            b.al_queue_buffer(src, buf);
        }
        b.al_play(src);
        src
    }

    fn script_direct(d: &mut Direct, s440: &[i16], s1k: &[i16], s300: &[i16]) -> u32 {
        // sfx_open: a new mixer with the stored reverb preset applied (RevcBackend does the same)
        d.mixer.set_reverb_params(&reverb());
        d.mixer.set_2d(0, true);
        d.mixer.start(0, s440, 22050);
        d.mixer.set_2d(3, false);
        d.mixer.start(3, s1k, 22050);
        d.mixer.set_position(3, 1.0, 0.0, 0.0);
        d.mixer.set_distances(3, 10.0, 1.0);
        d.mixer.set_pitch(3, 1.2);
        d.mixer.set_gain(3, 0.8);
        d.mixer.set_reverb_mix(3, 0.5);
        let src = d.streams.gen_sources(1)[0];
        for buf in d.streams.gen_buffers(3) {
            d.streams.buffer_data(buf, s300, 32000);
            d.streams.queue_buffer(src, buf);
        }
        d.streams.play(src);
        src
    }

    /// Unqueues the processed buffers and queues them again; returns their ids.
    fn requeue_backend(b: &mut dyn AudioBackend, src: u32) -> Vec<u32> {
        let n = b.al_buffers_processed(src);
        let ids: Vec<u32> = (0..n).filter_map(|_| b.al_unqueue_buffer(src)).collect();
        ids.iter().for_each(|&id| b.al_queue_buffer(src, id));
        ids
    }

    fn requeue_direct(d: &mut Direct, src: u32) -> Vec<u32> {
        let n = d.streams.buffers_processed(src);
        let ids: Vec<u32> = (0..n).filter_map(|_| d.streams.unqueue_buffer(src)).collect();
        ids.iter().for_each(|&id| d.streams.queue_buffer(src, id));
        ids
    }

    fn assert_same(a: &[f32], b: &[f32], what: &str) {
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "{what}: sample {i}: {x} vs {y}");
        }
    }

    #[test]
    fn same_as_direct() {
        let (s440, s1k, s300) = (sine(440.0, 22050, 11025), sine(1000.0, 22050, 11025), sine(300.0, 32000, 4096));
        let mut b = RevcBackend::new();
        let mut d = Direct { mixer: Mixer::new(8), streams: AlStreams::default() };
        let src_b = script_backend(&mut b, &s440, &s1k, &s300);
        let src_d = script_direct(&mut d, &s440, &s1k, &s300);
        assert_eq!(src_b, src_d);
        let (mut ob, mut od) = (vec![0.0f32; BLOCK * 2], vec![0.0f32; BLOCK * 2]);
        let mut requeued = 0;
        for block in 0..100 {
            if block == 50 {
                // a mid-run change on both sides
                b.sfx_set_pitch(0, 2.0);
                d.mixer.set_pitch(0, 2.0);
                b.al_set_gain(src_b, 0.5);
                d.streams.set_gain(src_d, 0.5);
            }
            if block % 10 == 9 {
                let (ib, id) = (requeue_backend(&mut b, src_b), requeue_direct(&mut d, src_d));
                assert_eq!(ib, id, "block {block}: unqueued buffers");
                requeued += ib.len();
            }
            ob.fill(0.0);
            od.fill(0.0);
            b.render(&mut ob);
            d.render(&mut od);
            assert_same(&ob, &od, &format!("block {block}"));
        }
        assert!(requeued > 0, "the script never recycled a buffer");
        assert!(ob.iter().any(|&x| x != 0.0), "the script rendered silence");
        assert_eq!(b.take_stats(), Some(d.mixer.take_stats()));
        assert_eq!(b.take_stream_frames(), Some(d.streams.take_frames_mixed()));
    }

    #[test]
    fn two_backends_identical() {
        let (s440, s1k, s300) = (sine(440.0, 22050, 11025), sine(1000.0, 22050, 11025), sine(300.0, 32000, 4096));
        let mut a: Box<dyn AudioBackend> = Box::new(RevcBackend::new());
        let mut c: Box<dyn AudioBackend> = Box::new(RevcBackend::new());
        let (sa, sc) = (script_backend(a.as_mut(), &s440, &s1k, &s300), script_backend(c.as_mut(), &s440, &s1k, &s300));
        let (mut oa, mut oc) = (vec![0.0f32; BLOCK * 2], vec![0.0f32; BLOCK * 2]);
        for block in 0..100 {
            if block % 10 == 9 {
                assert_eq!(requeue_backend(a.as_mut(), sa), requeue_backend(c.as_mut(), sc));
            }
            oa.fill(0.0);
            oc.fill(0.0);
            a.render(&mut oa);
            c.render(&mut oc);
            assert_same(&oa, &oc, &format!("block {block}"));
        }
        assert_eq!(a.take_stats(), c.take_stats());
        assert_eq!(a.take_stream_frames(), c.take_stream_frames());
    }

    #[test]
    fn closed_mixer_is_silent_for_sfx() {
        let mut b = RevcBackend::new();
        b.sfx_set_2d(0, true);
        b.sfx_start(0, &sine(440.0, 22050, 1000), 22050);
        b.sfx_set_pitch(0, 2.0);
        assert!(!b.sfx_is_playing(0));
        assert!(b.take_stats().is_none());
        let mut out = vec![0.0f32; BLOCK * 2];
        b.render(&mut out);
        assert!(out.iter().all(|&x| x == 0.0), "no voices and no streams: silence");
        // streams work without an open mixer
        let src = b.al_gen_sources(1)[0];
        let buf = b.al_gen_buffers(1)[0];
        b.al_buffer_data(buf, &sine(440.0, 22050, 4096), 22050);
        b.al_queue_buffer(src, buf);
        b.al_play(src);
        b.render(&mut out);
        assert_eq!(b.al_state(src), revc_audio::al::State::Playing);
        assert!(out.iter().any(|&x| x != 0.0), "the stream rendered silence");
    }
}

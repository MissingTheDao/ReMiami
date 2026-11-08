//! Software mixer for the game's sound-effect voices, reproducing OpenAL Soft's behaviour for
//! the features the game uses: per-voice gain and pitch, listener-relative 3D position with the
//! inverse-distance-clamped model, 2D (unpanned) voices, loop counts and loop points.
//! Pure logic, unit-tested; SDL output is in `mod.rs`. First draft by the local model.

pub const OUTPUT_RATE: u32 = 48000;

#[derive(Clone, Debug)]
pub struct Voice {
    data: Vec<i16>,
    freq: u32,
    pitch: f32,
    gain: f32,
    pos: [f32; 3],
    ref_dist: f32,
    max_dist: f32,
    is_2d: bool,
    loop_count: i32,
    loops_left: i32,
    loop_start: i32,
    loop_end: i32,
    playing: bool,
    cursor: f64,
    /// Stored for the reverb stage; not used in mixing yet.
    reverb_mix: f32,
}

pub struct Mixer {
    voices: Vec<Voice>,
    stats: Stats,
}

/// Counters for diagnostics and the regression harness, since the last `take_stats()`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Voices started.
    pub starts: u64,
    /// Highest number of voices playing at once in a mix call.
    pub max_active: usize,
    /// Highest absolute output sample (before clamping).
    pub peak: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            freq: 32000,
            pitch: 1.0,
            gain: 1.0,
            pos: [0.0, 0.0, 0.0],
            ref_dist: 1.0,
            max_dist: f32::MAX,
            is_2d: false,
            loop_count: 1,
            loops_left: 0,
            loop_start: 0,
            loop_end: 0,
            playing: false,
            cursor: 0.0,
            reverb_mix: 0.0,
        }
    }
}

impl Mixer {
    pub fn new(num_voices: usize) -> Self {
        Self {
            voices: (0..num_voices).map(|_| Voice::default()).collect(),
            stats: Stats::default(),
        }
    }

    pub fn set_2d(&mut self, id: usize, is_2d: bool) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.is_2d = is_2d;
        }
    }

    pub fn start(&mut self, id: usize, data: &[i16], freq: u32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.data = data.to_vec();
            voice.freq = freq;
            voice.cursor = 0.0;
            if voice.loop_count >= 2 {
                voice.loops_left = voice.loop_count - 1;
            } else {
                voice.loops_left = 0;
            }
            voice.playing = !data.is_empty() && freq > 0;
            if voice.playing {
                self.stats.starts += 1;
            }
        }
    }

    /// Returns the counters since the previous call and resets them.
    pub fn take_stats(&mut self) -> Stats {
        std::mem::take(&mut self.stats)
    }

    pub fn stop(&mut self, id: usize) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.playing = false;
        }
    }

    pub fn reset(&mut self, id: usize) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.playing = false;
            voice.data.clear();
            voice.cursor = 0.0;
            voice.loop_count = 1;
            voice.loops_left = 0;
            voice.loop_start = 0;
            voice.loop_end = 0;
        }
    }

    pub fn is_playing(&self, id: usize) -> bool {
        self.voices.get(id).map(|v| v.playing).unwrap_or(false)
    }

    pub fn set_pitch(&mut self, id: usize, pitch: f32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.pitch = pitch;
        }
    }

    pub fn set_gain(&mut self, id: usize, gain: f32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.gain = gain;
        }
    }

    pub fn set_position(&mut self, id: usize, x: f32, y: f32, z: f32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.pos = [x, y, z];
        }
    }

    pub fn set_distances(&mut self, id: usize, max_dist: f32, ref_dist: f32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.max_dist = max_dist;
            voice.ref_dist = ref_dist;
        }
    }

    pub fn set_loop_points(&mut self, id: usize, start: i32, end: i32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.loop_start = start;
            voice.loop_end = end;
        }
    }

    pub fn set_loop_count(&mut self, id: usize, count: i32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.loop_count = count;
            if voice.playing {
                if count >= 2 {
                    voice.loops_left = count - 1;
                } else {
                    voice.loops_left = 0;
                }
            }
        }
    }

    pub fn set_reverb_mix(&mut self, id: usize, mix: f32) {
        if let Some(voice) = self.voices.get_mut(id) {
            voice.reverb_mix = mix;
        }
    }

    /// Mixes all playing voices into `out` (interleaved stereo), replacing its contents.
    pub fn mix(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let mut active = 0;
        for voice in &mut self.voices {
            if voice.playing {
                active += 1;
                voice.mix_into(out);
            }
        }
        self.stats.max_active = self.stats.max_active.max(active);
        for sample in out.iter_mut() {
            self.stats.peak = self.stats.peak.max(sample.abs());
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

impl Voice {
    /// Left/right gains: 2D voices are unpanned at full gain; 3D voices use OpenAL's inverse
    /// distance clamped model (rolloff 1) and constant-power panning on the x axis.
    fn gains(&self) -> (f32, f32) {
        let g = self.gain.clamp(0.0, 1.0);
        if self.is_2d {
            return (g, g);
        }
        let dist = (self.pos[0] * self.pos[0] + self.pos[1] * self.pos[1] + self.pos[2] * self.pos[2]).sqrt();
        let att = if self.ref_dist > 0.0 {
            let d = dist.max(self.ref_dist).min(self.max_dist.max(self.ref_dist));
            self.ref_dist / (self.ref_dist + (d - self.ref_dist))
        } else {
            1.0
        };
        let pan = if dist > 1e-4 { (self.pos[0] / dist).clamp(-1.0, 1.0) } else { 0.0 };
        (g * att * ((1.0 - pan) / 2.0).sqrt(), g * att * ((1.0 + pan) / 2.0).sqrt())
    }

    /// OpenAL ignores invalid loop points and loops the whole buffer.
    fn loop_region(&self) -> (usize, usize) {
        let len = self.data.len();
        if self.loop_start > 0 && self.loop_end > self.loop_start && self.loop_end as usize <= len {
            (self.loop_start as usize, self.loop_end as usize)
        } else {
            (0, len)
        }
    }

    fn mix_into(&mut self, out: &mut [f32]) {
        let len = self.data.len();
        let step = self.freq as f64 * self.pitch.max(0.0) as f64 / OUTPUT_RATE as f64;
        if len == 0 || step <= 0.0 {
            if len == 0 {
                self.playing = false;
            }
            return;
        }
        let (lg, rg) = self.gains();
        let (rs, re) = self.loop_region();
        for frame in out.chunks_exact_mut(2) {
            let looping = self.loop_count == 0 || self.loops_left > 0;
            let i = (self.cursor as usize).min(len - 1);
            let frac = (self.cursor - i as f64) as f32;
            let next = if looping && i + 1 == re { rs } else { (i + 1).min(len - 1) };
            let s = (self.data[i] as f32 + (self.data[next] as f32 - self.data[i] as f32) * frac) / 32768.0;
            frame[0] += s * lg;
            frame[1] += s * rg;

            self.cursor += step;
            if looping {
                if self.cursor >= re as f64 {
                    let region = (re - rs) as f64;
                    while self.cursor >= re as f64 {
                        self.cursor -= region;
                    }
                    if self.loop_count >= 2 {
                        self.loops_left -= 1; // one pass done
                    }
                }
            } else if self.cursor >= len as f64 {
                self.playing = false;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: usize = 480;

    /// Mixes `frames` stereo frames and returns them.
    fn run(m: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; frames * 2];
        m.mix(&mut buf);
        buf
    }

    fn started(data: &[i16], loop_count: i32) -> Mixer {
        let mut m = Mixer::new(1);
        m.set_2d(0, true);
        m.set_loop_count(0, loop_count);
        m.start(0, data, OUTPUT_RATE); // step 1.0: one sample per frame
        m
    }

    #[test]
    fn silence_when_idle() {
        let mut m = Mixer::new(4);
        assert!(run(&mut m, 256).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn voice_2d_full_gain() {
        let mut m = started(&[16384; LEN], 1);
        for s in run(&mut m, 100) {
            assert!((s - 0.5).abs() < 1e-6, "{s}");
        }
    }

    #[test]
    fn play_once_stops_at_end() {
        let mut m = started(&[16384; LEN], 1);
        run(&mut m, LEN - 1);
        assert!(m.is_playing(0));
        run(&mut m, 1);
        assert!(!m.is_playing(0));
    }

    #[test]
    fn loop_count_two_plays_twice() {
        let mut m = started(&[16384; LEN], 2);
        run(&mut m, LEN + 10); // into the second pass
        assert!(m.is_playing(0));
        run(&mut m, LEN - 10);
        assert!(!m.is_playing(0));
    }

    #[test]
    fn loop_count_set_before_start_counts() {
        let mut m = started(&[1; LEN], 3);
        run(&mut m, 3 * LEN - 1);
        assert!(m.is_playing(0));
        run(&mut m, 1);
        assert!(!m.is_playing(0));
    }

    #[test]
    fn loop_forever() {
        let mut m = started(&[16384; LEN], 0);
        run(&mut m, 10 * LEN);
        assert!(m.is_playing(0));
    }

    #[test]
    fn loop_points_used_on_repeat() {
        let data: Vec<i16> = (0..LEN as i16).collect();
        let mut m = Mixer::new(1);
        m.set_2d(0, true);
        m.set_loop_count(0, 0);
        m.set_loop_points(0, 100, 200);
        m.start(0, &data, OUTPUT_RATE);
        let out = run(&mut m, 200 + 50);
        // Frame 200 wraps to sample 100.
        assert!((out[199 * 2] * 32768.0 - 199.0).abs() < 0.5);
        assert!((out[200 * 2] * 32768.0 - 100.0).abs() < 0.5);
    }

    #[test]
    fn invalid_loop_points_loop_whole_buffer() {
        // -1 end arrives as 0 from the game (integer division); OpenAL loops everything then.
        let data: Vec<i16> = (0..LEN as i16).collect();
        let mut m = Mixer::new(1);
        m.set_2d(0, true);
        m.set_loop_count(0, 0);
        m.set_loop_points(0, 100, 0);
        m.start(0, &data, OUTPUT_RATE);
        let out = run(&mut m, LEN + 1);
        assert!((out[LEN * 2] * 32768.0).abs() < 0.5); // back at sample 0
    }

    #[test]
    fn pitch_doubles_speed() {
        let mut m = started(&[1; LEN], 1);
        m.set_pitch(0, 2.0);
        run(&mut m, LEN / 2);
        assert!(!m.is_playing(0));
    }

    #[test]
    fn panned_right_and_attenuated() {
        let mut m = Mixer::new(1);
        m.set_position(0, 2.0, 0.0, 0.0);
        m.set_distances(0, f32::MAX, 1.0);
        m.start(0, &[32767; LEN], OUTPUT_RATE);
        let out = run(&mut m, 4);
        assert!(out[0].abs() < 1e-6, "left {}", out[0]);
        assert!((out[1] - 0.5).abs() < 1e-3, "right {}", out[1]); // 1/(1+(2-1)) = 0.5
    }

    #[test]
    fn max_distance_clamps_attenuation() {
        let mut m = Mixer::new(1);
        m.set_position(0, 0.0, 100.0, 0.0);
        m.set_distances(0, 4.0, 1.0);
        m.start(0, &[32767; LEN], OUTPUT_RATE);
        let out = run(&mut m, 1);
        let expect = 0.25 * 0.5f32.sqrt(); // att at 4.0, centred
        assert!((out[0] - expect).abs() < 1e-3 && (out[1] - expect).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn reset_keeps_source_state() {
        let mut m = Mixer::new(1);
        m.set_gain(0, 0.25);
        m.set_2d(0, true);
        m.set_loop_count(0, 0);
        m.reset(0);
        m.start(0, &[32767; LEN], OUTPUT_RATE);
        let out = run(&mut m, LEN + 1);
        assert!((out[0] - 0.25).abs() < 1e-3); // gain kept
        assert!(!m.is_playing(0)); // loop count reset to play-once
    }

    #[test]
    fn out_of_range_ids_are_ignored() {
        let mut m = Mixer::new(1);
        m.set_2d(9, true);
        m.start(9, &[1], OUTPUT_RATE);
        m.stop(9);
        m.reset(9);
        m.set_pitch(9, 1.0);
        m.set_gain(9, 1.0);
        m.set_position(9, 0.0, 0.0, 0.0);
        m.set_distances(9, 1.0, 1.0);
        m.set_loop_points(9, 0, 1);
        m.set_loop_count(9, 1);
        m.set_reverb_mix(9, 0.0);
        assert!(!m.is_playing(9));
        run(&mut m, 8);
    }

    #[test]
    fn stats_count_starts_and_peak() {
        let mut m = started(&[16384; LEN], 1);
        run(&mut m, 4);
        let s = m.take_stats();
        assert_eq!((s.starts, s.max_active), (1, 1));
        assert!((s.peak - 0.5).abs() < 1e-6);
        assert_eq!(m.take_stats(), Stats::default());
    }

    #[test]
    fn output_is_clamped() {
        let mut m = Mixer::new(3);
        for id in 0..3 {
            m.set_2d(id, true);
            m.start(id, &[32767; LEN], OUTPUT_RATE);
        }
        assert!(run(&mut m, 8).iter().all(|&s| s <= 1.0));
    }
}

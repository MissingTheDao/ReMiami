//! Minimal software implementation of OpenAL's streaming-source semantics: sources with
//! buffer queues, PROCESSED/QUEUED counts, offsets, starvation, pause/stop/play rules.
//! cpp/re3-miami/src/audio/oal/stream.cpp drives it through the renames in oal/rust_al.h, so the
//! game's stream logic runs unchanged. First draft by the local model; mixing restructured.

use std::collections::{HashMap, VecDeque};
use crate::mixer::OUTPUT_RATE; // u32 = 48000, already exists

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State { Initial, Playing, Paused, Stopped }

struct Buffer { data: Vec<i16>, freq: u32 }
struct Source {
    gain: f32, pitch: f32, pos: [f32; 3], rolloff: f32,
    /// Stored for fidelity; the game only uses listener-relative stream sources.
    #[allow(dead_code)]
    relative: bool,
    state: State,
    queue: VecDeque<u32>,   // buffer ids, oldest first
    current: usize,         // index in queue of the buffer being played == number of processed buffers
    cursor: f64,            // fractional sample position inside queue[current]
}
pub struct AlStreams {
    sources: HashMap<u32, Source>,
    buffers: HashMap<u32, Buffer>,
    next_id: u32,
    /// Source-frames mixed since the last `take_frames_mixed()` (diagnostics/tests).
    frames_mixed: u64,
}

impl Default for Source {
    fn default() -> Self {
        Source {
            gain: 1.0, pitch: 1.0, pos: [0.0; 3], rolloff: 1.0, relative: false,
            state: State::Initial, queue: VecDeque::new(), current: 0, cursor: 0.0,
        }
    }
}

impl Source {
    fn gains(&self) -> (f32, f32) {
        let g = self.gain.clamp(0.0, 1.0);
        let p = &self.pos;
        let dist = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let ref_dist = 1.0;
        let att = if self.rolloff > 0.0 && dist > ref_dist {
            ref_dist / (ref_dist + self.rolloff * (dist - ref_dist))
        } else {
            1.0
        };
        let pan = if dist > 1e-4 { (p[0] / dist).clamp(-1.0, 1.0) } else { 0.0 };
        (g * att * ((1.0 - pan) / 2.0).sqrt(), g * att * ((1.0 + pan) / 2.0).sqrt())
    }

    /// Runs out of queued audio: OpenAL stops the source and marks everything processed.
    fn starve(&mut self) {
        self.state = State::Stopped;
        self.current = self.queue.len();
        self.cursor = 0.0;
    }

    /// The playable buffer at `current`, skipping empty/missing ones. None = queue exhausted.
    fn current_buffer<'a>(&mut self, buffers: &'a HashMap<u32, Buffer>) -> Option<&'a Buffer> {
        while self.current < self.queue.len() {
            match buffers.get(&self.queue[self.current]) {
                Some(b) if !b.data.is_empty() && b.freq > 0 => return Some(b),
                _ => self.current += 1,
            }
        }
        None
    }

    /// Returns the number of frames mixed.
    fn mix_into(&mut self, buffers: &HashMap<u32, Buffer>, out: &mut [f32]) -> u64 {
        let (lg, rg) = self.gains();
        let mut mixed = 0;
        for frame in out.chunks_exact_mut(2) {
            let Some(b) = self.current_buffer(buffers) else {
                self.starve();
                return mixed;
            };
            let step = b.freq as f64 * self.pitch.max(0.0) as f64 / OUTPUT_RATE as f64;
            if step <= 0.0 {
                return mixed; // pitch 0: hold position
            }
            let len = b.data.len();
            let i = (self.cursor as usize).min(len - 1);
            let frac = (self.cursor - i as f64) as f32;
            let next = if i + 1 < len {
                b.data[i + 1]
            } else {
                // Interpolate into the next queued buffer, like a continuous stream.
                self.queue
                    .get(self.current + 1)
                    .and_then(|id| buffers.get(id))
                    .and_then(|nb| nb.data.first().copied())
                    .unwrap_or(b.data[i])
            };
            let s = (b.data[i] as f32 + (next as f32 - b.data[i] as f32) * frac) / 32768.0;
            frame[0] += s * lg;
            frame[1] += s * rg;
            mixed += 1;

            self.cursor += step;
            if self.cursor >= len as f64 {
                self.cursor -= len as f64;
                self.current += 1;
                // OpenAL stops as soon as the last queued sample has played.
                if self.current_buffer(buffers).is_none() {
                    self.starve();
                    return mixed;
                }
            }
        }
        mixed
    }
}

impl Default for AlStreams {
    fn default() -> Self {
        AlStreams {
            sources: HashMap::new(),
            buffers: HashMap::new(),
            next_id: 1,
            frames_mixed: 0,
        }
    }
}

impl AlStreams {
    pub fn gen_sources(&mut self, n: usize) -> Vec<u32> {
        let mut ids = Vec::with_capacity(n);
        for _ in 0..n {
            let id = self.next_id;
            self.next_id += 1;
            ids.push(id);
            self.sources.insert(id, Source::default());
        }
        ids
    }

    pub fn delete_source(&mut self, id: u32) {
        self.sources.remove(&id);
    }

    pub fn gen_buffers(&mut self, n: usize) -> Vec<u32> {
        let mut ids = Vec::with_capacity(n);
        for _ in 0..n {
            let id = self.next_id;
            self.next_id += 1;
            ids.push(id);
            self.buffers.insert(id, Buffer { data: Vec::new(), freq: 0 });
        }
        ids
    }

    pub fn delete_buffer(&mut self, id: u32) {
        self.buffers.remove(&id);
    }

    pub fn is_buffer(&self, id: u32) -> bool {
        self.buffers.contains_key(&id)
    }

    pub fn buffer_data(&mut self, id: u32, data: &[i16], freq: u32) {
        if let Some(buf) = self.buffers.get_mut(&id) {
            buf.data = data.to_vec();
            buf.freq = freq;
        }
    }

    pub fn queue_buffer(&mut self, src: u32, buf: u32) {
        if self.sources.contains_key(&src) && self.buffers.contains_key(&buf) {
            self.sources.get_mut(&src).unwrap().queue.push_back(buf);
        }
    }

    pub fn unqueue_buffer(&mut self, src: u32) -> Option<u32> {
        let source = self.sources.get_mut(&src)?;
        let processed = if source.state == State::Stopped {
            source.queue.len()
        } else if source.state == State::Initial {
            0
        } else {
            source.current
        };
        if processed > 0 {
            let id = source.queue.pop_front()?;
            source.current = source.current.saturating_sub(1);
            if source.state == State::Stopped {
                // current tracks queue.len()
                source.current = source.queue.len();
            }
            Some(id)
        } else {
            None
        }
    }

    pub fn play(&mut self, src: u32) {
        if let Some(source) = self.sources.get_mut(&src) {
            if source.state == State::Paused {
                source.state = State::Playing;
            } else {
                source.current = 0;
                source.cursor = 0.0;
                if source.queue.is_empty() {
                    source.state = State::Stopped;
                } else {
                    source.state = State::Playing;
                }
            }
        }
    }

    pub fn pause(&mut self, src: u32) {
        if let Some(source) = self.sources.get_mut(&src) {
            if source.state == State::Playing {
                source.state = State::Paused;
            }
        }
    }

    pub fn stop(&mut self, src: u32) {
        if let Some(source) = self.sources.get_mut(&src) {
            source.state = State::Stopped;
            source.current = source.queue.len();
            source.cursor = 0.0;
        }
    }

    pub fn state(&self, src: u32) -> State {
        self.sources.get(&src).map(|s| s.state).unwrap_or(State::Initial)
    }

    pub fn buffers_queued(&self, src: u32) -> i32 {
        self.sources.get(&src).map(|s| s.queue.len() as i32).unwrap_or(0)
    }

    pub fn buffers_processed(&self, src: u32) -> i32 {
        match self.sources.get(&src) {
            None => 0,
            Some(s) => match s.state {
                State::Stopped => s.queue.len() as i32,
                State::Initial => 0,
                State::Playing | State::Paused => s.current as i32,
            },
        }
    }

    pub fn sample_offset(&self, src: u32) -> i32 {
        match self.sources.get(&src).map(|s| s.state) {
            Some(State::Playing) | Some(State::Paused) => {
                let s = self.sources.get(&src).unwrap();
                let mut offset = 0.0;
                for i in 0..s.current {
                    if let Some(buf_id) = s.queue.get(i) {
                        if let Some(buf) = self.buffers.get(buf_id) {
                            offset += buf.data.len() as f64;
                        }
                    }
                }
                offset += s.cursor;
                offset.floor() as i32
            }
            _ => 0
        }
    }

    pub fn byte_offset(&self, src: u32) -> i32 {
        self.sample_offset(src) * 2
    }

    pub fn set_gain(&mut self, src: u32, gain: f32) {
        if let Some(s) = self.sources.get_mut(&src) {
            s.gain = gain;
        }
    }

    pub fn set_pitch(&mut self, src: u32, pitch: f32) {
        if let Some(s) = self.sources.get_mut(&src) {
            s.pitch = pitch;
        }
    }

    pub fn set_position(&mut self, src: u32, x: f32, y: f32, z: f32) {
        if let Some(s) = self.sources.get_mut(&src) {
            s.pos = [x, y, z];
        }
    }

    pub fn set_rolloff(&mut self, src: u32, rolloff: f32) {
        if let Some(s) = self.sources.get_mut(&src) {
            s.rolloff = rolloff;
        }
    }

    pub fn set_relative(&mut self, src: u32, relative: bool) {
        if let Some(s) = self.sources.get_mut(&src) {
            s.relative = relative;
        }
    }

    /// Adds every playing source to `out` (interleaved stereo at OUTPUT_RATE). Does not clear
    /// or clamp; the caller does.
    pub fn mix(&mut self, out: &mut [f32]) {
        let buffers = &self.buffers;
        for source in self.sources.values_mut() {
            if source.state == State::Playing {
                self.frames_mixed += source.mix_into(buffers, out);
            }
        }
    }

    /// Source-frames mixed since the previous call, then resets the count.
    pub fn take_frames_mixed(&mut self) -> u64 {
        std::mem::take(&mut self.frames_mixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: &mut AlStreams, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        a.mix(&mut out);
        out
    }

    #[test]
    fn test_basic_playback() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(3);
        
        // Create buffers with 100 samples each at OUTPUT_RATE
        for &buf_id in &bufs {
            let data = vec![0i16; 100];
            a.buffer_data(buf_id, &data, OUTPUT_RATE);
        }
        
        for &buf_id in &bufs {
            a.queue_buffer(srcs[0], buf_id);
        }
        
        a.play(srcs[0]);
        run(&mut a, 150);
        
        assert_eq!(a.buffers_processed(srcs[0]), 1);
        assert_eq!(a.buffers_queued(srcs[0]), 3);
        assert_eq!(a.state(srcs[0]), State::Playing);
        assert_eq!(a.sample_offset(srcs[0]), 150);
    }

    #[test]
    fn test_unqueue() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(3);
        
        for &buf_id in &bufs {
            let data = vec![0i16; 100];
            a.buffer_data(buf_id, &data, OUTPUT_RATE);
        }
        
        for &buf_id in &bufs {
            a.queue_buffer(srcs[0], buf_id);
        }
        
        // Before playing, no processed buffers
        assert_eq!(a.unqueue_buffer(srcs[0]), None);
        
        a.play(srcs[0]);
        let _ = run(&mut a, 100);
        
        // After 100 frames, first buffer should be processed
        assert_eq!(a.unqueue_buffer(srcs[0]), Some(bufs[0]));
        
        // Now 0 processed
        assert_eq!(a.unqueue_buffer(srcs[0]), None);
    }

    #[test]
    fn test_starvation() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(3);
        
        for &buf_id in &bufs {
            let data = vec![0i16; 100];
            a.buffer_data(buf_id, &data, OUTPUT_RATE);
        }
        
        for &buf_id in &bufs {
            a.queue_buffer(srcs[0], buf_id);
        }
        
        a.play(srcs[0]);
        let _ = run(&mut a, 300);
        
        assert_eq!(a.state(srcs[0]), State::Stopped);
        assert_eq!(a.buffers_processed(srcs[0]), 3);
        assert_eq!(a.take_frames_mixed(), 300);
        assert_eq!(a.take_frames_mixed(), 0);
    }

    #[test]
    fn test_stop_restart() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(2);
        
        for &buf_id in &bufs {
            let data = vec![0i16; 100];
            a.buffer_data(buf_id, &data, OUTPUT_RATE);
        }
        
        for &buf_id in &bufs {
            a.queue_buffer(srcs[0], buf_id);
        }
        
        a.play(srcs[0]);
        let _ = run(&mut a, 50);
        a.stop(srcs[0]);
        
        assert_eq!(a.buffers_processed(srcs[0]), 2);
        
        a.play(srcs[0]);
        assert_eq!(a.buffers_processed(srcs[0]), 0);
        assert_eq!(a.sample_offset(srcs[0]), 0);
        assert_eq!(a.state(srcs[0]), State::Playing);
    }

    #[test]
    fn test_pause_resume() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(1);
        
        let data = vec![32767i16; 100];
        a.buffer_data(bufs[0], &data, OUTPUT_RATE);
        a.queue_buffer(srcs[0], bufs[0]);
        
        a.play(srcs[0]);
        let _ = run(&mut a, 50);
        assert_eq!(a.sample_offset(srcs[0]), 50);
        
        a.pause(srcs[0]);
        run(&mut a, 10);
        assert_eq!(a.sample_offset(srcs[0]), 50);
        // No mixing should have happened effectively for offset, but let's check out
        // Since paused, mix does nothing for this source.
        
        a.play(srcs[0]);
        let _ = run(&mut a, 10);
        assert_eq!(a.sample_offset(srcs[0]), 60);
    }

    #[test]
    fn test_empty_queue() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        
        a.play(srcs[0]);
        assert_eq!(a.state(srcs[0]), State::Stopped);
    }

    #[test]
    fn test_interpolation_boundary() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(2);
        a.buffer_data(bufs[0], &vec![0i16; 100], OUTPUT_RATE);
        a.buffer_data(bufs[1], &vec![32767i16; 100], OUTPUT_RATE);
        a.queue_buffer(srcs[0], bufs[0]);
        a.queue_buffer(srcs[0], bufs[1]);
        a.set_pitch(srcs[0], 0.5); // half speed: frames land between samples
        a.play(srcs[0]);
        let out = run(&mut a, 200);
        // Frame 199 reads sample 99.5 of buffer A, halfway to B's first sample.
        let centre = 0.5f32.sqrt(); // centred source, constant-power pan
        assert!((out[199 * 2] - 0.5 * centre).abs() < 1e-3, "{}", out[199 * 2]);
        // Then buffer B plays.
        let out = run(&mut a, 2);
        assert!((out[0] - centre).abs() < 1e-3, "{}", out[0]);
        assert_eq!(a.buffers_processed(srcs[0]), 1);
    }

    #[test]
    fn test_byte_offset_counts_unprocessed_queue() {
        let mut a = AlStreams::default();
        let s = a.gen_sources(1)[0];
        let bufs = a.gen_buffers(2);
        for &b in &bufs {
            a.buffer_data(b, &[1i16; 100], OUTPUT_RATE);
            a.queue_buffer(s, b);
        }
        a.play(s);
        run(&mut a, 130);
        assert_eq!(a.byte_offset(s), 260); // first buffer processed but not unqueued yet
        a.unqueue_buffer(s);
        assert_eq!(a.byte_offset(s), 60); // offset is relative to the queue head
    }

    #[test]
    fn test_requeue_after_starvation() {
        // The game's Update(): a starved (stopped) source gets buffers unqueued, refilled,
        // queued again and played.
        let mut a = AlStreams::default();
        let s = a.gen_sources(1)[0];
        let b = a.gen_buffers(1)[0];
        a.buffer_data(b, &[1i16; 50], OUTPUT_RATE);
        a.queue_buffer(s, b);
        a.play(s);
        run(&mut a, 60);
        assert_eq!(a.state(s), State::Stopped);
        assert_eq!(a.unqueue_buffer(s), Some(b));
        assert_eq!(a.buffers_queued(s), 0);
        a.queue_buffer(s, b);
        a.play(s);
        assert_eq!(a.state(s), State::Playing);
        assert_eq!(a.buffers_processed(s), 0);
    }

    #[test]
    fn test_panning() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(1);
        
        let data = vec![32767i16; 100];
        a.buffer_data(bufs[0], &data, OUTPUT_RATE);
        a.queue_buffer(srcs[0], bufs[0]);
        
        a.play(srcs[0]);
        
        // Left source
        a.set_position(srcs[0], -1.0, 0.0, 0.0);
        let mut out_l = vec![0.0; 2];
        a.mix(&mut out_l);
        assert!(out_l[0] > 0.4, "Left channel should be significant");
        assert!(out_l[1] < 0.1, "Right channel should be near zero");
        
        // Right source
        a.set_position(srcs[0], 1.0, 0.0, 0.0);
        let mut out_r = vec![0.0; 2];
        a.mix(&mut out_r);
        assert!(out_r[0] < 0.1, "Left channel should be near zero");
        assert!(out_r[1] > 0.4, "Right channel should be significant");
    }

    #[test]
    fn test_mix_adds() {
        let mut a = AlStreams::default();
        let srcs = a.gen_sources(1);
        let bufs = a.gen_buffers(1);
        
        let data = vec![0i16; 100];
        a.buffer_data(bufs[0], &data, OUTPUT_RATE);
        a.queue_buffer(srcs[0], bufs[0]);
        
        a.play(srcs[0]);
        
        let mut out = vec![0.25; 2];
        a.mix(&mut out);
        // Since data is 0, output should remain 0.25
        assert!((out[0] - 0.25).abs() < 1e-6);
        assert!((out[1] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_unknown_ids() {
        let mut a = AlStreams::default();
        assert_eq!(a.state(999), State::Initial);
        assert_eq!(a.buffers_queued(999), 0);
        assert_eq!(a.buffers_processed(999), 0);
        assert_eq!(a.sample_offset(999), 0);
        assert_eq!(a.byte_offset(999), 0);
        assert!(!a.is_buffer(999));
        a.unqueue_buffer(999);
        a.play(999);
        a.pause(999);
        a.stop(999);
        a.set_gain(999, 1.0);
        a.set_pitch(999, 1.0);
        a.set_position(999, 0.0, 0.0, 0.0);
        a.set_rolloff(999, 1.0);
        a.set_relative(999, false);
        let mut out = vec![0.0; 2];
        a.mix(&mut out);
    }
}

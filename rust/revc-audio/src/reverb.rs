//! Environmental reverb approximating OpenAL's EFX reverb, driven by the game's EAX3 presets
//! (converted in sampman_oal.cpp, EAX_SetAll). Structure: input HF damping, pre-delay with early
//! reflection taps, two allpass diffusers, and a 4-line feedback delay network whose per-line
//! gains/damping give the requested decay time and HF decay ratio.
//! Not modelled: air absorption, room rolloff, LF controls, modulation, echo (see the A/B audio
//! comparison planned before OpenAL is removed). First draft by the local model; DSP core rewritten.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbParams {
    pub density: f32,           // 0..1
    pub diffusion: f32,         // 0..1
    pub gain: f32,              // overall wet gain, 0..1
    pub gain_hf: f32,           // high-frequency gain of the wet input, 0..1
    pub decay_time: f32,        // seconds, RT60 of the late reverb, 0.1..20
    pub decay_hf_ratio: f32,    // HF decay time / decay time, 0.1..2
    pub reflections_gain: f32,  // 0..3.16
    pub reflections_delay: f32, // seconds, 0..0.3
    pub late_gain: f32,         // 0..10
    pub late_delay: f32,        // seconds after the reflections, 0..0.1
}

impl Default for ReverbParams {
    /// EFX defaults.
    fn default() -> Self {
        ReverbParams {
            density: 1.0,
            diffusion: 1.0,
            gain: 0.32,
            gain_hf: 0.89,
            decay_time: 1.49,
            decay_hf_ratio: 0.83,
            reflections_gain: 0.05,
            reflections_delay: 0.007,
            late_gain: 1.26,
            late_delay: 0.011,
        }
    }
}

impl ReverbParams {
    fn clamped(&self) -> Self {
        ReverbParams {
            density: self.density.clamp(0.0, 1.0),
            diffusion: self.diffusion.clamp(0.0, 1.0),
            gain: self.gain.clamp(0.0, 1.0),
            gain_hf: self.gain_hf.clamp(0.0, 1.0),
            decay_time: self.decay_time.clamp(0.1, 20.0),
            decay_hf_ratio: self.decay_hf_ratio.clamp(0.1, 2.0),
            reflections_gain: self.reflections_gain.clamp(0.0, 3.16),
            reflections_delay: self.reflections_delay.clamp(0.0, 0.3),
            late_gain: self.late_gain.clamp(0.0, 10.0),
            late_delay: self.late_delay.clamp(0.0, 0.1),
        }
    }
}

const MAX_PREDELAY_S: f32 = 0.4;
const DIFFUSER_MS: [f32; 2] = [5.0, 1.7];
const LINE_MS: [f32; 4] = [29.7, 37.1, 41.1, 43.7];
const ER_TAPS: [f32; 4] = [1.0, 1.21, 1.37, 1.53];

fn flush(v: f32) -> f32 {
    if v.abs() < 1e-20 { 0.0 } else { v }
}

/// Ring buffer. Read before writing the current sample: `read(d)` is the input from `d` samples ago.
struct Delay {
    buf: Vec<f32>,
    pos: usize, // next write position
}

impl Delay {
    fn new(len: usize) -> Self {
        Delay { buf: vec![0.0; len.max(2)], pos: 0 }
    }
    fn read(&self, delay: usize) -> f32 {
        let n = self.buf.len();
        let d = delay.clamp(1, n);
        self.buf[(self.pos + n - d) % n]
    }
    fn write(&mut self, v: f32) {
        self.buf[self.pos] = v;
        self.pos = (self.pos + 1) % self.buf.len();
    }
}

pub struct Reverb {
    sample_rate: f32,
    params: ReverbParams,
    // state
    lp: f32,
    predelay: Delay,
    diffusers: [Delay; 2],
    lines: [Delay; 4],
    damp_state: [f32; 4],
    // coefficients
    lp_coeff: f32,
    diff_coeff: f32,
    diff_len: [usize; 2],
    er_delay: [usize; 4],
    late_delay: usize,
    line_len: [usize; 4],
    damp_a: [f32; 4],
    damp_b: [f32; 4],
}

impl Reverb {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(1) as f32;
        let samples = |ms: f32| (ms / 1000.0 * sr).ceil() as usize + 2;
        let mut r = Reverb {
            sample_rate: sr,
            params: ReverbParams::default(),
            lp: 0.0,
            predelay: Delay::new((MAX_PREDELAY_S * sr) as usize + 2),
            diffusers: DIFFUSER_MS.map(|ms| Delay::new(samples(ms))),
            lines: LINE_MS.map(|ms| Delay::new(samples(ms * 1.4))), // largest density scale
            damp_state: [0.0; 4],
            lp_coeff: (-2.0 * PI * 5000.0 / sr).exp(),
            diff_coeff: 0.0,
            diff_len: DIFFUSER_MS.map(|ms| ((ms / 1000.0 * sr) as usize).max(1)),
            er_delay: [1; 4],
            late_delay: 1,
            line_len: [1; 4],
            damp_a: [0.0; 4],
            damp_b: [0.0; 4],
        };
        r.recompute();
        r
    }

    /// Changes parameters without touching the delay state, so tails continue smoothly.
    pub fn set_params(&mut self, p: &ReverbParams) {
        self.params = p.clamped();
        self.recompute();
    }

    pub fn params(&self) -> ReverbParams {
        self.params
    }

    fn recompute(&mut self) {
        let p = self.params;
        let sr = self.sample_rate;
        self.diff_coeff = 0.6 * p.diffusion;
        for (i, t) in ER_TAPS.iter().enumerate() {
            self.er_delay[i] = ((p.reflections_delay * t * sr).round() as usize).max(1);
        }
        self.late_delay = (((p.reflections_delay + p.late_delay) * sr).round() as usize).max(1);
        let scale = 0.6 + 0.8 * p.density;
        for i in 0..4 {
            let len = ((LINE_MS[i] * scale / 1000.0 * sr).round() as usize).max(1);
            self.line_len[i] = len;
            // Per pass through a line of L samples the level must drop by L/(T60*sr) * 60 dB.
            let g = 10f32.powf(-3.0 * len as f32 / (p.decay_time * sr));
            let g_hf = 10f32.powf(-3.0 * len as f32 / (p.decay_time * p.decay_hf_ratio * sr));
            // One-pole lowpass with DC gain g and Nyquist gain g_hf (no HF boost above g).
            let r = (g_hf / g).min(1.0);
            let a = (1.0 - r) / (1.0 + r);
            self.damp_a[i] = a;
            self.damp_b[i] = g * (1.0 - a);
        }
    }

    /// Adds the wet signal for the mono `input` into the interleaved stereo `out`.
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) {
        let p = self.params;
        for (&x, frame) in input.iter().zip(out.chunks_exact_mut(2)) {
            // 1. HF damping of the wet input (gain_hf at ~5 kHz and above).
            self.lp = flush((1.0 - self.lp_coeff) * x + self.lp_coeff * self.lp);
            let x2 = self.lp + p.gain_hf * (x - self.lp);

            // 2. Early reflections from the pre-delay line (read before this sample is written).
            let t = self.er_delay.map(|d| self.predelay.read(d));
            let er_l = p.reflections_gain * (0.6 * t[0] + 0.45 * t[2]);
            let er_r = p.reflections_gain * (0.5 * t[1] + 0.4 * t[3]);

            // 3. Late input: pre-delayed, then two Schroeder allpass diffusers.
            let mut v = self.predelay.read(self.late_delay);
            self.predelay.write(x2);
            for k in 0..2 {
                let d = self.diffusers[k].read(self.diff_len[k]);
                let w = flush(v - self.diff_coeff * d);
                self.diffusers[k].write(w);
                v = d + self.diff_coeff * w;
            }

            // 4. Feedback delay network: read, damp, Householder mix, write back.
            let outs: [f32; 4] = std::array::from_fn(|i| self.lines[i].read(self.line_len[i]));
            let mut f = [0.0f32; 4];
            for i in 0..4 {
                self.damp_state[i] = flush(self.damp_b[i] * outs[i] + self.damp_a[i] * self.damp_state[i]);
                f[i] = self.damp_state[i];
            }
            let half_sum = 0.5 * (f[0] + f[1] + f[2] + f[3]);
            for i in 0..4 {
                self.lines[i].write(f[i] - half_sum + 0.5 * v);
            }
            let late_l = p.late_gain * 0.5 * (outs[0] + outs[2]);
            let late_r = p.late_gain * 0.5 * (outs[1] + outs[3]);

            frame[0] += p.gain * (er_l + late_l);
            frame[1] += p.gain * (er_r + late_r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    fn late_only(decay: f32) -> ReverbParams {
        ReverbParams {
            decay_time: decay,
            density: 1.0,
            diffusion: 1.0,
            gain: 1.0,
            gain_hf: 1.0,
            decay_hf_ratio: 1.0,
            reflections_gain: 0.0,
            late_gain: 1.0,
            ..Default::default()
        }
    }

    fn impulse_response(p: &ReverbParams, seconds: f32) -> Vec<f32> {
        let mut r = Reverb::new(SR);
        r.set_params(p);
        let n = (seconds * SR as f32) as usize;
        let mut input = vec![0.0; n];
        input[0] = 1.0;
        let mut out = vec![0.0; n * 2];
        r.process(&input, &mut out);
        out
    }

    /// Time (s) at which 50 ms window energy has fallen 60 dB below the loudest window.
    fn measured_rt60(out: &[f32]) -> f32 {
        let win = (SR as f32 * 0.05) as usize * 2;
        let e: Vec<f32> = out.chunks(win).map(|w| w.iter().map(|v| v * v).sum()).collect();
        let (imax, &max) = e.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        let k = e.iter().skip(imax).position(|&v| v < max * 1e-6).expect("no 60 dB decay") + imax;
        k as f32 * 0.05
    }

    #[test]
    fn silence_in_silence_out() {
        let mut r = Reverb::new(SR);
        let mut out = vec![0.3; 200];
        r.process(&[0.0; 100], &mut out);
        assert!(out.iter().all(|&v| v == 0.3));
    }

    #[test]
    fn predelay() {
        let p = ReverbParams { reflections_delay: 0.02, reflections_gain: 1.0, gain: 1.0, ..late_only(0.5) };
        let out = impulse_response(&p, 0.1);
        let first = out.iter().position(|v| *v != 0.0).unwrap() / 2;
        assert_eq!(first, 960, "first wet sample at the reflections delay");
    }

    #[test]
    fn rt60_matches_decay_time() {
        for decay in [0.5f32, 1.0, 2.0, 4.42] {
            let t = measured_rt60(&impulse_response(&late_only(decay), decay * 2.5 + 0.5));
            assert!((t / decay - 1.0).abs() < 0.3, "decay {decay}: measured {t}");
        }
    }

    #[test]
    fn longer_decay_has_longer_tail() {
        let e = |d| impulse_response(&late_only(d), 2.0)[144000..192000].iter().map(|v| v * v).sum::<f32>();
        assert!(e(3.0) > e(1.0) * 10.0);
    }

    #[test]
    fn hf_ratio_darkens_tail() {
        // With a low HF ratio the tail loses its sample-to-sample (high-frequency) variation.
        let roughness = |out: &[f32]| out[96000..144000].chunks_exact(2).map(|f| f[0]).collect::<Vec<_>>()
            .windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>()
            / out[96000..144000].iter().map(|v| v.abs()).sum::<f32>();
        let bright = roughness(&impulse_response(&late_only(2.0), 1.6));
        let dark = roughness(&impulse_response(&ReverbParams { decay_hf_ratio: 0.2, ..late_only(2.0) }, 1.6));
        assert!(dark < bright * 0.7, "dark {dark} bright {bright}");
    }

    #[test]
    fn gain_scales_output() {
        let sum = |g| impulse_response(&ReverbParams { gain: g, ..late_only(1.0) }, 0.5).iter().map(|v| v.abs()).sum::<f32>();
        let (a, b) = (sum(1.0), sum(0.5));
        assert!((a - 2.0 * b).abs() < a * 0.01);
    }

    #[test]
    fn stable_with_long_decay() {
        let mut r = Reverb::new(SR);
        r.set_params(&late_only(20.0));
        let mut state: u32 = 12345;
        let noise: Vec<f32> = (0..SR * 10)
            .map(|_| {
                state = state.wrapping_mul(1103515245).wrapping_add(12345);
                (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
            })
            .collect();
        let mut out = vec![0.0; noise.len() * 2];
        r.process(&noise, &mut out);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 100.0));
    }

    #[test]
    fn set_params_keeps_tail() {
        let mut r = Reverb::new(SR);
        r.set_params(&late_only(1.0));
        let mut input = vec![0.0; 9600];
        input[0] = 1.0;
        let mut out = vec![0.0; 19200];
        r.process(&input, &mut out);
        r.set_params(&ReverbParams { gain: 0.5, ..late_only(1.0) });
        let mut after = vec![0.0; 200];
        r.process(&[0.0; 100], &mut after);
        assert!(after.iter().any(|v| v.abs() > 1e-6));
    }

    #[test]
    fn game_presets_are_stable() {
        // The two EAX3 presets the game interpolates between (converted like EFX_Set).
        let mb = |m: f32| 10f32.powf(m / 2000.0);
        let start = ReverbParams {
            density: (1.7f32.powi(3) / 16.0).min(1.0), diffusion: 0.8, gain: mb(-1000.0), gain_hf: mb(-1000.0),
            decay_time: 4.42, decay_hf_ratio: 0.14, reflections_gain: mb(429.0), reflections_delay: 0.014,
            late_gain: mb(1023.0), late_delay: 0.021,
        };
        let finish = ReverbParams {
            density: 1.0, diffusion: 1.0, gain: 1.0, gain_hf: mb(-1000.0), decay_time: 20.0, decay_hf_ratio: 1.39,
            reflections_gain: mb(1000.0), reflections_delay: 0.069, late_gain: mb(400.0), late_delay: 0.1,
        };
        for p in [start, finish] {
            let out = impulse_response(&p, 3.0);
            assert!(out.iter().all(|v| v.is_finite() && v.abs() < 10.0));
            assert!(out.iter().any(|v| v.abs() > 1e-4));
        }
    }
}

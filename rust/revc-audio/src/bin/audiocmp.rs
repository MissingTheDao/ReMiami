//! A/B audio comparison of two recordings of the same scene (scripts/audio-ab.sh).
//!
//! usage: audiocmp <reference.wav> <candidate.wav[+other.wav...]> [--json]
//! A candidate given as `a.wav+b.wav` is the sum of those recordings (a build that plays through
//! two outputs at once, e.g. sound effects on SDL and streams on OpenAL).
//! Exit code 0 if all checks pass, 1 if any fails, 2 on usage/IO errors.
//!
//! Recordings never match sample for sample (different engines, start times and game randomness),
//! so the comparison works on robust measures: the candidate is aligned to the reference by
//! cross-correlating 50 ms loudness envelopes, then overall level, left/right balance, the
//! low/mid/high band shape and the loudness-over-time correlation are compared on the windows
//! where there is sound. (A local-model draft got the stereo frame handling wrong throughout;
//! rewritten around explicit frames.)

use std::process::ExitCode;

const RATE: u32 = 48000;
const WIN: usize = 2400; // 50 ms
const MAX_LAG: i64 = 400; // windows (+-20 s)
const SILENCE_DB: f64 = -60.0;

type Frame = (f32, f32);

// ---------------------------------------------------------------- WAV input

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Parses PCM16, float32 and WAVE_FORMAT_EXTENSIBLE (of those) WAV data into stereo frames.
fn parse_wav(b: &[u8]) -> Result<(u32, Vec<Frame>), String> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let (mut fmt, mut data) = (None, None);
    let mut o = 12;
    while o + 8 <= b.len() {
        let id = &b[o..o + 4];
        let mut size = u32le(b, o + 4) as usize;
        let body = o + 8;
        if id == b"data" && (size == 0 || size == 0xFFFF_FFFF || body + size > b.len()) {
            size = b.len() - body; // writer was killed before finalizing the header
        }
        let end = (body + size).min(b.len());
        match id {
            b"fmt " => fmt = Some(&b[body..end]),
            b"data" => data = Some(&b[body..end]),
            _ => {}
        }
        o = end + (size & 1);
    }
    let fmt = fmt.ok_or("no fmt chunk")?;
    let data = data.ok_or("no data chunk")?;
    if fmt.len() < 16 {
        return Err("short fmt chunk".into());
    }
    let mut tag = u16le(fmt, 0);
    let channels = u16le(fmt, 2) as usize;
    let rate = u32le(fmt, 4);
    let bits = u16le(fmt, 14);
    if tag == 0xFFFE {
        if fmt.len() < 26 {
            return Err("short extensible fmt chunk".into());
        }
        tag = u16le(fmt, 24);
    }
    if channels == 0 || channels > 2 || rate == 0 {
        return Err(format!("unsupported layout: {channels} channels at {rate} Hz"));
    }
    let samples: Vec<f32> = match (tag, bits) {
        (1, 16) => data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
        (3, 32) => data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
        _ => return Err(format!("unsupported sample format: tag {tag}, {bits} bits")),
    };
    let frames = if channels == 1 {
        samples.iter().map(|&s| (s, s)).collect()
    } else {
        samples.chunks_exact(2).map(|c| (c[0], c[1])).collect()
    };
    Ok((rate, frames))
}

/// Linear-interpolation resampling of stereo frames.
fn resample(frames: &[Frame], from: u32, to: u32) -> Vec<Frame> {
    if from == to || frames.is_empty() {
        return frames.to_vec();
    }
    let n = (frames.len() as u64 * to as u64 / from as u64) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * from as f64 / to as f64;
            let j = (pos as usize).min(frames.len() - 1);
            let k = (j + 1).min(frames.len() - 1);
            let t = (pos - j as f64) as f32;
            (frames[j].0 + (frames[k].0 - frames[j].0) * t, frames[j].1 + (frames[k].1 - frames[j].1) * t)
        })
        .collect()
}

fn load_one(path: &str) -> Result<Vec<Frame>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let (rate, frames) = parse_wav(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(resample(&frames, rate, RATE))
}

/// Loads `a.wav` or the sum of `a.wav+b.wav+...` (aligned at their starts).
fn load(spec: &str) -> Result<Vec<Frame>, String> {
    let mut sum: Vec<Frame> = Vec::new();
    for part in spec.split('+') {
        let f = load_one(part)?;
        if f.len() > sum.len() {
            sum.resize(f.len(), (0.0, 0.0));
        }
        for (s, x) in sum.iter_mut().zip(f) {
            s.0 += x.0;
            s.1 += x.1;
        }
    }
    Ok(sum)
}

// ---------------------------------------------------------------- analysis

fn db(rms: f64) -> f64 {
    20.0 * rms.max(1e-6).log10()
}

/// Loudness envelope: RMS of the mid signal (l+r)/2 per 50 ms window, in dB.
fn envelope_db(frames: &[Frame]) -> Vec<f64> {
    frames
        .chunks_exact(WIN)
        .map(|w| db((w.iter().map(|&(l, r)| ((l + r) as f64 / 2.0).powi(2)).sum::<f64>() / WIN as f64).sqrt()))
        .collect()
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len());
    if n < 2 {
        return 0.0;
    }
    let (mx, my) = (x[..n].iter().sum::<f64>() / n as f64, y[..n].iter().sum::<f64>() / n as f64);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (dx, dy) = (x[i] - mx, y[i] - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return 0.0;
    }
    sxy / (sxx * syy).sqrt()
}

/// Window pairs (reference index, candidate index) overlapping when the candidate is shifted by
/// `lag` windows: candidate window i lines up with reference window i - lag.
fn overlap(nref: usize, ncand: usize, lag: i64) -> Vec<(usize, usize)> {
    (0..ncand as i64)
        .filter_map(|i| {
            let r = i - lag;
            (r >= 0 && (r as usize) < nref).then_some((r as usize, i as usize))
        })
        .collect()
}

/// The lag (in windows; positive = the candidate starts later) maximising envelope correlation.
fn best_lag(eref: &[f64], ecand: &[f64]) -> Option<i64> {
    let mut best: Option<(i64, f64)> = None;
    for lag in -MAX_LAG..=MAX_LAG {
        let pairs = overlap(eref.len(), ecand.len(), lag);
        if pairs.len() < 100 {
            continue;
        }
        let x: Vec<f64> = pairs.iter().map(|&(r, _)| eref[r]).collect();
        let y: Vec<f64> = pairs.iter().map(|&(_, c)| ecand[c]).collect();
        let c = pearson(&x, &y);
        if best.is_none_or(|(_, bc)| c > bc) {
            best = Some((lag, c));
        }
    }
    best.map(|(l, _)| l)
}

/// Overall measures of a set of frames.
#[derive(Clone, Copy, Debug, Default)]
struct Measures {
    level_db: f64,
    balance_db: f64,
    // Band levels relative to level_db (band shape, independent of overall loudness).
    low_db: f64,
    mid_db: f64,
    high_db: f64,
}

fn one_pole_lp(x: &[f64], cutoff: f64) -> Vec<f64> {
    let a = (-2.0 * std::f64::consts::PI * cutoff / RATE as f64).exp();
    let mut y = 0.0;
    x.iter().map(|&v| { y = (1.0 - a) * v + a * y; y }).collect()
}

fn rms(v: impl Iterator<Item = f64>) -> f64 {
    let (s, n) = v.fold((0.0, 0usize), |(s, n), x| (s + x * x, n + 1));
    if n == 0 { 0.0 } else { (s / n as f64).sqrt() }
}

fn measure(frames: &[Frame]) -> Measures {
    let mid: Vec<f64> = frames.iter().map(|&(l, r)| (l + r) as f64 / 2.0).collect();
    let level_db = db(rms(mid.iter().copied()));
    let lp300 = one_pole_lp(&mid, 300.0);
    let lp3k = one_pole_lp(&mid, 3000.0);
    Measures {
        level_db,
        balance_db: db(rms(frames.iter().map(|f| f.0 as f64))) - db(rms(frames.iter().map(|f| f.1 as f64))),
        low_db: db(rms(lp300.iter().copied())) - level_db,
        mid_db: db(rms(lp3k.iter().zip(&lp300).map(|(a, b)| a - b))) - level_db,
        high_db: db(rms(mid.iter().zip(&lp3k).map(|(a, b)| a - b))) - level_db,
    }
}

/// Frames of the given windows, concatenated.
fn gather(frames: &[Frame], windows: impl Iterator<Item = usize>) -> Vec<Frame> {
    windows.flat_map(|w| frames[w * WIN..(w + 1) * WIN].iter().copied()).collect()
}

struct Report {
    durations: (f64, f64),
    lag_s: f64,
    active: usize,
    reference: Measures,
    candidate: Measures,
    envelope_corr: f64,
    checks: Vec<(String, f64, f64, bool)>, // name, value, limit, pass
}

fn compare(a: &[Frame], b: &[Frame]) -> Result<Report, String> {
    let (ea, eb) = (envelope_db(a), envelope_db(b));
    let lag = best_lag(&ea, &eb).ok_or("recordings too short to align (need 5 s of overlap)")?;
    let active: Vec<(usize, usize)> = overlap(ea.len(), eb.len(), lag)
        .into_iter()
        .filter(|&(r, c)| ea[r] > SILENCE_DB || eb[c] > SILENCE_DB)
        .collect();
    if active.len() < 40 {
        return Err(format!("not enough audio: {} active windows (need 40)", active.len()));
    }
    let ra = measure(&gather(a, active.iter().map(|p| p.0)));
    let rb = measure(&gather(b, active.iter().map(|p| p.1)));
    let corr = pearson(
        &active.iter().map(|p| ea[p.0]).collect::<Vec<_>>(),
        &active.iter().map(|p| eb[p.1]).collect::<Vec<_>>(),
    );
    let diff = |name: &str, x: f64, y: f64, limit: f64| (name.to_string(), y - x, limit, (y - x).abs() <= limit);
    let checks = vec![
        diff("level (dB)", ra.level_db, rb.level_db, 3.0),
        diff("balance L/R (dB)", ra.balance_db, rb.balance_db, 2.0),
        diff("low band <300 Hz (dB)", ra.low_db, rb.low_db, 4.0),
        diff("mid band (dB)", ra.mid_db, rb.mid_db, 4.0),
        diff("high band >3 kHz (dB)", ra.high_db, rb.high_db, 4.0),
        ("loudness-over-time correlation".to_string(), corr, 0.6, corr >= 0.6),
    ];
    Ok(Report {
        durations: (a.len() as f64 / RATE as f64, b.len() as f64 / RATE as f64),
        lag_s: lag as f64 * WIN as f64 / RATE as f64,
        active: active.len(),
        reference: ra,
        candidate: rb,
        envelope_corr: corr,
        checks,
    })
}

fn print_text(r: &Report) {
    println!("durations: reference {:.1}s, candidate {:.1}s; candidate offset {:+.2}s; {} active windows ({:.1}s)",
        r.durations.0, r.durations.1, r.lag_s, r.active, r.active as f64 * WIN as f64 / RATE as f64);
    println!("{:<34} {:>10} {:>10}", "", "reference", "candidate");
    let m = |name: &str, a: f64, b: f64| println!("{name:<34} {a:>10.2} {b:>10.2}");
    m("level (dB)", r.reference.level_db, r.candidate.level_db);
    m("balance L/R (dB)", r.reference.balance_db, r.candidate.balance_db);
    m("low band, rel. to level (dB)", r.reference.low_db, r.candidate.low_db);
    m("mid band, rel. to level (dB)", r.reference.mid_db, r.candidate.mid_db);
    m("high band, rel. to level (dB)", r.reference.high_db, r.candidate.high_db);
    println!();
    for (name, v, limit, ok) in &r.checks {
        let what = if name.starts_with("loudness") { format!("{v:.2} (min {limit})") } else { format!("{v:+.2} (limit +-{limit})") };
        println!("{} {name}: {what}", if *ok { "PASS" } else { "FAIL" });
    }
}

fn print_json(r: &Report) {
    let m = |x: &Measures| format!(
        "{{\"level_db\":{:.3},\"balance_db\":{:.3},\"low_db\":{:.3},\"mid_db\":{:.3},\"high_db\":{:.3}}}",
        x.level_db, x.balance_db, x.low_db, x.mid_db, x.high_db
    );
    let pass = r.checks.iter().all(|c| c.3);
    println!("{{\"lag_s\":{:.3},\"active_windows\":{},\"reference\":{},\"candidate\":{},\"envelope_corr\":{:.4},\"pass\":{}}}",
        r.lag_s, r.active, m(&r.reference), m(&r.candidate), r.envelope_corr, pass);
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.len() != 2 {
        eprintln!("usage: audiocmp <reference.wav> <candidate.wav> [--json]");
        return ExitCode::from(2);
    }
    let (a, b) = match (load(files[0]), load(files[1])) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("audiocmp: {e}");
            return ExitCode::from(2);
        }
    };
    match compare(&a, &b) {
        Ok(r) => {
            if json { print_json(&r) } else { print_text(&r) }
            if r.checks.iter().all(|c| c.3) { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
            eprintln!("audiocmp: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(tag: u16, bits: u16, channels: u16, samples: &[u8]) -> Vec<u8> {
        let mut w = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        w.extend_from_slice(&tag.to_le_bytes());
        w.extend_from_slice(&channels.to_le_bytes());
        w.extend_from_slice(&RATE.to_le_bytes());
        w.extend_from_slice(&(RATE * channels as u32 * bits as u32 / 8).to_le_bytes());
        w.extend_from_slice(&(channels * bits / 8).to_le_bytes());
        w.extend_from_slice(&bits.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        w.extend_from_slice(samples);
        w
    }

    /// Synthetic "scene": noise bursts of varying loudness, deterministic.
    fn scene(seconds: usize, seed: u32) -> Vec<Frame> {
        let mut s = seed;
        (0..seconds * RATE as usize)
            .map(|i| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let n = (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                let env = if (i / 12000) % 3 == 0 { 0.5 } else { 0.05 } * (1.0 + (i as f32 / 30000.0).sin() * 0.5);
                (n * env, n * env * 0.8)
            })
            .collect()
    }

    #[test]
    fn parses_pcm16_stereo_and_float_mono() {
        let pcm: Vec<u8> = [16384i16, -16384, 0, 32767].iter().flat_map(|v| v.to_le_bytes()).collect();
        let (rate, f) = parse_wav(&wav(1, 16, 2, &pcm)).unwrap();
        assert_eq!(rate, RATE);
        assert_eq!(f, vec![(0.5, -0.5), (0.0, 32767.0 / 32768.0)]);
        let flt: Vec<u8> = [0.25f32, -1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let (_, f) = parse_wav(&wav(3, 32, 1, &flt)).unwrap();
        assert_eq!(f, vec![(0.25, 0.25), (-1.0, -1.0)]);
    }

    #[test]
    fn unfinalized_data_size_uses_rest_of_file() {
        let flt: Vec<u8> = [0.5f32, 0.5].iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut w = wav(3, 32, 2, &flt);
        let n = w.len();
        w[n - 12..n - 8].copy_from_slice(&0u32.to_le_bytes()); // data size = 0
        assert_eq!(parse_wav(&w).unwrap().1.len(), 1);
    }

    #[test]
    fn identical_recordings_pass_with_zero_lag() {
        let a = scene(20, 1);
        let r = compare(&a, &a).unwrap();
        assert_eq!(r.lag_s, 0.0);
        assert!(r.checks.iter().all(|c| c.3));
        assert!((r.envelope_corr - 1.0).abs() < 1e-9);
    }

    #[test]
    fn finds_offset_of_a_later_start() {
        let a = scene(20, 1);
        let mut b = vec![(0.0, 0.0); 7 * WIN]; // candidate starts 7 windows (0.35 s) later
        b.extend_from_slice(&a);
        let r = compare(&a, &b).unwrap();
        assert!((r.lag_s - 0.35).abs() < 1e-9, "lag {}", r.lag_s);
        assert!(r.checks.iter().all(|c| c.3));
    }

    #[test]
    fn detects_level_and_balance_changes() {
        let a = scene(20, 1);
        let quieter: Vec<Frame> = a.iter().map(|&(l, r)| (l * 0.5, r * 0.5)).collect();
        let r = compare(&a, &quieter).unwrap();
        assert!((r.checks[0].1 + 6.02).abs() < 0.05 && !r.checks[0].3, "{:?}", r.checks[0]);
        let swapped: Vec<Frame> = a.iter().map(|&(l, r)| (r, l)).collect();
        let r = compare(&a, &swapped).unwrap();
        assert!(!r.checks[1].3, "balance should fail: {:?}", r.checks[1]);
    }

    #[test]
    fn band_shape_separates_low_and_high() {
        let tone = |hz: f64| -> Vec<Frame> {
            (0..RATE).map(|i| { let v = (i as f64 * hz * 2.0 * std::f64::consts::PI / RATE as f64).sin() as f32 * 0.5; (v, v) }).collect()
        };
        let low = measure(&tone(100.0));
        let high = measure(&tone(8000.0));
        assert!(low.low_db > -1.0 && low.high_db < -20.0, "{low:?}");
        assert!(high.high_db > -3.0 && high.low_db < -20.0, "{high:?}"); // one-pole slopes are gentle
    }

    #[test]
    fn silence_is_not_enough_audio() {
        let s = vec![(0.0, 0.0); 20 * RATE as usize];
        assert!(compare(&s, &s).is_err());
    }
}

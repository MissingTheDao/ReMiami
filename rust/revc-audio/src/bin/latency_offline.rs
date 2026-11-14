//! Offline mixer latency (L_mix, LATENCY_METHOD.md 5.1) of reMiami's sound-effect mixer: an
//! impulse started between two mix calls, rendered at callback sizes 256/512/1024, and the frame
//! where it comes out, relative to the first block mixed after the start. P48 is a 48 kHz impulse
//! (no resampling: expected 0), P22 a 22.05 kHz one (resampling: the peak's offset).
//!
//! usage: latency_offline [--wav <dir>]   (writes each render as a 32-bit float stereo WAV)

use revc_audio::mixer::{Mixer, OUTPUT_RATE};
use std::path::Path;

const BLOCKS: usize = 20;
const START_BEFORE_BLOCK: usize = 6;

fn write_wav(path: &Path, data: &[f32]) -> std::io::Result<()> {
    let bytes = (data.len() * 4) as u32;
    let mut w = Vec::with_capacity(44 + bytes as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&OUTPUT_RATE.to_le_bytes());
    w.extend_from_slice(&(OUTPUT_RATE * 8).to_le_bytes());
    w.extend_from_slice(&8u16.to_le_bytes());
    w.extend_from_slice(&32u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&bytes.to_le_bytes());
    for s in data {
        w.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, w)
}

/// (offset of the onset from the first block after the start, peak |left|)
fn measure(block: usize, rate: u32, peak_mode: bool, wav: Option<&Path>, name: &str) -> (i64, f32) {
    let mut impulse = vec![0i16; (rate / 10) as usize];
    impulse[0] = 29490; // 0.9 full scale
    let mut mixer = Mixer::new(4);
    mixer.set_2d(0, true);
    let mut out = vec![0.0f32; BLOCKS * block * 2];
    for b in 0..BLOCKS {
        if b == START_BEFORE_BLOCK {
            mixer.start(0, &impulse, rate);
        }
        mixer.mix(&mut out[b * block * 2..(b + 1) * block * 2]);
    }
    let first = START_BEFORE_BLOCK * block;
    let left = |f: usize| out[f * 2].abs();
    let frames = first..BLOCKS * block;
    let peak = frames.clone().map(left).fold(0.0f32, f32::max);
    let onset = if peak_mode {
        frames.clone().find(|&f| left(f) == peak)
    } else {
        frames.clone().find(|&f| left(f) > 0.25)
    };
    if let Some(dir) = wav {
        let p = dir.join(format!("lmix_{name}_{block}.wav"));
        if let Err(e) = write_wav(&p, &out) {
            eprintln!("{}: {e}", p.display());
        }
    }
    (onset.map_or(-1, |f| (f - first) as i64), peak)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wav = args.iter().position(|a| a == "--wav").and_then(|i| args.get(i + 1)).map(Path::new);
    for block in [256, 512, 1024] {
        for (name, rate, peak_mode) in [("P48", 48000, false), ("P22", 22050, true)] {
            let (offset, peak) = measure(block, rate, peak_mode, wav, name);
            println!("block={block} probe={name} offset_frames={offset} peak={peak:.4}");
        }
    }
}

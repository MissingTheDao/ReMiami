#[cfg(test)]
mod tonelist {
    use crate::audio::{AudioBackend, RevcBackend};
    use crate::audio::RetroBackend;
    use revc_audio::mixer::OUTPUT_RATE;

    const REFERENCE_DB: f32 = -170.0;
    const TONE_FREQ: f32 = 1000.0;
    const TONE_RATE: u32 = 22050;
    const TONE_AMPLITUDE: i16 = 16384;
    const TONE_DURATION_SAMPLES: usize = 22050;
    const CLICK_DURATION_SAMPLES: usize = 2205;
    const RENDER_BLOCK: usize = 512;
    const MEASURE_RENDER_1: f32 = 0.7;
    const MEASURE_RENDER_2: f32 = 0.25;
    const MEASURE_START: f32 = 0.15;
    const MEASURE_END: f32 = 0.60;

    fn generate_tone() -> Vec<i16> {
        let mut buf = vec![0i16; TONE_DURATION_SAMPLES];
        for i in 0..TONE_DURATION_SAMPLES {
            let phase = (2.0 * std::f32::consts::PI * TONE_FREQ * i as f32 / TONE_RATE as f32).sin();
            buf[i] = (phase * TONE_AMPLITUDE as f32) as i16;
        }
        buf
    }

    fn generate_click() -> Vec<i16> {
        let tone = generate_tone();
        tone[..CLICK_DURATION_SAMPLES].to_vec()
    }

    /// Plays `data` on channel 0 with these parameters; (rms L, rms R over 0.15..0.60 s, seconds above 1e-3).
    fn measure(b: &mut dyn AudioBackend, data: &[i16], gain: f32, pos: [f32; 3], min: f32, max: f32, rate: u32, loops: i32) -> (f32, f32, f32) {
        b.sfx_set_2d(0, false);
        b.sfx_set_gain(0, gain);
        b.sfx_set_position(0, pos[0], pos[1], pos[2]);
        b.sfx_set_distances(0, max, min);
        b.sfx_set_pitch(0, rate as f32 / TONE_RATE as f32);
        b.sfx_set_loop_count(0, loops);
        b.sfx_set_reverb_mix(0, 0.0);
        b.sfx_start(0, data, TONE_RATE);
        let frames = (MEASURE_RENDER_1 * OUTPUT_RATE as f32) as usize;
        let mut buf = vec![0.0f32; frames * 2];
        let mut done = 0;
        while done < frames {
            let n = RENDER_BLOCK.min(frames - done);
            b.render(&mut buf[done * 2..(done + n) * 2]);
            done += n;
        }
        b.sfx_stop(0);
        let mut tail = vec![0.0f32; (MEASURE_RENDER_2 * OUTPUT_RATE as f32) as usize * 2];
        b.render(&mut tail);
        let (a, z) = ((MEASURE_START * OUTPUT_RATE as f32) as usize, (MEASURE_END * OUTPUT_RATE as f32) as usize);
        let rms = |c: usize| ((a..z).map(|f| buf[f * 2 + c] * buf[f * 2 + c]).sum::<f32>() / (z - a) as f32).sqrt();
        let loud: Vec<usize> = (0..frames).filter(|&f| buf[f * 2].abs().max(buf[f * 2 + 1].abs()) > 1e-3).collect();
        let dur = match (loud.first(), loud.last()) {
            (Some(f), Some(l)) => (l - f + 1) as f32 / OUTPUT_RATE as f32,
            _ => 0.0,
        };
        (rms(0), rms(1), dur)
    }

    #[test]
    #[ignore]
    fn tone_list_report() {
        let tone = generate_tone();
        let click = generate_click();
        // (group, index, data is click, gain, position, min, max, rate, loops)
        let mut list: Vec<(&str, i32, bool, f32, [f32; 3], f32, f32, u32, i32)> = Vec::new();
        for (i, vol) in [127, 96, 64, 32, 16].into_iter().enumerate() {
            list.push(("volume", i as i32, false, vol as f32 / 127.0, [0.0, 0.0, 1.0], 1.0, 100.0, 22050, 1));
        }
        for i in 0..24 {
            let az = (i * 15) as f32 * std::f32::consts::PI / 180.0;
            list.push(("pan", i, false, 1.0, [2.0 * az.sin(), 0.0, 2.0 * az.cos()], 1.0, 100.0, 22050, 1));
        }
        for (i, d) in [0.5f32, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0].into_iter().enumerate() {
            list.push(("distance", i as i32, false, 1.0, [0.0, 0.0, d], 1.0, 100.0, 22050, 1));
        }
        for (i, d) in [0.5f32, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0].into_iter().enumerate() {
            list.push(("distance", i as i32 + 100, false, 1.0, [0.0, 0.0, d], 5.0, 50.0, 22050, 1));
        }
        for (i, r) in [11025u32, 22050, 44100].into_iter().enumerate() {
            list.push(("pitch", i as i32, false, 1.0, [0.0, 0.0, 1.0], 1.0, 100.0, r, 1));
        }
        list.push(("loop", 0, true, 1.0, [0.0, 0.0, 1.0], 1.0, 100.0, 22050, 3));
        let backends: Vec<(&str, Box<dyn AudioBackend>)> = vec![("revc", Box::new(RevcBackend::new())), ("retro", Box::new(RetroBackend::new()))];
        for (name, mut b) in backends {
            b.sfx_open(8);
            let mut reference = 0.0;
            for (g, i, is_click, gain, pos, min, max, rate, loops) in &list {
                let data: &[i16] = if *is_click { &click } else { &tone };
                let (l, r, dur) = measure(b.as_mut(), data, *gain, *pos, *min, *max, *rate, *loops);
                if *g == "volume" && *i == 0 {
                    reference = (l + r) / 2.0;
                }
                let db = |v: f32| if v <= 0.0 || reference <= 0.0 { REFERENCE_DB } else { 20.0 * (v / reference).log10() };
                let shown = if *g == "pitch" || *g == "loop" { dur } else { 0.0 };
                println!("{name} {g} {i} {:.1} {:.1} {shown:.2}", db(l), db(r));
            }
        }
    }
}

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
                    // absolute level of the 1 m front tone (a full-scale tone's rms per channel would be 0.5 * 0.7071)
                    println!("{name} reference_rms {reference:.5} ({:.2} dB vs 0.5 * full-scale rms)", 20.0 * (reference / (0.5 * 0.5 * std::f32::consts::FRAC_1_SQRT_2)).log10());
                }
                let db = |v: f32| if v <= 0.0 || reference <= 0.0 { REFERENCE_DB } else { 20.0 * (v / reference).log10() };
                let shown = if *g == "pitch" || *g == "loop" { dur } else { 0.0 };
                println!("{name} {g} {i} {:.1} {:.1} {shown:.2}", db(l), db(r));
            }
        }
    }

    /// One click through a fresh backend with the reverb set as the game sets it (both calls).
    fn reverb_render(kind: &str, v: &[f32], mix: f32) -> Vec<f32> {
        let mb = |x: f32| if x > -10000.0 { 10f32.powf(x / 2000.0) } else { 0.0 };
        let params = revc_audio::reverb::ReverbParams {
            density: (v[0] * v[0] * v[0] / 16.0).clamp(0.0, 1.0),
            diffusion: v[1],
            gain: mb(v[2]),
            gain_hf: mb(v[3]),
            decay_time: v[5],
            decay_hf_ratio: v[6],
            reflections_gain: mb(v[8]).clamp(0.0, 3.16),
            reflections_delay: v[9],
            late_gain: mb(v[10]).clamp(0.0, 10.0),
            late_delay: v[11],
        };
        let listener = retro_audio::reverb::MillibelListener {
            environment_size: v[0],
            diffusion: v[1],
            room: v[2].round() as i32,
            room_hf: v[3].round() as i32,
            room_lf: v[4].round() as i32,
            decay_time: v[5],
            decay_hf_ratio: v[6],
            decay_lf_ratio: v[7],
            reflections: v[8].round() as i32,
            reflections_delay: v[9],
            reverb: v[10].round() as i32,
            reverb_delay: v[11],
            echo_time: v[12],
            echo_depth: v[13],
            modulation_time: v[14],
            modulation_depth: v[15],
            air_absorption_hf: v[16],
            hf_reference: v[17],
            lf_reference: v[18],
            room_rolloff_factor: v[19],
            decay_hf_limit: (v[20] as u32) & 0x20 != 0,
        };
        let mut b: Box<dyn AudioBackend> = if kind == "revc" { Box::new(RevcBackend::new()) } else { Box::new(RetroBackend::new()) };
        b.sfx_open(8);
        b.reverb_set(&params);
        b.reverb_set_listener(&listener);
        b.sfx_set_2d(0, false);
        b.sfx_set_gain(0, 1.0);
        b.sfx_set_position(0, 0.0, 0.0, 1.0);
        b.sfx_set_distances(0, 100.0, 1.0);
        b.sfx_set_pitch(0, 1.0);
        b.sfx_set_loop_count(0, 1);
        b.sfx_set_reverb_mix(0, mix);
        b.sfx_start(0, &generate_click(), TONE_RATE);
        let frames = ((v[5].min(8.0) * 1.5 + 0.5) * OUTPUT_RATE as f32) as usize;
        let mut buf = vec![0.0f32; frames * 2];
        let mut done = 0;
        while done < frames {
            let n = RENDER_BLOCK.min(frames - done);
            b.render(&mut buf[done * 2..(done + n) * 2]);
            done += n;
        }
        b.sfx_stop(0);
        buf
    }

    /// Reverb level in the mix: a 0.1 s click at 1 m with the per-source send `mix`, through both
    /// backends with the reverb set by both calls (reverb_set + reverb_set_listener). The sets come
    /// from a local file (REVC_REVERB_SETS: `name mix` + 21 listener values in millibels); renders
    /// go to REVC_REVERB_OUT for analysis.
    #[test]
    #[ignore]
    fn reverb_level_report() {
        let Ok(sets) = std::env::var("REVC_REVERB_SETS") else {
            println!("SKIPPED: REVC_REVERB_SETS not set");
            return;
        };
        let out = std::env::var("REVC_REVERB_OUT").unwrap_or_else(|_| "reverb_out".into());
        std::fs::create_dir_all(&out).unwrap();
        let energy = |b: &[f32], a: usize, z: usize| (a..z.min(b.len() / 2)).map(|f| b[f * 2] * b[f * 2] + b[f * 2 + 1] * b[f * 2 + 1]).sum::<f32>();
        let db = |e: f32| 10.0 * (e + 1e-30).log10();
        for line in std::fs::read_to_string(&sets).unwrap().lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if line.starts_with('#') || t.len() < 23 {
                continue;
            }
            let (name, mix): (&str, f32) = (t[0], t[1].parse().unwrap());
            let v: Vec<f32> = t[2..23].iter().map(|x| x.parse().unwrap()).collect();
            for kind in ["revc", "retro"] {
                for (suffix, m) in [("", mix), ("_dry", 0.0)] {
                    let buf = reverb_render(kind, &v, m);
                    let bytes: Vec<u8> = buf.iter().flat_map(|x| x.to_le_bytes()).collect();
                    std::fs::write(format!("{out}/{kind}_{name}{suffix}.f32"), bytes).unwrap();
                    let (a, z) = ((0.110 * OUTPUT_RATE as f32) as usize, (0.25 * OUTPUT_RATE as f32) as usize);
                    let (d, w) = (db(energy(&buf, 0, a)), db(energy(&buf, z, usize::MAX)));
                    println!("{kind} {name}{suffix} mix {m:.3} dry_db {d:.2} wet_db {w:.2} wet_minus_dry {:.2}", w - d);
                }
            }
        }
    }

}

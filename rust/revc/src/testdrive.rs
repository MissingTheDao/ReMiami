//! Regression-test driver. Enabled by `REVC_TEST`; driven once per frame from
//! `host::hostPollEvents`, using the C hooks in cpp/re3-miami/src/skel/host/testhooks.cpp.
//!
//! Scenarios:
//!   REVC_TEST=boot      reach the front-end menu, then quit.
//!   REVC_TEST=saveload  load save slot REVC_TEST_SLOT (1-8, default 1), play until
//!                       REVC_TEST_MS of game time (default 8000) have passed and the screen
//!                       is no longer fading, check the player position, optionally capture a
//!                       screenshot and compare it with a reference. Waiting on game time
//!                       rather than frames keeps the captured moment independent of fps.
//! Options:
//!   REVC_TEST_SHOT=path.png   write the final frame here
//!   REVC_TEST_REF=path.png    compare against this; writes path.png.diff.png on failure
//!   REVC_TEST_MAXDIFF=0.005   allowed fraction of differing pixels (runs differ by <0.02%)
//!   REVC_TEST_TIMEOUT=180     seconds before the run is failed
//! Results are printed as `[revc-test] ...` lines; the exit code is 0 pass, 1 fail, 3 timeout.

use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, Ordering};
use std::path::PathBuf;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn revc_test_state() -> i32;
    fn revc_test_frame() -> u32;
    fn revc_test_game_ms() -> u32;
    fn revc_test_fading() -> i32;
    fn revc_test_quit();
    fn revc_test_load_slot(slot: i32) -> i32;
    fn revc_test_player_pos(x: *mut f32, y: *mut f32, z: *mut f32);
}

const STATE_FRONTEND: i32 = 1;
const STATE_PLAYING: i32 = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scenario {
    Boot,
    SaveLoad,
}

#[derive(Debug)]
enum Phase {
    WaitFrontend { frames: u32 },
    WaitPlaying,
    Playing { start_ms: u32, start_frame: u32 },
    Capturing,
    Done,
}

struct Driver {
    scenario: Scenario,
    slot: i32,
    wait_ms: u32,
    shot: Option<PathBuf>,
    reference: Option<PathBuf>,
    max_diff: f64,
    phase: Phase,
    started: Instant,
    failures: Vec<String>,
    finished: bool,
}

thread_local! {
    static DRIVER: RefCell<Option<Driver>> = const { RefCell::new(None) };
}

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn log(msg: impl AsRef<str>) {
    println!("[revc-test] {}", msg.as_ref());
}

/// Reads REVC_TEST; returns true if a test scenario is active.
pub fn init() -> Result<bool, String> {
    let Ok(name) = std::env::var("REVC_TEST") else { return Ok(false) };
    let scenario = match name.as_str() {
        "boot" => Scenario::Boot,
        "saveload" => Scenario::SaveLoad,
        other => return Err(format!("unknown REVC_TEST scenario '{other}' (boot, saveload)")),
    };
    let timeout: u64 = env("REVC_TEST_TIMEOUT", 180);
    let d = Driver {
        scenario,
        slot: env("REVC_TEST_SLOT", 1i32) - 1,
        wait_ms: env("REVC_TEST_MS", 8000),
        shot: std::env::var_os("REVC_TEST_SHOT").map(PathBuf::from),
        reference: std::env::var_os("REVC_TEST_REF").map(PathBuf::from),
        max_diff: env("REVC_TEST_MAXDIFF", 0.005),
        phase: Phase::WaitFrontend { frames: 0 },
        started: Instant::now(),
        failures: Vec::new(),
        finished: false,
    };
    log(format!("scenario {scenario:?} started"));
    DRIVER.with(|cell| *cell.borrow_mut() = Some(d));

    // Watchdog: also catches hangs where the main loop stops calling us.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(timeout + 30));
        log(format!("FAIL timeout: no result after {}s (hang?)", timeout + 30));
        std::process::exit(3);
    });
    Ok(true)
}

/// Exit code for main(): 0 pass, 1 fail. Only meaningful in test mode.
pub fn exit_code() -> i32 {
    let finished = DRIVER.with(|c| c.borrow().as_ref().is_some_and(|d| d.finished));
    if !finished {
        log("FAIL game exited before the scenario finished");
        return 1;
    }
    EXIT_CODE.load(Ordering::Relaxed)
}

/// Called once per main-loop iteration (from hostPollEvents, after input dispatch).
pub fn tick() {
    let quit = DRIVER.with(|cell| {
        let mut b = cell.borrow_mut();
        let Some(d) = b.as_mut() else { return false };
        d.step()
    });
    if quit {
        unsafe { revc_test_quit() };
    }
}

impl Driver {
    fn fail(&mut self, msg: String) {
        log(format!("FAIL {msg}"));
        self.failures.push(msg);
    }

    /// Advances the scenario; returns true when the game should quit.
    fn step(&mut self) -> bool {
        if matches!(self.phase, Phase::Done) {
            return false;
        }
        let timeout = Duration::from_secs(env("REVC_TEST_TIMEOUT", 180));
        if self.started.elapsed() > timeout {
            self.fail(format!("timeout in phase {:?}", self.phase));
            return self.finish();
        }
        let state = unsafe { revc_test_state() };
        match self.phase {
            Phase::WaitFrontend { frames } => {
                if state != STATE_FRONTEND {
                    return false;
                }
                // Let the menu settle for a few frames.
                if frames < 30 {
                    self.phase = Phase::WaitFrontend { frames: frames + 1 };
                    return false;
                }
                log(format!("front end reached after {:.1}s", self.started.elapsed().as_secs_f32()));
                match self.scenario {
                    Scenario::Boot => return self.finish(),
                    Scenario::SaveLoad => {
                        if unsafe { revc_test_load_slot(self.slot) } == 0 {
                            self.fail(format!("save slot {} missing or invalid", self.slot + 1));
                            return self.finish();
                        }
                        log(format!("loading slot {}", self.slot + 1));
                        self.phase = Phase::WaitPlaying;
                    }
                }
            }
            Phase::WaitPlaying => {
                if state == STATE_PLAYING {
                    log(format!("playing after {:.1}s", self.started.elapsed().as_secs_f32()));
                    crate::audio::take_stats(); // count sound effects and streams from here on
                    crate::audio::take_stream_frames();
                    self.phase = Phase::Playing { start_ms: unsafe { revc_test_game_ms() }, start_frame: unsafe { revc_test_frame() } };
                }
            }
            Phase::Playing { start_ms, start_frame } => {
                if state != STATE_PLAYING {
                    self.fail("left the game unexpectedly".into());
                    return self.finish();
                }
                let elapsed = unsafe { revc_test_game_ms() }.wrapping_sub(start_ms);
                let frames = unsafe { revc_test_frame() }.wrapping_sub(start_frame);
                if elapsed < self.wait_ms || frames < 30 || unsafe { revc_test_fading() } != 0 {
                    return false;
                }
                log(format!("captured after {elapsed} ms game time, {frames} frames"));
                self.check_audio();
                self.check_player();
                if self.shot.is_some() || self.reference.is_some() {
                    crate::host::request_capture();
                    self.phase = Phase::Capturing;
                } else {
                    return self.finish();
                }
            }
            Phase::Capturing => {
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    self.check_image(w, h, &rgba);
                    return self.finish();
                }
            }
            Phase::Done => {}
        }
        false
    }

    /// The game must have driven the Rust mixer (sound effects started). Volume is 0 in the
    /// test settings, so the level isn't checked, only that voices were started and mixed.
    fn check_audio(&mut self) {
        match crate::audio::take_stats() {
            Some(s) => {
                log(format!("audio: {} sound effects started, up to {} at once", s.starts, s.max_active));
                if s.starts == 0 {
                    self.fail("no sound effects were started during play".into());
                }
            }
            None => log("audio: Rust mixer not in use (REVC_RUST_SFX=OFF?)"),
        }
        match crate::audio::take_stream_frames() {
            Some(frames) => {
                // Both channels of a stereo stream are separate sources, so /2 for seconds.
                let secs = frames as f64 / 2.0 / revc_audio::mixer::OUTPUT_RATE as f64;
                log(format!("audio: {secs:.1}s of streamed audio mixed"));
                if frames == 0 {
                    self.fail("no streamed audio (music/ambience) was mixed during play".into());
                }
            }
            None => log("audio: streams not on the Rust mixer (REVC_RUST_STREAMS=OFF?)"),
        }
    }

    fn check_player(&mut self) {
        let (mut x, mut y, mut z) = (0f32, 0f32, 0f32);
        unsafe { revc_test_player_pos(&mut x, &mut y, &mut z) };
        log(format!("player at ({x:.2}, {y:.2}, {z:.2})"));
        // Vice City spans roughly +-2500 in x/y. The origin or NaN means the player wasn't restored.
        let sane = [x, y, z].iter().all(|v| v.is_finite())
            && x.abs() < 3000.0
            && y.abs() < 3000.0
            && (-100.0..1000.0).contains(&z)
            && (x.abs() > 1.0 || y.abs() > 1.0);
        if !sane {
            self.fail(format!("implausible player position ({x}, {y}, {z})"));
        }
    }

    fn check_image(&mut self, w: u32, h: u32, rgba: &[u8]) {
        if let Some(path) = self.shot.clone() {
            match write_png(&path, w, h, rgba) {
                Ok(()) => log(format!("screenshot {}x{} -> {}", w, h, path.display())),
                Err(e) => self.fail(format!("cannot write {}: {e}", path.display())),
            }
        }
        let Some(reference) = self.reference.clone() else { return };
        let (rw, rh, rref) = match read_png(&reference) {
            Ok(v) => v,
            Err(e) => {
                self.fail(format!("cannot read reference {}: {e}", reference.display()));
                return;
            }
        };
        if (rw, rh) != (w, h) {
            self.fail(format!("size {w}x{h} differs from reference {rw}x{rh}"));
            return;
        }
        // A pixel "differs" if any channel is off by more than 32/255.
        let mut diff_img = vec![0u8; rgba.len()];
        let mut differing = 0usize;
        for (i, (a, b)) in rgba.chunks_exact(4).zip(rref.chunks_exact(4)).enumerate() {
            let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
            if d > 32 {
                differing += 1;
                diff_img[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
            } else {
                // Faded copy of the frame for orientation.
                for c in 0..3 {
                    diff_img[i * 4 + c] = a[c] / 4;
                }
                diff_img[i * 4 + 3] = 255;
            }
        }
        let frac = differing as f64 / (w as f64 * h as f64);
        log(format!("image diff: {:.3}% of pixels differ (limit {:.3}%)", frac * 100.0, self.max_diff * 100.0));
        if frac > self.max_diff {
            let mut p = self.shot.clone().unwrap_or_else(|| std::env::temp_dir().join("revc-test.png")).into_os_string();
            p.push(".diff.png");
            let p = PathBuf::from(p);
            let _ = write_png(&p, w, h, &diff_img);
            self.fail(format!("screenshot differs from reference (diff image {})", p.display()));
        }
    }

    fn finish(&mut self) -> bool {
        self.phase = Phase::Done;
        self.finished = true;
        if self.failures.is_empty() {
            log(format!("PASS {:?} in {:.1}s", self.scenario, self.started.elapsed().as_secs_f32()));
        } else {
            log(format!("FAIL {:?}: {} problem(s)", self.scenario, self.failures.len()));
            EXIT_CODE.store(1, Ordering::Relaxed);
        }
        true
    }
}

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

fn read_png(path: &PathBuf) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("expected 8-bit RGBA, got {:?} {:?}", info.color_type, info.bit_depth));
    }
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

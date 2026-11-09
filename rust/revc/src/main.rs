//! reVC entry point. Rust owns `main()`, the main loop, the SDL3 window/GL context and input
//! (see `host`); each loop iteration runs one step of the C++ game (src/skel/host/host.cpp).

mod audio;
mod formatcheck;
mod padtest;
mod texmods;
mod text;
mod textutil;
mod math;
mod messages;
mod pager;
mod savefile;
mod collision;
mod savefix;
mod tempcolmodels;
mod handling;
mod weaponinfo;
mod savegame;
mod host;
mod testdrive;

use std::ffi::{CString, c_char, c_int};
use std::path::PathBuf;
use std::process::ExitCode;

// The game's main loop, split into steps (cpp/re3-miami/src/skel/host/host.cpp).
unsafe extern "C" {
    /// Platform and RenderWare start-up, settings. Returns 0 on failure.
    fn revc_game_init(argc: c_int, argv: *mut *mut c_char) -> c_int;
    /// Start of an iteration: 1 = run a frame, 2 = session restarted (call again), 0 = exit.
    fn revc_game_frame_begin() -> c_int;
    /// One frame of the game state machine, after events were processed.
    fn revc_game_frame_run();
    /// Game, audio and RenderWare teardown.
    fn revc_game_shutdown();
}

/// The game loads data relative to the working directory. Uses REVC_GAME_DIR, else the
/// nearest `game/` folder (one containing `models/`) above the current directory or the
/// executable, which finds `revc/game` for both `cargo run` and the built binary.
/// No absolute path is compiled in, so builds are the same in every checkout.
fn game_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("REVC_GAME_DIR") {
        return Some(PathBuf::from(dir));
    }
    let starts = [std::env::current_dir().ok(), std::env::current_exe().ok()];
    starts.into_iter().flatten().find_map(|start| {
        start.ancestors().map(|a| a.join("game")).find(|g| g.join("models").is_dir())
    })
}

fn main() -> ExitCode {
    let Some(dir) = game_dir() else {
        eprintln!("revc: no game folder found; set REVC_GAME_DIR to your Vice City data folder");
        return ExitCode::FAILURE;
    };
    if let Err(e) = std::env::set_current_dir(&dir) {
        eprintln!("revc: cannot enter game directory {}: {e}", dir.display());
        eprintln!("revc: set REVC_GAME_DIR to your Vice City data folder");
        return ExitCode::FAILURE;
    }

    let testing = match testdrive::init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("revc: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = host::init() {
        eprintln!("revc: SDL initialisation failed: {e}");
        return ExitCode::FAILURE;
    }

    // C-style argv; the CStrings must outlive the call.
    let args: Vec<CString> = std::env::args_os()
        .map(|a| CString::new(a.into_encoded_bytes()).expect("argument contains NUL"))
        .collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr().cast_mut()).collect();
    argv.push(std::ptr::null_mut());

    // SAFETY: argv is a NULL-terminated array of valid C strings that live for the call.
    if unsafe { revc_game_init((argv.len() - 1) as c_int, argv.as_mut_ptr()) } == 0 {
        eprintln!("revc: game initialisation failed");
        host::shutdown();
        return ExitCode::FAILURE;
    }

    // The main loop. Task 8 turns this into a fixed-timestep loop with render interpolation.
    // Order matters and matches the original C++ loop: check, poll events, run the frame.
    loop {
        // SAFETY: called on the main thread after a successful revc_game_init().
        match unsafe { revc_game_frame_begin() } {
            0 => break,
            1 => {
                host::poll_events();
                unsafe { revc_game_frame_run() };
            }
            _ => {} // session restarted (game loaded or new game)
        }
    }
    unsafe { revc_game_shutdown() };

    host::shutdown();
    if testing {
        return ExitCode::from(testdrive::exit_code() as u8);
    }
    ExitCode::SUCCESS
}

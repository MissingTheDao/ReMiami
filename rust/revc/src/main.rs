//! reVC entry point. Rust owns `main()`, the SDL3 window/GL context and input (see `host`),
//! and hands control to the C++ game loop in src/skel/host/host.cpp.

mod host;
mod testdrive;

use std::ffi::{CString, c_char, c_int};
use std::path::PathBuf;
use std::process::ExitCode;

unsafe extern "C" {
    /// The game's original entry point, in src/skel/host/host.cpp.
    fn revc_cpp_main(argc: c_int, argv: *mut *mut c_char) -> c_int;
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
    let rc = unsafe { revc_cpp_main((argv.len() - 1) as c_int, argv.as_mut_ptr()) };

    host::shutdown();
    if testing {
        return ExitCode::from(testdrive::exit_code() as u8);
    }
    ExitCode::from(rc.clamp(0, 255) as u8)
}

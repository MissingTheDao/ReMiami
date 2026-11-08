//! Builds the reVC C++ code (re3-miami) as a static library with CMake and links it.
//! No crates.io dependencies: CMake is driven directly with std::process::Command.
//!
//! Env overrides:
//!   REVC_CPP_DIR   path to the re3-miami checkout (default: ../../cpp/re3-miami)
//!   REVC_CMAKE_ARGS extra space-separated -D flags passed to the configure step

use std::{env, path::PathBuf, process::Command};

fn run(cmd: &mut Command) {
    eprintln!("running: {cmd:?}");
    let status = cmd.status().unwrap_or_else(|e| panic!("failed to spawn {cmd:?}: {e}"));
    assert!(status.success(), "command failed ({status}): {cmd:?}");
}

fn which(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = env::var_os("REVC_CPP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../cpp/re3-miami"));
    let src = src.canonicalize().unwrap_or_else(|e| panic!("re3-miami not found at {}: {e}", src.display()));
    let build = PathBuf::from(env::var("OUT_DIR").unwrap()).join("cmake-build");

    let mut cfg = Command::new("cmake");
    cfg.arg("-S").arg(&src).arg("-B").arg(&build)
        .args(["-G", "Ninja", "-DCMAKE_BUILD_TYPE=RelWithDebInfo", "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"])
        .args(["-DLIBRW_PLATFORM=GL3", "-DLIBRW_GL3_GFXLIB=HOST"])
        .args(["-DREVC_AUDIO=OAL", "-DREVC_VENDORED_LIBRW=ON", "-DREVC_AS_STATIC_LIB=ON"])
        // Sound effects mixed in Rust. Passed every time so a one-off
        // REVC_CMAKE_ARGS=-DREVC_RUST_SFX=OFF build (OpenAL, for comparison) doesn't stick in the cache.
        .arg("-DREVC_RUST_SFX=ON")
        .args(["-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++"]);
    // Reproducible output: map checkout and build paths out of __FILE__ strings and debug
    // info, so a clone in any directory produces the same binary.
    let prefix_map = format!(
        "-ffile-prefix-map={}=re3-miami -ffile-prefix-map={}=cmake-build",
        src.display(),
        build.display()
    );
    cfg.arg(format!("-DCMAKE_C_FLAGS={prefix_map}"))
        .arg(format!("-DCMAKE_CXX_FLAGS=-Wno-everything -fpermissive {prefix_map}"));
    if which("sccache") {
        cfg.args(["-DCMAKE_C_COMPILER_LAUNCHER=sccache", "-DCMAKE_CXX_COMPILER_LAUNCHER=sccache"]);
    }
    if let Ok(extra) = env::var("REVC_CMAKE_ARGS") {
        cfg.args(extra.split_whitespace());
    }
    run(&mut cfg);

    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| "1".into());
    run(Command::new("cmake").arg("--build").arg(&build).args(["--target", "reVC", "-j", &jobs]));

    // Static archives produced by the CMake build.
    println!("cargo:rustc-link-search=native={}", build.join("src").display());
    println!("cargo:rustc-link-search=native={}", build.join("vendor/librw/src").display());
    // Order matters for static archives: game first, then librw it depends on.
    println!("cargo:rustc-link-lib=static:+whole-archive=reVC");
    println!("cargo:rustc-link-lib=static=rw");
    // System libraries (same set CMake links for the standalone executable).
    for lib in ["OpenGL", "openal", "mpg123", "pthread", "m", "dl", "stdc++"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }

    // Rebuild when the C++ tree changes. Directories make cargo walk them for mtimes.
    for d in ["src", "vendor/librw/src", "CMakeLists.txt", "src/CMakeLists.txt"] {
        println!("cargo:rerun-if-changed={}", src.join(d).display());
    }
    println!("cargo:rerun-if-env-changed=REVC_CPP_DIR");
    println!("cargo:rerun-if-env-changed=REVC_CMAKE_ARGS");
}

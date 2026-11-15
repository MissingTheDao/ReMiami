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
    if !src.join("src/core/Frontend.cpp").exists() {
        panic!(
            "the reVC C++ tree is missing at {}.\nThe repository does not ship the reVC source: build it from your own copy with\n    scripts/revc-source.sh prepare <reVC source folder, archive or git URL>\n(see docs/BUILDING.md)",
            src.display()
        );
    }
    let src = src.canonicalize().unwrap();
    // Windows: canonicalize() gives a verbatim `\\?\C:\...` path, which CMake's globs do not read.
    let src = match src.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(plain) => PathBuf::from(plain),
        None => src,
    };
    let build = PathBuf::from(env::var("OUT_DIR").unwrap()).join("cmake-build");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");

    let mut cfg = Command::new("cmake");
    cfg.arg("-S").arg(&src).arg("-B").arg(&build)
        .args(["-G", "Ninja", "-DCMAKE_BUILD_TYPE=RelWithDebInfo", "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"])
        // librw's WGPU platform: drawing in Rust (revc-rw, plan task 12). The GL3 platform is gone.
        .args(["-DLIBRW_PLATFORM=WGPU"])
        .args(["-DREVC_AUDIO=OAL", "-DREVC_VENDORED_LIBRW=ON"]);
    if msvc {
        // Windows: MSVC cl from the Visual Studio developer environment (run cargo inside it),
        // the release CRT (/MD) that Rust links. mpg123 comes from MPG123_DIR (docs/BUILDING.md).
        cfg.args(["-DCMAKE_C_COMPILER=cl", "-DCMAKE_CXX_COMPILER=cl"])
            .arg("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL")
            .arg("-DCMAKE_C_FLAGS=/DWIN32 /D_WINDOWS /W0 /utf-8")
            .arg("-DCMAKE_CXX_FLAGS=/DWIN32 /D_WINDOWS /EHsc /GR /W0 /utf-8 /std:c++17");
        if let Some(dir) = env::var_os("MPG123_DIR") {
            cfg.arg(format!("-Dmpg123_DIR={}", PathBuf::from(dir).display()));
        }
    } else {
        cfg.args(["-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++"]);
        // Reproducible output: map checkout and build paths out of __FILE__ strings and debug
        // info, so a clone in any directory produces the same binary.
        let prefix_map = format!(
            "-ffile-prefix-map={}=re3-miami -ffile-prefix-map={}=cmake-build",
            src.display(),
            build.display()
        );
        cfg.arg(format!("-DCMAKE_C_FLAGS={prefix_map}"))
            .arg(format!("-DCMAKE_CXX_FLAGS=-Wno-everything -fpermissive {prefix_map}"));
    }
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
    // librw is header-only since stage 2 step 7d3 (its code is the Rust in this crate).
    println!("cargo:rustc-link-lib=static:+whole-archive=reVC");
    // System libraries (same set CMake links for the standalone executable).
    if msvc {
        // static mpg123 (built with MSVC) and its one system dependency
        if let Some(dir) = env::var_os("MPG123_DIR") {
            println!("cargo:rustc-link-search=native={}", PathBuf::from(dir).join("lib").display());
        }
        println!("cargo:rustc-link-lib=static=mpg123");
        println!("cargo:rustc-link-lib=dylib=shlwapi");
        println!("cargo:rustc-link-lib=dylib=ole32"); // CoCreateInstance (sampman_oal.cpp folder dialogs)
    } else {
        for lib in ["OpenGL", "mpg123", "pthread", "m", "dl", "stdc++"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }

    // Rebuild when the C++ tree changes. Directories make cargo walk them for mtimes.
    for d in ["src", "vendor/librw/src", "CMakeLists.txt", "src/CMakeLists.txt", ".revc-source"] {
        println!("cargo:rerun-if-changed={}", src.join(d).display());
    }
    println!("cargo:rerun-if-env-changed=REVC_CPP_DIR");
    println!("cargo:rerun-if-env-changed=REVC_CMAKE_ARGS");
    println!("cargo:rerun-if-env-changed=MPG123_DIR");
}

// reMiami build kit for Windows: the launcher. The release .exe is this program with the kit appended (a .tar.gz
// of build.ps1, VERSION and the source with its vendored crates) and a trailer; it unpacks the kit into
// %LOCALAPPDATA%\remiami\<version>\kit with the tar.exe that comes with Windows and runs build.ps1 with the same arguments.
// Made by packaging/buildkit/make-buildkit-win.sh.

use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let extract_only = args.contains(&"--extract-only".to_string());

    // Step 1: Read self and extract payload
    let exe_path = std::env::current_exe().expect("Cannot determine exe path");
    let mut file = fs::File::open(&exe_path).expect("Cannot open exe file");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("Cannot read exe file");

    if buf.len() < 20 {
        eprintln!("remiami-kit: File too small for payload");
        std::process::exit(2);
    }

    let magic = &buf[buf.len() - 8..];
    if magic != b"RMKITTAR" {
        eprintln!("remiami-kit: Missing RMKITTAR marker");
        std::process::exit(2);
    }

    // Trailer: <payload .tar.gz><version bytes><u32 LE version length><u64 LE payload length>"RMKITTAR"
    let n = buf.len();
    let payload_len = u64::from_le_bytes(buf[n - 16..n - 8].try_into().unwrap()) as usize;
    let version_len = u32::from_le_bytes(buf[n - 20..n - 16].try_into().unwrap()) as usize;
    let tail = 20 + version_len;
    if n < 20 || version_len == 0 || version_len > 64 || payload_len > n - tail {
        eprintln!("remiami-kit: damaged kit (bad trailer)");
        std::process::exit(2);
    }
    let version = String::from_utf8_lossy(&buf[n - tail..n - 20]).trim().to_string();
    let payload = &buf[n - tail - payload_len..n - tail];
    if version.is_empty() || version.contains(['/', '\\']) || version.contains("..") {
        eprintln!("remiami-kit: damaged kit (bad version)");
        std::process::exit(2);
    }

    // Step 3: Kit folder setup
    let local_app_data = env::var("LOCALAPPDATA").expect("LOCALAPPDATA not set");
    let kit_base = Path::new(&local_app_data).join("remiami").join(&version).join("kit");
    let complete_file = kit_base.join(".complete");

    println!("remiami-kit: Kit folder: {}", kit_base.display());

    if !complete_file.exists() {
        if kit_base.exists() {
            fs::remove_dir_all(&kit_base).expect("Failed to remove existing kit folder");
        }
        fs::create_dir_all(&kit_base).expect("Failed to create kit folder");

        let tar_path = kit_base.join("kit.tar.gz");
        fs::write(&tar_path, payload).expect("Failed to write tar file");

        let tar_exe = format!("{}\\System32\\tar.exe", env::var("SystemRoot").unwrap_or_default());
        let status = Command::new(tar_exe)
            .args(&["-xf", &tar_path.to_string_lossy(), "-C", &kit_base.to_string_lossy()])
            .status();

        fs::remove_file(&tar_path).ok();

        if !matches!(status, Ok(ref s) if s.success()) {
            eprintln!("remiami-kit: tar.exe failed");
            std::process::exit(3);
        }

        fs::File::create(&complete_file).expect("Failed to create .complete file");
    }

    if extract_only {
        println!("{}", kit_base.display());
        return;
    }

    // Step 4: Run build.ps1
    let ps1_path = kit_base.join("build.ps1");
    let mut cmd = Command::new("powershell.exe");
    cmd.arg("-NoProfile");
    cmd.arg("-ExecutionPolicy");
    cmd.arg("Bypass");
    cmd.arg("-File");
    cmd.arg(&ps1_path);
    cmd.args(&args);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("remiami-kit: cannot start powershell.exe: {e}");
            std::process::exit(1);
        }
    }
}

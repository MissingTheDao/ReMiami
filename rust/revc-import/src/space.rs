//! Disk space for the imports (one place for every lift; the Windows call comes with roadmap R12)
//! and a file writer that leaves no partial file behind. Dependency: libc (statvfs, unix only).

use std::fs::{create_dir_all, remove_file};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

pub const MARGIN: u64 = 1 << 30;

/// Free bytes for unprivileged writes on the filesystem holding `dir` (walk up to the nearest existing ancestor of `dir`
/// first, since the destination folder may not exist yet); None when unknown (not unix, or the call fails).
#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    // the nearest existing ancestor (a relative path ends at the working directory)
    let mut current = dir;
    while !current.exists() {
        current = match current.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
    }
    let cstr = std::ffi::CString::new(current.as_os_str().as_bytes()).ok()?;
    let mut buf: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(cstr.as_ptr(), &mut buf) };
    if ret != 0 {
        return None;
    }
    Some(buf.f_bavail as u64 * buf.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// Bytes as "1.63 GB" (decimal, 2 decimals) for >= 1e9, "345.0 MB" (1 decimal) for >= 1e6, else "12 KB" (rounded, decimal).
pub fn human(b: u64) -> String {
    if b >= 1_000_000_000 {
        let val = b as f64 / 1_000_000_000.0;
        format!("{:.2} GB", val)
    } else if b >= 1_000_000 {
        let val = b as f64 / 1_000_000.0;
        format!("{:.1} MB", val)
    } else {
        format!("{} KB", (b as f64 / 1_000.0).round() as u64)
    }
}

/// Whether `needed` bytes (plus a 1 GiB margin) fit on the filesystem of `dest`.
/// `free` is the free space to use (Some for tests; pass free_bytes(dest) normally).
/// Ok(None): unknown free space (the caller proceeds and says so); Ok(Some(free)): enough; Err(message) when short:
/// "not enough disk space for the import: needs X, Y free on the drive of <dest>" with X = needed + margin and Y = free, both formatted by `human`.
pub fn check(dest: &Path, needed: u64, free: Option<u64>) -> Result<Option<u64>, String> {
    let required = needed + MARGIN;
    match free {
        Some(f) => {
            if f >= required {
                Ok(Some(f))
            } else {
                Err(format!(
                    "not enough disk space for the import: needs {}, {} free on the drive of {}",
                    human(required),
                    human(f),
                    dest.display()
                ))
            }
        }
        None => Ok(None),
    }
}

/// Writes `data` to `dest` via a sibling file named dest + ".part" and a rename, creating parent dirs.
/// On any error: removes the .part file (ignore errors doing so) and returns Err(format!("{}: {e}", dest.display())),
/// and when the error kind is StorageFull (std::io::ErrorKind::StorageFull) the message says "disk full while writing <dest>".
pub fn write_file(dest: &Path, data: &[u8]) -> Result<(), String> {
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part_path = PathBuf::from(part);
    
    // Create parent directories
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            create_dir_all(parent).map_err(|e| format!("{}: {}", dest.display(), e))?;
        }
    }

    let mut file = std::fs::File::create(&part_path).map_err(|e| {
        // If storage full, specific message
        if e.kind() == ErrorKind::StorageFull {
            format!("disk full while writing {}", dest.display())
        } else {
            format!("{}: {}", dest.display(), e)
        }
    })?;

    if let Err(e) = file.write_all(data) {
        let _ = remove_file(&part_path);
        if e.kind() == ErrorKind::StorageFull {
            return Err(format!("disk full while writing {}", dest.display()));
        }
        return Err(format!("{}: {}", dest.display(), e));
    }

    file.flush().map_err(|e| {
        let _ = remove_file(&part_path);
        if e.kind() == ErrorKind::StorageFull {
            format!("disk full while writing {}", dest.display())
        } else {
            format!("{}: {}", dest.display(), e)
        }
    })?;

    // Drop the file handle to ensure it's closed before rename
    drop(file);

    std::fs::rename(&part_path, dest).map_err(|e| {
        let _ = remove_file(&part_path);
        if e.kind() == ErrorKind::StorageFull {
            format!("disk full while writing {}", dest.display())
        } else {
            format!("{}: {}", dest.display(), e)
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_check_enough_space() {
        let free = 10_000_000_000u64; // 10 GB
        let needed = 1_000_000_000u64; // 1 GB
        let res = check(Path::new("/tmp"), needed, Some(free)).unwrap();
        assert!(res.is_some());
    }

    #[test]
    fn test_check_not_enough_space() {
        let free = 1_000_000_000u64; // 1 GB
        let needed = 1_000_000_000u64; // 1 GB
        let res = check(Path::new("/tmp"), needed, Some(free));
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("not enough disk space"));
    }

    #[test]
    fn test_check_unknown_space() {
        let needed = 1_000_000_000u64;
        let res = check(Path::new("/tmp"), needed, None).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn test_human_gb() {
        assert_eq!(human(1_630_000_000), "1.63 GB");
    }

    #[test]
    fn test_human_mb() {
        assert_eq!(human(345_000_000), "345.0 MB");
    }

    #[test]
    fn test_human_kb() {
        assert_eq!(human(12_000), "12 KB");
        assert_eq!(human(12_500), "13 KB"); // rounded
    }

    #[test]
    fn test_write_file() {
        let temp_dir = std::env::temp_dir();
        let unique_name = format!(
            "space_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dest = temp_dir.join(unique_name);
        let data = b"Hello, world!";

        let res = write_file(&dest, data);
        assert!(res.is_ok());
        assert!(dest.exists());
        let part_dest = PathBuf::from(format!("{}.part", dest.display()));
        assert!(!part_dest.exists());

        let content = fs::read(&dest).unwrap();
        assert_eq!(content, data);

        // Cleanup
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn test_free_bytes_unix() {
        #[cfg(unix)]
        {
            let temp_dir = std::env::temp_dir();
            let free = free_bytes(&temp_dir);
            assert!(free.is_some());
            assert!(free.unwrap() > 0);
        }
    }

    #[test]
    fn test_free_bytes_non_existing() {
        #[cfg(unix)]
        {
            let temp_dir = std::env::temp_dir();
            let existing_free = free_bytes(&temp_dir);
            let non_existing = temp_dir.join("non_existing_dir_12345");
            let non_existing_free = free_bytes(&non_existing);
            
            // (the same filesystem; not compared exactly, other processes write meanwhile)
            assert!(existing_free.is_some() && non_existing_free.is_some());
        }
    }
}

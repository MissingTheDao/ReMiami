//! Which version a disc is (by its boot executable's SHA-256) and whether the console styles
//! accept it (user 2026-11-16: "import isos should be hashed so it cannot import wrong
//! versions"). The PS2 style follows US v1.40, the Xbox style the US disc (Rev 1).

use crate::{DiscKind, Disc};

/// What `identify` found.
pub struct DiscVersion {
    pub kind: DiscKind,
    pub name: String,
    pub exe: String,
    pub sha256: String,
    pub accepted: bool,
}

struct Known {
    kind: DiscKind,
    sha256: &'static str,
    name: &'static str,
    accepted: bool,
}

const KNOWN: &[Known] = &[
    Known { kind: DiscKind::Ps2, sha256: "39f5abacc6aa77c498cd0a23c38158b78b87d303995734105ef8cbc7b0297b23", name: "PS2 US v1.40", accepted: true },
    Known { kind: DiscKind::Ps2, sha256: "e8e2547f2162f7e953c45063031e3b4a84a31ca0d32e4e7bc4e4e35483cd16f8", name: "PS2 US v3.00", accepted: false },
    Known { kind: DiscKind::Ps2, sha256: "7afb2114e467d5a1cf07b1197ac6c29485d57dc1bcd6f50e84370ed622218bc6", name: "PS2 EU v3.00", accepted: false },
    Known { kind: DiscKind::Xbox, sha256: "9367714de9f4aa1219d68065f959604cce00a8eb26f13a39b01d8700a591e98d", name: "Xbox US (Rev 1)", accepted: true },
];

/// The name of the version the console style accepts.
pub fn needed(kind: DiscKind) -> &'static str {
    for k in KNOWN {
        if k.kind == kind && k.accepted {
            return k.name;
        }
    }
    ""
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(data);
    h.iter().map(|b| format!("{:02x}", b)).collect()
}

/// The boot file named in SYSTEM.CNF (`BOOT2 = cdrom0:\SLUS_205.52;1` -> `SLUS_205.52`).
pub fn boot_exe(system_cnf: &str) -> Option<String> {
    for line in system_cnf.lines() {
        let Some((key, path)) = line.split_once('=') else { continue };
        if !key.trim().eq_ignore_ascii_case("BOOT2") {
            continue;
        }
        let path = path.trim();
        // after the last '\' (or the device's ':'), up to the ";1" version suffix
        let name = path.rsplit(|c| c == '\\' || c == ':').next().unwrap_or(path);
        let name = name.split(';').next().unwrap_or(name).trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// (name, accepted) of a known executable hash for `kind`.
pub fn known(kind: DiscKind, sha256: &str) -> Option<(&'static str, bool)> {
    #[cfg(test)]
    if ACCEPT_ANY.with(|a| a.get()) && sha256 == "test" {
        return Some(("test disc", true));
    }
    for k in KNOWN {
        if k.kind == kind && k.sha256 == sha256 {
            return Some((k.name, k.accepted));
        }
    }
    None
}

/// The disc's version from its boot executable (PS2: the SYSTEM.CNF boot file; Xbox: default.xbe).
pub fn identify(disc: &mut Disc) -> Result<DiscVersion, String> {
    #[cfg(test)]
    if ACCEPT_ANY.with(|a| a.get()) {
        return Ok(DiscVersion { kind: disc.kind, name: "test disc".into(), exe: String::new(), sha256: "test".into(), accepted: true });
    }
    let exe = match disc.kind {
        DiscKind::Ps2 => {
            let cnf_e = disc.find("SYSTEM.CNF").ok_or("no SYSTEM.CNF")?.clone();
            let cnf = disc.read(&cnf_e).map_err(|e| format!("SYSTEM.CNF: {e}"))?;
            boot_exe(&String::from_utf8_lossy(&cnf)).ok_or("SYSTEM.CNF names no boot file")?
        }
        DiscKind::Xbox => "default.xbe".to_string(),
    };

    let exe_entry = disc.find(&exe).ok_or(format!("the boot file {} is missing", exe))?.clone();
    let data = disc.read(&exe_entry).map_err(|e| format!("{exe}: {e}"))?;
    let hash = sha256_hex(&data);

    let (name, accepted) = match known(disc.kind, &hash) {
        Some((n, a)) => (n.to_string(), a),
        None if disc.kind == DiscKind::Ps2 => (format!("PS2 disc {exe} (unknown version)"), false),
        None => ("Xbox disc (unknown version)".to_string(), false),
    };

    Ok(DiscVersion {
        kind: disc.kind,
        name,
        exe,
        sha256: hash,
        accepted,
    })
}

// Tests of the file lifting use made-up discs: this lets them through (per test thread).
#[cfg(test)]
thread_local! {
    pub(crate) static ACCEPT_ANY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_exe() {
        assert_eq!(boot_exe("BOOT2 = cdrom0:SLUS_205.52;1"), Some("SLUS_205.52".to_string()));
        assert_eq!(boot_exe("BOOT2 = cdrom0:\\SLUS_205.52;1\r\nVER = 1.40\r\n"), Some("SLUS_205.52".to_string()));
        assert_eq!(boot_exe("boot2=cdrom0:\\SLES_510.61;1"), Some("SLES_510.61".to_string()));
        assert_eq!(boot_exe("VER = 1.0"), None);
    }

    #[test]
    fn test_known() {
        assert_eq!(known(DiscKind::Ps2, "39f5abacc6aa77c498cd0a23c38158b78b87d303995734105ef8cbc7b0297b23"), Some(("PS2 US v1.40", true)));
        assert_eq!(known(DiscKind::Xbox, "39f5abacc6aa77c498cd0a23c38158b78b87d303995734105ef8cbc7b0297b23"), None);
    }

    #[test]
    fn test_sha256_hex() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn test_needed() {
        assert_eq!(needed(DiscKind::Ps2), "PS2 US v1.40");
    }
}

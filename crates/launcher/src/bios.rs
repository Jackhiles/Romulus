//! BIOS files from RomM into RetroDECK's `bios` folder.
//!
//! RetroArch (its system folder), PCSX2 and DuckStation all read RetroDECK's `bios`
//! folder directly; Flycast (Dreamcast) reads `bios/dc`. Systems whose emulators need
//! their own layouts (Switch keys and firmware, Xbox, PS3 firmware, ...) are reported
//! as unsupported rather than guessed. A file already on the device is never replaced,
//! even when it differs from RomM's.

use crate::Result;
use md5::{Digest, Md5};
use romm_api::Firmware;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Platforms whose emulators keep firmware somewhere Romulus doesn't manage yet.
const UNSUPPORTED: &[&str] = &[
    "switch", "xbox", "xbox360", "ps3", "ps4", "psvita", "wiiu", "wii", "3ds", "n3ds", "win",
];

/// Where a platform's BIOS files go, or None if Romulus doesn't place them.
pub fn target_dir(bios_root: &Path, platform_slug: &str) -> Option<PathBuf> {
    if UNSUPPORTED.contains(&platform_slug) {
        return None;
    }
    Some(match platform_slug {
        "dc" | "dreamcast" => bios_root.join("dc"),
        _ => bios_root.to_path_buf(),
    })
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BiosState {
    /// On the device and identical to RomM's.
    Present,
    /// Not on the device.
    Missing,
    /// A file with the same name is there but differs; left alone.
    Different,
    /// Romulus doesn't place BIOS for this platform.
    Unsupported,
}

fn md5_file(p: &Path) -> std::io::Result<String> {
    Ok(hex::encode(Md5::digest(std::fs::read(p)?)))
}

/// The file on the device with this name (case-insensitive), if any.
fn existing(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_file())
        .map(|e| e.path())
}

pub fn check(bios_root: &Path, platform_slug: &str, fw: &Firmware) -> BiosState {
    let Some(dir) = target_dir(bios_root, platform_slug) else {
        return BiosState::Unsupported;
    };
    let Some(path) = existing(&dir, &fw.file_name) else {
        return BiosState::Missing;
    };
    match (&fw.md5_hash, md5_file(&path)) {
        (Some(want), Ok(have)) if want.eq_ignore_ascii_case(&have) => BiosState::Present,
        // RomM has no hash: same size counts as present.
        (None, Ok(_))
            if std::fs::metadata(&path).map(|m| m.len()).ok() == Some(fw.file_size_bytes) =>
        {
            BiosState::Present
        }
        _ => BiosState::Different,
    }
}

/// Put a downloaded BIOS file in place. Refuses to replace an existing file and checks
/// RomM's md5 first. Returns the path.
pub fn install(
    bios_root: &Path,
    platform_slug: &str,
    fw: &Firmware,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dir = target_dir(bios_root, platform_slug).ok_or_else(|| {
        crate::Error::BadLayout(format!("BIOS for {platform_slug} isn't supported"))
    })?;
    let backslash = char::from(92);
    if fw.file_name.is_empty()
        || fw.file_name.contains(['/', backslash])
        || fw.file_name.starts_with('.')
    {
        return Err(crate::Error::BadLayout(format!(
            "unsafe BIOS file name {:?}",
            fw.file_name
        )));
    }
    if existing(&dir, &fw.file_name).is_some() {
        return Err(crate::Error::BadLayout(format!(
            "{} is already there; not replacing it",
            fw.file_name
        )));
    }
    if let Some(want) = &fw.md5_hash
        && !hex::encode(Md5::digest(bytes)).eq_ignore_ascii_case(want)
    {
        return Err(crate::Error::Corrupt(fw.file_name.clone()));
    }
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(&fw.file_name);
    let tmp = dir.join(format!(".{}.romulus-part", fw.file_name));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &dest)?;
    tracing::info!("installed BIOS {}", dest.display());
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fw(name: &str, data: &[u8]) -> Firmware {
        Firmware {
            id: 1,
            platform_id: 1,
            file_name: name.into(),
            file_size_bytes: data.len() as u64,
            is_verified: true,
            md5_hash: Some(hex::encode(Md5::digest(data))),
            sha1_hash: None,
        }
    }

    #[test]
    fn checks_and_installs_without_replacing() {
        let d = tempfile::tempdir().unwrap();
        let bios = d.path();
        let f = fw("scph1001.bin", b"BIOS");
        assert_eq!(check(bios, "psx", &f), BiosState::Missing);
        // Corrupt download: refused, nothing written.
        assert!(install(bios, "psx", &f, b"XXXX").is_err());
        assert_eq!(check(bios, "psx", &f), BiosState::Missing);
        install(bios, "psx", &f, b"BIOS").unwrap();
        assert_eq!(check(bios, "psx", &f), BiosState::Present);
        // Never replaces what is there.
        assert!(install(bios, "psx", &f, b"BIOS").is_err());
        // Same name, other content (case-insensitive match): different, left alone.
        std::fs::write(bios.join("SCPH5501.BIN"), b"mine").unwrap();
        let other = fw("scph5501.bin", b"theirs");
        assert_eq!(check(bios, "psx", &other), BiosState::Different);
        assert!(install(bios, "psx", &other, b"theirs").is_err());
        assert_eq!(std::fs::read(bios.join("SCPH5501.BIN")).unwrap(), b"mine");
    }

    #[test]
    fn places_by_platform() {
        let root = Path::new("/sd/bios");
        assert_eq!(target_dir(root, "ps2"), Some(root.to_path_buf()));
        assert_eq!(target_dir(root, "dc"), Some(root.join("dc")));
        assert_eq!(target_dir(root, "switch"), None);
        assert_eq!(
            check(root, "switch", &fw("prod.keys", b"k")),
            BiosState::Unsupported
        );
        assert!(install(root, "psx", &fw("../evil", b"x"), b"x").is_err());
    }
}

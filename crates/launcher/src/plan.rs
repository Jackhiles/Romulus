//! Which files of a ROM to download, where they go, and which one RetroDECK should start.
//!
//! RomM records each file's folder relative to its library (`roms/<platform>/<game
//! folder>[/<sub folder>]`). The same layout is recreated under the RetroDECK system
//! folder, so a multi-disc game keeps its discs and `.m3u` playlist together.

use crate::{Error, Result};
use romm_api::{RomFile, RomFiles};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// File categories that are not needed to play (RomM's `RomFileCategory`).
const SKIP: &[&str] = &["manual", "walkthrough", "soundtrack", "screenshot"];

/// Launch preference among a game folder's top-level files: playlists and cue sheets
/// first (they reference the rest), then disc images.
const LAUNCH_ORDER: &[&str] = &[
    "m3u", "cue", "gdi", "ccd", "mds", "chd", "rvz", "gcz", "wbfs", "iso", "cso", "pbp", "nsp",
    "xci", "wua", "rpx",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlannedFile {
    pub file: RomFile,
    pub dest: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Plan {
    pub rom_id: i64,
    /// What the install consists of: the single file, or the game's folder.
    pub root: PathBuf,
    pub files: Vec<PlannedFile>,
    /// The file to hand to RetroDECK.
    pub launch: PathBuf,
    pub total_bytes: u64,
}

/// A relative path from RomM, as safe path components (no `..`, no absolute paths).
fn safe_relative(s: &str) -> Result<PathBuf> {
    let p = Path::new(s);
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return Err(Error::BadLayout(format!("unsafe path {s:?}"))),
        }
    }
    Ok(out)
}

fn safe_name(s: &str) -> Result<&str> {
    if s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\']) {
        return Err(Error::BadLayout(format!("unsafe file name {s:?}")));
    }
    Ok(s)
}

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Plan installing `rom` into `system_dir` (e.g. `<roms>/psx`).
pub fn plan(rom: &RomFiles, system_dir: &Path) -> Result<Plan> {
    let base = safe_relative(&rom.fs_path)?;
    let mut files = Vec::new();
    for f in &rom.files {
        if f.category.as_deref().is_some_and(|c| SKIP.contains(&c)) {
            continue;
        }
        let dir = safe_relative(&f.file_path)?;
        let rel = dir
            .strip_prefix(&base)
            .map_err(|_| Error::BadLayout(format!("{} is outside {}", f.file_path, rom.fs_path)))?;
        let dest = system_dir.join(rel).join(safe_name(&f.file_name)?);
        files.push(PlannedFile {
            file: f.clone(),
            dest,
        });
    }
    if files.is_empty() {
        return Err(Error::BadLayout("no game files".into()));
    }

    // A single file directly in the system folder, loose files there (a playlist with
    // its discs), or a game folder.
    let loose = files.iter().all(|f| f.dest.parent() == Some(system_dir));
    let root = if loose {
        files[0].dest.clone()
    } else {
        let first = files[0]
            .dest
            .strip_prefix(system_dir)
            .ok()
            .and_then(|r| r.components().next())
            .ok_or_else(|| Error::BadLayout("file outside the system folder".into()))?;
        let root = system_dir.join(first);
        if files.iter().any(|f| !f.dest.starts_with(&root)) {
            return Err(Error::BadLayout("files spread over several folders".into()));
        }
        root
    };

    let launch = if files.len() == 1 {
        files[0].dest.clone()
    } else {
        // Top-level game files of the folder (or the loose files), best format first,
        // else the largest.
        let top_dir = if loose { system_dir } else { root.as_path() };
        let top: Vec<&PlannedFile> = files
            .iter()
            .filter(|f| f.dest.parent() == Some(top_dir))
            .filter(|f| f.file.category.as_deref().is_none_or(|c| c == "game"))
            .collect();
        let by_order = LAUNCH_ORDER
            .iter()
            .find_map(|e| top.iter().find(|f| ext(&f.dest) == *e))
            .copied();
        by_order
            .or_else(|| top.iter().max_by_key(|f| f.file.file_size_bytes).copied())
            .ok_or_else(|| Error::BadLayout("no game file to start".into()))?
            .dest
            .clone()
    };

    // Loose files: the install is the launch file plus its parts.
    let root = if loose { launch.clone() } else { root };
    let total_bytes = files.iter().map(|f| f.file.file_size_bytes).sum();
    Ok(Plan {
        rom_id: rom.id,
        root,
        files,
        launch,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rom(v: serde_json::Value) -> RomFiles {
        serde_json::from_value(v).unwrap()
    }

    fn f(id: i64, name: &str, path: &str, size: u64, cat: &str) -> serde_json::Value {
        json!({"id": id, "file_name": name, "file_path": path, "file_size_bytes": size, "is_top_level": true, "category": cat})
    }

    // Layouts below are real responses from a RomM 5.4.1 server (docs/DECK_FINDINGS.md).

    #[test]
    fn single_file() {
        let r = rom(
            json!({"id": 2372, "fs_name": "Hot Wheels (USA, Europe).gba", "fs_path": "roms/gba",
            "platform_fs_slug": "gba", "files": [f(12089, "Hot Wheels (USA, Europe).gba", "roms/gba", 16777216, "game")]}),
        );
        let p = plan(&r, Path::new("/sd/roms/gba")).unwrap();
        assert_eq!(
            p.root,
            Path::new("/sd/roms/gba/Hot Wheels (USA, Europe).gba")
        );
        assert_eq!(p.launch, p.root);
        assert_eq!(p.total_bytes, 16777216);
    }

    #[test]
    fn multi_disc_launches_the_playlist() {
        let d = "roms/psx/Metal Gear Solid (USA) (Rev 1)";
        let r = rom(
            json!({"id": 1653, "fs_name": "Metal Gear Solid (USA) (Rev 1)", "fs_path": "roms/psx", "platform_fs_slug": "psx",
            "files": [f(10615, "Metal Gear Solid (USA) (Disc 1) (Rev 1).chd", d, 405492465, "game"),
                      f(10617, "Metal Gear Solid (USA) (Disc 2) (Rev 1).chd", d, 372815953, "game"),
                      f(10616, "Metal Gear Solid (USA) (Rev 1).m3u", d, 90, "game")]}),
        );
        let p = plan(&r, Path::new("/sd/roms/psx")).unwrap();
        assert_eq!(
            p.root,
            Path::new("/sd/roms/psx/Metal Gear Solid (USA) (Rev 1)")
        );
        assert_eq!(p.launch, p.root.join("Metal Gear Solid (USA) (Rev 1).m3u"));
        assert_eq!(p.files.len(), 3);
    }

    #[test]
    fn cue_sheet_beats_the_bin() {
        let d = "roms/ps2/Crazy Taxi (USA)";
        let r = rom(
            json!({"id": 1295, "fs_name": "Crazy Taxi (USA)", "fs_path": "roms/ps2", "platform_fs_slug": "ps2",
            "files": [f(1253, "Crazy Taxi (USA).bin", d, 750130416, "game"), f(1254, "Crazy Taxi (USA).cue", d, 82, "game")]}),
        );
        let p = plan(&r, Path::new("/sd/roms/ps2")).unwrap();
        assert_eq!(
            p.launch,
            Path::new("/sd/roms/ps2/Crazy Taxi (USA)/Crazy Taxi (USA).cue")
        );
    }

    #[test]
    fn skips_the_soundtrack() {
        let d = "roms/ps2/Katamari Damacy (USA)";
        let r = rom(
            json!({"id": 1296, "fs_name": "Katamari Damacy (USA)", "fs_path": "roms/ps2", "platform_fs_slug": "ps2",
            "files": [f(1260, "01 nananan katamari.mp3", &format!("{d}/soundtrack"), 1733075, "soundtrack"),
                      f(1255, "Katamari Damacy (USA).chd", d, 1308408890, "game")]}),
        );
        let p = plan(&r, Path::new("/sd/roms/ps2")).unwrap();
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.root, Path::new("/sd/roms/ps2/Katamari Damacy (USA)"));
        assert_eq!(p.launch, p.root.join("Katamari Damacy (USA).chd"));
        assert_eq!(p.total_bytes, 1308408890);
    }

    #[test]
    fn loose_playlist_with_its_discs() {
        // How this server stores multi-disc PS1 games: playlist and discs as loose files.
        let r = rom(
            json!({"id": 1424, "fs_name": "Alone (USA).m3u", "fs_path": "roms/psx", "platform_fs_slug": "psx",
            "files": [f(1, "Alone (USA).m3u", "roms/psx", 116, "game"),
                      f(2, "Alone (USA) (Disc 1).chd", "roms/psx", 375904839, "game"),
                      f(3, "Alone (USA) (Disc 2).chd", "roms/psx", 403382529, "game")]}),
        );
        let p = plan(&r, Path::new("/sd/roms/psx")).unwrap();
        assert_eq!(p.launch, Path::new("/sd/roms/psx/Alone (USA).m3u"));
        assert_eq!(p.root, p.launch);
        assert_eq!(p.files.len(), 3);
        assert_eq!(p.total_bytes, 116 + 375904839 + 403382529);
    }

    #[test]
    fn rejects_unsafe_paths() {
        let r = rom(
            json!({"id": 1, "fs_name": "x", "fs_path": "roms/gba", "platform_fs_slug": "gba",
            "files": [f(1, "evil.gba", "roms/gba/../../etc", 1, "game")]}),
        );
        assert!(matches!(
            plan(&r, Path::new("/sd/roms/gba")),
            Err(Error::BadLayout(_))
        ));
        let r = rom(
            json!({"id": 1, "fs_name": "x", "fs_path": "roms/gba", "platform_fs_slug": "gba",
            "files": [f(1, "../x.gba", "roms/gba", 1, "game")]}),
        );
        assert!(plan(&r, Path::new("/sd/roms/gba")).is_err());
    }
}

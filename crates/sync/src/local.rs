//! The save on the device: where it is, its fingerprint, packing it for upload,
//! applying a download, and backups.
//!
//! Every write goes through [`apply`], which is only called after [`backup`]. Backups
//! are never deleted by Romulus.

use crate::engine::Local;
use md5::{Digest, Md5};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Where a game's save lives.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveSpec {
    /// One file (RetroArch battery saves, `<game>.srm`).
    File { path: PathBuf },
    /// The game's folders on a PCSX2 folder memory card: every folder in `card_dir`
    /// whose name starts with `prefix` (the disc serial, e.g. `BASLUS-21008`).
    CardFolders { card_dir: PathBuf, prefix: String },
}

/// Rewritten by PCSX2 whenever the card is opened; not progress.
const VOLATILE: &[&str] = &["_pcsx2_index"];

fn md5_hex(b: &[u8]) -> String {
    hex::encode(Md5::digest(b))
}

/// RomM's hash for a zip: md5 of the sorted "name:md5" lines of its files.
fn listing_hash(entries: &[(String, String)]) -> String {
    let mut lines: Vec<String> = entries.iter().map(|(n, h)| format!("{n}:{h}")).collect();
    lines.sort();
    md5_hex(lines.join("\n").as_bytes())
}

/// Files of the game's card folders, as (zip entry name, path), sorted.
fn card_files(card_dir: &Path, prefix: &str) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(card_dir) else {
        return Ok(out);
    };
    for e in rd {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with(prefix) || !e.file_type()?.is_dir() {
            continue;
        }
        let mut stack = vec![(name, e.path())];
        while let Some((rel, dir)) = stack.pop() {
            for f in std::fs::read_dir(&dir)? {
                let f = f?;
                let child = format!("{rel}/{}", f.file_name().to_string_lossy());
                if f.file_type()?.is_dir() {
                    stack.push((child, f.path()));
                } else {
                    out.push((child, f.path()));
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The device's save, or None if there is none.
pub fn read(spec: &SaveSpec) -> std::io::Result<Option<Local>> {
    match spec {
        SaveSpec::File { path } => match std::fs::read(path) {
            Ok(b) => {
                let h = md5_hex(&b);
                Ok(Some(Local {
                    fp: h.clone(),
                    hash: h,
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        },
        SaveSpec::CardFolders { card_dir, prefix } => {
            let files = card_files(card_dir, prefix)?;
            if files.is_empty() {
                return Ok(None);
            }
            let mut all = Vec::new();
            let mut stable = Vec::new();
            for (name, path) in files {
                let h = md5_hex(&std::fs::read(&path)?);
                let base = name.rsplit('/').next().unwrap_or_default();
                if !VOLATILE.contains(&base) {
                    stable.push((name.clone(), h.clone()));
                }
                all.push((name, h));
            }
            Ok(Some(Local {
                fp: listing_hash(&stable),
                hash: listing_hash(&all),
            }))
        }
    }
}

/// Last modification time of the save (for showing the user in a conflict).
pub fn modified(spec: &SaveSpec) -> Option<std::time::SystemTime> {
    match spec {
        SaveSpec::File { path } => std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        SaveSpec::CardFolders { card_dir, prefix } => card_files(card_dir, prefix)
            .ok()?
            .iter()
            .filter_map(|(_, p)| std::fs::metadata(p).and_then(|m| m.modified()).ok())
            .max(),
    }
}

/// The bytes to upload: the file, or a zip of the card folders.
pub fn pack(spec: &SaveSpec) -> std::io::Result<Option<Vec<u8>>> {
    match spec {
        SaveSpec::File { path } => match std::fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        },
        SaveSpec::CardFolders { card_dir, prefix } => {
            let files = card_files(card_dir, prefix)?;
            if files.is_empty() {
                return Ok(None);
            }
            let mut buf = std::io::Cursor::new(Vec::new());
            {
                let mut z = zip::ZipWriter::new(&mut buf);
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);
                let mut dirs: Vec<String> = files
                    .iter()
                    .filter_map(|(n, _)| n.rsplit_once('/').map(|(d, _)| format!("{d}/")))
                    .collect();
                dirs.dedup();
                for d in dirs {
                    z.add_directory(d, opts).map_err(std::io::Error::other)?;
                }
                for (name, path) in files {
                    z.start_file(name, opts).map_err(std::io::Error::other)?;
                    z.write_all(&std::fs::read(path)?)?;
                }
                z.finish().map_err(std::io::Error::other)?;
            }
            Ok(Some(buf.into_inner()))
        }
    }
}

/// RomM's `content_hash` for bytes downloaded from it.
pub fn content_hash(bytes: &[u8]) -> String {
    match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(mut z) => {
            let mut entries = Vec::new();
            for i in 0..z.len() {
                let Ok(mut f) = z.by_index(i) else { continue };
                if f.is_dir() {
                    continue;
                }
                let Ok(name) = f.name().map(|n| n.into_owned()) else {
                    continue;
                };
                let mut b = Vec::new();
                if f.read_to_end(&mut b).is_ok() {
                    entries.push((name, md5_hex(&b)));
                }
            }
            listing_hash(&entries)
        }
        Err(_) => md5_hex(bytes),
    }
}

/// Copy the current save into `<backups>/<label>/`. Returns where, or None if there
/// was nothing to back up.
pub fn backup(spec: &SaveSpec, backups: &Path, label: &str) -> std::io::Result<Option<PathBuf>> {
    let dest = backups.join(label);
    match spec {
        SaveSpec::File { path } => {
            if !path.is_file() {
                return Ok(None);
            }
            std::fs::create_dir_all(&dest)?;
            let to = dest.join(path.file_name().unwrap_or_default());
            std::fs::copy(path, &to)?;
            Ok(Some(to))
        }
        SaveSpec::CardFolders { card_dir, prefix } => {
            let files = card_files(card_dir, prefix)?;
            if files.is_empty() {
                return Ok(None);
            }
            for (name, path) in files {
                let to = dest.join(&name);
                if let Some(p) = to.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::copy(path, to)?;
            }
            Ok(Some(dest))
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// Put a backup (made by [`backup`]) back. The current save is backed up first into
/// `<backups>/<now>-before-restore`. Returns that backup's folder, if one was made.
pub fn restore(
    spec: &SaveSpec,
    from: &Path,
    backups: &Path,
    now: &str,
) -> std::io::Result<Option<PathBuf>> {
    // Check the backup fits this save before touching anything.
    match spec {
        SaveSpec::File { path } => {
            let name = path.file_name().unwrap_or_default();
            if !from.join(name).is_file() {
                return Err(std::io::Error::other(
                    "this backup isn't a save of this game",
                ));
            }
        }
        SaveSpec::CardFolders { prefix, .. } => {
            let ok = std::fs::read_dir(from)?
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(prefix.as_str()));
            if !ok {
                return Err(std::io::Error::other(
                    "this backup isn't a save of this game",
                ));
            }
        }
    }
    let label = format!("{now}-before-restore");
    let made = backup(spec, backups, &label)?.map(|_| backups.join(&label));
    match spec {
        SaveSpec::File { path } => {
            let name = path.file_name().unwrap_or_default();
            apply(spec, &std::fs::read(from.join(name))?)?;
        }
        SaveSpec::CardFolders { card_dir, prefix } => {
            for e in std::fs::read_dir(card_dir)? {
                let e = e?;
                if e.file_name().to_string_lossy().starts_with(prefix.as_str())
                    && e.file_type()?.is_dir()
                {
                    std::fs::remove_dir_all(e.path())?;
                }
            }
            for e in std::fs::read_dir(from)? {
                let e = e?;
                if e.file_name().to_string_lossy().starts_with(prefix.as_str())
                    && e.file_type()?.is_dir()
                {
                    copy_dir(&e.path(), &card_dir.join(e.file_name()))?;
                }
            }
        }
    }
    Ok(made)
}

fn safe_entry(name: &str, prefix: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    let mut comps = p.components();
    match comps.next() {
        Some(Component::Normal(first)) if first.to_string_lossy().starts_with(prefix) => {}
        _ => return None,
    }
    p.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| p.to_path_buf())
}

/// Replace the device's save with `bytes` from RomM. Call [`backup`] first.
pub fn apply(spec: &SaveSpec, bytes: &[u8]) -> std::io::Result<()> {
    match spec {
        SaveSpec::File { path } => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("romulus-tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, path)
        }
        SaveSpec::CardFolders { card_dir, prefix } => {
            let mut z =
                zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(std::io::Error::other)?;
            // Check everything before touching the card.
            let mut entries = Vec::new();
            for i in 0..z.len() {
                let f = z.by_index(i).map_err(std::io::Error::other)?;
                let name = f.name().map_err(std::io::Error::other)?.into_owned();
                let rel = safe_entry(&name, prefix).ok_or_else(|| {
                    std::io::Error::other(format!("save archive entry {name:?} is not this game's"))
                })?;
                entries.push((i, rel, f.is_dir()));
            }
            // The game's old folders go (they are in the backup), then the new ones in.
            for e in std::fs::read_dir(card_dir)? {
                let e = e?;
                if e.file_name().to_string_lossy().starts_with(prefix.as_str())
                    && e.file_type()?.is_dir()
                {
                    std::fs::remove_dir_all(e.path())?;
                }
            }
            for (i, rel, is_dir) in entries {
                let to = card_dir.join(&rel);
                if is_dir {
                    std::fs::create_dir_all(&to)?;
                    continue;
                }
                if let Some(p) = to.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut f = z.by_index(i).map_err(std::io::Error::other)?;
                let mut out = std::fs::File::create(&to)?;
                std::io::copy(&mut f, &mut out)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(dir: &Path) {
        for (p, b) in [
            ("BASLUS-21008Opt/icon.sys", &b"icon"[..]),
            ("BASLUS-21008Opt/katamari_save01", b"save1"),
            ("BASLUS-21008Opt/_pcsx2_index", b"idx1"),
            ("BASLUS-20370Save/data", b"other game"),
        ] {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        std::fs::write(dir.join("_pcsx2_superblock"), b"sb").unwrap();
    }

    #[test]
    fn file_saves_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let spec = SaveSpec::File {
            path: d.path().join("Game/Game.srm"),
        };
        assert_eq!(read(&spec).unwrap(), None);
        apply(&spec, b"SRAM").unwrap();
        let l = read(&spec).unwrap().unwrap();
        assert_eq!(l.hash, content_hash(b"SRAM"));
        let b = backup(&spec, &d.path().join("backups"), "1")
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(b).unwrap(), b"SRAM");
    }

    #[test]
    fn card_folders_pack_hash_and_apply() {
        let d = tempfile::tempdir().unwrap();
        let cardd = d.path().join("Memcard.ps2");
        card(&cardd);
        let spec = SaveSpec::CardFolders {
            card_dir: cardd.clone(),
            prefix: "BASLUS-21008".into(),
        };
        let l = read(&spec).unwrap().unwrap();
        let zip = pack(&spec).unwrap().unwrap();
        // The zip hashes like RomM would hash it, and matches the folder hash.
        assert_eq!(content_hash(&zip), l.hash);

        // The volatile index doesn't change the fingerprint, real data does.
        std::fs::write(cardd.join("BASLUS-21008Opt/_pcsx2_index"), b"idx2").unwrap();
        let l2 = read(&spec).unwrap().unwrap();
        assert_eq!(l2.fp, l.fp);
        assert_ne!(l2.hash, l.hash);
        std::fs::write(
            cardd.join("BASLUS-21008Opt/katamari_save01"),
            b"more progress",
        )
        .unwrap();
        assert_ne!(read(&spec).unwrap().unwrap().fp, l.fp);

        // Applying the old zip restores this game's folders and leaves the others.
        backup(&spec, &d.path().join("backups"), "x").unwrap();
        std::fs::write(cardd.join("BASLUS-21008Opt/stray"), b"x").unwrap();
        apply(&spec, &zip).unwrap();
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-21008Opt/katamari_save01")).unwrap(),
            b"save1"
        );
        assert!(!cardd.join("BASLUS-21008Opt/stray").exists());
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-20370Save/data")).unwrap(),
            b"other game"
        );
        assert!(cardd.join("_pcsx2_superblock").exists());
        assert_eq!(
            std::fs::read(d.path().join("backups/x/BASLUS-21008Opt/katamari_save01")).unwrap(),
            b"more progress"
        );
    }

    #[test]
    fn restore_puts_a_backup_back_and_keeps_the_current_save() {
        let d = tempfile::tempdir().unwrap();
        let backups = d.path().join("backups");
        // File saves.
        let spec = SaveSpec::File {
            path: d.path().join("saves/psx/Game.srm"),
        };
        apply(&spec, b"OLD").unwrap();
        let b = backup(&spec, &backups, "t1-before-download")
            .unwrap()
            .unwrap();
        apply(&spec, b"NEW").unwrap();
        restore(&spec, b.parent().unwrap(), &backups, "t2").unwrap();
        assert_eq!(
            std::fs::read(d.path().join("saves/psx/Game.srm")).unwrap(),
            b"OLD"
        );
        assert_eq!(
            std::fs::read(backups.join("t2-before-restore/Game.srm")).unwrap(),
            b"NEW"
        );

        // Card folders.
        let cardd = d.path().join("Memcard.ps2");
        card(&cardd);
        let cspec = SaveSpec::CardFolders {
            card_dir: cardd.clone(),
            prefix: "BASLUS-21008".into(),
        };
        let cb = backup(&cspec, &backups, "c1").unwrap().unwrap();
        std::fs::write(cardd.join("BASLUS-21008Opt/katamari_save01"), b"newer").unwrap();
        restore(&cspec, &cb, &backups, "c2").unwrap();
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-21008Opt/katamari_save01")).unwrap(),
            b"save1"
        );
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-20370Save/data")).unwrap(),
            b"other game"
        );

        // A backup of another game is refused and nothing changes.
        assert!(restore(&cspec, &backups.join("t1-before-download"), &backups, "c3").is_err());
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-21008Opt/katamari_save01")).unwrap(),
            b"save1"
        );
    }

    #[test]
    fn apply_rejects_entries_for_other_games_or_paths() {
        let d = tempfile::tempdir().unwrap();
        let cardd = d.path().join("Memcard.ps2");
        card(&cardd);
        let spec = SaveSpec::CardFolders {
            card_dir: cardd.clone(),
            prefix: "BASLUS-21008".into(),
        };
        for bad in [
            "BASLUS-20370Save/data",
            "../evil",
            "BASLUS-21008Opt/../../evil",
        ] {
            let mut buf = std::io::Cursor::new(Vec::new());
            {
                let mut z = zip::ZipWriter::new(&mut buf);
                z.start_file(bad, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(b"x").unwrap();
                z.finish().unwrap();
            }
            assert!(apply(&spec, buf.get_ref()).is_err(), "{bad}");
        }
        // Nothing was touched.
        assert_eq!(
            std::fs::read(cardd.join("BASLUS-21008Opt/katamari_save01")).unwrap(),
            b"save1"
        );
    }
}

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
    /// The game's folders on a memory card folder: every folder in `card_dir` whose
    /// name starts with `prefix` (PCSX2: the disc serial, `BASLUS-21008`; PPSSPP: the
    /// game ID, `ULUS10567`).
    CardFolders { card_dir: PathBuf, prefix: String },
    /// Dolphin's GameCube memory card folder: the game's `.gci` files in `dir`, named
    /// `<maker>-<game id>-<name>.gci` (`01-GM8E-MetroidPrime A.gci`).
    GciFiles { dir: PathBuf, game_id: String },
    /// Everything in one folder (a Wii title's `data`, a Wii U title's save folder).
    Folder { dir: PathBuf },
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

/// Is `name` a GCI file of game `id` (`01-GM8E-MetroidPrime A.gci`)?
fn gci_of(name: &str, id: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".gci")
        && name
            .split('-')
            .nth(1)
            .is_some_and(|g| g.eq_ignore_ascii_case(id))
}

/// For the multi-file kinds: the folder the save's names are relative to, and whether a
/// top-level entry of it belongs to the save.
fn base(spec: &SaveSpec) -> Option<&Path> {
    match spec {
        SaveSpec::File { .. } => None,
        SaveSpec::CardFolders { card_dir, .. } => Some(card_dir),
        SaveSpec::GciFiles { dir, .. } | SaveSpec::Folder { dir } => Some(dir),
    }
}

fn owns(spec: &SaveSpec, top: &str, is_dir: bool) -> bool {
    match spec {
        SaveSpec::File { .. } => false,
        SaveSpec::CardFolders { prefix, .. } => is_dir && top.starts_with(prefix.as_str()),
        SaveSpec::GciFiles { game_id, .. } => !is_dir && gci_of(top, game_id),
        SaveSpec::Folder { .. } => true,
    }
}

/// The save's files, as (name relative to the base folder, path), sorted. Empty for a
/// single-file save.
fn save_files(spec: &SaveSpec) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let Some(base) = base(spec) else {
        return Ok(out);
    };
    let Ok(rd) = std::fs::read_dir(base) else {
        return Ok(out);
    };
    for e in rd {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        let is_dir = e.file_type()?.is_dir();
        if !owns(spec, &name, is_dir) {
            continue;
        }
        if !is_dir {
            out.push((name, e.path()));
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
    if let SaveSpec::File { path } = spec {
        return match std::fs::read(path) {
            Ok(b) => {
                let h = md5_hex(&b);
                Ok(Some(Local {
                    fp: h.clone(),
                    hash: h,
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        };
    }
    let files = save_files(spec)?;
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

/// Last modification time of the save (for showing the user in a conflict).
pub fn modified(spec: &SaveSpec) -> Option<std::time::SystemTime> {
    match spec {
        SaveSpec::File { path } => std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        _ => save_files(spec)
            .ok()?
            .iter()
            .filter_map(|(_, p)| std::fs::metadata(p).and_then(|m| m.modified()).ok())
            .max(),
    }
}

/// The bytes to upload: the file, or a zip of the save's files.
pub fn pack(spec: &SaveSpec) -> std::io::Result<Option<Vec<u8>>> {
    if let SaveSpec::File { path } = spec {
        return match std::fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        };
    }
    let files = save_files(spec)?;
    if files.is_empty() {
        return Ok(None);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        // Every parent folder, once, in order.
        let mut dirs = std::collections::BTreeSet::new();
        for (n, _) in &files {
            let mut at = 0;
            while let Some(i) = n[at..].find('/') {
                dirs.insert(format!("{}/", &n[..at + i]));
                at += i + 1;
            }
        }
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
    if let SaveSpec::File { path } = spec {
        if !path.is_file() {
            return Ok(None);
        }
        std::fs::create_dir_all(&dest)?;
        let to = dest.join(path.file_name().unwrap_or_default());
        std::fs::copy(path, &to)?;
        return Ok(Some(to));
    }
    let files = save_files(spec)?;
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

/// Remove the save's top-level entries from its base folder (they are in a backup).
fn clear(spec: &SaveSpec, base: &Path) -> std::io::Result<()> {
    let Ok(rd) = std::fs::read_dir(base) else {
        return Ok(());
    };
    for e in rd {
        let e = e?;
        let is_dir = e.file_type()?.is_dir();
        if owns(spec, &e.file_name().to_string_lossy(), is_dir) {
            if is_dir {
                std::fs::remove_dir_all(e.path())?;
            } else {
                std::fs::remove_file(e.path())?;
            }
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
    let fits = match spec {
        SaveSpec::File { path } => from.join(path.file_name().unwrap_or_default()).is_file(),
        _ => std::fs::read_dir(from)?.flatten().any(|e| {
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            owns(spec, &e.file_name().to_string_lossy(), is_dir)
        }),
    };
    if !fits {
        return Err(std::io::Error::other(
            "this backup isn't a save of this game",
        ));
    }
    let label = format!("{now}-before-restore");
    let made = backup(spec, backups, &label)?.map(|_| backups.join(&label));
    match (spec, base(spec)) {
        (SaveSpec::File { path }, _) => {
            let name = path.file_name().unwrap_or_default();
            apply(spec, &std::fs::read(from.join(name))?)?;
        }
        (_, Some(base)) => {
            clear(spec, base)?;
            std::fs::create_dir_all(base)?;
            for e in std::fs::read_dir(from)? {
                let e = e?;
                let is_dir = e.file_type()?.is_dir();
                if !owns(spec, &e.file_name().to_string_lossy(), is_dir) {
                    continue;
                }
                if is_dir {
                    copy_dir(&e.path(), &base.join(e.file_name()))?;
                } else {
                    std::fs::copy(e.path(), base.join(e.file_name()))?;
                }
            }
        }
        (_, None) => {}
    }
    Ok(made)
}

/// A zip entry's path, if it is safe and belongs to this save.
fn safe_entry(spec: &SaveSpec, name: &str, is_dir: bool) -> Option<PathBuf> {
    let p = Path::new(name);
    if !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let mut comps = p.components();
    let first = comps.next()?.as_os_str().to_string_lossy().into_owned();
    // The top-level entry is a folder when the name goes deeper.
    let top_is_dir = is_dir || comps.next().is_some();
    owns(spec, &first, top_is_dir).then(|| p.to_path_buf())
}

/// Replace the device's save with `bytes` from RomM. Call [`backup`] first.
pub fn apply(spec: &SaveSpec, bytes: &[u8]) -> std::io::Result<()> {
    let base = match spec {
        SaveSpec::File { path } => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("romulus-tmp");
            std::fs::write(&tmp, bytes)?;
            return std::fs::rename(&tmp, path);
        }
        _ => base(spec).ok_or_else(|| std::io::Error::other("no save folder"))?,
    };
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(std::io::Error::other)?;
    // Check everything before touching the save.
    let mut entries = Vec::new();
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(std::io::Error::other)?;
        let name = f.name().map_err(std::io::Error::other)?.into_owned();
        let rel = safe_entry(spec, &name, f.is_dir()).ok_or_else(|| {
            std::io::Error::other(format!("save archive entry {name:?} is not this game's"))
        })?;
        entries.push((i, rel, f.is_dir()));
    }
    // The game's old files go (they are in the backup), then the new ones in.
    clear(spec, base)?;
    std::fs::create_dir_all(base)?;
    for (i, rel, is_dir) in entries {
        let to = base.join(&rel);
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

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, b) in entries {
                z.start_file(*n, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn gci_files_are_the_games_only() {
        let d = tempfile::tempdir().unwrap();
        let card = d.path().join("GC/USA/Card A");
        std::fs::create_dir_all(&card).unwrap();
        std::fs::write(card.join("01-GM8E-MetroidPrime A.gci"), b"samus").unwrap();
        std::fs::write(card.join("01-GAFE-DobutsunomoriP_MURA.gci"), b"town").unwrap();
        let spec = SaveSpec::GciFiles {
            dir: card.clone(),
            game_id: "GM8E".into(),
        };
        let zip = pack(&spec).unwrap().unwrap();
        assert_eq!(content_hash(&zip), read(&spec).unwrap().unwrap().hash);

        backup(&spec, &d.path().join("b"), "1").unwrap();
        apply(&spec, &zip_of(&[("01-GM8E-MetroidPrime B.gci", b"new")])).unwrap();
        assert!(!card.join("01-GM8E-MetroidPrime A.gci").exists());
        assert_eq!(
            std::fs::read(card.join("01-GM8E-MetroidPrime B.gci")).unwrap(),
            b"new"
        );
        assert_eq!(
            std::fs::read(card.join("01-GAFE-DobutsunomoriP_MURA.gci")).unwrap(),
            b"town"
        );
        assert_eq!(
            std::fs::read(d.path().join("b/1/01-GM8E-MetroidPrime A.gci")).unwrap(),
            b"samus"
        );
        for bad in ["01-GAFE-Other.gci", "sub/01-GM8E-x.gci", "../01-GM8E-x.gci"] {
            assert!(apply(&spec, &zip_of(&[(bad, b"x")])).is_err(), "{bad}");
        }
        assert_eq!(
            std::fs::read(card.join("01-GM8E-MetroidPrime B.gci")).unwrap(),
            b"new"
        );

        // Restore puts the old file back.
        restore(&spec, &d.path().join("b/1"), &d.path().join("b"), "2").unwrap();
        assert_eq!(
            std::fs::read(card.join("01-GM8E-MetroidPrime A.gci")).unwrap(),
            b"samus"
        );
        assert!(!card.join("01-GM8E-MetroidPrime B.gci").exists());
    }

    #[test]
    fn folder_saves_round_trip_with_subfolders() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("title/00010000/524d4345/data");
        let spec = SaveSpec::Folder { dir: dir.clone() };
        assert_eq!(read(&spec).unwrap(), None);
        assert_eq!(pack(&spec).unwrap(), None);
        // A download for a game never played here creates the folder.
        let zip = zip_of(&[
            ("rksys.dat", b"karts"),
            ("banner.bin", b"b"),
            ("sub/x", b"y"),
        ]);
        apply(&spec, &zip).unwrap();
        assert_eq!(std::fs::read(dir.join("sub/x")).unwrap(), b"y");
        let again = pack(&spec).unwrap().unwrap();
        assert_eq!(content_hash(&again), content_hash(&zip));
        assert_eq!(read(&spec).unwrap().unwrap().hash, content_hash(&zip));
        // The next download replaces the folder's contents.
        backup(&spec, &d.path().join("b"), "1").unwrap();
        apply(&spec, &zip_of(&[("rksys.dat", b"faster")])).unwrap();
        assert!(!dir.join("sub").exists());
        assert_eq!(std::fs::read(d.path().join("b/1/sub/x")).unwrap(), b"y");
        assert!(apply(&spec, &zip_of(&[("../escape", b"x")])).is_err());
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

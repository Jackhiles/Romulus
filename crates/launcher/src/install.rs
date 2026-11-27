//! Downloading a [`Plan`] onto the device, and removing it again.
//!
//! Each file goes to `<dest>.romulus-part` first. An interrupted download keeps its part
//! file and continues with an HTTP Range request next time. A finished file is checked
//! against RomM's size and SHA-1 before it is renamed into place, so RetroDECK never
//! sees a half-written game.

use crate::plan::Plan;
use crate::{Error, Result};
use futures_util::StreamExt;
use romm_api::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Kept free on the device after an install.
const SPACE_MARGIN: u64 = 64 * 1024 * 1024;
const PART_SUFFIX: &str = "romulus-part";

/// What an install put on the device (stored in the cache, used to remove it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Installed {
    pub rom_id: i64,
    /// The system folder the install lives in; removal never goes outside it.
    pub system_dir: PathBuf,
    pub root: PathBuf,
    pub launch: PathBuf,
    pub files: Vec<PathBuf>,
    pub size_bytes: u64,
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(PART_SUFFIX);
    dest.with_file_name(name)
}

fn file_len(p: &Path) -> Option<u64> {
    std::fs::metadata(p)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

/// A file already in place with the right size counts as installed (hashing a multi-GB
/// disc image on every check would be too slow).
fn already_there(dest: &Path, size: u64) -> bool {
    file_len(dest).is_some_and(|l| size == 0 || l == size)
}

/// Is everything in `plan` already on disk?
pub fn is_installed(plan: &Plan) -> bool {
    plan.files
        .iter()
        .all(|f| already_there(&f.dest, f.file.file_size_bytes))
}

pub fn installed_record(plan: &Plan, system_dir: &Path) -> Installed {
    Installed {
        rom_id: plan.rom_id,
        system_dir: system_dir.to_path_buf(),
        root: plan.root.clone(),
        launch: plan.launch.clone(),
        files: plan.files.iter().map(|f| f.dest.clone()).collect(),
        size_bytes: plan.total_bytes,
    }
}

/// Bytes still to download, counting partial downloads.
fn remaining(plan: &Plan) -> u64 {
    plan.files
        .iter()
        .filter(|f| !already_there(&f.dest, f.file.file_size_bytes))
        .map(|f| {
            f.file
                .file_size_bytes
                .saturating_sub(file_len(&part_path(&f.dest)).unwrap_or(0))
        })
        .sum()
}

async fn hash_existing(path: &Path, hasher: &mut Sha1) -> std::io::Result<()> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

/// Download everything in `plan` that is not there yet. `progress(done, total)` covers
/// the whole install. Setting `cancel` stops it, keeping partial files for a resume.
pub async fn install(
    client: &Client,
    plan: &Plan,
    system_dir: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<Installed> {
    let total = plan.total_bytes;
    let need = remaining(plan);
    let free = fs4::available_space(system_dir)?;
    if need + SPACE_MARGIN > free {
        return Err(Error::NoSpace {
            need_mb: need.div_ceil(1 << 20),
            free_mb: free / (1 << 20),
        });
    }

    let mut done = total.saturating_sub(need);
    progress(done, total);
    for pf in &plan.files {
        let size = pf.file.file_size_bytes;
        if already_there(&pf.dest, size) {
            continue;
        }
        if let Some(dir) = pf.dest.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let part = part_path(&pf.dest);
        let mut have = file_len(&part).unwrap_or(0);
        if size > 0 && have > size {
            // Bigger than the real file: not ours to continue.
            tokio::fs::remove_file(&part).await?;
            have = 0;
        }
        let want_sha1 = pf
            .file
            .sha1_hash
            .as_deref()
            .filter(|h| h.len() == 40)
            .map(str::to_ascii_lowercase);
        let mut hasher = Sha1::new();

        let dl = client.download_file(&pf.file, have).await?;
        let mut out = if have > 0 && dl.partial {
            if want_sha1.is_some() {
                hash_existing(&part, &mut hasher).await?;
            }
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&part)
                .await?
        } else {
            // Fresh start (or the server ignored the Range request).
            done -= have;
            have = 0;
            tokio::fs::File::create(&part).await?
        };

        let mut stream = dl.response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                out.flush().await?;
                return Err(Error::Cancelled);
            }
            let chunk = chunk.map_err(romm_api::Error::from)?;
            if want_sha1.is_some() {
                hasher.update(&chunk);
            }
            out.write_all(&chunk).await?;
            have += chunk.len() as u64;
            done += chunk.len() as u64;
            progress(done, total);
        }
        out.flush().await?;
        out.sync_all().await?;
        drop(out);

        let size_ok = size == 0 || have == size;
        let hash_ok = want_sha1
            .as_ref()
            .is_none_or(|w| hex::encode(hasher.finalize()) == *w);
        if !size_ok || !hash_ok {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Corrupt(pf.file.file_name.clone()));
        }
        tokio::fs::rename(&part, &pf.dest).await?;
        tracing::info!("downloaded {}", pf.dest.display());
    }
    Ok(installed_record(plan, system_dir))
}

/// Remove an install's files (and its folder once empty). Only paths inside its system
/// folder are touched; saves and states live elsewhere and are never affected.
pub fn remove(inst: &Installed) -> Result<()> {
    let inside = |p: &Path| p.starts_with(&inst.system_dir) && p != inst.system_dir;
    for f in &inst.files {
        if !inside(f) {
            return Err(Error::BadLayout(format!(
                "{} is outside {}",
                f.display(),
                inst.system_dir.display()
            )));
        }
        for p in [f.clone(), part_path(f)] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    // Empty folders left behind, deepest first, up to and including the game folder.
    if inside(&inst.root) && inst.root.is_dir() {
        let mut dirs: Vec<PathBuf> = inst
            .files
            .iter()
            .filter_map(|f| f.parent().map(Path::to_path_buf))
            .collect();
        dirs.push(inst.root.clone());
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        dirs.dedup();
        for d in dirs {
            if inside(&d) && d.starts_with(&inst.root) {
                // Fails (and is left alone) when something else is still in it.
                let _ = std::fs::remove_dir(&d);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan;
    use romm_api::{Credentials, RomFiles, normalize_server_url};
    use serde_json::json;
    use wiremock::matchers::{header, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client(s: &MockServer) -> Client {
        let c = Client::new(normalize_server_url(&s.uri()).unwrap(), "t").unwrap();
        c.set_credentials(Some(Credentials::ClientToken {
            token: "rmm_t".into(),
            device_id: None,
            expires_at: None,
        }))
        .await;
        c
    }

    fn sha1(b: &[u8]) -> String {
        hex::encode(Sha1::digest(b))
    }

    fn rom(files: Vec<(i64, &str, &[u8])>) -> RomFiles {
        let files: Vec<_> = files
            .iter()
            .map(|(id, name, data)| {
                json!({"id": id, "file_name": name, "file_path": "roms/psx/Game (USA)",
                "file_size_bytes": data.len(), "category": "game", "sha1_hash": sha1(data)})
            })
            .collect();
        serde_json::from_value(
            json!({"id": 9, "fs_name": "Game (USA)", "fs_path": "roms/psx",
            "platform_fs_slug": "psx", "files": files}),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn installs_resumes_verifies_and_removes() {
        let s = MockServer::start().await;
        let disc = b"0123456789abcdef".to_vec();
        Mock::given(path_regex("/api/roms/1/files/content/.*"))
            .and(header("range", "bytes=6-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 6-15/16")
                    .set_body_bytes(disc[6..].to_vec()),
            )
            .mount(&s)
            .await;
        Mock::given(path_regex("/api/roms/2/files/content/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"FILE \"d.chd\"".to_vec()))
            .mount(&s)
            .await;
        let c = client(&s).await;
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path().join("psx");
        std::fs::create_dir(&sys).unwrap();
        let r = rom(vec![
            (1, "Game (USA).chd", &disc),
            (2, "Game (USA).m3u", b"FILE \"d.chd\""),
        ]);
        let p = plan(&r, &sys).unwrap();
        assert!(!is_installed(&p));

        // Six bytes from an earlier, interrupted download.
        let game_dir = sys.join("Game (USA)");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(part_path(&game_dir.join("Game (USA).chd")), &disc[..6]).unwrap();

        let last = std::sync::Mutex::new((0, 0));
        let inst = install(&c, &p, &sys, &AtomicBool::new(false), |d, t| {
            *last.lock().unwrap() = (d, t)
        })
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(game_dir.join("Game (USA).chd")).unwrap(),
            disc
        );
        assert_eq!(*last.lock().unwrap(), (p.total_bytes, p.total_bytes));
        assert!(is_installed(&p));
        assert_eq!(inst.launch, game_dir.join("Game (USA).m3u"));

        // A save next to the game is not ours: removal leaves it, and the folder with it.
        std::fs::write(game_dir.join("Game (USA).srm"), b"save").unwrap();
        remove(&inst).unwrap();
        assert!(!game_dir.join("Game (USA).chd").exists());
        assert!(game_dir.join("Game (USA).srm").exists());
        std::fs::remove_file(game_dir.join("Game (USA).srm")).unwrap();
        remove(&inst).unwrap();
        assert!(!game_dir.exists());
        assert!(sys.exists());
    }

    #[tokio::test]
    async fn corrupt_download_is_discarded() {
        let s = MockServer::start().await;
        Mock::given(path_regex("/api/roms/1/files/content/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"XXXXXXXXXXXXXXXX".to_vec()))
            .mount(&s)
            .await;
        let c = client(&s).await;
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path().join("psx");
        std::fs::create_dir(&sys).unwrap();
        let r = rom(vec![(1, "Game (USA).chd", b"0123456789abcdef")]);
        let p = plan(&r, &sys).unwrap();
        let e = install(&c, &p, &sys, &AtomicBool::new(false), |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Corrupt(_)));
        assert!(!p.files[0].dest.exists());
        assert!(!part_path(&p.files[0].dest).exists());
    }

    #[tokio::test]
    async fn cancel_keeps_the_part_file() {
        let s = MockServer::start().await;
        Mock::given(path_regex("/api/roms/1/files/content/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"0123456789abcdef".to_vec()))
            .mount(&s)
            .await;
        let c = client(&s).await;
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path().join("psx");
        std::fs::create_dir(&sys).unwrap();
        let r = rom(vec![(1, "Game (USA).chd", b"0123456789abcdef")]);
        let p = plan(&r, &sys).unwrap();
        let e = install(&c, &p, &sys, &AtomicBool::new(true), |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Cancelled));
        assert!(part_path(&p.files[0].dest).exists());
        assert!(!p.files[0].dest.exists());
    }

    #[test]
    fn remove_refuses_paths_outside_the_system_folder() {
        let inst = Installed {
            rom_id: 1,
            system_dir: PathBuf::from("/sd/roms/psx"),
            root: PathBuf::from("/sd/roms/psx/G"),
            launch: PathBuf::from("/sd/saves/psx/G.srm"),
            files: vec![PathBuf::from("/sd/saves/psx/G.srm")],
            size_bytes: 1,
        };
        assert!(matches!(remove(&inst), Err(Error::BadLayout(_))));
    }
}

//! Disk cache for cover art and other server assets, with a size limit.

use crate::Result;
use romm_api::Client;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct CoverCache {
    dir: PathBuf,
    max_bytes: u64,
}

impl CoverCache {
    pub fn new(dir: PathBuf, max_bytes: u64) -> Self {
        Self { dir, max_bytes }
    }

    /// File name for a server path. RomM puts `?ts=<updated_at>` on cover paths, so a
    /// changed cover gets a new name and old files age out.
    fn file_for(&self, server_path: &str) -> PathBuf {
        let hash = hex::encode(&Sha256::digest(server_path.as_bytes())[..12]);
        let clean = server_path.split('?').next().unwrap_or_default();
        let ext = Path::new(clean)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or("img");
        self.dir.join(format!("{hash}.{ext}"))
    }

    /// Cached file for `server_path`, if it is on disk already.
    pub fn cached(&self, server_path: &str) -> Option<PathBuf> {
        let f = self.file_for(server_path);
        f.is_file().then_some(f)
    }

    /// Local file for `server_path`, downloading it if needed.
    pub async fn get(&self, client: &Client, server_path: &str) -> Result<PathBuf> {
        let file = self.file_for(server_path);
        if file.is_file() {
            // Touch for LRU pruning; failure only affects eviction order.
            if let Ok(f) = std::fs::File::options().append(true).open(&file) {
                let _ = f.set_modified(SystemTime::now());
            }
            return Ok(file);
        }
        let bytes = client.fetch_asset(server_path).await?;
        tokio::fs::create_dir_all(&self.dir).await?;
        let tmp = file.with_extension(format!("part{}", std::process::id()));
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &file).await?;
        Ok(file)
    }

    /// Delete least recently used files until the cache is under its size limit.
    /// Returns the number of files removed.
    pub fn prune(&self) -> Result<usize> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Ok(0);
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                m.is_file().then(|| {
                    (
                        m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        m.len(),
                        e.path(),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort_by_key(|f| f.0);
        let mut removed = 0;
        for (_, len, path) in files {
            if total <= self.max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn downloads_once_and_prunes() {
        let s = MockServer::start().await;
        Mock::given(path("/assets/romm/resources/roms/1/2/cover/small.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 100]))
            .expect(1)
            .mount(&s)
            .await;
        let client = Client::new(romm_api::normalize_server_url(&s.uri()).unwrap(), "t").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let cache = CoverCache::new(dir.path().to_path_buf(), 150);
        let p = "/assets/romm/resources/roms/1/2/cover/small.png?ts=2026-11-20";
        let f = cache.get(&client, p).await.unwrap();
        assert_eq!(f.extension().unwrap(), "png");
        assert_eq!(cache.get(&client, p).await.unwrap(), f);
        assert_eq!(cache.cached(p), Some(f.clone()));

        std::fs::write(dir.path().join("other.png"), vec![0u8; 100]).unwrap();
        // Make "other" the older one.
        let old = SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .append(true)
            .open(dir.path().join("other.png"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(cache.prune().unwrap(), 1);
        assert!(f.exists());
    }
}

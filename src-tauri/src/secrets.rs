//! RomM credentials: the system keyring (Secret Service) when it works, otherwise a file
//! readable only by this user.
//!
//! In Gaming Mode on SteamOS the Secret Service (KWallet) may be missing or locked, so the
//! keyring is tried first and the `0600` file is the fallback. Settings shows which one
//! is in use.

use crate::app::APP_ID;
use romm_api::Credentials;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Keyring,
    File,
}

pub struct Secrets {
    file: PathBuf,
}

fn entry(account: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(APP_ID, account)
}

impl Secrets {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            file: config_dir.join("credentials.json"),
        }
    }

    /// Save credentials for `server`. Returns where they went.
    pub fn save(&self, server: &str, creds: &Credentials) -> std::io::Result<Store> {
        let json = serde_json::to_string(creds).map_err(std::io::Error::other)?;
        match entry(server).and_then(|e| e.set_password(&json)) {
            Ok(()) => {
                // Don't leave an older copy in the file.
                let _ = std::fs::remove_file(&self.file);
                Ok(Store::Keyring)
            }
            Err(e) => {
                tracing::warn!("keyring unavailable ({e}); storing credentials in a private file");
                self.write_file(server, &json)?;
                Ok(Store::File)
            }
        }
    }

    pub fn load(&self, server: &str) -> Option<(Credentials, Store)> {
        if let Ok(json) = entry(server).and_then(|e| e.get_password())
            && let Ok(c) = serde_json::from_str(&json)
        {
            return Some((c, Store::Keyring));
        }
        let text = std::fs::read_to_string(&self.file).ok()?;
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).ok()?;
        let c = serde_json::from_value(map.get(server)?.clone()).ok()?;
        Some((c, Store::File))
    }

    pub fn clear(&self, server: &str) {
        if let Ok(e) = entry(server) {
            let _ = e.delete_credential();
        }
        let _ = std::fs::remove_file(&self.file);
    }

    fn write_file(&self, server: &str, json: &str) -> std::io::Result<()> {
        let mut map = serde_json::Map::new();
        map.insert(
            server.to_string(),
            serde_json::from_str(json).map_err(std::io::Error::other)?,
        );
        let text = serde_json::to_string(&map).map_err(std::io::Error::other)?;
        let tmp = self.file.with_extension("tmp");
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(tmp, &self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_fallback_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let s = Secrets::new(dir.path());
        let c = Credentials::ClientToken {
            token: "rmm_x".into(),
            device_id: None,
            expires_at: None,
        };
        s.write_file("https://a/", &serde_json::to_string(&c).unwrap())
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("credentials.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let text = std::fs::read_to_string(dir.path().join("credentials.json")).unwrap();
        assert!(text.contains("rmm_x"));
    }
}

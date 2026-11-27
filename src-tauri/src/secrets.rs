//! RomM credentials, in `credentials.json` in the config folder, readable only by this
//! user (mode 0600), like SSH keys.
//!
//! The system keyring is not used: on SteamOS the Secret Service (KWallet) only runs in
//! Desktop Mode, so credentials stored there could not be read in Gaming Mode and the Deck
//! looked signed out. Credentials an earlier version put in the keyring are moved to the
//! file the first time they are read.

use crate::app::APP_ID;
use romm_api::Credentials;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Store {
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

    /// Save credentials for `server`.
    pub fn save(&self, server: &str, creds: &Credentials) -> std::io::Result<Store> {
        let json = serde_json::to_string(creds).map_err(std::io::Error::other)?;
        self.write_file(server, &json)?;
        Ok(Store::File)
    }

    pub fn load(&self, server: &str) -> Option<(Credentials, Store)> {
        if let Some(c) = self.read_file(server) {
            return Some((c, Store::File));
        }
        // Stored in the keyring by version 0.1.0 (only reachable in Desktop Mode).
        let json = entry(server).and_then(|e| e.get_password()).ok()?;
        let c: Credentials = serde_json::from_str(&json).ok()?;
        match self.write_file(server, &json) {
            Ok(()) => {
                if let Ok(e) = entry(server) {
                    let _ = e.delete_credential();
                }
                tracing::info!("moved RomM credentials from the keyring to the credentials file");
            }
            Err(e) => tracing::warn!("could not move credentials out of the keyring: {e}"),
        }
        Some((c, Store::File))
    }

    pub fn clear(&self, server: &str) {
        if let Ok(e) = entry(server) {
            let _ = e.delete_credential();
        }
        let _ = std::fs::remove_file(&self.file);
    }

    fn read_file(&self, server: &str) -> Option<Credentials> {
        let text = std::fs::read_to_string(&self.file).ok()?;
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).ok()?;
        serde_json::from_value(map.get(server)?.clone()).ok()
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
        assert_eq!(s.read_file("https://a/"), Some(c));
        assert_eq!(s.read_file("https://b/"), None);
    }
}

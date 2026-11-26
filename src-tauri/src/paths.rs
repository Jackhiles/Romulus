//! Where Romulus keeps its files (XDG base directories).
//!
//! - config:  `~/.config/romulus/config.toml`
//! - data:    `~/.local/share/romulus/` (library databases, later save backups)
//! - cache:   `~/.cache/romulus/covers/`
//! - logs:    `~/.local/state/romulus/logs/`

use crate::app::APP_ID;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn from_system() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let data = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
        let cache = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
        let state = dirs::state_dir().unwrap_or_else(|| home.join(".local/state"));
        Self {
            config_dir: config.join(APP_ID),
            data_dir: data.join(APP_ID),
            cache_dir: cache.join(APP_ID),
            log_dir: state.join(APP_ID).join("logs"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.cache_dir.join("covers")
    }

    /// One library database per server, so switching servers never mixes libraries.
    pub fn library_db(&self, server_url: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let h = hex::encode(&Sha256::digest(server_url.as_bytes())[..8]);
        self.data_dir.join(format!("library-{h}.db"))
    }

    pub fn create_all(&self) -> std::io::Result<()> {
        for d in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

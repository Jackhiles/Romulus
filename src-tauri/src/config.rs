//! User settings, stored as TOML. Secrets never go here (see `secrets.rs`).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// RomM base URL, normalized (`https://romm.example.net/`).
    pub server_url: Option<String>,
    /// Username shown in settings (password sign-in only).
    pub username: Option<String>,
    /// Stable id for this install, sent when pairing so RomM recognizes the device.
    pub device_identifier: String,
    /// Log level: error, warn, info, debug, trace.
    pub log_level: String,
    /// Check GitHub for a new version on start.
    pub check_updates: bool,
    /// Start in fullscreen.
    pub fullscreen: bool,
    /// Max size of the cover cache in MB.
    pub cover_cache_mb: u64,
    /// Sync saves with RomM before and after playing.
    pub save_sync: bool,
    /// Use WebKitGTK's DMA-BUF renderer (default; much faster, but a blank window on
    /// some GPUs, in which case Settings or ROMULUS_RENDERER=compatible switch it off).
    pub fast_renderer: bool,
    /// Use the browser Gamepad API instead of reading the controller natively.
    pub web_gamepad: bool,
    /// Version that last ran, for "what's new" after an update.
    pub last_run_version: Option<String>,
    /// RetroDECK's ROM folder, when detection should be overridden.
    pub retrodeck_roms: Option<String>,
    /// Systems screen layout: carousel, list or grid.
    pub systems_view: String,
    /// Games screen layout per platform slug: list or grid (list when missing).
    pub games_view: std::collections::BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: None,
            username: None,
            device_identifier: String::new(),
            log_level: "info".into(),
            check_updates: true,
            fullscreen: true,
            cover_cache_mb: 1024,
            fast_renderer: true,
            save_sync: true,
            web_gamepad: false,
            last_run_version: None,
            retrodeck_roms: None,
            systems_view: "carousel".into(),
            games_view: Default::default(),
        }
    }
}

impl Config {
    /// Load the config, or defaults if the file is missing or unreadable. A broken file is
    /// kept as `config.toml.broken` instead of being overwritten silently.
    pub fn load(path: &Path) -> Self {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("config file is invalid ({e}); starting with defaults");
                    let _ = std::fs::rename(path, path.with_extension("toml.broken"));
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        };
        if cfg.device_identifier.is_empty() {
            use rand::Rng;
            let bytes: [u8; 16] = rand::rng().random();
            cfg.device_identifier = hex::encode(bytes);
        }
        cfg
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let mut c = Config::load(&p);
        assert_eq!(c.device_identifier.len(), 32);
        c.server_url = Some("https://romm.example.net/".into());
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "server_url = [").unwrap();
        let c = Config::load(&p);
        assert_eq!(c.server_url, None);
        assert!(dir.path().join("config.toml.broken").exists());
    }
}

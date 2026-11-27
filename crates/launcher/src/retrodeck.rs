//! RetroDECK's folders. The source of truth is `retrodeck.json` in its Flatpak config
//! (object `paths`); see docs/DECK_FINDINGS.md. Folders may be on an SD card whose label
//! contains spaces.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const FLATPAK_ID: &str = "net.retrodeck.retrodeck";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RetroDeck {
    pub roms: PathBuf,
    pub bios: Option<PathBuf>,
    pub saves: Option<PathBuf>,
    pub states: Option<PathBuf>,
}

impl RetroDeck {
    /// Find RetroDECK for the user whose home is `home`. `roms_override` (from settings)
    /// wins when set.
    pub fn detect(home: &Path, roms_override: Option<&Path>) -> Option<Self> {
        if let Some(r) = roms_override.filter(|p| p.is_dir()) {
            let base = r.parent().map(Path::to_path_buf);
            let sub = |n: &str| base.as_ref().map(|b| b.join(n)).filter(|p| p.is_dir());
            return Some(Self {
                roms: r.to_path_buf(),
                bios: sub("bios"),
                saves: sub("saves"),
                states: sub("states"),
            });
        }
        let config = home
            .join(".var/app")
            .join(FLATPAK_ID)
            .join("config/retrodeck/retrodeck.json");
        if let Some(rd) = std::fs::read_to_string(&config)
            .ok()
            .and_then(|t| Self::from_config(&t))
        {
            return Some(rd);
        }
        let fallback = home.join("retrodeck");
        fallback.join("roms").is_dir().then(|| Self {
            roms: fallback.join("roms"),
            bios: Some(fallback.join("bios")),
            saves: Some(fallback.join("saves")),
            states: Some(fallback.join("states")),
        })
    }

    /// Parse `retrodeck.json`. Only returns a value when the ROM folder exists (an SD
    /// card that is not inserted gives None).
    pub fn from_config(text: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let p = v.get("paths")?;
        let path = |k: &str| {
            p.get(k)
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        };
        let roms = path("roms_path").filter(|r| r.is_dir())?;
        Some(Self {
            roms,
            bios: path("bios_path"),
            saves: path("saves_path"),
            states: path("states_path"),
        })
    }

    /// Is the RetroDECK Flatpak installed (system or user)?
    pub fn flatpak_installed(home: &Path) -> bool {
        Path::new("/var/lib/flatpak/app").join(FLATPAK_ID).is_dir()
            || home
                .join(".local/share/flatpak/app")
                .join(FLATPAK_ID)
                .is_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_paths_from_config() {
        let dir = tempfile::tempdir().unwrap();
        let roms = dir.path().join("1TB SD/retrodeck/roms");
        std::fs::create_dir_all(&roms).unwrap();
        let cfg = serde_json::json!({"version": "0.10.10b", "paths": {
            "roms_path": roms, "bios_path": dir.path().join("1TB SD/retrodeck/bios"),
            "saves_path": "", "states_path": dir.path().join("1TB SD/retrodeck/states")
        }});
        let rd = RetroDeck::from_config(&cfg.to_string()).unwrap();
        assert_eq!(rd.roms, roms);
        assert_eq!(rd.saves, None);
        // SD card not inserted: the ROM folder is missing.
        let gone =
            serde_json::json!({"paths": {"roms_path": "/run/media/deck/nope/retrodeck/roms"}});
        assert_eq!(RetroDeck::from_config(&gone.to_string()), None);
    }

    #[test]
    fn detect_prefers_override_then_config_then_home() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(RetroDeck::detect(home.path(), None), None);
        std::fs::create_dir_all(home.path().join("retrodeck/roms")).unwrap();
        assert_eq!(
            RetroDeck::detect(home.path(), None).unwrap().roms,
            home.path().join("retrodeck/roms")
        );
        let other = home.path().join("other/roms");
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(
            RetroDeck::detect(home.path(), Some(&other)).unwrap().roms,
            other
        );
    }
}

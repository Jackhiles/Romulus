//! Where an emulator keeps a game's save, read from the emulator's own settings in
//! RetroDECK.

use crate::emulator::Emulator;
use crate::local::SaveSpec;
use std::path::{Path, PathBuf};

/// RetroDECK's emulator settings and save folder.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// RetroDECK's `saves` folder.
    pub saves: PathBuf,
    pub retroarch_cfg: PathBuf,
    pub pcsx2_ini: PathBuf,
}

impl Dirs {
    pub fn retrodeck(home: &Path, saves: PathBuf) -> Self {
        let cfg = home.join(".var/app/net.retrodeck.retrodeck/config");
        Self {
            saves,
            retroarch_cfg: cfg.join("retroarch/retroarch.cfg"),
            pcsx2_ini: cfg.join("PCSX2/inis/PCSX2.ini"),
        }
    }
}

/// A game's save location and how it is named on RomM.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub spec: SaveSpec,
    /// `emulator` on RomM.
    pub emulator: String,
    /// File name for uploads (RomM adds a timestamp tag).
    pub upload_name: String,
    /// Extension of compatible saves on RomM (`srm`, `zip`).
    pub ext: &'static str,
}

/// `key = "value"` lines (retroarch.cfg).
fn cfg_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// `key = value` in an ini `[section]`.
fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            inside = l == format!("[{section}]");
        } else if inside
            && let Some((k, v)) = l.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Where `emu` keeps the save of the game started from `launch`. `save_target` and
/// `layout` are RomM's (`BASLUS-21008`, `folder-prefix`). Err says why it isn't
/// supported.
pub fn locate(
    emu: &Emulator,
    launch: &Path,
    dirs: &Dirs,
    save_target: Option<&str>,
    layout: Option<&str>,
) -> Result<Located, String> {
    match emu {
        Emulator::RetroArch { .. } => {
            let cfg = std::fs::read_to_string(&dirs.retroarch_cfg).unwrap_or_default();
            if cfg_value(&cfg, "sort_savefiles_enable").as_deref() == Some("true") {
                return Err(
                    "RetroArch sorts saves by core name, which Romulus can't follow yet".into(),
                );
            }
            let mut dir = cfg_value(&cfg, "savefile_directory")
                .filter(|d| !d.is_empty() && d != "default")
                .map(PathBuf::from)
                .unwrap_or_else(|| dirs.saves.clone());
            if cfg_value(&cfg, "sort_savefiles_by_content_enable").as_deref() == Some("true") {
                let content_dir = launch
                    .parent()
                    .and_then(|p| p.file_name())
                    .ok_or("game has no folder")?;
                dir = dir.join(content_dir);
            }
            let stem = launch
                .file_stem()
                .ok_or("game has no file name")?
                .to_string_lossy()
                .into_owned();
            Ok(Located {
                spec: SaveSpec::File {
                    path: dir.join(format!("{stem}.srm")),
                },
                emulator: emu.romm_name(),
                upload_name: format!("{stem}.srm"),
                ext: "srm",
            })
        }
        Emulator::Pcsx2 => {
            let ini = std::fs::read_to_string(&dirs.pcsx2_ini).unwrap_or_default();
            if ini_value(&ini, "MemoryCards", "Slot1_Enable").as_deref() == Some("false") {
                return Err("PCSX2's memory card slot 1 is off".into());
            }
            let cards = ini_value(&ini, "Folders", "MemoryCards")
                .filter(|p| Path::new(p).is_absolute())
                .map(PathBuf::from)
                .unwrap_or_else(|| dirs.saves.join("ps2/pcsx2/memcards"));
            let card = cards.join(
                ini_value(&ini, "MemoryCards", "Slot1_Filename")
                    .unwrap_or_else(|| "Mcd001.ps2".into()),
            );
            if !card.is_dir() {
                return Err(
                    "PCSX2 uses a single-file memory card; per-game sync needs a folder card"
                        .into(),
                );
            }
            let prefix = match (save_target, layout) {
                (Some(t), Some("folder-prefix")) if !t.is_empty() => t.to_string(),
                _ => return Err("RomM doesn't know this game's PS2 serial".into()),
            };
            Ok(Located {
                spec: SaveSpec::CardFolders {
                    card_dir: card,
                    prefix,
                },
                emulator: emu.romm_name(),
                upload_name: "autosave.zip".into(),
                ext: "zip",
            })
        }
        Emulator::Other { label } => Err(format!("save sync for {label} isn't supported yet")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(d: &Path, ra: &str, pcsx2: &str) -> Dirs {
        std::fs::write(d.join("retroarch.cfg"), ra).unwrap();
        std::fs::write(d.join("PCSX2.ini"), pcsx2).unwrap();
        Dirs {
            saves: d.join("saves"),
            retroarch_cfg: d.join("retroarch.cfg"),
            pcsx2_ini: d.join("PCSX2.ini"),
        }
    }

    #[test]
    fn retroarch_follows_its_settings() {
        let d = tempfile::tempdir().unwrap();
        // RetroDECK 0.10.10b's settings on the Deck.
        let ra = "savefile_directory = \"/sd/retrodeck/saves\"\nsort_savefiles_by_content_enable = \"true\"\nsort_savefiles_enable = \"false\"\n";
        let ds = dirs(d.path(), ra, "");
        let emu = Emulator::RetroArch {
            core: "swanstation".into(),
        };
        let mgs = Path::new(
            "/sd/retrodeck/roms/psx/Metal Gear Solid (USA) (Rev 1)/Metal Gear Solid (USA) (Rev 1).m3u",
        );
        let l = locate(&emu, mgs, &ds, Some("SLUS-00594"), Some("file-prefix")).unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::File {
                path: PathBuf::from(
                    "/sd/retrodeck/saves/Metal Gear Solid (USA) (Rev 1)/Metal Gear Solid (USA) (Rev 1).srm"
                )
            }
        );
        assert_eq!(l.emulator, "retroarch-swanstation");
        let single = locate(
            &emu,
            Path::new("/sd/retrodeck/roms/psx/Alundra.chd"),
            &ds,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            single.spec,
            SaveSpec::File {
                path: PathBuf::from("/sd/retrodeck/saves/psx/Alundra.srm")
            }
        );

        let ds = dirs(d.path(), "sort_savefiles_enable = \"true\"\n", "");
        assert!(locate(&emu, mgs, &ds, None, None).is_err());
    }

    #[test]
    fn pcsx2_uses_the_folder_card_and_the_serial() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(
            d.path(),
            "",
            "[MemoryCards]\nSlot1_Enable = true\nSlot1_Filename = Memcard.ps2\n",
        );
        let game =
            Path::new("/sd/retrodeck/roms/ps2/Katamari Damacy (USA)/Katamari Damacy (USA).chd");
        // The card is a file, not a folder: not supported.
        assert!(
            locate(
                &Emulator::Pcsx2,
                game,
                &ds,
                Some("BASLUS-21008"),
                Some("folder-prefix")
            )
            .is_err()
        );
        std::fs::create_dir_all(d.path().join("saves/ps2/pcsx2/memcards/Memcard.ps2")).unwrap();
        let l = locate(
            &Emulator::Pcsx2,
            game,
            &ds,
            Some("BASLUS-21008"),
            Some("folder-prefix"),
        )
        .unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::CardFolders {
                card_dir: d.path().join("saves/ps2/pcsx2/memcards/Memcard.ps2"),
                prefix: "BASLUS-21008".into()
            }
        );
        assert_eq!((l.emulator.as_str(), l.ext), ("pcsx2", "zip"));
        assert!(locate(&Emulator::Pcsx2, game, &ds, None, None).is_err());
    }
}

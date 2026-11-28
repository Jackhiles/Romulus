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
    /// PPSSPP's memory stick (holds `PSP/SAVEDATA`).
    pub ppsspp_memstick: PathBuf,
    /// DuckStation's user folder (`settings.ini`, `savestates`).
    pub duckstation: PathBuf,
    /// DuckStation's game database (`gamedb.yaml`), for its memory card names.
    pub duckstation_gamedb: PathBuf,
    pub dolphin_ini: PathBuf,
    /// Dolphin's user folder (`GC`, `Wii`, `StateSaves`).
    pub dolphin_user: PathBuf,
    pub cemu_settings: PathBuf,
    /// RetroDECK's flatpak data folder, which its emulators see as `/var/data`.
    pub flatpak_data: PathBuf,
}

impl Dirs {
    pub fn retrodeck(home: &Path, saves: PathBuf) -> Self {
        let cfg = home.join(".var/app/net.retrodeck.retrodeck/config");
        let data = home.join(".var/app/net.retrodeck.retrodeck/data");
        Self {
            saves,
            retroarch_cfg: cfg.join("retroarch/retroarch.cfg"),
            pcsx2_ini: cfg.join("PCSX2/inis/PCSX2.ini"),
            // RetroDECK links PSP/SAVEDATA here to saves/PSP/PPSSPP-SA.
            ppsspp_memstick: cfg.join("ppsspp"),
            duckstation: cfg.join("duckstation"),
            duckstation_gamedb: flatpak_file(
                home,
                "retrodeck/components/duckstation/bin/resources/gamedb.yaml",
            ),
            dolphin_ini: cfg.join("dolphin-emu/Dolphin.ini"),
            dolphin_user: data.join("dolphin-emu"),
            cemu_settings: cfg.join("Cemu/settings.xml"),
            flatpak_data: data,
        }
    }

    /// A path from an emulator's settings, as seen from outside the flatpak.
    pub fn host_path(&self, p: &str) -> PathBuf {
        match p.strip_prefix("/var/data/") {
            Some(rest) => self.flatpak_data.join(rest),
            None => PathBuf::from(p),
        }
    }
}

/// A file inside RetroDECK's flatpak: the user installation, else the system one.
pub fn flatpak_file(home: &Path, rel: &str) -> PathBuf {
    let app = "net.retrodeck.retrodeck/current/active/files";
    let user = home.join(".local/share/flatpak/app").join(app).join(rel);
    if user.is_file() {
        user
    } else {
        Path::new("/var/lib/flatpak/app").join(app).join(rel)
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
    /// Extensions of compatible saves on RomM (`srm`, `zip`).
    pub exts: &'static [&'static str],
    /// Something the player should know before playing (e.g. an unformatted card).
    pub warning: Option<String>,
    /// The folders are on a PCSX2 folder memory card (which may need formatting).
    pub pcsx2_card: bool,
}

/// PCSX2 treats a folder card as formatted only when `_pcsx2_superblock` holds a PS2
/// card header (it checks byte 0x16, the "o" of "Format"). A new card's superblock is
/// empty until a game or the PS2 BIOS formats it, and until then saves in its folders
/// don't appear. (PCSX2: MemoryCardFolder.cpp, IsFormatted / LoadMemoryCardData.)
/// The superblock a PS2 writes when it formats a standard 8 MB card, for PCSX2's folder
/// cards (`_pcsx2_superblock`, one 8 KB block). PCSX2 rebuilds the allocation table of
/// a folder card from these fields (MemoryCardFolder.h `superblock`, CreateFat,
/// SetSizeInClusters), so this is all a new card needs to count as formatted.
pub fn formatted_superblock() -> Vec<u8> {
    const CLUSTERS: u32 = 8192;
    let alloc_offset = CLUSTERS / 0x100 + 9; // 41, as PCSX2's SetSizeInClusters
    let blocks = CLUSTERS / 8;
    let mut b = vec![0u8; 8192];
    b[0x00..0x1c].copy_from_slice(b"Sony PS2 Memory Card Format ");
    b[0x1c..0x23].copy_from_slice(b"1.2.0.0");
    b[0x28..0x2a].copy_from_slice(&512u16.to_le_bytes()); // page_len
    b[0x2a..0x2c].copy_from_slice(&2u16.to_le_bytes()); // pages_per_cluster
    b[0x2c..0x2e].copy_from_slice(&16u16.to_le_bytes()); // pages_per_block
    b[0x2e..0x30].copy_from_slice(&0xFF00u16.to_le_bytes());
    b[0x30..0x34].copy_from_slice(&CLUSTERS.to_le_bytes());
    b[0x34..0x38].copy_from_slice(&alloc_offset.to_le_bytes());
    b[0x38..0x3c].copy_from_slice(&(CLUSTERS - 0x10 - alloc_offset).to_le_bytes()); // alloc_end
    b[0x3c..0x40].copy_from_slice(&0u32.to_le_bytes()); // rootdir_cluster
    b[0x40..0x44].copy_from_slice(&(blocks - 1).to_le_bytes()); // backup_block1
    b[0x44..0x48].copy_from_slice(&(blocks - 2).to_le_bytes()); // backup_block2
    b[0x50..0x54].copy_from_slice(&8u32.to_le_bytes()); // ifc_list[0]
    for i in 0..32 {
        let o = 0xd0 + i * 4; // bad_block_list: none
        b[o..o + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    }
    b[0x150] = 2; // card_type: PS2
    b[0x151] = 0x52; // card_flags
    b
}

pub fn folder_card_formatted(card: &Path) -> bool {
    std::fs::read(card.join("_pcsx2_superblock")).is_ok_and(|b| b.get(0x16) == Some(&0x6F))
}

const DAMAGED_CARD: &str = "PCSX2's memory card header (_pcsx2_superblock) is damaged, so saves on it may not show. Romulus leaves it alone; check the card in PCSX2's memory card settings.";

/// Make a new (never formatted) PCSX2 folder card formatted, as a PS2 would on first
/// use, so restored saves show. Only a missing or empty superblock is replaced, after
/// copying it to `backup_dir`; a non-empty one that isn't valid is left alone. Returns
/// whether it wrote one. Checked on a Deck: PCSX2 then indexes the card's folders.
pub fn ensure_card_formatted(card: &Path, backup_dir: &Path) -> std::io::Result<bool> {
    let sb = card.join("_pcsx2_superblock");
    let len = match std::fs::metadata(&sb) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };
    if len != 0 || !card.is_dir() {
        return Ok(false);
    }
    if sb.exists() {
        std::fs::create_dir_all(backup_dir)?;
        std::fs::copy(&sb, backup_dir.join("_pcsx2_superblock"))?;
    }
    let tmp = card.join("_pcsx2_superblock.romulus-tmp");
    std::fs::write(&tmp, formatted_superblock())?;
    std::fs::rename(&tmp, &sb)?;
    tracing::info!("formatted PCSX2 folder card {}", card.display());
    Ok(true)
}

/// A superblock that exists, isn't empty, and isn't valid.
fn card_damaged(card: &Path) -> bool {
    std::fs::read(card.join("_pcsx2_superblock"))
        .is_ok_and(|b| !b.is_empty() && b.get(0x16) != Some(&0x6F))
}

/// RetroArch cores that keep saves somewhere other than `<game>.srm` (memory card
/// folders or images in the system folder).
const NON_SRM_CORES: &[&str] = &[
    "ppsspp", "pcsx2", "lrps2", "pcee2", "dolphin", "flycast", "play",
];

/// PS1 emulators whose save is the raw 128 KB memory card image, so their saves can
/// be swapped: SwanStation, Beetle PSX and PCSX ReARMed (`.srm`), DuckStation (`.mcd`).
const PSX_CARD_CORES: &[&str] = &[
    "swanstation",
    "mednafen_psx",
    "mednafen_psx_hw",
    "pcsx_rearmed",
];
const PSX_CARD: &[&str] = &["srm", "mcd"];

/// `<tag>value</tag>` (Cemu's settings.xml).
fn xml_value(text: &str, tag: &str) -> Option<String> {
    let start = text.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = start + text[start..].find(&format!("</{tag}>"))?;
    let v = text[start..end].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// RomM's `folder-exact` target: a title ID's low half, 8 hex digits.
fn title_id(save_target: Option<&str>, layout: Option<&str>) -> Option<String> {
    match (save_target, layout) {
        (Some(t), Some("folder-exact"))
            if t.len() == 8 && t.bytes().all(|c| c.is_ascii_hexdigit()) =>
        {
            Some(t.to_ascii_lowercase())
        }
        _ => None,
    }
}

/// A save kept as a zip of files on RomM.
fn zipped(spec: SaveSpec, emu: &Emulator) -> Located {
    Located {
        spec,
        emulator: emu.romm_name(),
        upload_name: "autosave.zip".into(),
        exts: &["zip"],
        warning: None,
        pcsx2_card: false,
    }
}

/// `key = "value"` lines (retroarch.cfg).
pub(crate) fn cfg_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// `key = value` in an ini `[section]`.
pub(crate) fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
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
        Emulator::RetroArch { core } if core == "flycast" => Err(
            "Flycast keeps one VMU for every Dreamcast game, so its saves can't sync per game"
                .into(),
        ),
        Emulator::RetroArch { core } if NON_SRM_CORES.contains(&core.as_str()) => Err(format!(
            "save sync for RetroArch's {core} core isn't supported yet"
        )),
        Emulator::Ppsspp => {
            let dir = dirs.ppsspp_memstick.join("PSP/SAVEDATA");
            let prefix = match (save_target, layout) {
                (Some(t), Some("folder-prefix")) if !t.is_empty() => t.to_string(),
                _ => return Err("RomM doesn't know this game's PSP ID".into()),
            };
            Ok(Located {
                spec: SaveSpec::CardFolders {
                    card_dir: dir,
                    prefix,
                },
                emulator: emu.romm_name(),
                upload_name: "autosave.zip".into(),
                exts: &["zip"],
                warning: None,
                pcsx2_card: false,
            })
        }
        Emulator::DuckStation => {
            let ini =
                std::fs::read_to_string(dirs.duckstation.join("settings.ini")).unwrap_or_default();
            let s = crate::duckstation::card_settings(
                &ini,
                &dirs.saves.join("psx/duckstation/memcards"),
            );
            let serial = match (save_target, layout) {
                (Some(t), Some("file-prefix")) if !t.is_empty() => Some(t),
                _ => None,
            };
            let path = crate::duckstation::card_path(&s, &dirs.duckstation_gamedb, launch, serial)?;
            let upload_name = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .ok_or("bad memory card name")?;
            Ok(Located {
                spec: SaveSpec::File { path },
                emulator: emu.romm_name(),
                upload_name,
                exts: PSX_CARD,
                warning: None,
                pcsx2_card: false,
            })
        }
        Emulator::RetroArch { core } => {
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
            // DeSmuME keeps its own `.dsv` (the save plus a 122-byte footer).
            let (ext, exts): (&str, &'static [&'static str]) = if core.starts_with("desmume") {
                ("dsv", &["dsv"])
            } else if PSX_CARD_CORES.contains(&core.as_str()) {
                ("srm", PSX_CARD)
            } else {
                ("srm", &["srm"])
            };
            Ok(Located {
                spec: SaveSpec::File {
                    path: dir.join(format!("{stem}.{ext}")),
                },
                emulator: emu.romm_name(),
                upload_name: format!("{stem}.{ext}"),
                exts,
                warning: None,
                pcsx2_card: false,
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
            let warning = card_damaged(&card).then(|| DAMAGED_CARD.to_string());
            Ok(Located {
                spec: SaveSpec::CardFolders {
                    card_dir: card,
                    prefix,
                },
                emulator: emu.romm_name(),
                upload_name: "autosave.zip".into(),
                exts: &["zip"],
                warning,
                pcsx2_card: true,
            })
        }
        Emulator::Dolphin => {
            let ini = std::fs::read_to_string(&dirs.dolphin_ini).unwrap_or_default();
            let disc = crate::disc::read(launch);
            let wii = match &disc {
                Some(d) => d.wii,
                None => layout == Some("folder-exact"),
            };
            if wii {
                let title = disc
                    .as_ref()
                    .map(|d| d.wii_title())
                    .or_else(|| title_id(save_target, layout))
                    .ok_or("Romulus couldn't read this Wii game's ID")?;
                let nand = ini_value(&ini, "General", "NANDRootPath")
                    .filter(|p| !p.is_empty())
                    .map(|p| dirs.host_path(&p))
                    .unwrap_or_else(|| dirs.dolphin_user.join("Wii"));
                let dir = nand.join("title/00010000").join(title).join("data");
                return Ok(zipped(SaveSpec::Folder { dir }, emu));
            }
            match ini_value(&ini, "Core", "SlotA").as_deref() {
                None | Some("8") => {}
                Some("1") => {
                    return Err("Dolphin uses a memory card file in slot A; per-game sync needs its GCI folder".into());
                }
                Some(_) => return Err("Dolphin has no memory card in slot A".into()),
            }
            let code = disc
                .map(|d| d.code().to_string())
                .or_else(|| {
                    save_target
                        .filter(|t| layout == Some("file-prefix") && crate::disc::valid_code(t))
                        .map(str::to_string)
                })
                .ok_or("Romulus couldn't read this GameCube game's ID")?;
            let dir = ini_value(&ini, "Core", "GCIFolderAPathOverride")
                .filter(|p| !p.is_empty())
                .map(|p| dirs.host_path(&p))
                .unwrap_or_else(|| {
                    dirs.dolphin_user
                        .join("GC")
                        .join(crate::disc::gc_region(&code))
                        .join("Card A")
                });
            Ok(zipped(SaveSpec::GciFiles { dir, game_id: code }, emu))
        }
        Emulator::Cemu => {
            let xml = std::fs::read_to_string(&dirs.cemu_settings).unwrap_or_default();
            let mlc = xml_value(&xml, "mlc_path").ok_or("Cemu's MLC folder isn't set")?;
            let title = title_id(save_target, layout)
                .ok_or("RomM doesn't know this Wii U game's title ID")?;
            let dir = dirs.host_path(&mlc).join("usr/save/00050000").join(title);
            Ok(zipped(SaveSpec::Folder { dir }, emu))
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
            ppsspp_memstick: d.join("ppsspp"),
            duckstation: d.join("duckstation"),
            duckstation_gamedb: d.join("gamedb.yaml"),
            dolphin_ini: d.join("Dolphin.ini"),
            dolphin_user: d.join("data/dolphin-emu"),
            cemu_settings: d.join("cemu.xml"),
            flatpak_data: d.join("data"),
        }
    }

    #[test]
    fn duckstation_uses_its_per_title_card_and_takes_swanstation_saves() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        std::fs::write(
            d.path().join("gamedb.yaml"),
            "SLUS-00553:\n  name: \"Alundra (USA)\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.path().join("duckstation")).unwrap();
        std::fs::write(
            d.path().join("duckstation/settings.ini"),
            "[MemoryCards]\nCard1Type = PerGameTitle\n",
        )
        .unwrap();
        let game = Path::new("/sd/retrodeck/roms/psx/Alundra (USA) (Rev 1).m3u");
        let l = locate(
            &Emulator::DuckStation,
            game,
            &ds,
            Some("SLUS-00553"),
            Some("file-prefix"),
        )
        .unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::File {
                path: d
                    .path()
                    .join("saves/psx/duckstation/memcards/Alundra (USA)_1.mcd")
            }
        );
        assert_eq!(
            (l.emulator.as_str(), l.upload_name.as_str(), l.exts),
            ("duckstation", "Alundra (USA)_1.mcd", PSX_CARD)
        );
        let swan = Emulator::RetroArch {
            core: "swanstation".into(),
        };
        assert_eq!(locate(&swan, game, &ds, None, None).unwrap().exts, PSX_CARD);
        let snes = Emulator::RetroArch {
            core: "snes9x".into(),
        };
        assert_eq!(locate(&snes, game, &ds, None, None).unwrap().exts, ["srm"]);
    }

    fn disc_image(path: &Path, id: &str, wii: bool) {
        let mut h = vec![0u8; 0x40];
        h[..6].copy_from_slice(id.as_bytes());
        let (at, magic) = if wii {
            (0x18, 0x5D1C_9EA3u32)
        } else {
            (0x1C, 0xC233_9F3Du32)
        };
        h[at..at + 4].copy_from_slice(&magic.to_be_bytes());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, h).unwrap();
    }

    #[test]
    fn dolphin_gamecube_uses_the_games_gci_files() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        // RetroDECK's Dolphin.ini on the Deck.
        std::fs::write(
            &ds.dolphin_ini,
            "[Core]\nSlotA = 8\nSlotB = 255\n[General]\nNANDRootPath = /var/data/dolphin-emu/Wii/\n",
        )
        .unwrap();
        let game = d.path().join("roms/gc/Metroid Prime/Metroid Prime.iso");
        disc_image(&game, "GM8E01", false);
        let l = locate(
            &Emulator::Dolphin,
            &game,
            &ds,
            Some("GM8E"),
            Some("file-prefix"),
        )
        .unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::GciFiles {
                dir: d.path().join("data/dolphin-emu/GC/USA/Card A"),
                game_id: "GM8E".into()
            }
        );
        assert_eq!((l.emulator.as_str(), l.exts), ("dolphin", &["zip"][..]));

        // RomM reports "CISO" for compressed images; the disc header is read instead,
        // and without a readable header that target isn't trusted.
        let ciso = d.path().join("roms/gc/Batman (Europe).ciso");
        let mut b = vec![0u8; 0x8000];
        b.extend(&std::fs::read(&game).unwrap());
        std::fs::write(&ciso, b).unwrap();
        let l = locate(
            &Emulator::Dolphin,
            &ciso,
            &ds,
            Some("CISO"),
            Some("file-prefix"),
        )
        .unwrap();
        assert!(matches!(l.spec, SaveSpec::GciFiles { ref game_id, .. } if game_id == "GM8E"));
        let missing = d.path().join("roms/gc/Gone.ciso");
        assert!(
            locate(
                &Emulator::Dolphin,
                &missing,
                &ds,
                Some("CISO"),
                Some("file-prefix")
            )
            .is_err()
        );
        let l = locate(
            &Emulator::Dolphin,
            &missing,
            &ds,
            Some("GM8P"),
            Some("file-prefix"),
        )
        .unwrap();
        assert!(
            matches!(l.spec, SaveSpec::GciFiles { ref dir, .. } if dir.ends_with("GC/EUR/Card A"))
        );

        // A memory card file can't be split per game.
        std::fs::write(&ds.dolphin_ini, "[Core]\nSlotA = 1\n").unwrap();
        assert!(locate(&Emulator::Dolphin, &game, &ds, None, None).is_err());
    }

    #[test]
    fn dolphin_wii_uses_the_titles_nand_folder() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        std::fs::write(
            &ds.dolphin_ini,
            "[General]\nNANDRootPath = /var/data/dolphin-emu/Wii/\n",
        )
        .unwrap();
        let game = d.path().join("roms/wii/Mario Kart Wii.wbfs");
        let mut b = vec![0u8; 0x200];
        disc_image(&d.path().join("h"), "RMCE01", true);
        b.extend(std::fs::read(d.path().join("h")).unwrap());
        std::fs::create_dir_all(game.parent().unwrap()).unwrap();
        std::fs::write(&game, b).unwrap();
        let want = SaveSpec::Folder {
            dir: d
                .path()
                .join("data/dolphin-emu/Wii/title/00010000/524d4345/data"),
        };
        let l = locate(
            &Emulator::Dolphin,
            &game,
            &ds,
            Some("524d4345"),
            Some("folder-exact"),
        )
        .unwrap();
        assert_eq!(l.spec, want);
        // Without a readable image, RomM's title ID.
        let gone = d.path().join("roms/wii/Gone.gcz");
        let l = locate(
            &Emulator::Dolphin,
            &gone,
            &ds,
            Some("524D4345"),
            Some("folder-exact"),
        )
        .unwrap();
        assert_eq!(l.spec, want);
    }

    #[test]
    fn cemu_uses_the_titles_save_folder() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        let game = Path::new("/sd/retrodeck/roms/wiiu/Nintendo Land.wua");
        assert!(
            locate(
                &Emulator::Cemu,
                game,
                &ds,
                Some("10102000"),
                Some("folder-exact")
            )
            .is_err()
        );
        std::fs::write(
            &ds.cemu_settings,
            "<content>\n    <mlc_path>/sd/retrodeck/bios/cemu</mlc_path>\n</content>",
        )
        .unwrap();
        let l = locate(
            &Emulator::Cemu,
            game,
            &ds,
            Some("10102000"),
            Some("folder-exact"),
        )
        .unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::Folder {
                dir: PathBuf::from("/sd/retrodeck/bios/cemu/usr/save/00050000/10102000")
            }
        );
        assert!(locate(&Emulator::Cemu, game, &ds, None, None).is_err());
    }

    #[test]
    fn desmume_keeps_dsv_files_and_flycast_is_explained() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        let game = Path::new("/sd/retrodeck/roms/nds/Club House Games.nds");
        let desmume = Emulator::RetroArch {
            core: "desmume".into(),
        };
        let l = locate(&desmume, game, &ds, None, None).unwrap();
        assert_eq!(
            (l.upload_name.as_str(), l.exts),
            ("Club House Games.dsv", &["dsv"][..])
        );
        let flycast = Emulator::RetroArch {
            core: "flycast".into(),
        };
        assert!(
            locate(&flycast, game, &ds, None, None)
                .unwrap_err()
                .contains("VMU")
        );
    }

    #[test]
    fn ppsspp_uses_savedata_folders_by_game_id() {
        let d = tempfile::tempdir().unwrap();
        let ds = dirs(d.path(), "", "");
        let game = Path::new("/sd/retrodeck/roms/psp/3rd Birthday, The (USA).iso");
        let l = locate(
            &Emulator::Ppsspp,
            game,
            &ds,
            Some("ULUS10567"),
            Some("folder-prefix"),
        )
        .unwrap();
        assert_eq!(
            l.spec,
            SaveSpec::CardFolders {
                card_dir: d.path().join("ppsspp/PSP/SAVEDATA"),
                prefix: "ULUS10567".into()
            }
        );
        assert_eq!(
            (l.emulator.as_str(), l.exts, l.pcsx2_card),
            ("ppsspp", &["zip"][..], false)
        );
        assert!(locate(&Emulator::Ppsspp, game, &ds, None, None).is_err());
        // RetroArch's PPSSPP core doesn't use .srm files.
        let core = Emulator::RetroArch {
            core: "ppsspp".into(),
        };
        assert!(locate(&core, game, &ds, None, None).is_err());
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
    fn formatted_superblock_matches_pcsx2() {
        let b = formatted_superblock();
        assert_eq!(b.len(), 8192); // PCSX2 reads exactly one block
        assert_eq!(b[0x16], 0x6F); // its IsFormatted() check
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        // The values PCSX2's SetSizeInClusters(8192) computes.
        assert_eq!((u32_at(0x30), u32_at(0x34), u32_at(0x38)), (8192, 41, 8135));
        assert_eq!((u32_at(0x40), u32_at(0x44)), (1023, 1022));
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("_pcsx2_superblock"), &b).unwrap();
        assert!(folder_card_formatted(d.path()));
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
        assert_eq!((l.emulator.as_str(), l.exts), ("pcsx2", &["zip"][..]));
        // A new card (empty superblock, as PCSX2 creates it) gets formatted, with a
        // backup of the empty file; no warning.
        let cardd = d.path().join("saves/ps2/pcsx2/memcards/Memcard.ps2");
        std::fs::write(cardd.join("_pcsx2_superblock"), b"").unwrap();
        let l = locate(
            &Emulator::Pcsx2,
            game,
            &ds,
            Some("BASLUS-21008"),
            Some("folder-prefix"),
        )
        .unwrap();
        assert_eq!(l.warning, None);
        let bk = d.path().join("bk");
        assert!(ensure_card_formatted(&cardd, &bk).unwrap());
        assert!(folder_card_formatted(&cardd));
        assert_eq!(std::fs::read(bk.join("_pcsx2_superblock")).unwrap(), b"");
        // Already formatted: left alone.
        assert!(!ensure_card_formatted(&cardd, &bk).unwrap());
        // Damaged: left alone, with a warning.
        std::fs::write(cardd.join("_pcsx2_superblock"), b"garbage").unwrap();
        assert!(!ensure_card_formatted(&cardd, &bk).unwrap());
        assert_eq!(
            std::fs::read(cardd.join("_pcsx2_superblock")).unwrap(),
            b"garbage"
        );
        let l = locate(
            &Emulator::Pcsx2,
            game,
            &ds,
            Some("BASLUS-21008"),
            Some("folder-prefix"),
        )
        .unwrap();
        assert!(l.warning.is_some());
        assert!(locate(&Emulator::Pcsx2, game, &ds, None, None).is_err());
    }
}

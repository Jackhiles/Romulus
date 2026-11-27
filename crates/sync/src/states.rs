//! Save states: each state file of a game is synced on its own with RomM.
//!
//! States only load in the emulator (and usually the version) that made them, so only
//! states RomM got from the same emulator are used: ones made in RomM's web player, or
//! with another emulator, are left alone. RomM keeps every uploaded state (each gets a
//! timestamp tag), so an upload never replaces anything there; a download replaces the
//! Deck's file only after it was copied to the backups folder.
//!
//! The decision per file is like the save engine's, except that when both sides
//! changed the newer one wins (the other is backed up): states are snapshots the
//! player makes on purpose, not one save that progresses.

use crate::emulator::Emulator;
use crate::layout::{Dirs, cfg_value, ini_value};
use crate::run::stamp;
use crate::{Ctx, Error, Result};
use md5::{Digest, Md5};
use romm_api::ServerState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Which files in a folder are this game's states.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// RetroArch: `<stem>.state`, `<stem>.state<N>`, `<stem>.state.auto`.
    RetroArch { stem: String },
    /// `<prefix>…<suffix>`: PCSX2 `SLUS-21008 (CRC).01.p2s`, PPSSPP
    /// `ULUS10567_1.00_0.ppst`, DuckStation `SLUS-00553_1.sav`.
    Affix { prefix: String, suffix: String },
}

impl Pattern {
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Pattern::RetroArch { stem } => name
                .strip_prefix(stem.as_str())
                .and_then(|r| r.strip_prefix(".state"))
                .is_some_and(|r| r == ".auto" || r.chars().all(|c| c.is_ascii_digit())),
            Pattern::Affix { prefix, suffix } => {
                name.len() > prefix.len() + suffix.len()
                    && name.starts_with(prefix.as_str())
                    && name.ends_with(suffix.as_str())
            }
        }
    }
}

/// Where a game's states are.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSet {
    pub dir: PathBuf,
    pub pattern: Pattern,
    /// `emulator` on RomM.
    pub emulator: String,
}

/// RetroDECK's `states` folder, next to `saves`.
fn states_root(dirs: &Dirs) -> PathBuf {
    dirs.saves.with_file_name("states")
}

/// PS2 serial from RomM's memory card folder prefix (`BASLUS-21008` → `SLUS-21008`).
fn ps2_serial(save_target: Option<&str>) -> Option<String> {
    let t = save_target?;
    // B + region letter (A, E, I), then the disc serial.
    let s = if t.starts_with('B') { t.get(2..)? } else { t };
    (s.len() > 4 && s.contains('-')).then(|| s.to_string())
}

/// Where `emu` keeps the states of the game started from `launch`. Err says why they
/// aren't synced.
pub fn locate(
    emu: &Emulator,
    launch: &Path,
    dirs: &Dirs,
    save_target: Option<&str>,
) -> std::result::Result<StateSet, String> {
    let emulator = emu.romm_name();
    let set = |dir: PathBuf, pattern| {
        Ok(StateSet {
            dir,
            pattern,
            emulator: emulator.clone(),
        })
    };
    let affix = |prefix: String, suffix: &str| Pattern::Affix {
        prefix,
        suffix: suffix.to_string(),
    };
    let target = save_target.filter(|t| !t.is_empty());
    match emu {
        Emulator::RetroArch { .. } => {
            let cfg = std::fs::read_to_string(&dirs.retroarch_cfg).unwrap_or_default();
            if cfg_value(&cfg, "sort_savestates_enable").as_deref() == Some("true") {
                return Err(
                    "RetroArch sorts states by core name, which Romulus can't follow yet".into(),
                );
            }
            let dir = if cfg_value(&cfg, "savestates_in_content_dir").as_deref() == Some("true") {
                launch.parent().ok_or("game has no folder")?.to_path_buf()
            } else {
                let mut d = cfg_value(&cfg, "savestate_directory")
                    .filter(|d| !d.is_empty() && d != "default")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| states_root(dirs));
                if cfg_value(&cfg, "sort_savestates_by_content_enable").as_deref() == Some("true") {
                    d = d.join(
                        launch
                            .parent()
                            .and_then(|p| p.file_name())
                            .ok_or("game has no folder")?,
                    );
                }
                d
            };
            let stem = launch
                .file_stem()
                .ok_or("game has no file name")?
                .to_string_lossy()
                .into_owned();
            set(dir, Pattern::RetroArch { stem })
        }
        Emulator::Pcsx2 => {
            let ini = std::fs::read_to_string(&dirs.pcsx2_ini).unwrap_or_default();
            let dir = ini_value(&ini, "Folders", "SaveStates")
                .filter(|p| Path::new(p).is_absolute())
                .map(PathBuf::from)
                .unwrap_or_else(|| states_root(dirs).join("ps2/pcsx2"));
            let serial = ps2_serial(target).ok_or("RomM doesn't know this game's PS2 serial")?;
            set(dir, affix(format!("{serial} ("), ".p2s"))
        }
        Emulator::Ppsspp => {
            let id = target.ok_or("RomM doesn't know this game's PSP ID")?;
            set(
                dirs.ppsspp_memstick.join("PSP/PPSSPP_STATE"),
                affix(format!("{id}_"), ".ppst"),
            )
        }
        Emulator::DuckStation => {
            let serial = target.ok_or("RomM doesn't know this game's PS1 serial")?;
            set(
                dirs.duckstation.join("savestates"),
                affix(format!("{serial}_"), ".sav"),
            )
        }
        Emulator::Other { label } => Err(format!("state sync for {label} isn't supported yet")),
    }
}

/// One state file as it was when last in sync.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StateRecord {
    /// md5 of the Deck's file.
    pub local: Option<String>,
    /// RomM's state as `id@updated_at`.
    pub server: Option<String>,
    #[serde(default)]
    pub synced_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateAction {
    Nothing,
    InSync,
    Upload,
    Download,
    /// Both changed (or never synced and both exist): compare, then the newer wins.
    Both,
}

pub fn decide(local: Option<&str>, server: Option<&str>, rec: Option<&StateRecord>) -> StateAction {
    use StateAction::*;
    match (local, server, rec) {
        (None, None, _) => Nothing,
        (Some(_), None, _) => Upload,
        (None, Some(_), _) => Download,
        (Some(_), Some(_), None) => Both,
        (Some(l), Some(s), Some(r)) => {
            let lc = r.local.as_deref() != Some(l);
            let sc = r.server.as_deref() != Some(s);
            match (lc, sc) {
                (false, false) => InSync,
                (true, false) => Upload,
                (false, true) => Download,
                (true, true) => Both,
            }
        }
    }
}

/// What a sync did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub uploaded: usize,
    pub downloaded: usize,
    /// New records by file name, to store.
    pub records: Vec<(String, StateRecord)>,
}

fn md5_file(p: &Path) -> std::io::Result<String> {
    Ok(hex::encode(Md5::digest(std::fs::read(p)?)))
}

/// The game's state files on the Deck: name → md5.
pub fn local_states(set: &StateSet) -> std::io::Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(&set.dir) else {
        return Ok(out);
    };
    for e in rd {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if set.pattern.matches(&name) && e.file_type()?.is_file() {
            out.insert(name, md5_file(&e.path())?);
        }
    }
    Ok(out)
}

/// Names of the game's state files that changed since their last sync (no network).
pub fn changed_locally(
    set: &StateSet,
    records: &HashMap<String, StateRecord>,
) -> std::io::Result<Vec<String>> {
    let mut v: Vec<String> = local_states(set)?
        .into_iter()
        .filter(|(n, h)| records.get(n).and_then(|r| r.local.as_deref()) != Some(h.as_str()))
        .map(|(n, _)| n)
        .collect();
    v.sort();
    Ok(v)
}

/// Newest compatible state on RomM per file name.
fn server_states(list: Vec<ServerState>, set: &StateSet) -> HashMap<String, ServerState> {
    let mut out: HashMap<String, ServerState> = HashMap::new();
    for s in list {
        if s.emulator.as_deref() != Some(set.emulator.as_str()) {
            continue;
        }
        let name = s.base_name();
        if !set.pattern.matches(&name) {
            continue;
        }
        let newer = out
            .get(&name)
            .is_none_or(|o| (s.updated_at.as_str(), s.id) > (o.updated_at.as_str(), o.id));
        if newer {
            out.insert(name, s);
        }
    }
    out
}

fn version(s: &ServerState) -> String {
    format!("{}@{}", s.id, s.updated_at)
}

fn now() -> Option<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).ok()
}

/// RomM's time ("2026-11-27T10:00:00" or with an offset) as unix seconds.
fn server_time(t: &str) -> Option<i64> {
    OffsetDateTime::parse(t, &Rfc3339)
        .or_else(|_| OffsetDateTime::parse(&format!("{t}Z"), &Rfc3339))
        .ok()
        .map(|t| t.unix_timestamp())
}

fn local_time(p: &Path) -> Option<i64> {
    let t = std::fs::metadata(p).and_then(|m| m.modified()).ok()?;
    Some(OffsetDateTime::from(t).unix_timestamp())
}

/// Copy a file into `<backups>/states/<rom>/<time>-<reason>/`.
fn backup_file(ctx: &Ctx<'_>, rom_id: i64, reason: &str, name: &str, bytes: &[u8]) -> Result<()> {
    let dir = ctx
        .backups
        .join("states")
        .join(rom_id.to_string())
        .join(format!("{}-{reason}", stamp()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), bytes)?;
    Ok(())
}

/// Sync the game's states. Before playing (`before`), RomM's newer states come down;
/// after, only the Deck's changes go up (a newer RomM state waits for the next start).
pub async fn sync(
    ctx: &Ctx<'_>,
    rom_id: i64,
    set: &StateSet,
    records: &HashMap<String, StateRecord>,
    before: bool,
) -> Result<Summary> {
    let local = local_states(set)?;
    if !before
        && local
            .iter()
            .all(|(n, h)| records.get(n).and_then(|r| r.local.as_deref()) == Some(h.as_str()))
    {
        return Ok(Summary::default());
    }
    let server = server_states(ctx.client.states(rom_id).await?, set);
    let mut names: Vec<&String> = local.keys().chain(server.keys()).collect();
    names.sort();
    names.dedup();
    let mut sum = Summary::default();
    for name in names {
        let path = set.dir.join(name);
        let l = local.get(name).map(String::as_str);
        let s = server.get(name);
        let ver = s.map(version);
        let mut action = decide(l, ver.as_deref(), records.get(name));
        let mut server_bytes = None;
        if action == StateAction::Both {
            let srv = s.ok_or(Error::Inconsistent)?;
            let bytes = ctx.client.download_state(srv.id).await?;
            action = if Some(hex::encode(Md5::digest(&bytes)).as_str()) == l {
                StateAction::InSync
            } else if server_time(&srv.updated_at) > local_time(&path) {
                StateAction::Download
            } else {
                StateAction::Upload
            };
            server_bytes = Some(bytes);
        }
        if action == StateAction::Download && !before {
            continue;
        }
        let rec = match action {
            StateAction::Nothing => continue,
            StateAction::InSync => StateRecord {
                local: l.map(str::to_string),
                server: ver,
                synced_at: now(),
            },
            StateAction::Upload => {
                let bytes = std::fs::read(&path)?;
                if let Some(old) = &server_bytes {
                    // RomM keeps it too, but keep the replaced version where restores are.
                    backup_file(ctx, rom_id, "romm-version", name, old)?;
                }
                let up = ctx
                    .client
                    .upload_state(rom_id, &set.emulator, name, bytes)
                    .await?;
                tracing::info!("uploaded state {name} for rom {rom_id}");
                sum.uploaded += 1;
                StateRecord {
                    local: l.map(str::to_string),
                    server: Some(version(&up)),
                    synced_at: now(),
                }
            }
            StateAction::Download => {
                let srv = s.ok_or(Error::Inconsistent)?;
                let bytes = match server_bytes {
                    Some(b) => b,
                    None => ctx.client.download_state(srv.id).await?,
                };
                if path.is_file() {
                    backup_file(ctx, rom_id, "before-download", name, &std::fs::read(&path)?)?;
                }
                std::fs::create_dir_all(&set.dir)?;
                let tmp = set.dir.join(format!("{name}.romulus-tmp"));
                std::fs::write(&tmp, &bytes)?;
                std::fs::rename(&tmp, &path)?;
                tracing::info!("restored state {name} for rom {rom_id} from RomM");
                sum.downloaded += 1;
                StateRecord {
                    local: Some(hex::encode(Md5::digest(&bytes))),
                    server: ver,
                    synced_at: now(),
                }
            }
            StateAction::Both => unreachable!("resolved above"),
        };
        sum.records.push((name.clone(), rec));
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use StateAction::*;

    /// (name, Deck md5, RomM version, last sync, expected)
    type Case = (
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        Option<StateRecord>,
        StateAction,
    );

    #[test]
    fn every_case() {
        let rec = |l: &str, s: &str| StateRecord {
            local: Some(l.into()),
            server: Some(s.into()),
            synced_at: None,
        };
        let r = rec("a", "1@t");
        #[rustfmt::skip]
        let cases: Vec<Case> = vec![
            ("nothing anywhere",            None,      None,        None,            Nothing),
            ("only on the Deck",            Some("a"), None,        None,            Upload),
            ("only on RomM",                None,      Some("1@t"), None,            Download),
            ("deleted on the Deck",         None,      Some("1@t"), Some(r.clone()), Download),
            ("both, never synced",          Some("a"), Some("1@t"), None,            Both),
            ("nothing changed",             Some("a"), Some("1@t"), Some(r.clone()), InSync),
            ("saved on the Deck",           Some("b"), Some("1@t"), Some(r.clone()), Upload),
            ("saved on another device",     Some("a"), Some("2@u"), Some(r.clone()), Download),
            ("saved on both",               Some("b"), Some("2@u"), Some(r.clone()), Both),
        ];
        for (name, l, s, rec, want) in cases {
            assert_eq!(decide(l, s, rec.as_ref()), want, "{name}");
        }
    }

    #[test]
    fn patterns() {
        let ra = Pattern::RetroArch {
            stem: "Alundra (USA)".into(),
        };
        for n in [
            "Alundra (USA).state",
            "Alundra (USA).state3",
            "Alundra (USA).state.auto",
        ] {
            assert!(ra.matches(n), "{n}");
        }
        for n in [
            "Alundra (USA).state.png",
            "Alundra (USA) (Rev 1).state",
            "Alundra (USA).srm",
        ] {
            assert!(!ra.matches(n), "{n}");
        }
        let p2 = Pattern::Affix {
            prefix: "SLUS-21008 (".into(),
            suffix: ".p2s".into(),
        };
        assert!(p2.matches("SLUS-21008 (1A2B3C4D).01.p2s"));
        assert!(!p2.matches("SLUS-21008 (1A2B3C4D).01.p2s.backup"));
        assert!(!p2.matches("SLUS-21009 (1A2B3C4D).01.p2s"));
        assert_eq!(
            ps2_serial(Some("BASLUS-21008")).as_deref(),
            Some("SLUS-21008")
        );
        assert_eq!(
            ps2_serial(Some("BESLES-50330")).as_deref(),
            Some("SLES-50330")
        );
        assert_eq!(ps2_serial(None), None);
    }

    #[test]
    fn locates_each_emulators_states() {
        let d = tempfile::tempdir().unwrap();
        let cfg = d.path().join("retroarch.cfg");
        // RetroDECK 0.10.10b's settings on the Deck.
        std::fs::write(
            &cfg,
            "savestate_directory = \"/sd/retrodeck/states\"\nsort_savestates_by_content_enable = \"true\"\nsort_savestates_enable = \"false\"\nsavestates_in_content_dir = \"false\"\n",
        )
        .unwrap();
        let ini = d.path().join("PCSX2.ini");
        std::fs::write(
            &ini,
            "[Folders]\nSaveStates = /sd/retrodeck/states/ps2/pcsx2\n",
        )
        .unwrap();
        let dirs = Dirs {
            saves: PathBuf::from("/sd/retrodeck/saves"),
            retroarch_cfg: cfg,
            pcsx2_ini: ini,
            ppsspp_memstick: PathBuf::from("/cfg/ppsspp"),
            duckstation: PathBuf::from("/cfg/duckstation"),
            duckstation_gamedb: PathBuf::from("/x"),
        };
        let swan = Emulator::RetroArch {
            core: "swanstation".into(),
        };
        let mgs = Path::new("/sd/retrodeck/roms/psx/MGS (USA)/MGS (USA).m3u");
        let s = locate(&swan, mgs, &dirs, None).unwrap();
        assert_eq!(s.dir, PathBuf::from("/sd/retrodeck/states/MGS (USA)"));
        assert_eq!(s.emulator, "retroarch-swanstation");
        let kd = Path::new("/sd/retrodeck/roms/ps2/Katamari.chd");
        let s = locate(&Emulator::Pcsx2, kd, &dirs, Some("BASLUS-21008")).unwrap();
        assert_eq!(s.dir, PathBuf::from("/sd/retrodeck/states/ps2/pcsx2"));
        assert!(s.pattern.matches("SLUS-21008 (ABCD1234).01.p2s"));
        let s = locate(&Emulator::Ppsspp, kd, &dirs, Some("ULUS10567")).unwrap();
        assert_eq!(s.dir, PathBuf::from("/cfg/ppsspp/PSP/PPSSPP_STATE"));
        assert!(s.pattern.matches("ULUS10567_1.00_0.ppst"));
        let s = locate(&Emulator::DuckStation, mgs, &dirs, Some("SLUS-00594")).unwrap();
        assert_eq!(s.dir, PathBuf::from("/cfg/duckstation/savestates"));
        assert!(s.pattern.matches("SLUS-00594_resume.sav"));
        assert!(locate(&Emulator::DuckStation, mgs, &dirs, None).is_err());
    }
}

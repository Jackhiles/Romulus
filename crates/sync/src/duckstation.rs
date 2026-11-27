//! DuckStation's per-game memory cards: which file a game uses.
//!
//! With `Card1Type = PerGameTitle` (RetroDECK's setting) DuckStation names the card
//! after the game's title in its game database (`gamedb.yaml`), as
//! `<title>_1.mcd`. For a multi-disc game it uses the disc set's name instead, unless a
//! card for the disc's own title already exists (System.cpp, GetGameMemoryCardPath).
//! Titles are made file-name safe the way DuckStation's Path::SanitizeFileName does on
//! Linux.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// A game's entry in DuckStation's database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbEntry {
    pub title: String,
    pub disc_set: Option<String>,
}

/// Undo YAML double-quote escapes (only `\"` and `\\` occur in names).
fn unquote(v: &str) -> String {
    let v = v.trim();
    let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return v.trim_matches('\'').to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Look `serial` up in the text of `gamedb.yaml`: a top-level `SERIAL:` key with a
/// two-space indented `name:` and an optional `discSet:` block with its own `name:`.
pub fn lookup(db: &str, serial: &str) -> Option<DbEntry> {
    let key = format!("{serial}:");
    let mut lines = db.lines();
    lines.by_ref().find(|l| l.trim_end() == key)?;
    let mut title = None;
    let mut disc_set = None;
    let mut in_set = false;
    for l in lines {
        if !l.starts_with(' ') {
            break; // the next entry
        }
        if let Some(v) = l.strip_prefix("  name:") {
            title = Some(unquote(v));
        } else if l.starts_with("  ") && !l.starts_with("   ") {
            in_set = l.trim_end() == "  discSet:";
        } else if in_set && let Some(v) = l.strip_prefix("    name:") {
            disc_set = Some(unquote(v));
        }
    }
    Some(DbEntry {
        title: title?,
        disc_set,
    })
}

/// DuckStation's Path::SanitizeFileName on Linux: `/` and `*` become `_`.
pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c == '/' || c == '*' { '_' } else { c })
        .collect()
}

/// The database is 4 MB; read it once per path.
fn cached_lookup(db_path: &Path, serial: &str) -> Option<DbEntry> {
    static DBS: OnceLock<Mutex<HashMap<PathBuf, Option<String>>>> = OnceLock::new();
    let mut dbs = DBS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let text = dbs
        .entry(db_path.to_path_buf())
        .or_insert_with(|| std::fs::read_to_string(db_path).ok());
    lookup(text.as_deref()?, serial)
}

/// DuckStation's memory card settings (`settings.ini`, `[MemoryCards]`).
#[derive(Debug, Clone, PartialEq)]
pub struct CardSettings {
    pub card_type: String,
    pub directory: PathBuf,
    pub use_playlist_title: bool,
}

/// Read them, with DuckStation's defaults for anything missing.
pub fn card_settings(ini: &str, default_dir: &Path) -> CardSettings {
    let get = |k: &str| crate::layout::ini_value(ini, "MemoryCards", k);
    CardSettings {
        card_type: get("Card1Type").unwrap_or_else(|| "PerGameTitle".into()),
        directory: get("Directory")
            .filter(|d| Path::new(d).is_absolute())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_dir.to_path_buf()),
        use_playlist_title: get("UsePlaylistTitle").is_none_or(|v| v != "false"),
    }
}

/// The card file DuckStation uses for slot 1 of the game started from `launch` with
/// disc serial `serial`. Err says why it can't be told.
pub fn card_path(
    s: &CardSettings,
    gamedb: &Path,
    launch: &Path,
    serial: Option<&str>,
) -> Result<PathBuf, String> {
    let card = |name: &str| s.directory.join(format!("{}_1.mcd", sanitize(name)));
    let file_title = || {
        launch
            .file_stem()
            .map(|f| f.to_string_lossy().into_owned())
            .ok_or_else(|| "game has no file name".to_string())
    };
    match s.card_type.as_str() {
        "PerGame" => serial
            .map(card)
            .ok_or_else(|| "RomM doesn't know this game's PS1 serial".into()),
        "PerGameFileTitle" => Ok(card(&file_title()?)),
        "PerGameTitle" => {
            let entry = serial.and_then(|s| cached_lookup(gamedb, s));
            let Some(e) = entry else {
                return Ok(card(&file_title()?));
            };
            let own = card(&e.title);
            match &e.disc_set {
                Some(set) if s.use_playlist_title && !own.exists() => Ok(card(set)),
                _ => Ok(own),
            }
        }
        other => Err(format!(
            "DuckStation uses a {other} memory card, not one per game"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from the gamedb.yaml in RetroDECK's DuckStation (2026-11).
    const DB: &str = r#"SLUS-00553:
  name: "Alundra (USA)"
  controllers:
    - DigitalController
  metadata:
    publisher: "Working Designs"
SLUS-00594:
  name: "Metal Gear Solid (USA) (Disc 1)"
  discSet:
    name: "Metal Gear Solid (USA)"
    serials:
      - SLUS-00594
      - SLUS-00776
  compatibility:
    rating: NoIssues
SLPM-86271:
  name: "Apocalypse (aka Apocalypse - Starring Bruce Willis / Bruce Willis' Apocalypse)"
SLUS-99999:
  name: "Quote \"Test\""
"#;

    #[test]
    fn reads_titles_and_disc_sets() {
        assert_eq!(
            lookup(DB, "SLUS-00553"),
            Some(DbEntry {
                title: "Alundra (USA)".into(),
                disc_set: None
            })
        );
        assert_eq!(
            lookup(DB, "SLUS-00594").unwrap().disc_set.as_deref(),
            Some("Metal Gear Solid (USA)")
        );
        assert_eq!(lookup(DB, "SLUS-99999").unwrap().title, "Quote \"Test\"");
        assert_eq!(lookup(DB, "SLUS-00776"), None);
        assert_eq!(
            sanitize(&lookup(DB, "SLPM-86271").unwrap().title),
            "Apocalypse (aka Apocalypse - Starring Bruce Willis _ Bruce Willis' Apocalypse)"
        );
    }

    #[test]
    fn picks_the_card_like_duckstation() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("gamedb.yaml");
        std::fs::write(&db, DB).unwrap();
        let cards = d.path().join("memcards");
        std::fs::create_dir_all(&cards).unwrap();
        // RetroDECK's settings on the Deck.
        let ini = format!(
            "[MemoryCards]\nCard1Type = PerGameTitle\nUsePlaylistTitle = true\nDirectory = {}\n",
            cards.display()
        );
        let s = card_settings(&ini, Path::new("/unused"));
        assert_eq!(s.directory, cards);
        let alundra = Path::new("/roms/psx/Alundra (USA) (Rev 1).m3u");
        assert_eq!(
            card_path(&s, &db, alundra, Some("SLUS-00553")).unwrap(),
            cards.join("Alundra (USA)_1.mcd")
        );
        // Multi-disc: the disc set's card, unless the disc's own card exists.
        let mgs = Path::new("/roms/psx/MGS/MGS.m3u");
        assert_eq!(
            card_path(&s, &db, mgs, Some("SLUS-00594")).unwrap(),
            cards.join("Metal Gear Solid (USA)_1.mcd")
        );
        std::fs::write(cards.join("Metal Gear Solid (USA) (Disc 1)_1.mcd"), b"x").unwrap();
        assert_eq!(
            card_path(&s, &db, mgs, Some("SLUS-00594")).unwrap(),
            cards.join("Metal Gear Solid (USA) (Disc 1)_1.mcd")
        );
        // Not in the database: the file's title.
        assert_eq!(
            card_path(&s, &db, Path::new("/roms/psx/Homebrew.chd"), None).unwrap(),
            cards.join("Homebrew_1.mcd")
        );
        let serial = CardSettings {
            card_type: "PerGame".into(),
            ..s.clone()
        };
        assert_eq!(
            card_path(&serial, &db, alundra, Some("SLUS-00553")).unwrap(),
            cards.join("SLUS-00553_1.mcd")
        );
        let shared = CardSettings {
            card_type: "Shared".into(),
            ..s
        };
        assert!(card_path(&shared, &db, alundra, Some("SLUS-00553")).is_err());
        // Defaults when the section is missing.
        let def = card_settings("", Path::new("/saves/psx/duckstation/memcards"));
        assert_eq!(def.card_type, "PerGameTitle");
        assert!(def.use_playlist_title);
    }
}

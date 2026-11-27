use crate::Result;
use romm_api::{Platform, Rom};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Schema migrations, applied in order. `PRAGMA user_version` holds how many have run.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE platforms (
    id          INTEGER PRIMARY KEY,
    slug        TEXT NOT NULL,
    fs_slug     TEXT NOT NULL,
    name        TEXT NOT NULL,
    rom_count   INTEGER NOT NULL DEFAULT 0,
    logo        TEXT,
    updated_at  TEXT,
    json        TEXT NOT NULL
);

CREATE TABLE roms (
    id           INTEGER PRIMARY KEY,
    platform_id  INTEGER NOT NULL,
    title        TEXT NOT NULL,
    sort_key     TEXT NOT NULL,
    fs_name      TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL DEFAULT 0,
    cover_small  TEXT,
    cover_large  TEXT,
    ra_id        INTEGER,
    updated_at   TEXT,
    json         TEXT NOT NULL
);
CREATE INDEX roms_platform_sort ON roms (platform_id, sort_key);

CREATE VIRTUAL TABLE roms_fts USING fts5 (title, alt_names, fs_name, content='', tokenize='unicode61 remove_diacritics 2');
"#,
    r#"
-- Games downloaded onto this device. `data` is the launcher's install record (JSON).
CREATE TABLE installs (
    rom_id       INTEGER PRIMARY KEY,
    data         TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    installed_at INTEGER NOT NULL
);

-- Finished play sessions not yet accepted by RomM.
CREATE TABLE play_sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    rom_id      INTEGER NOT NULL,
    start_time  TEXT NOT NULL,
    end_time    TEXT NOT NULL,
    duration_ms INTEGER NOT NULL
);
"#,
    r#"
-- Each game's save as it was when last in sync with RomM (the sync crate's Record, JSON).
CREATE TABLE save_sync (
    rom_id INTEGER PRIMARY KEY,
    data   TEXT NOT NULL
);
"#,
    r#"
-- When each game was last started on this device (unix seconds).
CREATE TABLE played (
    rom_id      INTEGER PRIMARY KEY,
    last_played INTEGER NOT NULL
);
"#,
    r#"
-- Downloads waiting or running, so they continue after a restart.
CREATE TABLE download_queue (
    rom_id INTEGER PRIMARY KEY,
    seq    INTEGER NOT NULL
);
"#,
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlatformRow {
    pub id: i64,
    pub slug: String,
    pub fs_slug: String,
    pub name: String,
    /// ROMs in the local cache for this platform.
    pub rom_count: i64,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RomRow {
    pub id: i64,
    pub platform_id: i64,
    pub title: String,
    pub fs_name: String,
    pub size_bytes: i64,
    pub cover_small: Option<String>,
    pub cover_large: Option<String>,
    pub updated_at: Option<String>,
}

/// A play session waiting to be reported to RomM (times are RFC 3339).
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedSession {
    pub rom_id: i64,
    pub start_time: String,
    pub end_time: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Title,
    Newest,
}

pub struct Cache {
    conn: Connection,
}

/// "The Legend of Zelda" sorts under L, like RomM's own sort key.
fn sort_key(rom: &Rom) -> String {
    if let Some(k) = rom.name_sort_key.as_deref().filter(|k| !k.is_empty()) {
        return k.to_lowercase();
    }
    let t = rom.title().to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = t.strip_prefix(article) {
            return rest.to_string();
        }
    }
    t
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        let version: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tx = conn.transaction()?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i + 1)?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    // ---- meta --------------------------------------------------------------------------

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_meta(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM meta WHERE key = ?1", [key])?;
        Ok(())
    }

    /// Ids of all cached ROMs.
    pub fn rom_ids(&self) -> Result<Vec<i64>> {
        self.ids("SELECT id FROM roms")
    }

    // ---- writes ------------------------------------------------------------------------

    pub fn upsert_platforms(&mut self, platforms: &[Platform]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO platforms (id, slug, fs_slug, name, rom_count, logo, updated_at, json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET slug = excluded.slug, fs_slug = excluded.fs_slug,
                   name = excluded.name, rom_count = excluded.rom_count, logo = excluded.logo,
                   updated_at = excluded.updated_at, json = excluded.json",
            )?;
            for p in platforms {
                st.execute(params![
                    p.id,
                    p.slug,
                    p.fs_slug,
                    p.label(),
                    p.rom_count,
                    p.url_logo,
                    p.updated_at,
                    serde_json::to_string(p)?
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_roms(&mut self, roms: &[Rom]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            // Contentless FTS rows are replaced by deleting with the old values.
            let mut old = tx.prepare_cached("SELECT title, json FROM roms WHERE id = ?1")?;
            let mut fts_del = tx.prepare_cached(
                "INSERT INTO roms_fts (roms_fts, rowid, title, alt_names, fs_name) VALUES ('delete', ?1, ?2, ?3, ?4)",
            )?;
            let mut fts_add = tx.prepare_cached(
                "INSERT INTO roms_fts (rowid, title, alt_names, fs_name) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut st = tx.prepare_cached(
                "INSERT INTO roms (id, platform_id, title, sort_key, fs_name, size_bytes, cover_small, cover_large, ra_id, updated_at, json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET platform_id = excluded.platform_id, title = excluded.title,
                   sort_key = excluded.sort_key, fs_name = excluded.fs_name, size_bytes = excluded.size_bytes,
                   cover_small = excluded.cover_small, cover_large = excluded.cover_large, ra_id = excluded.ra_id,
                   updated_at = excluded.updated_at, json = excluded.json",
            )?;
            for r in roms {
                let previous: Option<(String, String)> = old
                    .query_row([r.id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .optional()?;
                if let Some((title, json)) = previous {
                    let prev: Rom = serde_json::from_str(&json)?;
                    fts_del.execute(params![
                        r.id,
                        title,
                        prev.alternative_names.join(" "),
                        prev.fs_name
                    ])?;
                }
                st.execute(params![
                    r.id,
                    r.platform_id,
                    r.title(),
                    sort_key(r),
                    r.fs_name,
                    r.fs_size_bytes,
                    r.path_cover_small.as_deref().filter(|s| !s.is_empty()),
                    r.path_cover_large.as_deref().filter(|s| !s.is_empty()),
                    r.ra_id,
                    r.updated_at,
                    serde_json::to_string(r)?
                ])?;
                fts_add.execute(params![
                    r.id,
                    r.title(),
                    r.alternative_names.join(" "),
                    r.fs_name
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove platforms (and their ROMs) the server no longer lists. Returns how many
    /// were removed.
    pub fn retain_platforms(&mut self, keep: &[i64]) -> Result<usize> {
        let keep: HashSet<i64> = keep.iter().copied().collect();
        let gone: Vec<i64> = self
            .ids("SELECT id FROM platforms")?
            .into_iter()
            .filter(|id| !keep.contains(id))
            .collect();
        if gone.is_empty() {
            return Ok(0);
        }
        let rom_ids: Vec<i64> = {
            let mut st = self
                .conn
                .prepare("SELECT id FROM roms WHERE platform_id = ?1")?;
            let mut out = Vec::new();
            for p in &gone {
                out.extend(
                    st.query_map([p], |r| r.get::<_, i64>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                );
            }
            out
        };
        self.delete_roms(&rom_ids)?;
        let tx = self.conn.transaction()?;
        for p in &gone {
            tx.execute("DELETE FROM platforms WHERE id = ?1", [p])?;
        }
        tx.commit()?;
        Ok(gone.len())
    }

    /// Remove ROMs the server no longer lists. Returns how many were removed.
    pub fn retain_roms(&mut self, keep: &[i64]) -> Result<usize> {
        let keep: HashSet<i64> = keep.iter().copied().collect();
        let gone: Vec<i64> = self
            .ids("SELECT id FROM roms")?
            .into_iter()
            .filter(|id| !keep.contains(id))
            .collect();
        self.delete_roms(&gone)?;
        Ok(gone.len())
    }

    fn delete_roms(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut old = tx.prepare_cached("SELECT title, json FROM roms WHERE id = ?1")?;
            let mut fts_del = tx.prepare_cached(
                "INSERT INTO roms_fts (roms_fts, rowid, title, alt_names, fs_name) VALUES ('delete', ?1, ?2, ?3, ?4)",
            )?;
            let mut del = tx.prepare_cached("DELETE FROM roms WHERE id = ?1")?;
            for id in ids {
                if let Some((title, json)) = old
                    .query_row([id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .optional()?
                {
                    let prev: Rom = serde_json::from_str(&json)?;
                    fts_del.execute(params![
                        id,
                        title,
                        prev.alternative_names.join(" "),
                        prev.fs_name
                    ])?;
                }
                del.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn ids(&self, sql: &str) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare(sql)?;
        let ids = st
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    // ---- reads -------------------------------------------------------------------------

    /// Platforms that have at least one cached ROM, by name.
    pub fn platforms(&self) -> Result<Vec<PlatformRow>> {
        let mut st = self.conn.prepare_cached(
            "SELECT p.id, p.slug, p.fs_slug, p.name, COUNT(r.id), p.logo
             FROM platforms p JOIN roms r ON r.platform_id = p.id
             GROUP BY p.id ORDER BY p.name COLLATE NOCASE",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(PlatformRow {
                    id: r.get(0)?,
                    slug: r.get(1)?,
                    fs_slug: r.get(2)?,
                    name: r.get(3)?,
                    rom_count: r.get(4)?,
                    logo: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every cached platform, including ones without cached ROMs.
    pub fn platforms_all(&self) -> Result<Vec<PlatformRow>> {
        let mut st = self.conn.prepare_cached(
            "SELECT p.id, p.slug, p.fs_slug, p.name, (SELECT COUNT(*) FROM roms r WHERE r.platform_id = p.id), p.logo
             FROM platforms p ORDER BY p.name COLLATE NOCASE",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(PlatformRow {
                    id: r.get(0)?,
                    slug: r.get(1)?,
                    fs_slug: r.get(2)?,
                    name: r.get(3)?,
                    rom_count: r.get(4)?,
                    logo: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn roms(&self, platform_id: i64, order: SortOrder) -> Result<Vec<RomRow>> {
        let sql = match order {
            SortOrder::Title => {
                format!("{ROM_SELECT} WHERE platform_id = ?1 ORDER BY sort_key, id")
            }
            SortOrder::Newest => format!("{ROM_SELECT} WHERE platform_id = ?1 ORDER BY id DESC"),
        };
        let mut st = self.conn.prepare_cached(&sql)?;
        let rows = st
            .query_map([platform_id], rom_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn rom(&self, id: i64) -> Result<Option<(RomRow, serde_json::Value)>> {
        let mut st = self.conn.prepare_cached(
            "SELECT roms.id, platform_id, title, fs_name, size_bytes, cover_small, cover_large, updated_at, json
             FROM roms WHERE id = ?1",
        )?;
        let row = st
            .query_row([id], |r| Ok((rom_row(r)?, r.get::<_, String>(8)?)))
            .optional()?;
        match row {
            Some((row, json)) => Ok(Some((row, serde_json::from_str(&json)?))),
            None => Ok(None),
        }
    }

    /// Full-text search over titles, alternative names and file names. Each word matches
    /// as a prefix ("zel oca" finds "The Legend of Zelda: Ocarina of Time").
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<RomRow>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{w}\"*"))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "{ROM_SELECT} JOIN roms_fts ON roms_fts.rowid = roms.id WHERE roms_fts MATCH ?1 ORDER BY roms_fts.rank LIMIT ?2"
        );
        let mut st = self.conn.prepare_cached(&sql)?;
        let rows = st
            .query_map(params![terms.join(" "), limit as i64], rom_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Up to `n` small cover paths per platform (most recently updated games first).
    pub fn platform_covers(&self, n: usize) -> Result<HashMap<i64, Vec<String>>> {
        let mut st = self.conn.prepare_cached(
            "SELECT platform_id, cover_small FROM (
               SELECT platform_id, cover_small,
                      ROW_NUMBER() OVER (PARTITION BY platform_id ORDER BY updated_at DESC, id DESC) AS rn
               FROM roms WHERE cover_small IS NOT NULL)
             WHERE rn <= ?1",
        )?;
        let mut out: HashMap<i64, Vec<String>> = HashMap::new();
        let rows = st.query_map([n as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (p, c) = row?;
            out.entry(p).or_default().push(c);
        }
        Ok(out)
    }

    // ---- installs and play sessions (device state) --------------------------------------

    pub fn set_install(&self, rom_id: i64, data: &str, size_bytes: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO installs (rom_id, data, size_bytes, installed_at) VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT(rom_id) DO UPDATE SET data = excluded.data, size_bytes = excluded.size_bytes,
               installed_at = excluded.installed_at",
            params![rom_id, data, size_bytes as i64],
        )?;
        Ok(())
    }

    pub fn install(&self, rom_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM installs WHERE rom_id = ?1",
                [rom_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn installed_ids(&self) -> Result<Vec<i64>> {
        self.ids("SELECT rom_id FROM installs")
    }

    pub fn remove_install(&self, rom_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM installs WHERE rom_id = ?1", [rom_id])?;
        Ok(())
    }

    pub fn save_record(&self, rom_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM save_sync WHERE rom_id = ?1",
                [rom_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_save_record(&self, rom_id: i64, data: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO save_sync (rom_id, data) VALUES (?1, ?2) ON CONFLICT(rom_id) DO UPDATE SET data = excluded.data",
            params![rom_id, data],
        )?;
        Ok(())
    }

    pub fn queue_play_session(&self, s: &QueuedSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO play_sessions (rom_id, start_time, end_time, duration_ms) VALUES (?1, ?2, ?3, ?4)",
            params![s.rom_id, s.start_time, s.end_time, s.duration_ms as i64],
        )?;
        Ok(())
    }

    /// Queued sessions, oldest first, with their queue ids.
    pub fn queued_play_sessions(&self) -> Result<Vec<(i64, QueuedSession)>> {
        let mut st = self.conn.prepare_cached(
            "SELECT id, rom_id, start_time, end_time, duration_ms FROM play_sessions ORDER BY id",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    QueuedSession {
                        rom_id: r.get(1)?,
                        start_time: r.get(2)?,
                        end_time: r.get(3)?,
                        duration_ms: r.get::<_, i64>(4)? as u64,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_play_sessions(&self, ids: &[i64]) -> Result<()> {
        let mut st = self
            .conn
            .prepare_cached("DELETE FROM play_sessions WHERE id = ?1")?;
        for id in ids {
            st.execute([id])?;
        }
        Ok(())
    }

    pub fn queue_download(&self, rom_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO download_queue (rom_id, seq)
             VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM download_queue))",
            [rom_id],
        )?;
        Ok(())
    }

    pub fn unqueue_download(&self, rom_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM download_queue WHERE rom_id = ?1", [rom_id])?;
        Ok(())
    }

    /// Queued downloads, oldest first.
    pub fn queued_downloads(&self) -> Result<Vec<i64>> {
        self.ids("SELECT rom_id FROM download_queue ORDER BY seq")
    }

    pub fn mark_played_now(&self, rom_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO played (rom_id, last_played) VALUES (?1, unixepoch())
             ON CONFLICT(rom_id) DO UPDATE SET last_played = excluded.last_played",
            [rom_id],
        )?;
        Ok(())
    }

    /// Recently played games, newest first: played here, or (per RomM) on any device.
    pub fn recently_played(&self, limit: usize) -> Result<Vec<RomRow>> {
        let sql = format!(
            "{ROM_SELECT}
             LEFT JOIN played ON played.rom_id = roms.id
             WHERE played.last_played IS NOT NULL OR json_extract(roms.json, '$.rom_user.last_played') IS NOT NULL
             ORDER BY MAX(COALESCE(played.last_played, 0),
                          COALESCE(unixepoch(json_extract(roms.json, '$.rom_user.last_played')), 0)) DESC
             LIMIT ?1"
        );
        let mut st = self.conn.prepare_cached(&sql)?;
        let rows = st
            .query_map([limit as i64], rom_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Games downloaded to this device, by title.
    pub fn installed_roms(&self) -> Result<Vec<RomRow>> {
        let sql = format!(
            "{ROM_SELECT} JOIN installs ON installs.rom_id = roms.id ORDER BY roms.sort_key, roms.id"
        );
        let mut st = self.conn.prepare_cached(&sql)?;
        let rows = st
            .query_map([], rom_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn rom_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM roms", [], |r| r.get(0))?)
    }
}

const ROM_SELECT: &str = "SELECT roms.id, roms.platform_id, roms.title, roms.fs_name, roms.size_bytes, roms.cover_small, roms.cover_large, roms.updated_at FROM roms";

fn rom_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RomRow> {
    Ok(RomRow {
        id: r.get(0)?,
        platform_id: r.get(1)?,
        title: r.get(2)?,
        fs_name: r.get(3)?,
        size_bytes: r.get(4)?,
        cover_small: r.get(5)?,
        cover_large: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub fn rom(id: i64, platform_id: i64, name: &str) -> Rom {
        serde_json::from_value(json!({
            "id": id, "platform_id": platform_id, "name": name, "fs_name": format!("{name}.bin"),
            "alternative_names": [], "updated_at": "2026-11-20T10:00:00"
        }))
        .unwrap()
    }

    pub fn platform(id: i64, name: &str) -> Platform {
        serde_json::from_value(
            json!({"id": id, "slug": name, "fs_slug": name, "name": name, "rom_count": 0}),
        )
        .unwrap()
    }

    #[test]
    fn upsert_and_read() {
        let mut c = Cache::open_in_memory().unwrap();
        c.upsert_platforms(&[platform(1, "snes"), platform(2, "gba")])
            .unwrap();
        c.upsert_roms(&[
            rom(10, 1, "The Legend of Zelda"),
            rom(11, 1, "Aladdin"),
            rom(12, 1, "Contra"),
        ])
        .unwrap();
        let titles: Vec<_> = c
            .roms(1, SortOrder::Title)
            .unwrap()
            .into_iter()
            .map(|r| r.title)
            .collect();
        assert_eq!(titles, ["Aladdin", "Contra", "The Legend of Zelda"]);
        // gba has no ROMs, so it is hidden.
        assert_eq!(c.platforms().unwrap().len(), 1);
        assert_eq!(c.platforms().unwrap()[0].rom_count, 3);

        // Update in place.
        c.upsert_roms(&[rom(11, 1, "Aladdin (Rev 1)")]).unwrap();
        assert_eq!(c.rom(11).unwrap().unwrap().0.title, "Aladdin (Rev 1)");
        assert_eq!(c.rom_count().unwrap(), 3);
    }

    #[test]
    fn search_follows_updates_and_deletes() {
        let mut c = Cache::open_in_memory().unwrap();
        c.upsert_roms(&[rom(1, 1, "Super Metroid"), rom(2, 1, "Metroid Fusion")])
            .unwrap();
        assert_eq!(c.search("metr", 10).unwrap().len(), 2);
        assert_eq!(c.search("sup met", 10).unwrap()[0].id, 1);
        c.upsert_roms(&[rom(1, 1, "Super Mario World")]).unwrap();
        assert_eq!(c.search("metroid", 10).unwrap().len(), 1);
        assert_eq!(c.search("mario", 10).unwrap()[0].id, 1);
        c.retain_roms(&[1]).unwrap();
        assert!(c.search("fusion", 10).unwrap().is_empty());
        assert!(c.search("  ", 10).unwrap().is_empty());
        // Quotes and FTS syntax in the query are treated as plain text.
        assert!(c.search("\" OR *", 10).unwrap().is_empty());
    }

    #[test]
    fn download_queue_keeps_order() {
        let c = Cache::open_in_memory().unwrap();
        for id in [5, 3, 9, 3] {
            c.queue_download(id).unwrap();
        }
        assert_eq!(c.queued_downloads().unwrap(), [5, 3, 9]);
        c.unqueue_download(3).unwrap();
        c.queue_download(3).unwrap();
        assert_eq!(c.queued_downloads().unwrap(), [5, 9, 3]);
    }

    #[test]
    fn recently_played_and_installed() {
        let mut c = Cache::open_in_memory().unwrap();
        let with_user = |id: i64, name: &str, last: Option<&str>| -> Rom {
            serde_json::from_value(json!({
                "id": id, "platform_id": 1, "name": name, "fs_name": "g.bin",
                "rom_user": {"last_played": last}
            }))
            .unwrap()
        };
        c.upsert_roms(&[
            with_user(1, "Old on RomM", Some("2020-01-01T00:00:00")),
            with_user(2, "Never", None),
            with_user(3, "Played here", None),
        ])
        .unwrap();
        c.mark_played_now(3).unwrap();
        let ids: Vec<i64> = c
            .recently_played(10)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, [3, 1]);

        c.set_install(2, "{}", 1).unwrap();
        assert_eq!(c.installed_roms().unwrap()[0].id, 2);
    }

    #[test]
    fn installs_and_play_sessions() {
        let c = Cache::open_in_memory().unwrap();
        c.set_install(5, r#"{"launch":"/x"}"#, 100).unwrap();
        c.set_install(5, r#"{"launch":"/y"}"#, 200).unwrap();
        assert_eq!(c.install(5).unwrap().as_deref(), Some(r#"{"launch":"/y"}"#));
        assert_eq!(c.installed_ids().unwrap(), [5]);
        c.remove_install(5).unwrap();
        assert!(c.install(5).unwrap().is_none());

        assert!(c.save_record(5).unwrap().is_none());
        c.set_save_record(5, "{}").unwrap();
        c.set_save_record(5, r#"{"server_id":3}"#).unwrap();
        assert_eq!(
            c.save_record(5).unwrap().as_deref(),
            Some(r#"{"server_id":3}"#)
        );

        let s = QueuedSession {
            rom_id: 5,
            start_time: "2026-11-27T10:00:00Z".into(),
            end_time: "2026-11-27T10:30:00Z".into(),
            duration_ms: 1_800_000,
        };
        c.queue_play_session(&s).unwrap();
        c.queue_play_session(&s).unwrap();
        let q = c.queued_play_sessions().unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].1, s);
        c.delete_play_sessions(&[q[0].0]).unwrap();
        assert_eq!(c.queued_play_sessions().unwrap().len(), 1);
    }

    #[test]
    fn platform_covers_are_limited_per_platform() {
        let mut c = Cache::open_in_memory().unwrap();
        let with_cover = |id: i64, p: i64, updated: &str| -> Rom {
            serde_json::from_value(json!({
                "id": id, "platform_id": p, "name": format!("G{id}"), "fs_name": "g.bin",
                "path_cover_small": format!("/c/{id}.png"), "updated_at": updated
            }))
            .unwrap()
        };
        c.upsert_roms(&[
            with_cover(1, 1, "2026-01-01T00:00:00"),
            with_cover(2, 1, "2026-03-01T00:00:00"),
            with_cover(3, 1, "2026-02-01T00:00:00"),
            with_cover(4, 2, "2026-01-01T00:00:00"),
            rom(5, 2, "No cover"),
        ])
        .unwrap();
        let covers = c.platform_covers(2).unwrap();
        assert_eq!(covers[&1], ["/c/2.png", "/c/3.png"]);
        assert_eq!(covers[&2], ["/c/4.png"]);
    }

    #[test]
    fn retain_removes_missing() {
        let mut c = Cache::open_in_memory().unwrap();
        c.upsert_platforms(&[platform(1, "snes"), platform(2, "gba")])
            .unwrap();
        c.upsert_roms(&[rom(1, 1, "A"), rom(2, 2, "B"), rom(3, 2, "C")])
            .unwrap();
        assert_eq!(c.retain_roms(&[1, 2]).unwrap(), 1);
        assert_eq!(c.retain_platforms(&[1]).unwrap(), 1);
        assert_eq!(c.rom_count().unwrap(), 1);
        assert!(c.rom(2).unwrap().is_none());
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("lib.db");
        Cache::open(&p).unwrap().set_meta("k", "v").unwrap();
        assert_eq!(
            Cache::open(&p).unwrap().meta("k").unwrap().as_deref(),
            Some("v")
        );
    }
}

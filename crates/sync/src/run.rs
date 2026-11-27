//! Syncing one game's save around a play session: [`before`] it starts and [`after`]
//! it exits.

use crate::engine::{self, Action, Local, Record};
use crate::layout::Located;
use crate::{Error, Result, local};
use romm_api::{Client, SaveUpload, ServerSave};
use serde::Serialize;
use std::path::Path;
use time::OffsetDateTime;

/// RomM slot Romulus syncs (the same one decky-romm-sync used).
pub const SLOT: &str = "autosave";

pub struct Ctx<'a> {
    pub client: &'a Client,
    /// This Deck's RomM device id (paired sign-in); enables the server's own conflict check.
    pub device_id: Option<&'a str>,
    /// Backups go in `<backups>/<rom id>/<time>-<reason>/`.
    pub backups: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    KeepLocal,
    UseServer,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConflictInfo {
    /// When the Deck's save last changed (RFC 3339).
    pub local_modified: Option<String>,
    /// When RomM's save was uploaded (as RomM reports it).
    pub server_updated: String,
    pub server_device: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// In sync (or nothing to sync). `changed` says whether anything was transferred.
    Synced {
        record: Record,
        changed: Option<&'static str>,
    },
    /// Both sides changed; the user has to pick (before) or will be asked next time (after).
    Conflict(ConflictInfo),
}

/// A sortable timestamp for backup folder names.
pub fn stamp() -> String {
    let t = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn rfc3339(t: std::time::SystemTime) -> Option<String> {
    OffsetDateTime::from(t)
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

/// The newest save on RomM in our slot with a compatible format.
pub fn pick(saves: &[ServerSave], ext: &str) -> Option<ServerSave> {
    saves
        .iter()
        .filter(|s| s.slot.as_deref() == Some(SLOT))
        .filter(|s| {
            s.file_name
                .rsplit('.')
                .next()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        })
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at).then(a.id.cmp(&b.id)))
        .cloned()
}

fn record_of(local: Option<&Local>, server: Option<&ServerSave>) -> Record {
    Record {
        local_fp: local.map(|l| l.fp.clone()),
        server_id: server.map(|s| s.id),
        server_hash: server.and_then(|s| s.content_hash.clone()),
        synced_at: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .ok(),
    }
}

/// Has the save on the device changed since it was last in sync? (No network.)
pub fn changed_locally(game: &Located, record: Option<&Record>) -> Result<bool> {
    let now = local::read(&game.spec)?;
    Ok(match (now, record.and_then(|r| r.local_fp.as_deref())) {
        (Some(l), Some(fp)) => l.fp != fp,
        (Some(_), None) => true,
        (None, _) => false,
    })
}

fn conflict(game: &Located, server: &ServerSave) -> ConflictInfo {
    ConflictInfo {
        local_modified: local::modified(&game.spec).and_then(rfc3339),
        server_updated: server.updated_at.clone(),
        server_device: server.origin_device_id.clone(),
    }
}

async fn download(
    ctx: &Ctx<'_>,
    rom_id: i64,
    game: &Located,
    server: &ServerSave,
) -> Result<Record> {
    if let Some(b) = local::backup(
        &game.spec,
        &ctx.backups.join(rom_id.to_string()),
        &format!("{}-before-download", stamp()),
    )? {
        tracing::info!("backed up the Deck's save to {}", b.display());
    }
    let bytes = ctx.client.download_save(server.id, ctx.device_id).await?;
    local::apply(&game.spec, &bytes)?;
    let hash = local::content_hash(&bytes);
    if let Some(d) = ctx.device_id
        && let Err(e) = ctx.client.confirm_save_download(server.id, d, &hash).await
    {
        tracing::warn!("could not confirm the save download: {e}");
    }
    let now = local::read(&game.spec)?;
    tracing::info!("restored save {} from RomM", server.file_name);
    Ok(record_of(now.as_ref(), Some(server)))
}

/// Upload the Deck's save. Ok(None) means RomM refused (another device uploaded since).
async fn upload(
    ctx: &Ctx<'_>,
    rom_id: i64,
    game: &Located,
    l: &Local,
    overwrite: bool,
) -> Result<Option<Record>> {
    let Some(bytes) = local::pack(&game.spec)? else {
        return Err(Error::Io(std::io::ErrorKind::NotFound.into()));
    };
    let up = SaveUpload {
        rom_id,
        emulator: &game.emulator,
        slot: SLOT,
        device_id: ctx.device_id,
        content_hash: &l.hash,
        file_name: &game.upload_name,
        bytes,
        overwrite,
    };
    match ctx.client.upload_save(&up).await {
        Ok(s) => {
            tracing::info!("uploaded save for rom {rom_id} as {}", s.file_name);
            Ok(Some(record_of(Some(l), Some(&s))))
        }
        Err(e) if e.is_conflict() => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Before playing: bring the Deck's save up to date, or report a conflict.
pub async fn before(
    ctx: &Ctx<'_>,
    rom_id: i64,
    game: &Located,
    record: Option<&Record>,
    choice: Option<Choice>,
) -> Result<Outcome> {
    // A never-formatted PCSX2 card ignores its folders; format it as a PS2 would.
    if let (true, local::SaveSpec::CardFolders { card_dir, .. }) = (game.pcsx2_card, &game.spec) {
        let label = format!("{}-before-card-format", stamp());
        crate::layout::ensure_card_formatted(
            card_dir,
            &ctx.backups.join("pcsx2-card").join(label),
        )?;
    }
    let saves = ctx.client.saves(rom_id, ctx.device_id).await?;
    let server = pick(&saves, game.ext);
    let local = local::read(&game.spec)?;
    let srv = server.as_ref().map(|s| engine::Server {
        id: s.id,
        hash: s.content_hash.clone(),
    });
    let action = engine::decide(local.as_ref(), srv.as_ref(), record);
    match (action, choice) {
        (Action::Nothing, _) => Ok(Outcome::Synced {
            record: Record::default(),
            changed: None,
        }),
        (Action::InSync, _) => Ok(Outcome::Synced {
            record: record_of(local.as_ref(), server.as_ref()),
            changed: None,
        }),
        (Action::Download, _) | (Action::Conflict, Some(Choice::UseServer)) => {
            let s = server.as_ref().ok_or(Error::Inconsistent)?;
            Ok(Outcome::Synced {
                record: download(ctx, rom_id, game, s).await?,
                changed: Some("downloaded"),
            })
        }
        (Action::Upload, _) => {
            let l = local.as_ref().ok_or(Error::Inconsistent)?;
            match upload(ctx, rom_id, game, l, false).await? {
                Some(r) => Ok(Outcome::Synced {
                    record: r,
                    changed: Some("uploaded"),
                }),
                None => match server.as_ref() {
                    Some(s) => Ok(Outcome::Conflict(conflict(game, s))),
                    None => Err(Error::Inconsistent),
                },
            }
        }
        (Action::Conflict, None) => Ok(Outcome::Conflict(conflict(
            game,
            server.as_ref().ok_or(Error::Inconsistent)?,
        ))),
        (Action::Conflict, Some(Choice::KeepLocal)) => {
            let l = local.as_ref().ok_or(Error::Inconsistent)?;
            // RomM keeps old versions, but keep the one being replaced locally too.
            if let Some(s) = server.as_ref() {
                let bytes = ctx.client.download_save(s.id, None).await?;
                let dir = ctx
                    .backups
                    .join(rom_id.to_string())
                    .join(format!("{}-romm-version", stamp()));
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join(&s.file_name), bytes)?;
            }
            let r = upload(ctx, rom_id, game, l, true)
                .await?
                .ok_or(Error::Inconsistent)?;
            Ok(Outcome::Synced {
                record: r,
                changed: Some("uploaded"),
            })
        }
    }
}

/// After playing: upload what changed. A conflict is left for the next [`before`].
pub async fn after(
    ctx: &Ctx<'_>,
    rom_id: i64,
    game: &Located,
    record: Option<&Record>,
) -> Result<Outcome> {
    let Some(l) = local::read(&game.spec)? else {
        return Ok(Outcome::Synced {
            record: record.cloned().unwrap_or_default(),
            changed: None,
        });
    };
    if record.and_then(|r| r.local_fp.as_deref()) == Some(l.fp.as_str()) {
        return Ok(Outcome::Synced {
            record: record.cloned().unwrap_or_default(),
            changed: None,
        });
    }
    let saves = ctx.client.saves(rom_id, ctx.device_id).await?;
    let server = pick(&saves, game.ext);
    let srv = server.as_ref().map(|s| engine::Server {
        id: s.id,
        hash: s.content_hash.clone(),
    });
    match engine::decide(Some(&l), srv.as_ref(), record) {
        Action::InSync => Ok(Outcome::Synced {
            record: record_of(Some(&l), server.as_ref()),
            changed: None,
        }),
        Action::Upload => match upload(ctx, rom_id, game, &l, false).await? {
            Some(r) => Ok(Outcome::Synced {
                record: r,
                changed: Some("uploaded"),
            }),
            None => Ok(Outcome::Conflict(conflict(
                game,
                server.as_ref().ok_or(Error::Inconsistent)?,
            ))),
        },
        // Local changed and RomM changed too (another device played meanwhile).
        _ => match server.as_ref() {
            Some(s) => Ok(Outcome::Conflict(conflict(game, s))),
            None => Err(Error::Inconsistent),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(id: i64, name: &str, slot: &str, at: &str) -> ServerSave {
        ServerSave {
            id,
            rom_id: 1,
            file_name: name.into(),
            emulator: Some("retroarch-swanstation".into()),
            slot: Some(slot.into()),
            content_hash: Some(format!("h{id}")),
            file_size_bytes: 1,
            updated_at: at.into(),
            origin_device_id: None,
        }
    }

    #[test]
    fn picks_the_newest_compatible_save() {
        let saves = vec![
            save(1, "MGS [a].srm", "autosave", "2026-10-25T17:11:46"),
            save(5, "MGS [b].srm", "autosave", "2026-11-20T16:25:34"),
            save(6, "MGS [c].srm", "manual", "2026-11-26T00:00:00"),
            save(7, "autosave [d].zip", "autosave", "2026-11-27T00:00:00"),
        ];
        assert_eq!(pick(&saves, "srm").unwrap().id, 5);
        assert_eq!(pick(&saves, "zip").unwrap().id, 7);
        assert_eq!(pick(&[], "srm"), None);
    }
}

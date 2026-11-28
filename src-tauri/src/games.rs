//! Commands for getting games onto the Deck and playing them (Phase 2). The work itself
//! is in the `launcher` crate; this wires it to the cache, the UI and the controller.

use crate::state::{AppState, lock};
use launcher::{Installed, RetroDeck};
use library_cache::{Cache, QueuedSession};
use romm_api::{Client, PlaySession};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn home() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default()
}

pub fn retrodeck(state: &AppState) -> Option<RetroDeck> {
    let over = lock(&state.config).retrodeck_roms.clone();
    RetroDeck::detect(&home(), over.as_deref().map(std::path::Path::new))
}

async fn session_parts(state: &AppState) -> Res<(Client, Arc<Mutex<Cache>>)> {
    let s = state.session.read().await;
    let s = s.as_ref().ok_or("no server set")?;
    Ok((s.client.clone(), s.cache.clone()))
}

fn record(cache: &Mutex<Cache>, rom_id: i64) -> Option<Installed> {
    let json = lock(cache).install(rom_id).ok()??;
    serde_json::from_str(&json).ok()
}

#[derive(Serialize)]
pub struct RetroDeckStatus {
    found: bool,
    flatpak: bool,
    roms: Option<String>,
}

#[tauri::command]
pub fn retrodeck_status(state: State<'_, AppState>) -> RetroDeckStatus {
    let rd = retrodeck(&state);
    RetroDeckStatus {
        found: rd.is_some(),
        flatpak: RetroDeck::flatpak_installed(&home()),
        roms: rd.map(|r| r.roms.to_string_lossy().into_owned()),
    }
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InstallStatus {
    Installed {
        size_bytes: u64,
        path: String,
    },
    Downloading,
    NotInstalled,
    /// Can't be installed here; `reason` says why.
    Unavailable {
        reason: String,
    },
}

#[tauri::command]
pub async fn install_status(state: State<'_, AppState>, rom_id: i64) -> Res<InstallStatus> {
    if state
        .downloads
        .lock()
        .map(|d| d.contains_key(&rom_id))
        .unwrap_or(false)
    {
        return Ok(InstallStatus::Downloading);
    }
    let (_, cache) = session_parts(&state).await?;
    // Installed before discs came with their playlist: only the playlist is there.
    let parts = lock(&cache)
        .group_of(rom_id)
        .ok()
        .flatten()
        .map(|g| g.parts.len())
        .unwrap_or(0);
    if let Some(r) = record(&cache, rom_id).filter(|r| parts == 0 || r.files.len() > parts) {
        if r.launch.is_file() {
            return Ok(InstallStatus::Installed {
                size_bytes: r.size_bytes,
                path: r.launch.to_string_lossy().into_owned(),
            });
        }
        // Deleted outside Romulus (or the SD card is out).
        if retrodeck(&state).is_some() {
            let _ = lock(&cache).remove_install(rom_id);
        }
    }
    let Some(rd) = retrodeck(&state) else {
        return Ok(InstallStatus::Unavailable {
            reason: "RetroDECK was not found (is the SD card inserted?)".into(),
        });
    };
    let row = lock(&cache).rom(rom_id).map_err(err)?;
    let Some((_, json)) = row else {
        return Err("unknown game".into());
    };
    let fs = json["platform_fs_slug"].as_str().unwrap_or_default();
    let slug = json["platform_slug"].as_str().unwrap_or_default();
    if launcher::systems::folder_for(&rd.roms, fs, slug).is_none() {
        let name = json["platform_display_name"].as_str().unwrap_or(fs);
        return Ok(InstallStatus::Unavailable {
            reason: format!("RetroDECK doesn't support {name}"),
        });
    }
    Ok(InstallStatus::NotInstalled)
}

#[tauri::command]
pub async fn installed_ids(state: State<'_, AppState>) -> Res<Vec<i64>> {
    let (_, cache) = session_parts(&state).await?;
    let ids = lock(&cache).installed_ids().map_err(err)?;
    Ok(ids)
}

#[derive(Serialize, Clone)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum DownloadEvent {
    Queued { rom_id: i64, position: usize },
    Progress { rom_id: i64, done: u64, total: u64 },
    Done { rom_id: i64 },
    Cancelled { rom_id: i64 },
    Failed { rom_id: i64, message: String },
}

/// Queue a game for download. Downloads run one at a time, in order, and continue after
/// a restart. Progress arrives as `download` events.
#[tauri::command]
pub async fn download_rom(app: AppHandle, state: State<'_, AppState>, rom_id: i64) -> Res<()> {
    retrodeck(&state).ok_or("RetroDECK was not found")?;
    let (_, cache) = session_parts(&state).await?;
    enqueue(&app, &state, &cache, rom_id);
    Ok(())
}

fn enqueue(app: &AppHandle, state: &AppState, cache: &Mutex<Cache>, rom_id: i64) {
    {
        let mut d = lock(&state.downloads);
        if d.contains_key(&rom_id) {
            return;
        }
        d.insert(rom_id, Arc::new(AtomicBool::new(false)));
    }
    if let Err(e) = lock(cache).queue_download(rom_id) {
        tracing::warn!("could not save the download queue: {e}");
    }
    let position = {
        let mut q = lock(&state.queue);
        q.push_back(rom_id);
        q.len()
    };
    let _ = app.emit("download", DownloadEvent::Queued { rom_id, position });
    // Start the worker unless one is running (it clears the flag under the queue lock
    // when it finds the queue empty, so a game queued meanwhile is never stranded).
    if !state.download_worker.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { download_worker(app).await });
    }
}

async fn download_worker(app: AppHandle) {
    loop {
        let st = app.state::<AppState>();
        let next = {
            let mut q = lock(&st.queue);
            let n = q.pop_front();
            if n.is_none() {
                st.download_worker.store(false, Ordering::SeqCst);
            }
            n
        };
        let Some(rom_id) = next else { return };
        // Cancelled while it was waiting.
        let Some(cancel) = lock(&st.downloads).get(&rom_id).cloned() else {
            continue;
        };
        let event = run_download(&app, rom_id, &cancel).await;
        lock(&st.downloads).remove(&rom_id);
        *lock(&st.active_download) = None;
        if let Ok((_, cache)) = session_parts(&st).await {
            let _ = lock(&cache).unqueue_download(rom_id);
        }
        let _ = app.emit("download", event);
    }
}

async fn run_download(app: &AppHandle, rom_id: i64, cancel: &AtomicBool) -> DownloadEvent {
    let st = app.state::<AppState>();
    let Ok((client, cache)) = session_parts(&st).await else {
        return DownloadEvent::Failed {
            rom_id,
            message: "not signed in".into(),
        };
    };
    let Some(rd) = retrodeck(&st) else {
        return DownloadEvent::Failed {
            rom_id,
            message: "RetroDECK was not found".into(),
        };
    };
    let slug = lock(&cache)
        .rom(rom_id)
        .ok()
        .flatten()
        .and_then(|(_, j)| j["platform_slug"].as_str().map(str::to_string))
        .unwrap_or_default();
    *lock(&st.active_download) = Some((rom_id, 0, 0));
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let result: launcher::Result<Installed> = async {
        let mut files = client.rom_files(rom_id).await?;
        // A playlist stored as a loose file: its disc images are separate entries in
        // RomM (siblings) and come with it.
        let parts = lock(&cache)
            .group_of(rom_id)
            .ok()
            .flatten()
            .map(|g| g.parts)
            .unwrap_or_default();
        for part in parts {
            files.files.extend(client.rom_files(part).await?.files);
        }
        let folder = launcher::systems::folder_for(&rd.roms, &files.platform_fs_slug, &slug)
            .ok_or_else(|| launcher::Error::NoSystemFolder(files.platform_fs_slug.clone()))?;
        let system_dir = rd.roms.join(folder);
        let plan = launcher::plan(&files, &system_dir)?;
        if launcher::install::is_installed(&plan) {
            tracing::info!("rom {rom_id} is already on the device");
            return Ok(launcher::install::installed_record(&plan, &system_dir));
        }
        launcher::install(&client, &plan, &system_dir, cancel, |done, total| {
            *lock(&st.active_download) = Some((rom_id, done, total));
            let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
            if l.elapsed() >= Duration::from_millis(250) || done == total {
                *l = Instant::now();
                let _ = app.emit(
                    "download",
                    DownloadEvent::Progress {
                        rom_id,
                        done,
                        total,
                    },
                );
            }
        })
        .await
    }
    .await;
    match result {
        Ok(inst) => match serde_json::to_string(&inst).map_err(err).and_then(|j| {
            lock(&cache)
                .set_install(rom_id, &j, inst.size_bytes)
                .map_err(err)
        }) {
            Ok(()) => DownloadEvent::Done { rom_id },
            Err(e) => DownloadEvent::Failed { rom_id, message: e },
        },
        Err(launcher::Error::Cancelled) => DownloadEvent::Cancelled { rom_id },
        Err(e) => {
            tracing::warn!("download of rom {rom_id} failed: {e}");
            DownloadEvent::Failed {
                rom_id,
                message: e.to_string(),
            }
        }
    }
}

/// Stop a download: a waiting one leaves the queue; a running one stops and keeps its
/// partial file, so downloading again continues where it was.
#[tauri::command]
pub async fn cancel_download(app: AppHandle, state: State<'_, AppState>, rom_id: i64) -> Res<()> {
    let waiting = {
        let mut q = lock(&state.queue);
        let before = q.len();
        q.retain(|id| *id != rom_id);
        q.len() != before
    };
    if waiting {
        lock(&state.downloads).remove(&rom_id);
        if let Ok((_, cache)) = session_parts(&state).await {
            let _ = lock(&cache).unqueue_download(rom_id);
        }
        let _ = app.emit("download", DownloadEvent::Cancelled { rom_id });
    } else if let Some(c) = lock(&state.downloads).get(&rom_id) {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct DownloadItem {
    rom_id: i64,
    title: String,
    platform: String,
    /// "active" or "queued".
    state: &'static str,
    done: u64,
    total: u64,
}

/// The running download, then the queue in order.
#[tauri::command]
pub async fn downloads(state: State<'_, AppState>) -> Res<Vec<DownloadItem>> {
    let (_, cache) = session_parts(&state).await?;
    let active = *lock(&state.active_download);
    let queued: Vec<i64> = lock(&state.queue).iter().copied().collect();
    let c = lock(&cache);
    let item = |id: i64, state: &'static str, done: u64, total: u64| {
        let (title, platform) = c
            .rom(id)
            .ok()
            .flatten()
            .map(|(r, j)| {
                (
                    r.title,
                    j["platform_display_name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .unwrap_or_else(|| (format!("Game {id}"), String::new()));
        DownloadItem {
            rom_id: id,
            title,
            platform,
            state,
            done,
            total,
        }
    };
    let mut out = Vec::new();
    if let Some((id, done, total)) = active {
        out.push(item(id, "active", done, total));
    }
    out.extend(queued.into_iter().map(|id| item(id, "queued", 0, 0)));
    Ok(out)
}

/// Put downloads that were queued or running when Romulus last closed back in the queue.
pub async fn resume_downloads(app: &AppHandle) {
    let st = app.state::<AppState>();
    let Ok((_, cache)) = session_parts(&st).await else {
        return;
    };
    if retrodeck(&st).is_none() {
        return;
    }
    let ids = lock(&cache).queued_downloads().unwrap_or_default();
    if !ids.is_empty() {
        tracing::info!("resuming {} queued download(s)", ids.len());
    }
    for id in ids {
        enqueue(app, &st, &cache, id);
    }
}

/// Delete a downloaded game from the device (its ROM files only; saves stay).
#[tauri::command]
pub async fn remove_rom(state: State<'_, AppState>, rom_id: i64) -> Res<()> {
    let (_, cache) = session_parts(&state).await?;
    let inst = record(&cache, rom_id).ok_or("this game isn't on the device")?;
    tokio::task::spawn_blocking(move || launcher::remove(&inst))
        .await
        .map_err(err)?
        .map_err(err)?;
    lock(&cache).remove_install(rom_id).map_err(err)?;
    tracing::info!("removed rom {rom_id} from the device");
    Ok(())
}

#[derive(Serialize)]
pub struct DownloadAllPlan {
    /// Games not on the device and not already queued.
    rom_ids: Vec<i64>,
    bytes: u64,
    free_bytes: u64,
    fits: bool,
}

/// What downloading all of `rom_ids` would take (for "Download all" on a collection).
#[tauri::command]
pub async fn plan_download_all(
    state: State<'_, AppState>,
    rom_ids: Vec<i64>,
) -> Res<DownloadAllPlan> {
    let rd = retrodeck(&state).ok_or("RetroDECK was not found")?;
    let (_, cache) = session_parts(&state).await?;
    let queued: std::collections::HashSet<i64> = lock(&state.downloads).keys().copied().collect();
    let (ids, bytes) = {
        let c = lock(&cache);
        let installed: std::collections::HashSet<i64> =
            c.installed_ids().map_err(err)?.into_iter().collect();
        let mut ids = Vec::new();
        let mut bytes = 0u64;
        for id in rom_ids {
            if installed.contains(&id) || queued.contains(&id) || ids.contains(&id) {
                continue;
            }
            if let Some((row, _)) = c.rom(id).map_err(err)? {
                bytes += row.size_bytes.max(0) as u64;
                ids.push(id);
            }
        }
        (ids, bytes)
    };
    let (free, _) = launcher::install::space(&rd.roms).map_err(err)?;
    Ok(DownloadAllPlan {
        rom_ids: ids,
        bytes,
        free_bytes: free,
        fits: launcher::install::fits(bytes, free),
    })
}

/// Queue every game in `rom_ids` (from [`plan_download_all`]).
#[tauri::command]
pub async fn download_all(
    app: AppHandle,
    state: State<'_, AppState>,
    rom_ids: Vec<i64>,
) -> Res<usize> {
    retrodeck(&state).ok_or("RetroDECK was not found")?;
    let (_, cache) = session_parts(&state).await?;
    for &id in &rom_ids {
        enqueue(&app, &state, &cache, id);
    }
    tracing::info!("queued {} downloads", rom_ids.len());
    Ok(rom_ids.len())
}

#[derive(Serialize)]
pub struct Storage {
    free_bytes: u64,
    total_bytes: u64,
    games: Vec<library_cache::InstalledGame>,
}

/// The drive RetroDECK's games are on, and the games Romulus put there, largest first.
#[tauri::command]
pub async fn storage(state: State<'_, AppState>) -> Res<Storage> {
    let rd = retrodeck(&state).ok_or("RetroDECK was not found")?;
    let (_, cache) = session_parts(&state).await?;
    let games = lock(&cache).installed_games().map_err(err)?;
    let (free_bytes, total_bytes) = launcher::install::space(&rd.roms).map_err(err)?;
    Ok(Storage {
        free_bytes,
        total_bytes,
        games,
    })
}

#[derive(Serialize, Clone)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum GameEvent {
    Started {
        rom_id: i64,
    },
    /// `save` says what happened to the save afterwards (see [`save_note`]), `states`
    /// how many save states were uploaded.
    Ended {
        rom_id: i64,
        duration_ms: u64,
        save: Option<String>,
        states: usize,
    },
    Failed {
        rom_id: i64,
        message: String,
    },
}

/// What `play_rom` did.
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PlayStart {
    /// The game is starting; `save` says what happened to its save first.
    Started {
        save: Option<String>,
        /// Something to tell the player before the game appears.
        warning: Option<String>,
        /// Save states restored from RomM.
        states: usize,
    },
    /// The Deck and RomM both have newer saves: ask, then call `play_rom` with a choice.
    Conflict { info: sync::ConflictInfo },
}

/// The emulator RetroDECK uses for a game, its folders, and RomM's data for the game.
type GameEnv = (
    sync::emulator::Emulator,
    sync::layout::Dirs,
    serde_json::Value,
);

fn game_env(state: &AppState, cache: &Mutex<Cache>, inst: &Installed) -> Result<GameEnv, String> {
    let rd = retrodeck(state).ok_or("RetroDECK was not found")?;
    let rd_home = rd
        .roms
        .parent()
        .ok_or("unexpected RetroDECK folder")?
        .to_path_buf();
    let system = inst
        .system_dir
        .file_name()
        .ok_or("unexpected system folder")?
        .to_string_lossy()
        .into_owned();
    let game_rel = inst
        .launch
        .strip_prefix(&inst.system_dir)
        .map_err(|_| "unexpected game path")?
        .to_string_lossy()
        .replace('\\', "/");
    let es = sync::emulator::EsDe::retrodeck(&home(), &rd_home);
    let emu = sync::emulator::resolve(&es, &system, &game_rel)
        .ok_or("RetroDECK has no emulator for this system")?;
    let dirs = sync::layout::Dirs::retrodeck(
        &home(),
        rd.saves.clone().unwrap_or_else(|| rd_home.join("saves")),
    );
    let json = lock(cache)
        .rom(inst.rom_id)
        .ok()
        .flatten()
        .map(|(_, j)| j)
        .unwrap_or_default();
    Ok((emu, dirs, json))
}

/// Where this game's save lives, or why it can't be synced.
fn save_location(
    state: &AppState,
    cache: &Mutex<Cache>,
    inst: &Installed,
) -> Result<sync::layout::Located, String> {
    let (emu, dirs, json) = game_env(state, cache, inst)?;
    sync::layout::locate(
        &emu,
        &inst.launch,
        &dirs,
        json["save_target"].as_str(),
        json["save_target_layout"].as_str(),
    )
}

/// Where this game's save states live, or why they can't be synced.
fn state_location(
    state: &AppState,
    cache: &Mutex<Cache>,
    inst: &Installed,
) -> Result<sync::states::StateSet, String> {
    let (emu, dirs, json) = game_env(state, cache, inst)?;
    sync::states::locate(&emu, &inst.launch, &dirs, json["save_target"].as_str())
}

fn load_state_records(
    cache: &Mutex<Cache>,
    rom_id: i64,
) -> std::collections::HashMap<String, sync::states::StateRecord> {
    lock(cache)
        .state_records(rom_id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(n, j)| Some((n, serde_json::from_str(&j).ok()?)))
        .collect()
}

fn store_state_records(cache: &Mutex<Cache>, rom_id: i64, sum: &sync::states::Summary) {
    for (name, r) in &sum.records {
        let stored = serde_json::to_string(r)
            .map_err(|e| e.to_string())
            .and_then(|j| {
                lock(cache)
                    .set_state_record(rom_id, name, &j)
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = stored {
            tracing::warn!("could not store state sync record: {e}");
        }
    }
}

/// Sync a game's save states; failures are logged, never in the way of playing.
async fn sync_states(
    ctx: &sync::Ctx<'_>,
    cache: &Mutex<Cache>,
    rom_id: i64,
    set: &sync::states::StateSet,
    before: bool,
) -> sync::Result<sync::states::Summary> {
    let recs = load_state_records(cache, rom_id);
    let sum = match tokio::time::timeout(
        Duration::from_secs(120),
        sync::states::sync(ctx, rom_id, set, &recs, before),
    )
    .await
    {
        Ok(r) => r?,
        Err(_) => {
            return Err(sync::Error::Api(romm_api::Error::Decode(
                "timed out".into(),
            )));
        }
    };
    store_state_records(cache, rom_id, &sum);
    if sum.uploaded + sum.downloaded > 0 {
        tracing::info!(
            "states of rom {rom_id}: {} uploaded, {} downloaded",
            sum.uploaded,
            sum.downloaded
        );
    }
    Ok(sum)
}

fn load_record(cache: &Mutex<Cache>, rom_id: i64) -> Option<sync::Record> {
    serde_json::from_str(&lock(cache).save_record(rom_id).ok()??).ok()
}

fn store_record(cache: &Mutex<Cache>, rom_id: i64, r: &sync::Record) {
    match serde_json::to_string(r) {
        Ok(j) => {
            if let Err(e) = lock(cache).set_save_record(rom_id, &j) {
                tracing::warn!("could not store save sync state: {e}");
            }
        }
        Err(e) => tracing::warn!("could not store save sync state: {e}"),
    }
}

/// Short message for the UI about a sync result.
fn save_note(outcome: &sync::Result<sync::Outcome>) -> Option<String> {
    match outcome {
        Ok(sync::Outcome::Synced {
            changed: Some("downloaded"),
            ..
        }) => Some("downloaded".into()),
        Ok(sync::Outcome::Synced {
            changed: Some("uploaded"),
            ..
        }) => Some("uploaded".into()),
        Ok(sync::Outcome::Synced { .. }) => None,
        Ok(sync::Outcome::Conflict(_)) => Some("conflict".into()),
        // Unreachable, or the server (or the proxy in front of it) failing: try again next time.
        Err(sync::Error::Api(romm_api::Error::Network(_)))
        | Err(sync::Error::Api(romm_api::Error::Http {
            status: 500..=599, ..
        })) => Some("offline".into()),
        Err(e) => Some(format!("failed: {e}")),
    }
}

/// Start a downloaded game, syncing its save with RomM first and after it exits.
/// Controller input to Romulus is paused while it runs. Events: `game`
/// {stage: started | ended | failed}.
#[tauri::command]
pub async fn play_rom(
    app: AppHandle,
    state: State<'_, AppState>,
    rom_id: i64,
    choice: Option<sync::Choice>,
) -> Res<PlayStart> {
    let (client, cache) = session_parts(&state).await?;
    let inst = record(&cache, rom_id).ok_or("this game isn't on the device")?;
    if !inst.launch.is_file() {
        return Err("the game's files are missing; download it again".into());
    }
    if state.playing.swap(true, Ordering::SeqCst) {
        return Err("a game is already running".into());
    }

    // Save sync before playing. Failures never stop the game from starting.
    let device = client
        .credentials()
        .await
        .and_then(|c| c.device_id().map(str::to_string));
    let backups = state.paths.data_dir.join("backups");
    let located = if lock(&state.config).save_sync {
        match save_location(&state, &cache, &inst) {
            Ok(l) => Some(l),
            Err(why) => {
                tracing::info!("no save sync for rom {rom_id}: {why}");
                None
            }
        }
    } else {
        None
    };
    let mut note = None;
    if let Some(game) = &located {
        let ctx = sync::Ctx {
            client: &client,
            device_id: device.as_deref(),
            backups: &backups,
        };
        let rec = load_record(&cache, rom_id);
        let outcome = match tokio::time::timeout(
            Duration::from_secs(60),
            sync::before(&ctx, rom_id, game, rec.as_ref(), choice),
        )
        .await
        {
            Ok(o) => o,
            Err(_) => Err(sync::Error::Api(romm_api::Error::Decode(
                "timed out".into(),
            ))),
        };
        match &outcome {
            Ok(sync::Outcome::Conflict(info)) => {
                state.playing.store(false, Ordering::SeqCst);
                return Ok(PlayStart::Conflict { info: info.clone() });
            }
            Ok(sync::Outcome::Synced { record, .. }) => store_record(&cache, rom_id, record),
            Err(e) => tracing::warn!("save sync before rom {rom_id} failed: {e}"),
        }
        note = save_note(&outcome);
    }

    let states = if lock(&state.config).state_sync {
        match state_location(&state, &cache, &inst) {
            Ok(s) => Some(s),
            Err(why) => {
                tracing::info!("no state sync for rom {rom_id}: {why}");
                None
            }
        }
    } else {
        None
    };
    let mut states_restored = 0;
    if let Some(set) = &states {
        let ctx = sync::Ctx {
            client: &client,
            device_id: device.as_deref(),
            backups: &backups,
        };
        match sync_states(&ctx, &cache, rom_id, set, true).await {
            Ok(sum) => states_restored = sum.downloaded,
            Err(e) => tracing::warn!("state sync before rom {rom_id} failed: {e}"),
        }
    }

    let warning = located.as_ref().and_then(|l| l.warning.clone());
    crate::music::stop(&state);
    if let Err(e) = lock(&cache).mark_played_now(rom_id) {
        tracing::warn!("could not record last played: {e}");
    }
    state.input_paused.store(true, Ordering::Relaxed);
    let _ = app.emit("game", GameEvent::Started { rom_id });

    tauri::async_runtime::spawn(async move {
        let result = launcher::play(&inst.launch).await;
        let st = app.state::<AppState>();
        let web_gamepad = lock(&st.config).web_gamepad;
        st.input_paused.store(web_gamepad, Ordering::Relaxed);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_focus();
        }
        match result {
            Ok(s) if s.failed_to_start() => {
                st.playing.store(false, Ordering::SeqCst);
                tracing::warn!(
                    "rom {rom_id} exited at once with {:?}: {}",
                    s.exit_code,
                    s.stderr_tail
                );
                let last = s
                    .stderr_tail
                    .lines()
                    .last()
                    .unwrap_or("no details")
                    .to_string();
                let _ = app.emit(
                    "game",
                    GameEvent::Failed {
                        rom_id,
                        message: format!("RetroDECK exited at once ({last})"),
                    },
                );
            }
            Ok(s) => {
                // Save first: it matters more than the play time.
                let mut save = None;
                if let Some(game) = &located {
                    let ctx = sync::Ctx {
                        client: &client,
                        device_id: device.as_deref(),
                        backups: &backups,
                    };
                    let rec = load_record(&cache, rom_id);
                    let outcome = sync::after(&ctx, rom_id, game, rec.as_ref()).await;
                    match &outcome {
                        Ok(sync::Outcome::Synced { record, .. }) => {
                            store_record(&cache, rom_id, record)
                        }
                        Ok(sync::Outcome::Conflict(_)) => {
                            tracing::warn!("save conflict for rom {rom_id}; will ask next time")
                        }
                        Err(e) => tracing::warn!("save sync after rom {rom_id} failed: {e}"),
                    }
                    save = save_note(&outcome);
                }
                let mut states_uploaded = 0;
                if let Some(set) = &states {
                    let ctx = sync::Ctx {
                        client: &client,
                        device_id: device.as_deref(),
                        backups: &backups,
                    };
                    match sync_states(&ctx, &cache, rom_id, set, false).await {
                        Ok(sum) => states_uploaded = sum.uploaded,
                        Err(e) => tracing::warn!("state sync after rom {rom_id} failed: {e}"),
                    }
                }
                st.playing.store(false, Ordering::SeqCst);
                // Very short runs are failed starts, not play.
                if s.duration_ms >= 30_000 {
                    let q = QueuedSession {
                        rom_id,
                        start_time: fmt(s.start),
                        end_time: fmt(s.end),
                        duration_ms: s.duration_ms,
                    };
                    if let Err(e) = lock(&cache).queue_play_session(&q) {
                        tracing::warn!("could not queue play session: {e}");
                    }
                    if let Err(e) = client.mark_played(rom_id).await {
                        tracing::warn!("could not update last played: {e}");
                    }
                    report_play_sessions(&client, &cache).await;
                }
                let _ = app.emit(
                    "game",
                    GameEvent::Ended {
                        rom_id,
                        duration_ms: s.duration_ms,
                        save,
                        states: states_uploaded,
                    },
                );
            }
            Err(e) => {
                st.playing.store(false, Ordering::SeqCst);
                tracing::warn!("could not start rom {rom_id}: {e}");
                let _ = app.emit(
                    "game",
                    GameEvent::Failed {
                        rom_id,
                        message: e.to_string(),
                    },
                );
            }
        }
    });
    Ok(PlayStart::Started {
        save: note,
        warning,
        states: states_restored,
    })
}

/// How this game's save syncs, for the detail page.
#[derive(Serialize)]
pub struct SaveSyncStatus {
    /// Saves sync for this game.
    syncs: bool,
    /// RomM's emulator name, when it syncs.
    emulator: Option<String>,
    /// Why it doesn't sync, or something to know (RFC: plain sentence).
    note: Option<String>,
    /// Last time it was in sync (RFC 3339).
    synced_at: Option<String>,
    /// Changed on the Deck since then; will upload.
    pending: bool,
    /// How save states sync (a sentence), or None when state sync is off.
    states: Option<String>,
}

/// A sentence about this game's save states for the detail page.
fn states_line(state: &AppState, cache: &Mutex<Cache>, inst: &Installed) -> Option<String> {
    if !lock(&state.config).state_sync {
        return None;
    }
    Some(match state_location(state, cache, inst) {
        Ok(set) => {
            let recs = load_state_records(cache, inst.rom_id);
            let pending = sync::states::changed_locally(&set, &recs)
                .map(|v| v.len())
                .unwrap_or(0);
            match pending {
                0 => "Save states sync too".into(),
                1 => "Save states sync too · 1 waiting to upload".into(),
                n => format!("Save states sync too · {n} waiting to upload"),
            }
        }
        Err(why) => format!("Save states don't sync: {why}"),
    })
}

#[tauri::command]
pub async fn save_sync_status(
    state: State<'_, AppState>,
    rom_id: i64,
) -> Res<Option<SaveSyncStatus>> {
    if !lock(&state.config).save_sync {
        return Ok(Some(SaveSyncStatus {
            syncs: false,
            emulator: None,
            note: Some("Save sync is off (Settings)".into()),
            synced_at: None,
            pending: false,
            states: None,
        }));
    }
    let (_, cache) = session_parts(&state).await?;
    let Some(inst) = record(&cache, rom_id) else {
        return Ok(None);
    };
    let states = states_line(&state, &cache, &inst);
    Ok(Some(match save_location(&state, &cache, &inst) {
        Ok(l) => {
            let rec = load_record(&cache, rom_id);
            let pending = sync::changed_locally(&l, rec.as_ref()).unwrap_or(false);
            SaveSyncStatus {
                syncs: true,
                emulator: Some(l.emulator.clone()),
                note: l.warning.clone(),
                synced_at: rec.and_then(|r| r.synced_at),
                pending,
                states,
            }
        }
        Err(why) => SaveSyncStatus {
            syncs: false,
            emulator: None,
            note: Some(why),
            synced_at: None,
            pending: false,
            states,
        },
    }))
}

/// Upload saves that changed on the Deck since their last sync: ones whose upload
/// failed (RomM was down) and ones played outside Romulus (e.g. from ES-DE). Only games
/// whose save actually changed are contacted. Emits `save-sync` {uploaded, conflicts}.
pub async fn sync_changed_saves(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !lock(&state.config).save_sync || state.playing.load(Ordering::SeqCst) {
        return;
    }
    let Ok((client, cache)) = session_parts(&state).await else {
        return;
    };
    let ids = lock(&cache).installed_ids().unwrap_or_default();
    let device = client
        .credentials()
        .await
        .and_then(|c| c.device_id().map(str::to_string));
    let backups = state.paths.data_dir.join("backups");
    let (mut uploaded, mut conflicts) = (0u32, 0u32);
    let mut offline = false;
    for rom_id in ids.iter().copied() {
        let Some(inst) = record(&cache, rom_id) else {
            continue;
        };
        let Ok(game) = save_location(&state, &cache, &inst) else {
            continue;
        };
        let rec = load_record(&cache, rom_id);
        if !sync::changed_locally(&game, rec.as_ref()).unwrap_or(false) {
            continue;
        }
        let ctx = sync::Ctx {
            client: &client,
            device_id: device.as_deref(),
            backups: &backups,
        };
        match sync::after(&ctx, rom_id, &game, rec.as_ref()).await {
            Ok(sync::Outcome::Synced { record, changed }) => {
                store_record(&cache, rom_id, &record);
                if changed == Some("uploaded") {
                    uploaded += 1;
                }
            }
            Ok(sync::Outcome::Conflict(_)) => conflicts += 1,
            Err(e) => {
                tracing::info!("save of rom {rom_id} stays pending: {e}");
                // RomM is probably unreachable; no point trying the rest now.
                if matches!(
                    e,
                    sync::Error::Api(
                        romm_api::Error::Network(_)
                            | romm_api::Error::Http {
                                status: 500..=599,
                                ..
                            }
                    )
                ) {
                    offline = true;
                    break;
                }
            }
        }
    }
    // Save states made outside Romulus (or whose upload failed) go up too.
    let mut states = 0usize;
    if lock(&state.config).state_sync && !offline {
        for rom_id in ids {
            let Some(inst) = record(&cache, rom_id) else {
                continue;
            };
            let Ok(set) = state_location(&state, &cache, &inst) else {
                continue;
            };
            let recs = load_state_records(&cache, rom_id);
            if !matches!(sync::states::changed_locally(&set, &recs), Ok(v) if !v.is_empty()) {
                continue;
            }
            let ctx = sync::Ctx {
                client: &client,
                device_id: device.as_deref(),
                backups: &backups,
            };
            match sync_states(&ctx, &cache, rom_id, &set, false).await {
                Ok(sum) => states += sum.uploaded,
                Err(e) => {
                    tracing::info!("states of rom {rom_id} stay pending: {e}");
                    break;
                }
            }
        }
    }
    if states > 0 {
        tracing::info!("background state sync: {states} uploaded");
    }
    if uploaded > 0 || conflicts > 0 {
        tracing::info!("background save sync: {uploaded} uploaded, {conflicts} conflicts");
        let _ = app.emit(
            "save-sync",
            serde_json::json!({ "uploaded": uploaded, "conflicts": conflicts }),
        );
    }
}

fn fmt(t: time::OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Send queued play sessions to RomM; they stay queued if that fails (offline).
pub async fn report_play_sessions(client: &Client, cache: &Mutex<Cache>) {
    let queued = match lock(cache).queued_play_sessions() {
        Ok(q) if !q.is_empty() => q,
        _ => return,
    };
    let parse = |s: &str| {
        time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
    };
    let mut ids = Vec::new();
    let mut sessions = Vec::new();
    for (id, q) in &queued {
        ids.push(*id);
        if let (Some(start_time), Some(end_time)) = (parse(&q.start_time), parse(&q.end_time)) {
            sessions.push(PlaySession {
                rom_id: q.rom_id,
                start_time,
                end_time,
                duration_ms: q.duration_ms,
            });
        }
    }
    let device = client
        .credentials()
        .await
        .and_then(|c| c.device_id().map(str::to_string));
    let ok = sessions.is_empty()
        || client
            .report_play_sessions(device.as_deref(), &sessions)
            .await
            .is_ok();
    if ok {
        let _ = lock(cache).delete_play_sessions(&ids);
        tracing::info!("reported {} play session(s) to RomM", sessions.len());
    } else {
        tracing::info!(
            "{} play session(s) stay queued until RomM is reachable",
            sessions.len()
        );
    }
}

#[derive(Serialize)]
pub struct BackupVersion {
    /// Folder name: "<time>-<reason>".
    label: String,
}

#[derive(Serialize)]
pub struct GameBackups {
    rom_id: i64,
    title: String,
    /// Newest first.
    versions: Vec<BackupVersion>,
}

/// Save backups Romulus made, per game.
#[tauri::command]
pub async fn save_backups(state: State<'_, AppState>) -> Res<Vec<GameBackups>> {
    let (_, cache) = session_parts(&state).await?;
    let root = state.paths.data_dir.join("backups");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let Ok(rom_id) = e.file_name().to_string_lossy().parse::<i64>() else {
            continue;
        };
        let mut versions: Vec<BackupVersion> = std::fs::read_dir(e.path())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|v| v.path().is_dir())
            .map(|v| BackupVersion {
                label: v.file_name().to_string_lossy().into_owned(),
            })
            .collect();
        if versions.is_empty() {
            continue;
        }
        versions.sort_by(|a, b| b.label.cmp(&a.label));
        let title = lock(&cache)
            .rom(rom_id)
            .ok()
            .flatten()
            .map(|(r, _)| r.title)
            .unwrap_or_else(|| format!("Game {rom_id}"));
        out.push(GameBackups {
            rom_id,
            title,
            versions,
        });
    }
    out.sort_by_key(|g| g.title.to_lowercase());
    Ok(out)
}

/// Put a backed-up save back (the current one is backed up first). The game must be on
/// the Deck, since that decides where its save lives.
#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, rom_id: i64, label: String) -> Res<()> {
    if label.contains(['/', char::from(92)]) || label.starts_with('.') {
        return Err("bad backup name".into());
    }
    let (_, cache) = session_parts(&state).await?;
    let inst =
        record(&cache, rom_id).ok_or("download the game first; its save location depends on it")?;
    let game = save_location(&state, &cache, &inst)?;
    let dir = state
        .paths
        .data_dir
        .join("backups")
        .join(rom_id.to_string());
    sync::local::restore(&game.spec, &dir.join(&label), &dir, &sync::run::stamp()).map_err(err)?;
    tracing::info!("restored save backup {label} for rom {rom_id}");
    Ok(())
}

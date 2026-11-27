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
    if let Some(r) = record(&cache, rom_id) {
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
    Progress { rom_id: i64, done: u64, total: u64 },
    Done { rom_id: i64 },
    Cancelled { rom_id: i64 },
    Failed { rom_id: i64, message: String },
}

/// Download a game in the background. Progress arrives as `download` events.
#[tauri::command]
pub async fn download_rom(app: AppHandle, state: State<'_, AppState>, rom_id: i64) -> Res<()> {
    let (client, cache) = session_parts(&state).await?;
    let rd = retrodeck(&state).ok_or("RetroDECK was not found")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut d = lock(&state.downloads);
        if d.contains_key(&rom_id) {
            return Ok(());
        }
        d.insert(rom_id, cancel.clone());
    }
    let slug = lock(&cache)
        .rom(rom_id)
        .ok()
        .flatten()
        .and_then(|(_, j)| j["platform_slug"].as_str().map(str::to_string))
        .unwrap_or_default();

    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let result: launcher::Result<Installed> = async {
            let files = client.rom_files(rom_id).await?;
            let folder = launcher::systems::folder_for(&rd.roms, &files.platform_fs_slug, &slug)
                .ok_or_else(|| launcher::Error::NoSystemFolder(files.platform_fs_slug.clone()))?;
            let system_dir = rd.roms.join(folder);
            let plan = launcher::plan(&files, &system_dir)?;
            if launcher::install::is_installed(&plan) {
                tracing::info!("rom {rom_id} is already on the device");
                return Ok(launcher::install::installed_record(&plan, &system_dir));
            }
            launcher::install(&client, &plan, &system_dir, &cancel, |done, total| {
                let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
                if l.elapsed() >= Duration::from_millis(250) || done == total {
                    *l = Instant::now();
                    let _ = progress_app.emit(
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

        let st = app.state::<AppState>();
        lock(&st.downloads).remove(&rom_id);
        let event = match result {
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
        };
        let _ = app.emit("download", event);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, rom_id: i64) {
    if let Some(c) = lock(&state.downloads).get(&rom_id) {
        c.store(true, Ordering::Relaxed);
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

#[derive(Serialize, Clone)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum GameEvent {
    Started { rom_id: i64 },
    Ended { rom_id: i64, duration_ms: u64 },
    Failed { rom_id: i64, message: String },
}

/// Start a downloaded game. Controller input to Romulus is paused until it exits.
/// Events: `game` {stage: started | ended | failed}.
#[tauri::command]
pub async fn play_rom(app: AppHandle, state: State<'_, AppState>, rom_id: i64) -> Res<()> {
    let (client, cache) = session_parts(&state).await?;
    let inst = record(&cache, rom_id).ok_or("this game isn't on the device")?;
    if !inst.launch.is_file() {
        return Err("the game's files are missing; download it again".into());
    }
    if state.playing.swap(true, Ordering::SeqCst) {
        return Err("a game is already running".into());
    }
    state.input_paused.store(true, Ordering::Relaxed);
    let _ = app.emit("game", GameEvent::Started { rom_id });

    tauri::async_runtime::spawn(async move {
        let result = launcher::play(&inst.launch).await;
        let st = app.state::<AppState>();
        let web_gamepad = lock(&st.config).web_gamepad;
        st.input_paused.store(web_gamepad, Ordering::Relaxed);
        st.playing.store(false, Ordering::SeqCst);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_focus();
        }
        match result {
            Ok(s) => {
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
                    },
                );
            }
            Err(e) => {
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
    Ok(())
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

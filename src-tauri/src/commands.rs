//! Tauri commands: thin wrappers that validate input, call the crates and turn errors
//! into messages for the UI. Logic lives in the crates.

use crate::app::{self, APP_NAME, RELEASE_REPO, VERSION};
use crate::secrets::Store;
use crate::state::{AppState, lock};
use library_cache::{PlatformRow, RefreshProgress, RomRow, SortOrder};
use romm_api::{DeviceInfo, DevicePoll};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct Status {
    app_name: &'static str,
    version: &'static str,
    server_url: Option<String>,
    signed_in: bool,
    credential_store: Option<Store>,
    rom_count: i64,
    /// Unix seconds of the last successful library refresh.
    last_sync_at: Option<i64>,
    refreshing: bool,
    updatable: bool,
    fullscreen: bool,
    in_steam: bool,
    gaming_mode: bool,
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Res<Status> {
    let session = state.session.read().await;
    let (rom_count, last_sync_at) = match session.as_ref() {
        Some(s) => {
            let c = lock(&s.cache);
            (
                c.rom_count().unwrap_or(0),
                c.meta("last_sync_at")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok()),
            )
        }
        None => (0, None),
    };
    Ok(Status {
        app_name: APP_NAME,
        version: VERSION,
        server_url: session.as_ref().map(|s| s.server.clone()),
        signed_in: session.as_ref().is_some_and(|s| s.signed_in),
        credential_store: session.as_ref().and_then(|s| s.store),
        rom_count,
        last_sync_at,
        refreshing: state.refreshing.load(Ordering::Relaxed),
        updatable: updater::updatable().is_ok(),
        fullscreen: lock(&state.config).fullscreen,
        in_steam: crate::steam::has_shortcut(),
        gaming_mode: crate::steam::in_gaming_mode(),
    })
}

#[derive(Serialize)]
pub struct ServerInfo {
    url: String,
    version: String,
}

/// Check that `url` is a RomM server and make it the current server.
#[tauri::command]
pub async fn set_server(state: State<'_, AppState>, url: String) -> Res<ServerInfo> {
    let base = romm_api::normalize_server_url(&url).map_err(err)?;
    let probe = romm_api::Client::new(base.clone(), &app::user_agent()).map_err(err)?;
    let hb = probe
        .heartbeat()
        .await
        .map_err(|e| format!("{} doesn't look like a RomM server: {e}", base))?;
    state.open_session(base.as_str()).await?;
    {
        let mut cfg = lock(&state.config);
        cfg.server_url = Some(base.to_string());
    }
    state.save_config();
    tracing::info!("server set to {base} (RomM {})", hb.system.version);
    Ok(ServerInfo {
        url: base.to_string(),
        version: hb.system.version,
    })
}

#[derive(Serialize)]
pub struct PairInfo {
    device_code: String,
    user_code: String,
    url: String,
    expires_in: u64,
    interval: u64,
}

#[tauri::command]
pub async fn pair_start(state: State<'_, AppState>) -> Res<PairInfo> {
    let session = state.session.read().await;
    let s = session.as_ref().ok_or("set a server first")?;
    let id = lock(&state.config).device_identifier.clone();
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let name = match host.trim() {
        "" => "Steam Deck".to_string(),
        h => format!("{APP_NAME} on {h}"),
    };
    let info = DeviceInfo {
        client_device_identifier: &id,
        name: &name,
        client: APP_NAME,
        platform: "SteamOS",
        client_version: VERSION,
    };
    let init = s.client.device_auth_init(&info).await.map_err(err)?;
    Ok(PairInfo {
        url: s.client.verification_url(&init),
        device_code: init.device_code,
        user_code: init.user_code,
        expires_in: init.expires_in,
        interval: init.interval,
    })
}

/// One poll of the pairing flow: "pending", "slow_down", "approved", "denied" or "expired".
#[tauri::command]
pub async fn pair_poll(state: State<'_, AppState>, device_code: String) -> Res<&'static str> {
    let poll = {
        let session = state.session.read().await;
        let s = session.as_ref().ok_or("set a server first")?;
        s.client.device_auth_poll(&device_code).await.map_err(err)?
    };
    Ok(match poll {
        DevicePoll::Pending => "pending",
        DevicePoll::SlowDown => "slow_down",
        DevicePoll::Denied => "denied",
        DevicePoll::Expired => "expired",
        DevicePoll::Approved(creds) => {
            state.sign_in(creds).await?;
            tracing::info!("paired with RomM");
            "approved"
        }
    })
}

#[tauri::command]
pub async fn password_login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Res<()> {
    let creds = {
        let session = state.session.read().await;
        let s = session.as_ref().ok_or("set a server first")?;
        s.client
            .password_login(username.trim(), &password)
            .await
            .map_err(|e| match e {
                romm_api::Error::Unauthorized => "Wrong username or password.".to_string(),
                romm_api::Error::Forbidden(d) => format!("RomM refused the sign-in: {d}"),
                e => e.to_string(),
            })?
    };
    state.sign_in(creds).await?;
    lock(&state.config).username = Some(username.trim().to_string());
    state.save_config();
    Ok(())
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Res<()> {
    let mut session = state.session.write().await;
    if let Some(s) = session.as_mut() {
        state.secrets.clear(&s.server);
        s.client.set_credentials(None).await;
        s.signed_in = false;
        s.store = None;
    }
    lock(&state.config).username = None;
    state.save_config();
    Ok(())
}

// ---- library (all from the local cache) ------------------------------------------------

async fn with_cache<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&library_cache::Cache) -> library_cache::Result<T> + Send + 'static,
) -> Res<T> {
    let cache = {
        let session = state.session.read().await;
        session.as_ref().ok_or("no server set")?.cache.clone()
    };
    tokio::task::spawn_blocking(move || f(&lock(&cache)))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub async fn platforms(state: State<'_, AppState>) -> Res<Vec<PlatformRow>> {
    with_cache(&state, |c| c.platforms()).await
}

#[tauri::command]
pub async fn roms(
    state: State<'_, AppState>,
    platform_id: i64,
    order: Option<SortOrder>,
) -> Res<Vec<RomRow>> {
    with_cache(&state, move |c| {
        c.roms(platform_id, order.unwrap_or_default())
    })
    .await
}

#[derive(Serialize)]
pub struct RomDetail {
    #[serde(flatten)]
    row: RomRow,
    data: serde_json::Value,
}

/// Cached detail for one ROM (the list schema, which has summary, metadata and
/// screenshots).
#[tauri::command]
pub async fn rom(state: State<'_, AppState>, id: i64) -> Res<Option<RomDetail>> {
    let r = with_cache(&state, move |c| c.rom(id)).await?;
    Ok(r.map(|(row, data)| RomDetail { row, data }))
}

/// Remember the games layout ("list" or "grid") for one platform.
#[tauri::command]
pub fn set_games_view(state: State<'_, AppState>, platform: String, view: String) -> Res<()> {
    if !["list", "grid"].contains(&view.as_str()) {
        return Err("unknown games view".into());
    }
    lock(&state.config).games_view.insert(platform, view);
    state.save_config();
    Ok(())
}

/// A few cover paths per platform, for the systems screen.
#[tauri::command]
pub async fn platform_covers(
    state: State<'_, AppState>,
    per_platform: usize,
) -> Res<std::collections::HashMap<i64, Vec<String>>> {
    with_cache(&state, move |c| c.platform_covers(per_platform.min(12))).await
}

#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: String) -> Res<Vec<RomRow>> {
    with_cache(&state, move |c| c.search(&query, 200)).await
}

/// Local file for a server asset (cover, screenshot), downloading it if needed. The UI
/// shows it through Tauri's asset protocol.
#[tauri::command]
pub async fn asset(state: State<'_, AppState>, path: String) -> Res<String> {
    if !path.starts_with('/') || path.contains("..") {
        return Err("bad asset path".into());
    }
    let (client, covers) = {
        let session = state.session.read().await;
        let s = session.as_ref().ok_or("no server set")?;
        if let Some(f) = s.covers.cached(&path) {
            return Ok(f.to_string_lossy().into_owned());
        }
        (s.client.clone(), s.covers.clone())
    };
    let f = covers.get(&client, &path).await.map_err(err)?;
    Ok(f.to_string_lossy().into_owned())
}

/// Start a background library refresh. Progress arrives as `refresh` events:
/// `{stage: ...}` while running, then `{stage: "done", stats}` or `{stage: "error", message}`.
#[tauri::command]
pub async fn refresh_library(app: AppHandle, state: State<'_, AppState>) -> Res<bool> {
    let (client, cache, covers) = {
        let session = state.session.read().await;
        let s = session.as_ref().ok_or("no server set")?;
        if !s.signed_in {
            return Err("not signed in".into());
        }
        (s.client.clone(), s.cache.clone(), s.covers.clone())
    };
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let result = library_cache::refresh(&client, &cache, move |p: RefreshProgress| {
            let _ = progress_app.emit("refresh", &p);
        })
        .await;
        let st = app.state::<AppState>();
        st.refreshing.store(false, Ordering::SeqCst);
        match result {
            Ok(stats) => {
                // A good moment to send play sessions recorded while offline.
                crate::games::report_play_sessions(&client, &cache).await;
                let _ = app.emit(
                    "refresh",
                    serde_json::json!({"stage": "done", "stats": stats}),
                );
            }
            Err(e) => {
                tracing::warn!("library refresh failed: {e}");
                let msg = match &e {
                    library_cache::Error::Api(romm_api::Error::Unauthorized) => {
                        "unauthorized".to_string()
                    }
                    e => e.to_string(),
                };
                let _ = app.emit(
                    "refresh",
                    serde_json::json!({"stage": "error", "message": msg}),
                );
            }
        }
        let _ = tokio::task::spawn_blocking(move || covers.prune()).await;
    });
    Ok(true)
}

// ---- updates ---------------------------------------------------------------------------

#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> Res<Option<updater::Release>> {
    let r = updater::check(&state.http, &updater::Source::github(RELEASE_REPO), VERSION)
        .await
        .map_err(err)?;
    *lock(&state.pending_update) = r.clone();
    Ok(r)
}

/// Download and install the release found by `check_update`. Progress arrives as
/// `update-progress` events `{done, total}`.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    let r = lock(&state.pending_update)
        .clone()
        .ok_or("no update to install")?;
    updater::install(&state.http, &r, move |done, total| {
        let _ = app.emit(
            "update-progress",
            serde_json::json!({"done": done, "total": total}),
        );
    })
    .await
    .map_err(err)?;
    Ok(())
}

/// Start the (new) AppImage and quit this one.
#[tauri::command]
pub fn restart_app(app: AppHandle) -> Res<()> {
    let exe = updater::program_path().map_err(err)?;
    let mut cmd = std::process::Command::new(&exe);
    // The AppImage runtime sets these for the running copy; the new one sets its own.
    for v in ["APPIMAGE", "APPDIR", "OWD", "ARGV0"] {
        cmd.env_remove(v);
    }
    // Don't hand our open files to the new copy. The AppImage runtime passes its FUSE
    // connection down without close-on-exec; if the new copy inherits it, the old
    // AppImage's mount (and runtime process) stays alive until Steam closes everything.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: close_range is async-signal-safe and only touches the child's table.
        unsafe {
            cmd.pre_exec(|| {
                libc::close_range(3, libc::c_uint::MAX, 0);
                Ok(())
            });
        }
    }
    cmd.current_dir(dirs::home_dir().unwrap_or_else(|| "/".into()));
    cmd.spawn()
        .map_err(|e| format!("could not start the new version: {e}"))?;
    app.exit(0);
    Ok(())
}

/// Release notes when this is the first start of a new version.
#[tauri::command]
pub async fn whats_new(state: State<'_, AppState>) -> Res<Option<String>> {
    let previous = lock(&state.config)
        .last_run_version
        .replace(VERSION.to_string());
    state.save_config();
    match previous {
        Some(p) if updater::newer(VERSION, &p) => Ok(updater::release_notes(
            &state.http,
            &updater::Source::github(RELEASE_REPO),
            VERSION,
        )
        .await
        .ok()),
        _ => Ok(None),
    }
}

// ---- settings and debug ----------------------------------------------------------------

#[tauri::command]
pub fn settings(state: State<'_, AppState>) -> crate::config::Config {
    lock(&state.config).clone()
}

#[derive(serde::Deserialize)]
pub struct SettingsPatch {
    check_updates: Option<bool>,
    fullscreen: Option<bool>,
    log_level: Option<String>,
    web_gamepad: Option<bool>,
    systems_view: Option<String>,
    fast_renderer: Option<bool>,
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Res<()> {
    {
        let mut cfg = lock(&state.config);
        if let Some(v) = patch.check_updates {
            cfg.check_updates = v;
        }
        if let Some(v) = patch.fullscreen {
            cfg.fullscreen = v;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_fullscreen(v);
            }
        }
        if let Some(v) = patch.log_level {
            if !["error", "warn", "info", "debug", "trace"].contains(&v.as_str()) {
                return Err("unknown log level".into());
            }
            cfg.log_level = v;
        }
        if let Some(v) = patch.web_gamepad {
            cfg.web_gamepad = v;
            state.input_paused.store(v, Ordering::Relaxed);
        }
        if let Some(v) = patch.fast_renderer {
            cfg.fast_renderer = v;
        }
        if let Some(v) = patch.systems_view {
            if !["carousel", "list", "grid"].contains(&v.as_str()) {
                return Err("unknown systems view".into());
            }
            cfg.systems_view = v;
        }
    }
    state.save_config();
    Ok(())
}

/// Text for "Copy debug info": versions, paths, settings (no secrets) and recent log lines.
#[tauri::command]
pub async fn debug_info(state: State<'_, AppState>) -> Res<String> {
    let os = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix("PRETTY_NAME=")
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let (server, romm_version, store, roms) = {
        let session = state.session.read().await;
        match session.as_ref() {
            Some(s) => {
                let v = s
                    .client
                    .heartbeat()
                    .await
                    .map(|h| h.system.version)
                    .unwrap_or_else(|e| format!("unreachable ({e})"));
                let roms = lock(&s.cache).rom_count().unwrap_or(0);
                (s.server.clone(), v, format!("{:?}", s.store), roms)
            }
            None => ("none".into(), "-".into(), "-".into(), 0),
        }
    };
    let cfg = lock(&state.config).clone();
    let p = &state.paths;
    Ok(format!(
        "{APP_NAME} {VERSION}\nOS: {os}\nAppImage: {}\nServer: {server}\nRomM: {romm_version}\nCredentials: {store}\nCached ROMs: {roms}\n\
         Log level: {}\nConfig: {}\nData: {}\nCache: {}\nLogs: {}\n\n--- recent log ---\n{}",
        std::env::var("APPIMAGE").unwrap_or_else(|_| "no".into()),
        cfg.log_level,
        p.config_file().display(),
        p.data_dir.display(),
        p.cache_dir.display(),
        p.log_dir.display(),
        crate::logging::tail(&p.log_dir, 200)
    ))
}

/// Ask Steam to show its on-screen keyboard (works in Gaming Mode; elsewhere it does
/// nothing and the user types normally).
#[tauri::command]
pub fn open_keyboard() {
    if std::env::var_os("SteamDeck").is_none() && std::env::var_os("SteamGamepadUI").is_none() {
        return;
    }
    if let Err(e) = std::process::Command::new("xdg-open")
        .arg("steam://open/keyboard")
        .spawn()
    {
        tracing::debug!("could not open Steam keyboard: {e}");
    }
}

/// Add Romulus itself to Steam as one non-Steam shortcut.
#[tauri::command]
pub async fn add_to_steam(state: State<'_, AppState>) -> Res<String> {
    let dir = state.paths.data_dir.clone();
    tokio::task::spawn_blocking(move || crate::steam::add_romulus(&dir))
        .await
        .map_err(err)?
}

/// Errors and warnings from the UI, so they end up in the log file.
#[tauri::command]
pub fn log_ui(level: String, message: String) {
    let message: String = message.chars().take(2000).collect();
    match level.as_str() {
        "error" => tracing::error!(target: "ui", "{message}"),
        "warn" => tracing::warn!(target: "ui", "{message}"),
        _ => tracing::info!(target: "ui", "{message}"),
    }
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

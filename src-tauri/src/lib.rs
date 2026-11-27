pub mod app;
mod bios;
mod commands;
pub mod config;
mod games;
mod input;
mod logging;
pub mod paths;
mod secrets;
mod state;
mod steam;

use state::{AppState, lock};
use tauri::Manager;

pub fn run() {
    let paths = paths::Paths::from_system();
    if let Err(e) = paths.create_all() {
        eprintln!("could not create app folders: {e}");
    }
    let config = config::Config::load(&paths.config_file());
    let _log_guard = logging::init(&paths.log_dir, &config.log_level);
    tracing::info!("{} {} starting", app::APP_NAME, app::VERSION);
    // Remove what the last self-update left behind.
    updater::cleanup();

    // WebKitGTK's DMA-BUF renderer is much faster (frames stay on the GPU) and is the
    // default; it shows a blank window on some GPU/driver combinations, so Settings →
    // Renderer can switch it off, and ROMULUS_RENDERER=compatible (e.g. in Steam's launch
    // options) always forces the safe path as a way back when nothing is visible.
    let forced_compat = std::env::var("ROMULUS_RENDERER").is_ok_and(|v| v == "compatible");
    let fast =
        !forced_compat && (config.fast_renderer || std::env::var_os("ROMULUS_DMABUF").is_some());
    tracing::info!(
        "renderer: {}",
        if fast { "fast (DMA-BUF)" } else { "compatible" }
    );
    // Set or clear explicitly: a copy started by the self-updater may have inherited the
    // previous copy's value.
    if fast {
        // SAFETY: called before any other thread is started.
        unsafe { std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER") };
    } else {
        // SAFETY: called before any other thread is started.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }

    let state = AppState::new(paths, config);
    state.save_config();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .setup(|app| {
            let handle = app.handle().clone();
            let st = app.state::<AppState>();
            let (fullscreen, web_gamepad, server) = {
                let c = lock(&st.config);
                (c.fullscreen, c.web_gamepad, c.server_url.clone())
            };
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_fullscreen(fullscreen);
            }
            st.input_paused
                .store(web_gamepad, std::sync::atomic::Ordering::Relaxed);
            input::spawn(handle.clone(), st.input_paused.clone());
            if let Some(server) = server {
                // Open the cached library before the UI asks for it.
                tauri::async_runtime::block_on(async {
                    if let Err(e) = st.open_session(&server).await {
                        tracing::error!("could not open library for {server}: {e}");
                    }
                });
                // Downloads that were waiting or running when Romulus closed.
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { games::resume_downloads(&h).await });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::set_server,
            commands::pair_start,
            commands::pair_poll,
            commands::password_login,
            commands::sign_out,
            commands::platforms,
            commands::roms,
            commands::rom,
            commands::search,
            commands::collection,
            commands::set_games_view,
            commands::platform_covers,
            commands::asset,
            commands::refresh_library,
            commands::check_update,
            commands::install_update,
            commands::restart_app,
            commands::whats_new,
            commands::settings,
            commands::update_settings,
            commands::debug_info,
            commands::open_keyboard,
            commands::add_to_steam,
            commands::log_ui,
            bios::bios_status,
            bios::download_bios,
            games::retrodeck_status,
            games::install_status,
            games::installed_ids,
            games::download_rom,
            games::cancel_download,
            games::downloads,
            games::remove_rom,
            games::play_rom,
            games::save_sync_status,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Romulus");
}

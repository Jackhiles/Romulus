use crate::app;
use crate::config::Config;
use crate::paths::Paths;
use crate::secrets::{Secrets, Store};
use library_cache::{Cache, CoverCache};
use romm_api::{Client, Credentials};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

/// A signed-in (or signing-in) connection to one RomM server and its local library.
pub struct Session {
    pub server: String,
    pub client: Client,
    pub cache: Arc<Mutex<Cache>>,
    pub covers: Arc<CoverCache>,
    pub signed_in: bool,
    pub store: Option<Store>,
}

pub struct AppState {
    pub paths: Paths,
    pub config: Mutex<Config>,
    pub secrets: Arc<Secrets>,
    pub session: RwLock<Option<Session>>,
    pub input_paused: Arc<AtomicBool>,
    pub refreshing: AtomicBool,
    pub pending_update: Mutex<Option<updater::Release>>,
    /// Cancel flags of running downloads, by ROM id.
    pub downloads: Mutex<std::collections::HashMap<i64, Arc<AtomicBool>>>,
    /// A game is running.
    pub playing: AtomicBool,
    /// Downloads waiting their turn, in order.
    pub queue: Mutex<std::collections::VecDeque<i64>>,
    /// A download worker is running.
    pub download_worker: AtomicBool,
    /// The running download: (rom id, bytes done, total).
    pub active_download: Mutex<Option<(i64, u64, u64)>>,
    /// Plain HTTP client for GitHub (updater).
    pub http: reqwest::Client,
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(paths: Paths, config: Config) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(app::user_agent())
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            secrets: Arc::new(Secrets::new(&paths.config_dir)),
            paths,
            config: Mutex::new(config),
            session: RwLock::new(None),
            input_paused: Arc::new(AtomicBool::new(false)),
            refreshing: AtomicBool::new(false),
            pending_update: Mutex::new(None),
            downloads: Mutex::new(Default::default()),
            playing: AtomicBool::new(false),
            queue: Mutex::new(Default::default()),
            download_worker: AtomicBool::new(false),
            active_download: Mutex::new(None),
            http,
        }
    }

    pub fn save_config(&self) {
        let cfg = lock(&self.config).clone();
        if let Err(e) = cfg.save(&self.paths.config_file()) {
            tracing::error!("could not save settings: {e}");
        }
    }

    /// Open the library and client for `server` (normalized URL), with stored credentials
    /// if there are any.
    pub async fn open_session(&self, server: &str) -> Result<(), String> {
        let base = romm_api::normalize_server_url(server).map_err(|e| e.to_string())?;
        let server = base.to_string();
        let secrets = self.secrets.clone();
        let key = server.clone();
        let client = Client::new(base, &app::user_agent())
            .map_err(|e| e.to_string())?
            .with_credentials_listener(Arc::new(move |c: &Credentials| {
                if let Err(e) = secrets.save(&key, c) {
                    tracing::error!("could not store refreshed credentials: {e}");
                }
            }));
        let stored = self.secrets.load(&server);
        let signed_in = stored.is_some();
        let store = stored.as_ref().map(|s| s.1);
        client.set_credentials(stored.map(|s| s.0)).await;

        let db = self.paths.library_db(&server);
        let cache = tokio::task::spawn_blocking(move || Cache::open(&db))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("could not open the library cache: {e}"))?;
        let max = lock(&self.config).cover_cache_mb * 1024 * 1024;
        let covers = CoverCache::new(self.paths.covers_dir(), max);
        *self.session.write().await = Some(Session {
            server,
            client,
            cache: Arc::new(Mutex::new(cache)),
            covers: Arc::new(covers),
            signed_in,
            store,
        });
        Ok(())
    }

    /// Store new credentials for the current session and mark it signed in.
    pub async fn sign_in(&self, creds: Credentials) -> Result<Store, String> {
        let mut guard = self.session.write().await;
        let s = guard.as_mut().ok_or("no server set")?;
        let store = self
            .secrets
            .save(&s.server, &creds)
            .map_err(|e| format!("could not store credentials: {e}"))?;
        s.client.set_credentials(Some(creds)).await;
        s.signed_in = true;
        s.store = Some(store);
        Ok(store)
    }
}

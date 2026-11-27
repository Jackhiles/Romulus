//! Client for the RomM REST API.
//!
//! Every path used here exists in `docs/reference/romm-openapi-5.4.1.json`; the
//! `paths_exist_in_openapi_snapshot` test checks that. See `docs/ROMM_API.md`.
//!
//! Auth: either a long-lived client token from device pairing (`rmm_...`, sent as a
//! Bearer token), or OAuth access/refresh tokens from the password grant. The client
//! refreshes an expired OAuth access token once and retries the request.

mod auth;
mod collections;
mod error;
mod files;
mod firmware;
mod saves;
mod states;
mod types;
mod user;

pub use auth::{Credentials, DEFAULT_SCOPES, DeviceAuthInit, DeviceInfo, DevicePoll};
pub use collections::{Collection, FAVOURITES_NAME};
pub use error::{Error, Result};
pub use files::{Download, PlaySession, RomFile, RomFiles};
pub use firmware::Firmware;
pub use saves::{SaveUpload, ServerSave};
pub use states::{ServerState, strip_time_tag};
pub use types::*;
pub use user::{EarnedAchievement, RaGameProgress, RaProgression};

use reqwest::{Method, RequestBuilder, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use url::Url;

/// Paths this client calls (for the OpenAPI snapshot test).
pub const USED_PATHS: &[&str] = &[
    "/api/heartbeat",
    "/api/token",
    "/api/auth/device/init",
    "/api/auth/device/token",
    "/api/platforms",
    "/api/platforms/identifiers",
    "/api/roms",
    "/api/roms/identifiers",
    "/api/roms/{id}",
    "/api/roms/{id}/files/content/{file_name}",
    "/api/roms/{id}/props",
    "/api/play-sessions",
    "/api/saves",
    "/api/firmware",
    "/api/users/me",
    "/api/firmware/{id}/content/{file_name}",
    "/api/saves/{id}/content",
    "/api/saves/{id}/downloaded",
    "/api/collections",
    "/api/collections/smart",
    "/api/collections/{id}/roms",
    "/api/states",
    "/api/states/{id}/content",
];

/// Called when the client gets new credentials (after a refresh), so they can be stored.
pub type CredentialsListener = Arc<dyn Fn(&Credentials) + Send + Sync>;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    creds: Arc<RwLock<Option<Credentials>>>,
    on_credentials: Option<CredentialsListener>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

/// Turn what the user typed ("romm.example.net", "https://romm.example.net/") into a base URL.
pub fn normalize_server_url(input: &str) -> Result<Url> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return Err(Error::InvalidUrl("empty".into()));
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|e| Error::InvalidUrl(e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::InvalidUrl(input.to_string()));
    }
    // Keep any sub-path (RomM behind a path prefix) and make joins append to it.
    if !url.path().ends_with('/') {
        let p = format!("{}/", url.path());
        url.set_path(&p);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

impl Client {
    pub fn new(base: Url, user_agent: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(15))
            // RomM builds a whole page before it sends anything, and a busy server can
            // take over a minute for one; the sync retries and resumes on its own.
            .read_timeout(Duration::from_secs(180))
            .gzip(true)
            .build()?;
        Ok(Self {
            http,
            base,
            creds: Arc::new(RwLock::new(None)),
            on_credentials: None,
        })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// The underlying HTTP client, for downloads that need streaming.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn with_credentials_listener(mut self, f: CredentialsListener) -> Self {
        self.on_credentials = Some(f);
        self
    }

    pub async fn set_credentials(&self, creds: Option<Credentials>) {
        *self.creds.write().await = creds;
    }

    pub async fn credentials(&self) -> Option<Credentials> {
        self.creds.read().await.clone()
    }

    /// Absolute URL for an API path ("/api/roms") or a server-relative asset path.
    pub fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path.trim_start_matches('/'))
            .map_err(|e| Error::InvalidUrl(e.to_string()))
    }

    fn request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        Ok(self.http.request(method, self.url(path)?))
    }

    async fn authorize(&self, rb: RequestBuilder) -> RequestBuilder {
        match self.creds.read().await.as_ref() {
            Some(c) => rb.bearer_auth(c.access_token()),
            None => rb,
        }
    }

    /// Send an authenticated request. On 401 with refreshable credentials, refresh once
    /// and retry.
    async fn send(&self, build: impl Fn() -> Result<RequestBuilder>) -> Result<reqwest::Response> {
        let resp = self.authorize(build()?).await.send().await?;
        if resp.status() != StatusCode::UNAUTHORIZED {
            return error::check(resp).await;
        }
        if !self.try_refresh().await? {
            return Err(Error::Unauthorized);
        }
        let resp = self.authorize(build()?).await.send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized);
        }
        error::check(resp).await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let resp = self
            .send(|| Ok(self.request(Method::GET, path)?.query(query)))
            .await?;
        let bytes = resp.bytes().await?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(format!("{path}: {e}")))
    }

    // ---- server ------------------------------------------------------------------------

    /// Server version and public configuration. Needs no auth.
    pub async fn heartbeat(&self) -> Result<Heartbeat> {
        let resp = self.request(Method::GET, "/api/heartbeat")?.send().await?;
        let resp = error::check(resp).await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }

    // ---- library -----------------------------------------------------------------------

    pub async fn platforms(
        &self,
        updated_after: Option<time::OffsetDateTime>,
    ) -> Result<Vec<Platform>> {
        let mut q = Vec::new();
        if let Some(t) = updated_after {
            q.push(("updated_after", types::format_time(t)));
        }
        self.get_json("/api/platforms", &q).await
    }

    pub async fn platform_ids(&self) -> Result<Vec<i64>> {
        self.get_json("/api/platforms/identifiers", &[]).await
    }

    pub async fn rom_ids(&self) -> Result<Vec<i64>> {
        self.get_json("/api/roms/identifiers", &[]).await
    }

    pub async fn roms(&self, q: &RomQuery) -> Result<Page<Rom>> {
        self.get_json("/api/roms", &q.to_query()).await
    }

    /// Full detail for one ROM, as raw JSON (the detail schema is large and only parts of
    /// it are shown).
    pub async fn rom_detail(&self, id: i64) -> Result<serde_json::Value> {
        self.get_json(&format!("/api/roms/{id}"), &[]).await
    }

    /// One ROM, typed like a list item (the detail schema is a superset of it).
    pub async fn rom(&self, id: i64) -> Result<Rom> {
        self.get_json(&format!("/api/roms/{id}"), &[]).await
    }

    /// Fetch a server-relative asset (cover, screenshot) with auth.
    pub async fn fetch_asset(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self.send(|| self.request(Method::GET, path)).await?;
        Ok(resp.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        let u = |s| normalize_server_url(s).unwrap().to_string();
        assert_eq!(u("romm.example.net"), "https://romm.example.net/");
        assert_eq!(u("https://romm.example.net/"), "https://romm.example.net/");
        assert_eq!(u("http://192.168.1.5:8080"), "http://192.168.1.5:8080/");
        assert_eq!(u("https://example.net/romm"), "https://example.net/romm/");
        assert!(normalize_server_url("").is_err());
        assert!(normalize_server_url("ftp://x").is_err());
    }

    #[test]
    fn joins_paths_under_prefix() {
        let c = Client::new(
            normalize_server_url("https://example.net/romm").unwrap(),
            "t",
        )
        .unwrap();
        assert_eq!(
            c.url("/api/roms").unwrap().as_str(),
            "https://example.net/romm/api/roms"
        );
    }

    #[test]
    fn paths_exist_in_openapi_snapshot() {
        let spec = include_str!("../../../docs/reference/romm-openapi-5.4.1.json");
        let spec: serde_json::Value = serde_json::from_str(spec).unwrap();
        let paths = spec["paths"].as_object().unwrap();
        for p in USED_PATHS {
            assert!(
                paths.contains_key(*p),
                "{p} is not in the RomM OpenAPI snapshot"
            );
        }
    }
}

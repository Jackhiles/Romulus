use crate::{Client, Error, Result, error};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Scopes Romulus asks for. Read access to the library plus what Phases 2-3 need
/// (user-rom props for last played, assets for saves/states, devices for sync).
pub const DEFAULT_SCOPES: &[&str] = &[
    "me.read",
    "roms.read",
    "platforms.read",
    "assets.read",
    "assets.write",
    "firmware.read",
    "roms.user.read",
    "roms.user.write",
    "collections.read",
    "devices.read",
    "devices.write",
];

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credentials {
    /// Client token from device pairing. Long-lived; revoked from the RomM web UI.
    ClientToken {
        token: String,
        device_id: Option<String>,
        expires_at: Option<i64>,
    },
    /// OAuth tokens from the password grant.
    OAuth {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: i64,
    },
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credentials::ClientToken { device_id, .. } => f
                .debug_struct("ClientToken")
                .field("device_id", device_id)
                .finish_non_exhaustive(),
            Credentials::OAuth { expires_at, .. } => f
                .debug_struct("OAuth")
                .field("expires_at", expires_at)
                .finish_non_exhaustive(),
        }
    }
}

impl Credentials {
    pub fn access_token(&self) -> &str {
        match self {
            Credentials::ClientToken { token, .. } => token,
            Credentials::OAuth { access_token, .. } => access_token,
        }
    }

    pub fn device_id(&self) -> Option<&str> {
        match self {
            Credentials::ClientToken { device_id, .. } => device_id.as_deref(),
            Credentials::OAuth { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthInit {
    pub device_code: String,
    pub user_code: String,
    pub verification_path: String,
    pub verification_path_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DevicePoll {
    Pending,
    SlowDown,
    Approved(Credentials),
    Denied,
    Expired,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires: i64,
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    device_id: Option<String>,
    expires_at: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceInfo<'a> {
    pub client_device_identifier: &'a str,
    pub name: &'a str,
    pub client: &'a str,
    pub platform: &'a str,
    pub client_version: &'a str,
}

impl Client {
    /// Start device pairing. Show `user_code` and `verification_url()` to the user, then
    /// poll [`Client::device_auth_poll`] every `interval` seconds.
    pub async fn device_auth_init(&self, info: &DeviceInfo<'_>) -> Result<DeviceAuthInit> {
        let body = serde_json::json!({
            "client_device_identifier": info.client_device_identifier,
            "name": info.name,
            "client": info.client,
            "platform": info.platform,
            "client_version": info.client_version,
            "requested_scopes": DEFAULT_SCOPES,
        });
        let resp = self
            .request(Method::POST, "/api/auth/device/init")?
            .json(&body)
            .send()
            .await?;
        let resp = error::check(resp).await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }

    /// Where the user approves the pairing (the RomM web UI).
    pub fn verification_url(&self, init: &DeviceAuthInit) -> String {
        self.url(&init.verification_path_complete)
            .map(|u| u.to_string())
            .unwrap_or_default()
    }

    pub async fn device_auth_poll(&self, device_code: &str) -> Result<DevicePoll> {
        let resp = self
            .request(Method::POST, "/api/auth/device/token")?
            .json(&serde_json::json!({ "device_code": device_code }))
            .send()
            .await?;
        match error::check(resp).await {
            Ok(resp) => {
                let t: DeviceTokenResponse = resp
                    .json()
                    .await
                    .map_err(|e| Error::Decode(e.to_string()))?;
                let expires_at = t
                    .expires_at
                    .and_then(|s| crate::types::parse_time(&s))
                    .map(|t| t.unix_timestamp());
                Ok(DevicePoll::Approved(Credentials::ClientToken {
                    token: t.access_token,
                    device_id: t.device_id,
                    expires_at,
                }))
            }
            Err(e) => match e.detail() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                Some("slow_down") => Ok(DevicePoll::SlowDown),
                Some("access_denied") => Ok(DevicePoll::Denied),
                Some("expired_token") => Ok(DevicePoll::Expired),
                _ => Err(e),
            },
        }
    }

    /// Username/password sign-in (OAuth password grant). Stores the credentials on success.
    pub async fn password_login(&self, username: &str, password: &str) -> Result<Credentials> {
        let scope = DEFAULT_SCOPES.join(" ");
        let form = [
            ("grant_type", "password"),
            ("username", username),
            ("password", password),
            ("scope", scope.as_str()),
        ];
        let creds = self.token_request(&form, None).await?;
        self.set_credentials(Some(creds.clone())).await;
        Ok(creds)
    }

    async fn token_request(
        &self,
        form: &[(&str, &str)],
        old_refresh: Option<String>,
    ) -> Result<Credentials> {
        let resp = self
            .request(Method::POST, "/api/token")?
            .form(form)
            .send()
            .await?;
        let resp = error::check(resp).await?;
        let t: TokenResponse = resp
            .json()
            .await
            .map_err(|e| Error::Decode(e.to_string()))?;
        Ok(Credentials::OAuth {
            access_token: t.access_token,
            refresh_token: t.refresh_token.or(old_refresh),
            expires_at: OffsetDateTime::now_utc().unix_timestamp() + t.expires,
        })
    }

    /// Refresh OAuth credentials. Returns false when there is nothing to refresh with.
    pub(crate) async fn try_refresh(&self) -> Result<bool> {
        let refresh = match self.creds.read().await.as_ref() {
            Some(Credentials::OAuth {
                refresh_token: Some(r),
                ..
            }) => r.clone(),
            _ => return Ok(false),
        };
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ];
        let creds = match self.token_request(&form, Some(refresh.clone())).await {
            Ok(c) => c,
            Err(Error::Unauthorized) | Err(Error::Http { status: 400, .. }) => return Ok(false),
            Err(e) => return Err(e),
        };
        tracing::info!("refreshed RomM access token");
        self.set_credentials(Some(creds.clone())).await;
        if let Some(f) = &self.on_credentials {
            f(&creds);
        }
        Ok(true)
    }
}

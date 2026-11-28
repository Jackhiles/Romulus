//! ROM files, downloads, and reporting play.

use crate::{Client, Error, Result};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// One file of a ROM (`RomFileSchema`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RomFile {
    pub id: i64,
    pub file_name: String,
    /// Folder relative to the library root, e.g. `roms/ps2/Katamari Damacy (USA)`.
    pub file_path: String,
    #[serde(default)]
    pub file_size_bytes: u64,
    #[serde(default)]
    pub is_top_level: bool,
    /// game, dlc, update, soundtrack, manual, ... (RomFileCategory).
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sha1_hash: Option<String>,
}

/// The parts of a ROM's detail record needed to install it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomFiles {
    pub id: i64,
    pub fs_name: String,
    /// `roms/<platform fs_slug>`.
    pub fs_path: String,
    pub platform_fs_slug: String,
    #[serde(default)]
    pub has_simple_single_file: bool,
    #[serde(default)]
    pub files: Vec<RomFile>,
}

/// A response body to stream into a file.
pub struct Download {
    pub response: reqwest::Response,
    /// True when the server honoured the Range request and the body continues from the
    /// requested offset; false means the body is the whole file.
    pub partial: bool,
    /// Total size of the file, when the server said.
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaySession {
    pub rom_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end_time: OffsetDateTime,
    pub duration_ms: u64,
}

impl Client {
    /// A ROM with its file list.
    pub async fn rom_files(&self, id: i64) -> Result<RomFiles> {
        self.get_json(&format!("/api/roms/{id}"), &[]).await
    }

    /// Start downloading one file, from byte `from` (0 for the whole file).
    pub async fn download_file(&self, file: &RomFile, from: u64) -> Result<Download> {
        let mut url = self.url(&format!("/api/roms/{}/files/content/", file.id))?;
        // The name is one path segment, percent-encoded (names contain spaces, brackets
        // and sometimes '#' or '?').
        url.path_segments_mut()
            .map_err(|_| Error::InvalidUrl("cannot be a base".into()))?
            .pop_if_empty()
            .push(&file.file_name);
        let response = self
            .send(|| {
                let rb = self.http().request(Method::GET, url.clone());
                Ok(if from > 0 {
                    rb.header(RANGE, format!("bytes={from}-"))
                } else {
                    rb
                })
            })
            .await?;
        let partial = response.status() == StatusCode::PARTIAL_CONTENT;
        let total = if partial {
            // "bytes 100-999/1000"
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|v| v.parse().ok())
        } else {
            response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
        };
        Ok(Download {
            response,
            partial,
            total,
        })
    }

    /// Set the ROM's "last played" to now for this user.
    pub async fn mark_played(&self, rom_id: i64) -> Result<()> {
        self.send(|| {
            Ok(self
                .request(Method::PUT, &format!("/api/roms/{rom_id}/props"))?
                .query(&[("update_last_played", "true")])
                .json(&serde_json::json!({})))
        })
        .await?;
        Ok(())
    }

    /// Change this user's data for a ROM (`RomUserData`: `now_playing`, `backlogged`,
    /// `status`, `rating` 0-10, ...). Only the fields in `patch` change; `"status": null`
    /// clears the status. Returns the new `rom_user` (`RomUserSchema`).
    pub async fn update_rom_user(
        &self,
        rom_id: i64,
        patch: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resp = self
            .send(|| {
                Ok(self
                    .request(Method::PUT, &format!("/api/roms/{rom_id}/props"))?
                    .json(patch))
            })
            .await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }

    /// Report finished play sessions.
    pub async fn report_play_sessions(
        &self,
        device_id: Option<&str>,
        sessions: &[PlaySession],
    ) -> Result<()> {
        let body = serde_json::json!({ "device_id": device_id, "sessions": sessions });
        self.send(|| {
            Ok(self
                .request(Method::POST, "/api/play-sessions")?
                .json(&body))
        })
        .await?;
        Ok(())
    }
}

//! Save files on the server (`/api/saves`).
//!
//! RomM keeps versions per slot: an upload with a slot gets a timestamp tag in its file
//! name, identical content is recognised by hash instead of stored twice, and an upload
//! from a device is refused with 409 when another device uploaded to the slot since
//! this one last synced.

use crate::{Client, Error, Result};
use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerSave {
    pub id: i64,
    pub rom_id: i64,
    pub file_name: String,
    #[serde(default)]
    pub emulator: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
    /// md5 of the file, or for a zip, md5 of its sorted "name:md5" entry lines.
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub file_size_bytes: u64,
    pub updated_at: String,
    #[serde(default)]
    pub origin_device_id: Option<String>,
}

pub struct SaveUpload<'a> {
    pub rom_id: i64,
    pub emulator: &'a str,
    pub slot: &'a str,
    pub device_id: Option<&'a str>,
    pub content_hash: &'a str,
    pub file_name: &'a str,
    pub bytes: Vec<u8>,
    /// Replace even if another device uploaded since our last sync.
    pub overwrite: bool,
}

impl Error {
    /// The server refused an upload because the slot changed since this device synced.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::Http { status: 409, .. })
    }
}

impl Client {
    pub async fn saves(&self, rom_id: i64, device_id: Option<&str>) -> Result<Vec<ServerSave>> {
        let mut q = vec![("rom_id", rom_id.to_string())];
        if let Some(d) = device_id {
            q.push(("device_id", d.to_string()));
        }
        self.get_json("/api/saves", &q).await
    }

    /// Download a save. With a device id the server records that this device got it.
    pub async fn download_save(&self, id: i64, device_id: Option<&str>) -> Result<Vec<u8>> {
        let path = format!("/api/saves/{id}/content");
        let resp = self
            .send(|| {
                let rb = self.request(Method::GET, &path)?;
                Ok(match device_id {
                    Some(d) => rb.query(&[("device_id", d), ("optimistic", "true")]),
                    None => rb,
                })
            })
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Tell the server the download arrived intact (completes its sync record).
    pub async fn confirm_save_download(
        &self,
        id: i64,
        device_id: &str,
        content_hash: &str,
    ) -> Result<()> {
        let body = serde_json::json!({ "device_id": device_id, "content_hash": content_hash });
        self.send(|| {
            Ok(self
                .request(Method::POST, &format!("/api/saves/{id}/downloaded"))?
                .json(&body))
        })
        .await?;
        Ok(())
    }

    /// Upload a save into a slot (a new version). 409 → [`Error::is_conflict`].
    pub async fn upload_save(&self, up: &SaveUpload<'_>) -> Result<ServerSave> {
        let mut q = vec![
            ("rom_id", up.rom_id.to_string()),
            ("emulator", up.emulator.to_string()),
            ("slot", up.slot.to_string()),
            ("content_hash", up.content_hash.to_string()),
            ("overwrite", up.overwrite.to_string()),
            ("autocleanup", "true".to_string()),
            ("autocleanup_limit", "10".to_string()),
        ];
        if let Some(d) = up.device_id {
            q.push(("device_id", d.to_string()));
        }
        let resp = self
            .send(|| {
                let part = Part::bytes(up.bytes.clone())
                    .file_name(up.file_name.to_string())
                    .mime_str("application/octet-stream")?;
                Ok(self
                    .request(Method::POST, "/api/saves")?
                    .query(&q)
                    .multipart(Form::new().part("saveFile", part)))
            })
            .await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }
}

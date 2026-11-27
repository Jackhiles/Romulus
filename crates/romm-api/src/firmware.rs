//! BIOS / firmware files on the server (`/api/firmware`).

use crate::{Client, Error, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Firmware {
    pub id: i64,
    pub platform_id: i64,
    pub file_name: String,
    #[serde(default)]
    pub file_size_bytes: u64,
    /// RomM matched it against its list of known good dumps.
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub md5_hash: Option<String>,
    #[serde(default)]
    pub sha1_hash: Option<String>,
}

impl Client {
    /// All firmware RomM has (optionally for one platform).
    pub async fn firmware(&self, platform_id: Option<i64>) -> Result<Vec<Firmware>> {
        let q: Vec<(&str, String)> = platform_id
            .map(|p| ("platform_id", p.to_string()))
            .into_iter()
            .collect();
        self.get_json("/api/firmware", &q).await
    }

    pub async fn download_firmware(&self, fw: &Firmware) -> Result<Vec<u8>> {
        let mut url = self.url(&format!("/api/firmware/{}/content/", fw.id))?;
        url.path_segments_mut()
            .map_err(|_| Error::InvalidUrl("cannot be a base".into()))?
            .pop_if_empty()
            .push(&fw.file_name);
        let resp = self
            .send(|| Ok(self.http().request(Method::GET, url.clone())))
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }
}

//! Save states on the server (`/api/states`).
//!
//! Each upload becomes a new state whose file name gets a timestamp tag
//! (`Game [2026-01-15 23-09-20-955].state`); older ones stay.

use crate::{Client, Error, Result};
use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerState {
    pub id: i64,
    pub rom_id: i64,
    pub file_name: String,
    #[serde(default)]
    pub emulator: Option<String>,
    #[serde(default)]
    pub file_size_bytes: u64,
    pub updated_at: String,
}

impl ServerState {
    /// The file name as uploaded: without RomM's timestamp tag.
    pub fn base_name(&self) -> String {
        strip_time_tag(&self.file_name)
    }
}

/// Remove a ` [YYYY-MM-DD HH-MM-SS-mmm]` tag (RomM's upload time) from a file name.
/// Other bracketed tags (`[T-En]`) are part of the name and stay.
pub fn strip_time_tag(name: &str) -> String {
    const SHAPE: &str = " [dddd-dd-dd dd-dd-dd-ddd]";
    let b = name.as_bytes();
    let s = SHAPE.as_bytes();
    let mut i = 0;
    while i + s.len() <= b.len() {
        let fits = s.iter().zip(&b[i..]).all(|(p, c)| match p {
            b'd' => c.is_ascii_digit(),
            p => p == c,
        });
        if fits {
            return format!("{}{}", &name[..i], &name[i + s.len()..]);
        }
        i += 1;
    }
    name.to_string()
}

impl Client {
    pub async fn states(&self, rom_id: i64) -> Result<Vec<ServerState>> {
        self.get_json("/api/states", &[("rom_id", rom_id.to_string())])
            .await
    }

    pub async fn download_state(&self, id: i64) -> Result<Vec<u8>> {
        let path = format!("/api/states/{id}/content");
        let resp = self.send(|| self.request(Method::GET, &path)).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn upload_state(
        &self,
        rom_id: i64,
        emulator: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<ServerState> {
        let q = [
            ("rom_id", rom_id.to_string()),
            ("emulator", emulator.to_string()),
        ];
        let resp = self
            .send(|| {
                let part = Part::bytes(bytes.clone())
                    .file_name(file_name.to_string())
                    .mime_str("application/octet-stream")?;
                Ok(self
                    .request(Method::POST, "/api/states")?
                    .query(&q)
                    .multipart(Form::new().part("stateFile", part)))
            })
            .await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_time_tag() {
        assert_eq!(
            strip_time_tag("Parasite eve [2026-01-15 23-09-20-955].state"),
            "Parasite eve.state"
        );
        assert_eq!(
            strip_time_tag("Game [T-En] [2026-01-15 23-09-20-955].state1"),
            "Game [T-En].state1"
        );
        assert_eq!(strip_time_tag("Game [T-En].state"), "Game [T-En].state");
        assert_eq!(strip_time_tag("Ünïcode.state"), "Ünïcode.state");
    }
}

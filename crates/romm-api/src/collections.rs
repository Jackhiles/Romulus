//! Collections (`/api/collections`): the user's own lists, smart (rule-based) ones, and
//! Favourites, which is a collection with `is_favorite`.

use crate::{Client, Error, Result};
use reqwest::Method;
use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub rom_ids: Vec<i64>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub is_smart: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The name RomM's web interface gives the favourites collection.
pub const FAVOURITES_NAME: &str = "Favourites";

impl Client {
    pub async fn collections(&self) -> Result<Vec<Collection>> {
        self.get_json("/api/collections", &[]).await
    }

    pub async fn smart_collections(&self) -> Result<Vec<Collection>> {
        let mut list: Vec<Collection> = self.get_json("/api/collections/smart", &[]).await?;
        for c in &mut list {
            c.is_smart = true;
        }
        Ok(list)
    }

    /// Create the favourites collection (when the user has none yet).
    pub async fn create_favourites(&self) -> Result<Collection> {
        let resp = self
            .send(|| {
                Ok(self
                    .request(Method::POST, "/api/collections")?
                    .query(&[("is_favorite", "true"), ("is_public", "false")])
                    .multipart(Form::new().text("name", FAVOURITES_NAME)))
            })
            .await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }

    /// Add (`add`) or remove ROMs from a collection; returns the updated collection.
    pub async fn collection_roms(&self, id: i64, rom_ids: &[i64], add: bool) -> Result<Collection> {
        let body = serde_json::json!({ "rom_ids": rom_ids });
        let method = if add { Method::POST } else { Method::DELETE };
        let path = format!("/api/collections/{id}/roms");
        let resp = self
            .send(|| Ok(self.request(method.clone(), &path)?.json(&body)))
            .await?;
        resp.json().await.map_err(|e| Error::Decode(e.to_string()))
    }
}

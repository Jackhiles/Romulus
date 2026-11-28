//! Local SQLite cache of one RomM server's library.
//!
//! The UI reads only from here. [`refresh`] brings it up to date with the server using
//! `updated_after` for changed rows and the identifier lists for deletions, so a refresh
//! with no changes costs four small requests.

mod covers;
mod db;
pub mod group;
mod refresh;

pub use covers::CoverCache;
pub use db::{Cache, CollectionRow, DiscoverPool, PlatformRow, QueuedSession, RomRow, SortOrder};
pub use refresh::{RefreshProgress, RefreshStats, refresh};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("RomM: {0}")]
    Api(#[from] romm_api::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    #[error("data: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

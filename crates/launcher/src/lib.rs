//! Getting games from RomM onto the device and running them through RetroDECK.
//!
//! - [`retrodeck`]: where RetroDECK keeps ROMs, BIOS, saves and states.
//! - [`systems`]: which RetroDECK folder a RomM platform goes in.
//! - [`plan`]: which files of a ROM to download, where to, and which one to launch.
//! - [`install`]: downloading (resumable, checked) and removing installs.
//! - [`play`]: starting a game and timing the session.
//!
//! Nothing here touches save data: installs only ever create and remove the ROM files
//! they downloaded, inside the ROM folder.

pub mod install;
pub mod plan;
pub mod play;
pub mod retrodeck;
pub mod systems;

pub use install::{Installed, install, remove};
pub use plan::{Plan, PlannedFile, plan};
pub use play::{Session, play};
pub use retrodeck::RetroDeck;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("RetroDECK was not found on this device")]
    NoRetroDeck,
    #[error("RetroDECK has no folder for {0}")]
    NoSystemFolder(String),
    #[error("RomM sent an unexpected file layout: {0}")]
    BadLayout(String),
    #[error("not enough free space: {need_mb} MB needed, {free_mb} MB free")]
    NoSpace { need_mb: u64, free_mb: u64 },
    #[error("download cancelled")]
    Cancelled,
    #[error("{0} is damaged (checksum or size mismatch); try again")]
    Corrupt(String),
    #[error("RomM: {0}")]
    Api(#[from] romm_api::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not start RetroDECK: {0}")]
    Launch(String),
}

pub type Result<T> = std::result::Result<T, Error>;

//! Save sync between the Deck (RetroDECK's emulators) and RomM.
//!
//! - [`engine`]: the pure decision (upload, download, conflict) with table tests.
//! - [`emulator`]: which emulator RetroDECK uses for a game.
//! - [`layout`]: where that emulator keeps the game's save.
//! - [`local`]: reading, packing, backing up and applying the save on the device.
//! - [`run`]: syncing before and after a play session.
//! - [`duckstation`]: which memory card file DuckStation uses for a game.
//! - [`disc`]: GameCube and Wii disc IDs, for Dolphin's save names.
//! - [`states`]: save state files, synced one by one.
//!
//! Saves on the device are never overwritten or removed without a backup first.

pub mod disc;
pub mod duckstation;
pub mod emulator;
pub mod engine;
pub mod layout;
pub mod local;
pub mod run;
pub mod states;

pub use engine::Record;
pub use run::{Choice, ConflictInfo, Ctx, Outcome, after, before, changed_locally};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("RomM: {0}")]
    Api(#[from] romm_api::Error),
    #[error("save file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the save changed while syncing; try again")]
    Inconsistent,
}

pub type Result<T> = std::result::Result<T, Error>;

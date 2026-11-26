//! The app's name and identity, in one place so a rename touches only this file and
//! `ui/src/lib/app.ts` (plus `productName` in tauri.conf.json).

/// Display name.
pub const APP_NAME: &str = "Romulus";
/// Lowercase id used for folders, the keyring service and log files.
pub const APP_ID: &str = "romulus";
/// GitHub `owner/name` that publishes releases for the updater.
pub const RELEASE_REPO: &str = "Jackhiles/Romulus";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn user_agent() -> String {
    format!("{APP_NAME}/{VERSION}")
}

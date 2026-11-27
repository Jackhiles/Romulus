//! Starting a game through RetroDECK and timing it.
//!
//! `flatpak run net.retrodeck.retrodeck <file>` stays in the foreground until the
//! emulator exits, so the session is the lifetime of that process. RetroDECK works out
//! the system from the `roms/<system>/` part of the path and picks the emulator set in
//! ES-DE for that game or system.

use crate::retrodeck::FLATPAK_ID;
use crate::{Error, Result};
use serde::Serialize;
use std::path::Path;
use std::time::Instant;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    pub duration_ms: u64,
    /// The emulator's exit code (None if it was killed by a signal).
    pub exit_code: Option<i32>,
}

/// Play `game` with RetroDECK and wait until it exits.
pub async fn play(game: &Path) -> Result<Session> {
    let game = game.to_string_lossy().into_owned();
    run("flatpak", &["run", FLATPAK_ID, &game]).await
}

/// Run `program args` and time it.
pub async fn run(program: &str, args: &[&str]) -> Result<Session> {
    let start = OffsetDateTime::now_utc();
    let clock = Instant::now();
    tracing::info!("starting {program} {args:?}");
    let status = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(false)
        .status()
        .await
        .map_err(|e| Error::Launch(e.to_string()))?;
    let duration_ms = clock.elapsed().as_millis() as u64;
    tracing::info!(
        "{program} exited with {status} after {} s",
        duration_ms / 1000
    );
    Ok(Session {
        start,
        end: start + time::Duration::milliseconds(duration_ms as i64),
        duration_ms,
        exit_code: status.code(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn times_the_process() {
        let s = run("sh", &["-c", "sleep 0.2; exit 3"]).await.unwrap();
        assert!(s.duration_ms >= 200, "{}", s.duration_ms);
        assert_eq!(s.exit_code, Some(3));
        assert!(s.end > s.start);
    }

    #[tokio::test]
    async fn missing_program_is_a_launch_error() {
        assert!(matches!(
            run("romulus-no-such-program", &[]).await,
            Err(Error::Launch(_))
        ));
    }
}

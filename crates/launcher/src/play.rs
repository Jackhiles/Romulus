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
    /// The last lines it wrote to stderr.
    pub stderr_tail: String,
}

impl Session {
    /// Exited with an error almost at once: the game never really started.
    pub fn failed_to_start(&self) -> bool {
        self.duration_ms < 10_000 && self.exit_code.is_some_and(|c| c != 0)
    }
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
    let mut cmd = crate::env::host_command(program);
    cmd.args(args);
    let mut child = tokio::process::Command::from(cmd)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| Error::Launch(e.to_string()))?;
    // Keep the last lines of error output (read continuously, so the pipe never fills).
    let tail = tokio::spawn({
        let stderr = child.stderr.take();
        async move {
            use tokio::io::AsyncBufReadExt;
            let mut last: std::collections::VecDeque<String> = Default::default();
            if let Some(s) = stderr {
                let mut lines = tokio::io::BufReader::new(s).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    // Steam's 32-bit overlay can't load into 64-bit programs; harmless noise.
                    if l.contains("gameoverlayrenderer.so") {
                        continue;
                    }
                    if last.len() == 12 {
                        last.pop_front();
                    }
                    last.push_back(l);
                }
            }
            last.into_iter().collect::<Vec<_>>().join(
                "
",
            )
        }
    });
    let status = child
        .wait()
        .await
        .map_err(|e| Error::Launch(e.to_string()))?;
    let stderr_tail = tail.await.unwrap_or_default();
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
        stderr_tail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn times_the_process() {
        let s = run(
            "sh",
            &["-c", "echo noise >&2; echo boom >&2; sleep 0.2; exit 3"],
        )
        .await
        .unwrap();
        assert!(s.duration_ms >= 200, "{}", s.duration_ms);
        assert_eq!(s.exit_code, Some(3));
        assert!(s.end > s.start);
        assert_eq!(
            s.stderr_tail,
            "noise
boom"
        );
        assert!(s.failed_to_start());
    }

    #[tokio::test]
    async fn missing_program_is_a_launch_error() {
        assert!(matches!(
            run("romulus-no-such-program", &[]).await,
            Err(Error::Launch(_))
        ));
    }
}

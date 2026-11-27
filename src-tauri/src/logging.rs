//! File logging with levels. Logs go to `~/.local/state/romulus/logs/romulus.log.<date>`,
//! one file per day, the last 7 kept. Also mirrored to stderr for development.

use crate::app::APP_ID;
use std::path::{Path, PathBuf};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init(log_dir: &Path, level: &str) -> Option<WorkerGuard> {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("{APP_ID}.log"))
        .max_log_files(7)
        .build(log_dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // RUST_LOG overrides the configured level.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("warn,romulus={level},romulus_lib={level},romm_api={level},library_cache={level},updater={level},launcher={level},ui={level}")));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(writer).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr))
        .try_init();
    Some(guard)
}

/// The newest log file.
pub fn current_log(log_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(log_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(APP_ID))
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

/// Last `n` lines of the newest log, with anything that looks like a token masked.
pub fn tail(log_dir: &Path, n: usize) -> String {
    let Some(text) = current_log(log_dir).and_then(|p| std::fs::read_to_string(p).ok()) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..]
        .iter()
        .map(|l| redact(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mask RomM client tokens and bearer tokens.
pub fn redact(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for (i, word) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
        if w.starts_with("rmm_") || (w.len() > 40 && w.matches('.').count() == 2) {
            out.push_str("[redacted]");
        } else {
            out.push_str(word);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens() {
        assert_eq!(redact("token rmm_abc123 ok"), "token [redacted] ok");
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJqYWNrIiwic2NvcGVzIjoiIn0.abcdefghijklmnop";
        assert_eq!(redact(&format!("Bearer {jwt}")), "Bearer [redacted]");
        assert_eq!(redact("fetched 3.2.1 items"), "fetched 3.2.1 items");
    }
}

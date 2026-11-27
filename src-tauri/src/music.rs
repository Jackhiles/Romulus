//! Music for the focused game. Sources, in order: a file in the user's music folder
//! named after the game, then the first soundtrack track RomM has for it (downloaded
//! once and cached). Copyrighted music is never fetched from anywhere else.

use crate::state::{AppState, lock};
use std::path::{Path, PathBuf};
use tauri::State;

const AUDIO: &[&str] = &["mp3", "ogg", "flac", "wav", "opus"];

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A track in `dir` whose name matches one of `names` (ignoring case, spaces and
/// punctuation).
pub fn user_track(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let wanted: Vec<String> = names
        .iter()
        .map(|n| norm(n))
        .filter(|n| !n.is_empty())
        .collect();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            AUDIO.contains(&ext.as_str())
                && p.file_stem()
                    .is_some_and(|s| wanted.contains(&norm(&s.to_string_lossy())))
        })
}

/// Play music for this game (keeps playing if it already is). Returns whether a track
/// was found.
#[tauri::command]
pub async fn preview_music(state: State<'_, AppState>, rom_id: i64) -> Result<bool, String> {
    if !lock(&state.config).music || state.playing.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(false);
    }
    if *lock(&state.music_for) == Some(rom_id) {
        return Ok(true);
    }
    let (client, cache) = {
        let s = state.session.read().await;
        let s = s.as_ref().ok_or("no server set")?;
        (s.client.clone(), s.cache.clone())
    };
    let Some((row, json)) = lock(&cache).rom(rom_id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    let music_dir = state.paths.data_dir.join("music");
    let stem = json["fs_name_no_ext"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mut track = user_track(&music_dir, &[&row.title, &stem]);

    if track.is_none() && json["has_soundtrack"].as_bool() == Some(true) {
        let files = client.rom_files(rom_id).await.map_err(|e| e.to_string())?;
        let mut st: Vec<_> = files
            .files
            .into_iter()
            .filter(|f| f.category.as_deref() == Some("soundtrack"))
            .collect();
        st.sort_by_key(|f| f.file_name.to_lowercase());
        if let Some(f) = st.into_iter().next() {
            let ext = Path::new(&f.file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mp3")
                .to_ascii_lowercase();
            let cached = state
                .paths
                .cache_dir
                .join("music")
                .join(format!("{}.{ext}", f.id));
            if !cached.is_file() {
                std::fs::create_dir_all(cached.parent().unwrap_or(Path::new(".")))
                    .map_err(|e| e.to_string())?;
                let dl = client
                    .download_file(&f, 0)
                    .await
                    .map_err(|e| e.to_string())?;
                let bytes = dl.response.bytes().await.map_err(|e| e.to_string())?;
                let tmp = cached.with_extension("part");
                std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &cached).map_err(|e| e.to_string())?;
            }
            track = Some(cached);
        }
    }
    // The focus may have moved on while downloading.
    match track {
        Some(t) => {
            *lock(&state.music_for) = Some(rom_id);
            state.music.play(t);
            Ok(true)
        }
        None => {
            stop(&state);
            Ok(false)
        }
    }
}

pub fn stop(state: &AppState) {
    if lock(&state.music_for).take().is_some() {
        state.music.stop();
    }
}

#[tauri::command]
pub fn stop_music(state: State<'_, AppState>) {
    stop(&state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_user_tracks_by_name() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Katamari Damacy.mp3"), b"x").unwrap();
        std::fs::write(d.path().join("notes.txt"), b"x").unwrap();
        assert!(user_track(d.path(), &["katamari damacy", "Katamari Damacy (USA)"]).is_some());
        assert!(user_track(d.path(), &["KATAMARI-DAMACY"]).is_some());
        assert!(user_track(d.path(), &["Notes"]).is_none());
        assert!(user_track(d.path(), &["Other"]).is_none());
    }
}

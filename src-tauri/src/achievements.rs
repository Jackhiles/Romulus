//! RetroAchievements for the detail page: the game's achievement list from RomM's
//! metadata (`merged_ra_metadata`, badges served by RomM), combined with the user's
//! progress from `/api/users/me` (cached at each library sync).

use crate::state::{AppState, lock};
use romm_api::RaProgression;
use serde::Serialize;
use tauri::State;

/// Cache meta key for the user's progress (JSON of `RaProgression`).
pub const META_KEY: &str = "ra_progression";

#[derive(Serialize, Debug, PartialEq)]
pub struct Achievement {
    title: String,
    description: String,
    points: u32,
    /// RomM asset path of the badge (locked version when not earned).
    badge: Option<String>,
    earned: bool,
    earned_at: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Achievements {
    total: usize,
    earned: usize,
    earned_hardcore: u32,
    points_total: u32,
    points_earned: u32,
    /// Earned first (newest first), then the rest in the game's order.
    list: Vec<Achievement>,
}

/// Combine a ROM's cached JSON with the user's progress. None when the game has no
/// RetroAchievements set.
pub fn build(rom: &serde_json::Value, progress: Option<&RaProgression>) -> Option<Achievements> {
    let ra_id = rom["ra_id"].as_i64()?;
    let items = rom["merged_ra_metadata"]["achievements"].as_array()?;
    if items.is_empty() {
        return None;
    }
    let game = progress.and_then(|p| p.results.iter().find(|g| g.rom_ra_id == ra_id));
    let earned_on = |badge: &str, ra: &str| {
        game.and_then(|g| {
            g.earned_achievements
                .iter()
                .find(|e| e.id == badge || e.id == ra)
        })
        .map(|e| e.date.clone())
    };
    let mut list: Vec<(i64, Achievement)> = items
        .iter()
        .map(|a| {
            let badge_id = a["badge_id"].as_str().unwrap_or_default();
            let ra = a["ra_id"]
                .as_i64()
                .map(|i| i.to_string())
                .unwrap_or_default();
            let date = earned_on(badge_id, &ra);
            let earned = date.is_some();
            let path = if earned {
                &a["badge_path"]
            } else {
                &a["badge_path_lock"]
            };
            (
                a["display_order"].as_i64().unwrap_or(0),
                Achievement {
                    title: a["title"].as_str().unwrap_or_default().to_string(),
                    description: a["description"].as_str().unwrap_or_default().to_string(),
                    points: a["points"].as_u64().unwrap_or(0) as u32,
                    badge: path
                        .as_str()
                        .filter(|p| p.starts_with('/'))
                        .map(str::to_string),
                    earned,
                    earned_at: date.flatten(),
                },
            )
        })
        .collect();
    list.sort_by(|(oa, a), (ob, b)| {
        b.earned
            .cmp(&a.earned)
            .then_with(|| b.earned_at.cmp(&a.earned_at))
            .then(oa.cmp(ob))
    });
    let list: Vec<Achievement> = list.into_iter().map(|(_, a)| a).collect();
    Some(Achievements {
        total: list.len(),
        earned: list.iter().filter(|a| a.earned).count(),
        earned_hardcore: game.map_or(0, |g| g.num_awarded_hardcore),
        points_total: list.iter().map(|a| a.points).sum(),
        points_earned: list.iter().filter(|a| a.earned).map(|a| a.points).sum(),
        list,
    })
}

#[tauri::command]
pub async fn achievements(
    state: State<'_, AppState>,
    rom_id: i64,
) -> Result<Option<Achievements>, String> {
    let cache = {
        let s = state.session.read().await;
        s.as_ref().ok_or("no server set")?.cache.clone()
    };
    let c = lock(&cache);
    let Some((_, rom)) = c.rom(rom_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let progress: Option<RaProgression> = c
        .meta(META_KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok());
    Ok(build(&rom, progress.as_ref()))
}

/// Fetch and cache the user's RetroAchievements progress (after a library sync).
pub async fn refresh_progress(
    client: &romm_api::Client,
    cache: &std::sync::Mutex<library_cache::Cache>,
) {
    match client.ra_progression().await {
        Ok(Some((_, p))) => {
            if let Ok(j) = serde_json::to_string(&p) {
                let _ = lock(cache).set_meta(META_KEY, &j);
            }
        }
        Ok(None) => {}
        Err(e) => tracing::info!("RetroAchievements progress not updated: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn combines_list_and_progress() {
        let rom = json!({"ra_id": 11244, "merged_ra_metadata": {"achievements": [
            {"ra_id": 1, "badge_id": "87936", "title": "First", "description": "a", "points": 5, "display_order": 1,
             "badge_path": "/b/87936.png", "badge_path_lock": "/b/87936_lock.png"},
            {"ra_id": 81392, "badge_id": "87992", "title": "Piece Be With You", "description": "b", "points": 10, "display_order": 2,
             "badge_path": "/b/87992.png", "badge_path_lock": "/b/87992_lock.png"}
        ]}});
        let p: RaProgression =
            serde_json::from_value(json!({"results": [{"rom_ra_id": 11244, "max_possible": 2,
            "num_awarded": 1, "num_awarded_hardcore": 0,
            "earned_achievements": [{"id": "87992", "date": "2026-10-09 02:54:14"}]}]}))
            .unwrap();
        let a = build(&rom, Some(&p)).unwrap();
        assert_eq!(
            (a.total, a.earned, a.points_total, a.points_earned),
            (2, 1, 15, 10)
        );
        assert_eq!(a.list[0].title, "Piece Be With You");
        assert_eq!(a.list[0].badge.as_deref(), Some("/b/87992.png"));
        assert_eq!(a.list[1].badge.as_deref(), Some("/b/87936_lock.png"));
        // Without progress everything is locked; without RA data there is nothing.
        assert_eq!(build(&rom, None).unwrap().earned, 0);
        assert_eq!(build(&json!({"ra_id": null}), Some(&p)), None);
    }
}

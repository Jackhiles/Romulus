//! The signed-in user (`/api/users/me`): RetroAchievements progress.

use crate::{Client, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EarnedAchievement {
    /// RomM reports the achievement's badge id here.
    pub id: String,
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RaGameProgress {
    pub rom_ra_id: i64,
    #[serde(default)]
    pub max_possible: u32,
    #[serde(default)]
    pub num_awarded: u32,
    #[serde(default)]
    pub num_awarded_hardcore: u32,
    #[serde(default)]
    pub earned_achievements: Vec<EarnedAchievement>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RaProgression {
    #[serde(default)]
    pub results: Vec<RaGameProgress>,
}

#[derive(Deserialize)]
struct Me {
    #[serde(default)]
    ra_username: Option<String>,
    #[serde(default)]
    ra_progression: Option<RaProgression>,
}

impl Client {
    /// RetroAchievements username and progress, if the account has RA linked in RomM.
    pub async fn ra_progression(&self) -> Result<Option<(String, RaProgression)>> {
        let me: Me = self.get_json("/api/users/me", &[]).await?;
        Ok(me
            .ra_username
            .filter(|u| !u.is_empty())
            .map(|u| (u, me.ra_progression.unwrap_or_default())))
    }
}

//! The subset of RomM schemas Romulus reads. Unknown fields are kept in `extra` so the
//! cache can store the full object.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) fn format_time(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// RomM returns ISO-8601 timestamps, sometimes without an offset (naive UTC).
pub fn parse_time(s: &str) -> Option<OffsetDateTime> {
    if let Ok(t) = OffsetDateTime::parse(s, &Rfc3339) {
        return Some(t);
    }
    let with_z = format!("{s}Z");
    OffsetDateTime::parse(&with_z, &Rfc3339).ok()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Heartbeat {
    #[serde(rename = "SYSTEM")]
    pub system: HeartbeatSystem,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatSystem {
    #[serde(rename = "VERSION")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub id: i64,
    pub slug: String,
    pub fs_slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default)]
    pub rom_count: i64,
    #[serde(default)]
    pub url_logo: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Platform {
    pub fn label(&self) -> &str {
        self.custom_name
            .as_deref()
            .or(self.display_name.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rom {
    pub id: i64,
    pub platform_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub name_sort_key: Option<String>,
    #[serde(default)]
    pub fs_name: String,
    #[serde(default)]
    pub fs_name_no_ext: Option<String>,
    #[serde(default)]
    pub fs_size_bytes: i64,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    #[serde(default)]
    pub path_cover_small: Option<String>,
    #[serde(default)]
    pub path_cover_large: Option<String>,
    #[serde(default)]
    pub ra_id: Option<i64>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Rom {
    /// Display title: matched name, else the file name without extension.
    pub fn title(&self) -> &str {
        self.name
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.fs_name_no_ext.as_deref())
            .unwrap_or(&self.fs_name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(default)]
    pub total: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RomQuery {
    pub platform_ids: Vec<i64>,
    pub updated_after: Option<OffsetDateTime>,
    pub search_term: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub with_total: bool,
}

impl RomQuery {
    pub(crate) fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut q = vec![
            ("limit", self.limit.clamp(1, 10_000).to_string()),
            ("offset", self.offset.to_string()),
            ("order_by", "id".into()),
            ("order_dir", "asc".into()),
            ("with_total", self.with_total.to_string()),
        ];
        for id in &self.platform_ids {
            q.push(("platform_ids", id.to_string()));
        }
        if let Some(t) = self.updated_after {
            q.push(("updated_after", format_time(t)));
        }
        if let Some(s) = &self.search_term {
            q.push(("search_term", s.clone()));
        }
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_naive_and_offset_times() {
        assert!(parse_time("2026-01-02T03:04:05Z").is_some());
        assert!(parse_time("2026-01-02T03:04:05.123456").is_some());
        assert!(parse_time("2026-01-02T03:04:05+02:00").is_some());
        assert!(parse_time("nope").is_none());
    }

    #[test]
    fn rom_title_falls_back_to_file_name() {
        let r: Rom = serde_json::from_value(serde_json::json!({
            "id": 1, "platform_id": 2, "fs_name": "Game (USA).sfc", "fs_name_no_ext": "Game (USA)", "name": ""
        }))
        .unwrap();
        assert_eq!(r.title(), "Game (USA)");
    }
}

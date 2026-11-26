use crate::{Cache, Result};
use romm_api::{Client, RomQuery, parse_time};
use serde::Serialize;
use std::sync::Mutex;
use time::OffsetDateTime;

/// How far before the newest seen `updated_at` the next refresh starts. Overlap costs a
/// few re-upserts; a gap would lose changes, so err on the side of overlap.
const CURSOR_OVERLAP: time::Duration = time::Duration::minutes(10);
const PAGE: u32 = 500;

pub const META_CURSOR: &str = "cursor";
pub const META_LAST_SYNC: &str = "last_sync_at";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum RefreshProgress {
    Platforms,
    Roms { done: u64, total: Option<i64> },
    Cleanup,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RefreshStats {
    pub full: bool,
    pub platforms_updated: usize,
    pub roms_updated: usize,
    pub platforms_removed: usize,
    pub roms_removed: usize,
}

fn lock(cache: &Mutex<Cache>) -> std::sync::MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(|e| e.into_inner())
}

/// Bring the cache up to date with the server.
///
/// 1. Platforms changed since the cursor.
/// 2. ROMs changed since the cursor (paged).
/// 3. The server's id lists, to drop what was deleted or hidden.
/// 4. Move the cursor to the newest `updated_at` seen (server clock, so the Deck's clock
///    does not matter).
///
/// The lock is only held while writing, so the UI keeps reading during a refresh.
pub async fn refresh(
    client: &Client,
    cache: &Mutex<Cache>,
    progress: impl Fn(RefreshProgress),
) -> Result<RefreshStats> {
    let cursor = lock(cache).meta(META_CURSOR)?.and_then(|s| parse_time(&s));
    let mut stats = RefreshStats {
        full: cursor.is_none(),
        ..Default::default()
    };
    let mut newest: Option<OffsetDateTime> = None;
    let mut see = |t: Option<&String>| {
        if let Some(t) = t.and_then(|s| parse_time(s)) {
            newest = Some(newest.map_or(t, |n| n.max(t)));
        }
    };

    progress(RefreshProgress::Platforms);
    let platforms = client.platforms(cursor).await?;
    platforms.iter().for_each(|p| see(p.updated_at.as_ref()));
    stats.platforms_updated = platforms.len();
    lock(cache).upsert_platforms(&platforms)?;

    let mut offset = 0u32;
    loop {
        let q = RomQuery {
            updated_after: cursor,
            limit: PAGE,
            offset,
            with_total: offset == 0,
            ..Default::default()
        };
        let page = client.roms(&q).await?;
        let n = page.items.len();
        page.items.iter().for_each(|r| see(r.updated_at.as_ref()));
        lock(cache).upsert_roms(&page.items)?;
        stats.roms_updated += n;
        progress(RefreshProgress::Roms {
            done: stats.roms_updated as u64,
            total: page.total,
        });
        if n < PAGE as usize {
            break;
        }
        offset += PAGE;
    }

    progress(RefreshProgress::Cleanup);
    let platform_ids = client.platform_ids().await?;
    let rom_ids = client.rom_ids().await?;
    {
        let mut c = lock(cache);
        stats.platforms_removed = c.retain_platforms(&platform_ids)?;
        stats.roms_removed = c.retain_roms(&rom_ids)?;
        if let Some(n) = newest {
            let next = (n - CURSOR_OVERLAP).max(cursor.unwrap_or(n - CURSOR_OVERLAP));
            c.set_meta(
                META_CURSOR,
                &next
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            )?;
        }
        c.set_meta(
            META_LAST_SYNC,
            &OffsetDateTime::now_utc().unix_timestamp().to_string(),
        )?;
    }
    tracing::info!(?stats, "library refresh done");
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SortOrder;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn rom(id: i64, name: &str, updated: &str) -> serde_json::Value {
        json!({"id": id, "platform_id": 1, "name": name, "fs_name": format!("{name}.sfc"), "updated_at": updated})
    }

    fn page(items: Vec<serde_json::Value>, offset: u32) -> ResponseTemplate {
        let total = items.len();
        ResponseTemplate::new(200)
            .set_body_json(json!({"items": items, "total": total, "limit": PAGE, "offset": offset}))
    }

    async fn mount_ids(s: &MockServer, platforms: &[i64], roms: &[i64]) {
        Mock::given(path("/api/platforms/identifiers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(platforms))
            .mount(s)
            .await;
        Mock::given(path("/api/roms/identifiers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(roms))
            .mount(s)
            .await;
    }

    #[tokio::test]
    async fn full_then_incremental() {
        let s = MockServer::start().await;
        let client = Client::new(romm_api::normalize_server_url(&s.uri()).unwrap(), "t").unwrap();
        let cache = Mutex::new(Cache::open_in_memory().unwrap());

        // First refresh: no cursor, everything comes down.
        Mock::given(method("GET"))
            .and(path("/api/platforms"))
            .and(query_param_is_missing("updated_after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "slug": "snes", "fs_slug": "snes", "name": "SNES", "rom_count": 2, "updated_at": "2026-11-01T00:00:00"}
            ])))
            .up_to_n_times(1)
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param_is_missing("updated_after"))
            .respond_with(page(
                vec![
                    rom(1, "Alpha", "2026-11-10T00:00:00"),
                    rom(2, "Beta", "2026-11-20T12:00:00"),
                ],
                0,
            ))
            .up_to_n_times(1)
            .mount(&s)
            .await;
        mount_ids(&s, &[1], &[1, 2]).await;

        let st = refresh(&client, &cache, |_| {}).await.unwrap();
        assert_eq!((st.full, st.roms_updated, st.roms_removed), (true, 2, 0));
        assert_eq!(
            cache.lock().unwrap().meta(META_CURSOR).unwrap().as_deref(),
            Some("2026-11-20T11:50:00Z")
        );

        // Second refresh: one change since the cursor, one ROM deleted on the server.
        s.reset().await;
        Mock::given(path("/api/platforms"))
            .and(query_param("updated_after", "2026-11-20T11:50:00Z"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param("updated_after", "2026-11-20T11:50:00Z"))
            .respond_with(page(vec![rom(2, "Beta Remix", "2026-11-25T00:00:00")], 0))
            .mount(&s)
            .await;
        mount_ids(&s, &[1], &[2]).await;

        let st = refresh(&client, &cache, |_| {}).await.unwrap();
        assert_eq!((st.full, st.roms_updated, st.roms_removed), (false, 1, 1));
        let c = cache.lock().unwrap();
        let titles: Vec<_> = c
            .roms(1, SortOrder::Title)
            .unwrap()
            .into_iter()
            .map(|r| r.title)
            .collect();
        assert_eq!(titles, ["Beta Remix"]);
        assert!(c.meta(META_LAST_SYNC).unwrap().is_some());
    }

    #[tokio::test]
    async fn pages_until_short_page() {
        let s = MockServer::start().await;
        let client = Client::new(romm_api::normalize_server_url(&s.uri()).unwrap(), "t").unwrap();
        let cache = Mutex::new(Cache::open_in_memory().unwrap());
        Mock::given(path("/api/platforms"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&s)
            .await;
        let first: Vec<_> = (0..PAGE as i64)
            .map(|i| rom(i + 1, &format!("G{i}"), "2026-11-01T00:00:00"))
            .collect();
        Mock::given(path("/api/roms"))
            .and(query_param("offset", "0"))
            .respond_with(page(first, 0))
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", PAGE.to_string()))
            .respond_with(page(vec![rom(9999, "Last", "2026-11-01T00:00:00")], PAGE))
            .mount(&s)
            .await;
        let all: Vec<i64> = (1..=PAGE as i64).chain([9999]).collect();
        mount_ids(&s, &[], &all).await;
        let st = refresh(&client, &cache, |_| {}).await.unwrap();
        assert_eq!(st.roms_updated, PAGE as usize + 1);
        assert_eq!(cache.lock().unwrap().rom_count().unwrap(), PAGE as i64 + 1);
    }
}

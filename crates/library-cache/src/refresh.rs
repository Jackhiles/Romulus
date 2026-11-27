use crate::{Cache, Result};
use romm_api::{Client, RomQuery, parse_time};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// How far before the newest seen `updated_at` the next refresh starts. Overlap costs a
/// few re-upserts; a gap would lose changes, so err on the side of overlap.
const CURSOR_OVERLAP: time::Duration = time::Duration::minutes(10);
/// ROMs per request. A busy RomM server can take a minute for 500, so pages stay small
/// enough that a retry or resume redoes little work.
const PAGE: u32 = 250;
/// Attempts after the first for a request that failed for a network or server reason.
const RETRIES: u32 = 3;
/// First retry waits this long; each later one twice as long.
const RETRY_BASE: Duration = if cfg!(test) {
    Duration::from_millis(5)
} else {
    Duration::from_secs(2)
};
/// More missing ROMs than this after a sync means paging went wrong: start over with a
/// full sync next time instead of fetching them one by one.
const RECONCILE_LIMIT: usize = 200;

pub const META_CURSOR: &str = "cursor";
pub const META_LAST_SYNC: &str = "last_sync_at";
/// Where an unfinished sync got to, so the next one continues instead of starting over.
pub const META_CHECKPOINT: &str = "sync_checkpoint";

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
    /// Offset an unfinished earlier sync was resumed from (0 when starting fresh).
    pub resumed_at: u32,
    pub platforms_updated: usize,
    pub roms_updated: usize,
    /// ROMs the server lists that paging missed, fetched one at a time.
    pub roms_reconciled: usize,
    pub platforms_removed: usize,
    pub roms_removed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct Checkpoint {
    /// The cursor this sync started from (raw meta value); a checkpoint only applies to
    /// a sync from the same cursor.
    base: Option<String>,
    /// Next offset to fetch.
    offset: u32,
    newest: Option<String>,
    total: Option<i64>,
}

fn lock(cache: &Mutex<Cache>) -> std::sync::MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(|e| e.into_inner())
}

fn fmt_time(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn bump(newest: &mut Option<OffsetDateTime>, t: Option<&String>) {
    if let Some(t) = t.and_then(|s| parse_time(s)) {
        *newest = Some(newest.map_or(t, |n| n.max(t)));
    }
}

/// Run `f`, trying again with backoff when it fails for a transient reason.
async fn retry<T, F, Fut>(what: &str, mut f: F) -> romm_api::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = romm_api::Result<T>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if e.is_transient() && attempt < RETRIES => {
                let wait = RETRY_BASE * 2u32.pow(attempt);
                attempt += 1;
                tracing::warn!("{what} failed ({e}); retry {attempt}/{RETRIES} in {wait:?}");
                tokio::time::sleep(wait).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Bring the cache up to date with the server.
///
/// 1. Platforms changed since the cursor.
/// 2. ROMs changed since the cursor, paged by id. Each page is committed with a
///    checkpoint, so a sync that fails (slow server, Deck asleep) resumes where it was.
/// 3. The server's id lists: drop what was deleted or hidden, and fetch any ROM the
///    server has that paging missed.
/// 4. Move the cursor to the newest `updated_at` seen (server clock, so the Deck's clock
///    does not matter).
///
/// The lock is only held while writing, so the UI keeps reading during a refresh.
pub async fn refresh(
    client: &Client,
    cache: &Mutex<Cache>,
    progress: impl Fn(RefreshProgress),
) -> Result<RefreshStats> {
    let (cursor_raw, checkpoint) = {
        let c = lock(cache);
        let raw = c.meta(META_CURSOR)?;
        let cp = c
            .meta(META_CHECKPOINT)?
            .and_then(|s| serde_json::from_str::<Checkpoint>(&s).ok())
            .filter(|cp| cp.base == raw);
        (raw, cp)
    };
    let cursor = cursor_raw.as_deref().and_then(parse_time);
    let mut newest = checkpoint
        .as_ref()
        .and_then(|c| c.newest.as_deref())
        .and_then(parse_time);
    let mut total = checkpoint.as_ref().and_then(|c| c.total);
    // Start a page early: ROMs deleted on the server meanwhile shift the rest left.
    let mut offset = checkpoint
        .as_ref()
        .map_or(0, |c| c.offset.saturating_sub(PAGE));
    let mut stats = RefreshStats {
        full: cursor.is_none(),
        resumed_at: offset,
        ..Default::default()
    };
    if offset > 0 {
        tracing::info!("resuming library sync at {offset}");
    }

    progress(RefreshProgress::Platforms);
    let platforms = retry("platform list", || client.platforms(cursor)).await?;
    for p in &platforms {
        bump(&mut newest, p.updated_at.as_ref());
    }
    stats.platforms_updated = platforms.len();
    lock(cache).upsert_platforms(&platforms)?;

    loop {
        let q = RomQuery {
            updated_after: cursor,
            limit: PAGE,
            offset,
            with_total: total.is_none(),
            ..Default::default()
        };
        let page = retry(&format!("ROM page at {offset}"), || client.roms(&q)).await?;
        let n = page.items.len();
        total = total.or(page.total);
        for r in &page.items {
            bump(&mut newest, r.updated_at.as_ref());
        }
        {
            let mut c = lock(cache);
            c.upsert_roms(&page.items)?;
            let cp = Checkpoint {
                base: cursor_raw.clone(),
                offset: offset + n as u32,
                newest: newest.map(fmt_time),
                total,
            };
            c.set_meta(META_CHECKPOINT, &serde_json::to_string(&cp)?)?;
        }
        stats.roms_updated += n;
        progress(RefreshProgress::Roms {
            done: (offset as usize + n) as u64,
            total,
        });
        if n < PAGE as usize {
            break;
        }
        offset += PAGE;
    }

    progress(RefreshProgress::Cleanup);
    let platform_ids = retry("platform ids", || client.platform_ids()).await?;
    let rom_ids = retry("ROM ids", || client.rom_ids()).await?;
    let missing: Vec<i64> = {
        let mut c = lock(cache);
        stats.platforms_removed = c.retain_platforms(&platform_ids)?;
        stats.roms_removed = c.retain_roms(&rom_ids)?;
        let local: HashSet<i64> = c.rom_ids()?.into_iter().collect();
        rom_ids
            .iter()
            .copied()
            .filter(|id| !local.contains(id))
            .collect()
    };

    let mut start_over = false;
    if missing.len() > RECONCILE_LIMIT {
        tracing::warn!(
            "{} ROMs missing after sync; the next sync will be a full one",
            missing.len()
        );
        start_over = true;
    } else if !missing.is_empty() {
        tracing::info!("fetching {} ROMs the sync missed", missing.len());
        let mut found = Vec::with_capacity(missing.len());
        for id in &missing {
            match retry(&format!("ROM {id}"), || client.rom(*id)).await {
                Ok(r) => found.push(r),
                // Deleted or hidden since the id list was read.
                Err(romm_api::Error::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        stats.roms_reconciled = found.len();
        lock(cache).upsert_roms(&found)?;
    }

    {
        let c = lock(cache);
        c.delete_meta(META_CHECKPOINT)?;
        if start_over {
            c.delete_meta(META_CURSOR)?;
        } else if let Some(n) = newest {
            let next = (n - CURSOR_OVERLAP).max(cursor.unwrap_or(n - CURSOR_OVERLAP));
            c.set_meta(META_CURSOR, &fmt_time(next))?;
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

    fn client(s: &MockServer) -> Client {
        Client::new(romm_api::normalize_server_url(&s.uri()).unwrap(), "t").unwrap()
    }

    fn roms(range: std::ops::Range<i64>) -> Vec<serde_json::Value> {
        range
            .map(|i| rom(i, &format!("G{i}"), "2026-11-01T00:00:00"))
            .collect()
    }

    async fn mount_platforms(s: &MockServer) {
        Mock::given(path("/api/platforms"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(s)
            .await;
    }

    #[tokio::test]
    async fn retries_transient_failures() {
        let s = MockServer::start().await;
        let cache = Mutex::new(Cache::open_in_memory().unwrap());
        mount_platforms(&s).await;
        Mock::given(path("/api/roms"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .respond_with(page(roms(1..4), 0))
            .mount(&s)
            .await;
        mount_ids(&s, &[], &[1, 2, 3]).await;
        let st = refresh(&client(&s), &cache, |_| {}).await.unwrap();
        assert_eq!(st.roms_updated, 3);
    }

    #[tokio::test]
    async fn failed_sync_resumes_from_checkpoint() {
        let s = MockServer::start().await;
        let cache = Mutex::new(Cache::open_in_memory().unwrap());
        let p = PAGE as i64;
        mount_platforms(&s).await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", "0"))
            .respond_with(page(roms(1..p + 1), 0))
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", PAGE.to_string()))
            .respond_with(page(roms(p + 1..2 * p + 1), PAGE))
            .mount(&s)
            .await;
        // The third page keeps failing: the sync gives up after its retries.
        Mock::given(path("/api/roms"))
            .and(query_param("offset", (2 * PAGE).to_string()))
            .respond_with(ResponseTemplate::new(502))
            .mount(&s)
            .await;
        assert!(refresh(&client(&s), &cache, |_| {}).await.is_err());
        assert_eq!(cache.lock().unwrap().rom_count().unwrap(), 2 * p);
        assert!(cache.lock().unwrap().meta(META_CURSOR).unwrap().is_none());

        // Next time it starts one page before where it stopped, not at 0.
        s.reset().await;
        mount_platforms(&s).await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", "0"))
            .respond_with(page(vec![], 0))
            .expect(0)
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", PAGE.to_string()))
            .respond_with(page(roms(p + 1..2 * p + 1), PAGE))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(path("/api/roms"))
            .and(query_param("offset", (2 * PAGE).to_string()))
            .respond_with(page(roms(2 * p + 1..2 * p + 11), 2 * PAGE))
            .mount(&s)
            .await;
        let all: Vec<i64> = (1..2 * p + 11).collect();
        mount_ids(&s, &[], &all).await;
        let st = refresh(&client(&s), &cache, |_| {}).await.unwrap();
        assert_eq!(st.resumed_at, PAGE);
        assert_eq!(cache.lock().unwrap().rom_count().unwrap(), 2 * p + 10);
        let c = cache.lock().unwrap();
        assert!(c.meta(META_CHECKPOINT).unwrap().is_none());
        assert!(c.meta(META_CURSOR).unwrap().is_some());
    }

    #[tokio::test]
    async fn fetches_roms_that_paging_missed() {
        let s = MockServer::start().await;
        let cache = Mutex::new(Cache::open_in_memory().unwrap());
        mount_platforms(&s).await;
        Mock::given(path("/api/roms"))
            .respond_with(page(roms(1..3), 0))
            .mount(&s)
            .await;
        Mock::given(path("/api/roms/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(rom(
                7,
                "Late",
                "2026-11-01T00:00:00",
            )))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(path("/api/roms/8"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"detail": "Not found"})))
            .mount(&s)
            .await;
        mount_ids(&s, &[], &[1, 2, 7, 8]).await;
        let st = refresh(&client(&s), &cache, |_| {}).await.unwrap();
        assert_eq!(st.roms_reconciled, 1);
        assert_eq!(
            cache.lock().unwrap().rom(7).unwrap().unwrap().0.title,
            "Late"
        );
    }
}

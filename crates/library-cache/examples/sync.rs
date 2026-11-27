//! Run a full library sync against a real server into a throwaway database, logging
//! progress and retries. Developer tool, not part of the app.
//!
//!     ROMM_URL=https://romm.example.net ROMM_TOKEN=rmm_... cargo run -p library-cache --example sync [DB]

use library_cache::{Cache, refresh};
use romm_api::{Client, Credentials, normalize_server_url};
use std::sync::Mutex;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let url = std::env::var("ROMM_URL").expect("ROMM_URL");
    let token = std::env::var("ROMM_TOKEN").expect("ROMM_TOKEN");
    let c = Client::new(normalize_server_url(&url).unwrap(), "romulus-sync-example").unwrap();
    c.set_credentials(Some(Credentials::ClientToken {
        token,
        device_id: None,
        expires_at: None,
    }))
    .await;
    let cache = match std::env::args().nth(1) {
        Some(p) => Cache::open(std::path::Path::new(&p)).unwrap(),
        None => Cache::open_in_memory().unwrap(),
    };
    let cache = Mutex::new(cache);
    let t = Instant::now();
    let r = refresh(&c, &cache, |p| {
        eprintln!("[{:>5.1}s] {p:?}", t.elapsed().as_secs_f32())
    })
    .await;
    println!(
        "{r:?} in {:.1}s, {} ROMs cached",
        t.elapsed().as_secs_f32(),
        cache.lock().unwrap().rom_count().unwrap()
    );
}

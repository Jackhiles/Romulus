//! Page through a real server's library the way a sync does and report timings and
//! full error chains. Developer tool, not part of the app.
//!
//!     ROMM_URL=https://romm.example.net ROMM_TOKEN=rmm_... cargo run -p romm-api --example probe

use romm_api::{Client, Credentials, RomQuery, normalize_server_url};
use std::error::Error as _;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let url = std::env::var("ROMM_URL").expect("ROMM_URL");
    let token = std::env::var("ROMM_TOKEN").expect("ROMM_TOKEN");
    let page: u32 = std::env::var("PAGE")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(500);
    let c = Client::new(normalize_server_url(&url).unwrap(), "romulus-probe").unwrap();
    c.set_credentials(Some(Credentials::ClientToken {
        token,
        device_id: None,
        expires_at: None,
    }))
    .await;
    let mut offset = 0;
    loop {
        let t = Instant::now();
        let q = RomQuery {
            limit: page,
            offset,
            with_total: offset == 0,
            ..Default::default()
        };
        match c.roms(&q).await {
            Ok(p) => {
                println!(
                    "offset {offset}: {} items in {:.1}s (total {:?})",
                    p.items.len(),
                    t.elapsed().as_secs_f32(),
                    p.total
                );
                if p.items.len() < page as usize {
                    break;
                }
                offset += page;
            }
            Err(e) => {
                println!(
                    "offset {offset}: FAILED after {:.1}s: {e}",
                    t.elapsed().as_secs_f32()
                );
                let mut s = e.source();
                while let Some(x) = s {
                    println!("  caused by: {x}");
                    s = x.source();
                }
                break;
            }
        }
    }
}

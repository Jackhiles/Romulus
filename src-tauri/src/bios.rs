//! BIOS commands: what RomM has per system, what is on the device, and downloading the
//! missing files. Placement and checks are in `launcher::bios`.

use crate::games::retrodeck;
use crate::state::{AppState, lock};
use launcher::bios::{self, BiosState};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

type Res<T> = Result<T, String>;

#[derive(Serialize)]
pub struct BiosFile {
    id: i64,
    name: String,
    size_bytes: u64,
    verified: bool,
    state: BiosState,
}

#[derive(Serialize)]
pub struct BiosSystem {
    platform_id: i64,
    name: String,
    slug: String,
    supported: bool,
    files: Vec<BiosFile>,
}

async fn parts(
    state: &AppState,
) -> Res<(
    romm_api::Client,
    std::sync::Arc<std::sync::Mutex<library_cache::Cache>>,
)> {
    let s = state.session.read().await;
    let s = s.as_ref().ok_or("no server set")?;
    Ok((s.client.clone(), s.cache.clone()))
}

fn bios_root(state: &AppState) -> Res<std::path::PathBuf> {
    let rd = retrodeck(state).ok_or("RetroDECK was not found (is the SD card inserted?)")?;
    Ok(rd
        .bios
        .unwrap_or_else(|| rd.roms.parent().unwrap_or(&rd.roms).join("bios")))
}

/// BIOS per system, from RomM, compared with the device.
#[tauri::command]
pub async fn bios_status(state: State<'_, AppState>) -> Res<Vec<BiosSystem>> {
    let (client, cache) = parts(&state).await?;
    let root = bios_root(&state)?;
    let all = client.firmware(None).await.map_err(|e| e.to_string())?;
    let platforms = lock(&cache).platforms_all().map_err(|e| e.to_string())?;
    let mut by: BTreeMap<String, BiosSystem> = BTreeMap::new();
    for fw in all {
        let Some(p) = platforms.iter().find(|p| p.id == fw.platform_id) else {
            continue;
        };
        let sys = by.entry(p.name.clone()).or_insert_with(|| BiosSystem {
            platform_id: p.id,
            name: p.name.clone(),
            slug: p.slug.clone(),
            supported: bios::target_dir(&root, &p.slug).is_some(),
            files: Vec::new(),
        });
        let st = bios::check(&root, &p.slug, &fw);
        sys.files.push(BiosFile {
            id: fw.id,
            name: fw.file_name,
            size_bytes: fw.file_size_bytes,
            verified: fw.is_verified,
            state: st,
        });
    }
    let mut out: Vec<BiosSystem> = by.into_values().collect();
    for s in &mut out {
        s.files.sort_by_key(|f| f.name.to_lowercase());
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct BiosResult {
    installed: u32,
    failed: Vec<String>,
}

/// Download a system's missing BIOS files. Existing files are never replaced.
#[tauri::command]
pub async fn download_bios(state: State<'_, AppState>, platform_id: i64) -> Res<BiosResult> {
    let (client, cache) = parts(&state).await?;
    let root = bios_root(&state)?;
    let slug = lock(&cache)
        .platforms_all()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == platform_id)
        .map(|p| p.slug)
        .ok_or("unknown system")?;
    let mut res = BiosResult {
        installed: 0,
        failed: Vec::new(),
    };
    for fw in client
        .firmware(Some(platform_id))
        .await
        .map_err(|e| e.to_string())?
    {
        if bios::check(&root, &slug, &fw) != BiosState::Missing {
            continue;
        }
        let r = match client.download_firmware(&fw).await {
            Ok(bytes) => bios::install(&root, &slug, &fw, &bytes)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match r {
            Ok(()) => res.installed += 1,
            Err(e) => {
                tracing::warn!("BIOS {} not installed: {e}", fw.file_name);
                res.failed.push(format!("{}: {e}", fw.file_name));
            }
        }
    }
    Ok(res)
}

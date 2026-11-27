//! Syncing around a play session against a mock RomM.

use md5::Digest;
use romm_api::{Client, Credentials, normalize_server_url};
use serde_json::json;
use sync::layout::Located;
use sync::local::{SaveSpec, content_hash};
use sync::{Choice, Ctx, Outcome, Record, after, before};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client(s: &MockServer) -> Client {
    let c = Client::new(normalize_server_url(&s.uri()).unwrap(), "t").unwrap();
    c.set_credentials(Some(Credentials::ClientToken {
        token: "rmm_t".into(),
        device_id: Some("dev".into()),
        expires_at: None,
    }))
    .await;
    c
}

fn save(id: i64, data: &[u8], at: &str) -> serde_json::Value {
    json!({"id": id, "rom_id": 7, "file_name": format!("Game [{id}].srm"), "emulator": "retroarch-swanstation",
           "slot": "autosave", "content_hash": content_hash(data), "file_size_bytes": data.len(), "updated_at": at})
}

async fn mount_list(s: &MockServer, saves: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/api/saves"))
        .respond_with(ResponseTemplate::new(200).set_body_json(saves))
        .mount(s)
        .await;
}

fn game(dir: &std::path::Path) -> Located {
    Located {
        spec: SaveSpec::File {
            path: dir.join("saves/psx/Game.srm"),
        },
        emulator: "retroarch-swanstation".into(),
        upload_name: "Game.srm".into(),
        exts: &["srm"],
        warning: None,
        pcsx2_card: false,
    }
}

fn backups(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("backups")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                stack.push(e.path())
            } else {
                out.push(e.path())
            }
        }
    }
    out
}

#[tokio::test]
async fn restores_newer_save_from_romm_with_a_backup() {
    let s = MockServer::start().await;
    mount_list(&s, json!([save(2, b"SERVER", "2026-11-27T10:00:00")])).await;
    Mock::given(path("/api/saves/2/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"SERVER".to_vec()))
        .mount(&s)
        .await;
    Mock::given(path("/api/saves/2/downloaded"))
        .respond_with(ResponseTemplate::new(200).set_body_json(save(2, b"SERVER", "x")))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s).await;
    let d = tempfile::tempdir().unwrap();
    let g = game(d.path());
    std::fs::create_dir_all(d.path().join("saves/psx")).unwrap();
    std::fs::write(d.path().join("saves/psx/Game.srm"), b"OLD").unwrap();
    // Last sync: the Deck had OLD, RomM had save 1. Since then RomM got save 2.
    let rec = Record {
        local_fp: Some(content_hash(b"OLD")),
        server_id: Some(1),
        server_hash: Some("h1".into()),
        ..Default::default()
    };
    let ctx = Ctx {
        client: &c,
        device_id: Some("dev"),
        backups: &d.path().join("backups"),
    };
    let out = before(&ctx, 7, &g, Some(&rec), None).await.unwrap();
    assert!(matches!(
        out,
        Outcome::Synced {
            changed: Some("downloaded"),
            ..
        }
    ));
    assert_eq!(
        std::fs::read(d.path().join("saves/psx/Game.srm")).unwrap(),
        b"SERVER"
    );
    let b = backups(d.path());
    assert_eq!(b.len(), 1);
    assert_eq!(std::fs::read(&b[0]).unwrap(), b"OLD");
}

#[tokio::test]
async fn uploads_after_playing_and_reports_conflicts() {
    let s = MockServer::start().await;
    mount_list(&s, json!([save(1, b"V1", "2026-11-27T10:00:00")])).await;
    Mock::given(method("POST"))
        .and(path("/api/saves"))
        .and(query_param("overwrite", "false"))
        .respond_with(ResponseTemplate::new(201).set_body_json(save(
            3,
            b"V2",
            "2026-11-27T11:00:00",
        )))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s).await;
    let d = tempfile::tempdir().unwrap();
    let g = game(d.path());
    std::fs::create_dir_all(d.path().join("saves/psx")).unwrap();
    std::fs::write(d.path().join("saves/psx/Game.srm"), b"V2").unwrap();
    let rec = Record {
        local_fp: Some(content_hash(b"V1")),
        server_id: Some(1),
        server_hash: Some(content_hash(b"V1")),
        ..Default::default()
    };
    let ctx = Ctx {
        client: &c,
        device_id: Some("dev"),
        backups: &d.path().join("backups"),
    };
    let out = after(&ctx, 7, &g, Some(&rec)).await.unwrap();
    let Outcome::Synced { record, changed } = out else {
        panic!("{out:?}")
    };
    assert_eq!(changed, Some("uploaded"));
    assert_eq!(record.server_id, Some(3));

    // Meanwhile another device uploaded save 4: both sides changed → conflict, no upload.
    s.reset().await;
    mount_list(&s, json!([save(4, b"OTHER", "2026-11-27T12:00:00")])).await;
    Mock::given(method("POST"))
        .and(path("/api/saves"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&s)
        .await;
    std::fs::write(d.path().join("saves/psx/Game.srm"), b"V3").unwrap();
    assert!(matches!(
        after(&ctx, 7, &g, Some(&record)).await.unwrap(),
        Outcome::Conflict(_)
    ));
    assert!(matches!(
        before(&ctx, 7, &g, Some(&record), None).await.unwrap(),
        Outcome::Conflict(_)
    ));
}

#[tokio::test]
async fn keep_local_backs_up_romms_copy_then_overwrites() {
    let s = MockServer::start().await;
    mount_list(&s, json!([save(4, b"OTHER", "2026-11-27T12:00:00")])).await;
    Mock::given(path("/api/saves/4/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"OTHER".to_vec()))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/saves"))
        .and(query_param("overwrite", "true"))
        .respond_with(ResponseTemplate::new(201).set_body_json(save(
            5,
            b"MINE",
            "2026-11-27T13:00:00",
        )))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s).await;
    let d = tempfile::tempdir().unwrap();
    let g = game(d.path());
    std::fs::create_dir_all(d.path().join("saves/psx")).unwrap();
    std::fs::write(d.path().join("saves/psx/Game.srm"), b"MINE").unwrap();
    let ctx = Ctx {
        client: &c,
        device_id: Some("dev"),
        backups: &d.path().join("backups"),
    };
    // Never synced and different: a conflict until the user picks.
    assert!(matches!(
        before(&ctx, 7, &g, None, None).await.unwrap(),
        Outcome::Conflict(_)
    ));
    let out = before(&ctx, 7, &g, None, Some(Choice::KeepLocal))
        .await
        .unwrap();
    assert!(matches!(
        out,
        Outcome::Synced {
            changed: Some("uploaded"),
            ..
        }
    ));
    assert_eq!(
        std::fs::read(d.path().join("saves/psx/Game.srm")).unwrap(),
        b"MINE"
    );
    let b = backups(d.path());
    assert_eq!(b.len(), 1);
    assert_eq!(std::fs::read(&b[0]).unwrap(), b"OTHER");
}

fn state(id: i64, name: &str, emulator: &str, at: &str) -> serde_json::Value {
    json!({"id": id, "rom_id": 7, "file_name": name, "emulator": emulator,
           "file_size_bytes": 1, "updated_at": at})
}

#[tokio::test]
async fn states_sync_by_file_and_ignore_other_emulators() {
    use std::collections::HashMap;
    use sync::states::{Pattern, StateSet, sync as sync_states};

    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/states"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            // From another device running the same emulator: newer than the Deck's.
            state(
                1,
                "Game [2026-11-27 10-00-00-000].state1",
                "retroarch-swanstation",
                "2026-11-27T10:00:00"
            ),
            // RomM's web player (another emulator): never used.
            state(
                2,
                "Game [2026-11-27 11-00-00-000].state2",
                "pcsx_rearmed",
                "2026-11-27T11:00:00"
            ),
        ])))
        .mount(&s)
        .await;
    Mock::given(path("/api/states/1/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"REMOTE1".to_vec()))
        .mount(&s)
        .await;
    Mock::given(path("/api/states/2/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"WEB".to_vec()))
        .expect(0)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/states"))
        .and(query_param("emulator", "retroarch-swanstation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(state(
            3,
            "Game [2026-11-27 12-00-00-000].state",
            "retroarch-swanstation",
            "2026-11-27T12:00:00",
        )))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s).await;
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("states/psx");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Game.state"), b"NEW").unwrap(); // only on the Deck
    std::fs::write(dir.join("Game.state1"), b"OLD1").unwrap(); // synced, then replaced elsewhere
    std::fs::write(dir.join("Game.state.png"), b"thumb").unwrap(); // not a state
    let set = StateSet {
        dir: dir.clone(),
        pattern: Pattern::RetroArch {
            stem: "Game".into(),
        },
        emulator: "retroarch-swanstation".into(),
    };
    let mut records = HashMap::new();
    records.insert(
        "Game.state1".to_string(),
        sync::states::StateRecord {
            local: Some(format!("{:x}", md5::Md5::digest(b"OLD1"))),
            server: Some("0@2026-11-01T00:00:00".into()),
            synced_at: None,
        },
    );
    let ctx = Ctx {
        client: &c,
        device_id: None,
        backups: &d.path().join("backups"),
    };
    // After playing: only the Deck's change goes up; RomM's newer state1 waits.
    let after = sync_states(&ctx, 7, &set, &records, false).await.unwrap();
    assert_eq!((after.uploaded, after.downloaded), (1, 0));
    assert_eq!(std::fs::read(dir.join("Game.state1")).unwrap(), b"OLD1");
    for (n, r) in after.records {
        records.insert(n, r);
    }
    s.verify().await;

    // Before playing: state1 comes down, after a backup of the Deck's copy. RomM now
    // also lists the state just uploaded.
    s.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/states"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            state(
                1,
                "Game [2026-11-27 10-00-00-000].state1",
                "retroarch-swanstation",
                "2026-11-27T10:00:00"
            ),
            state(
                3,
                "Game [2026-11-27 12-00-00-000].state",
                "retroarch-swanstation",
                "2026-11-27T12:00:00"
            ),
        ])))
        .mount(&s)
        .await;
    Mock::given(path("/api/states/1/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"REMOTE1".to_vec()))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/states"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&s)
        .await;
    let before = sync_states(&ctx, 7, &set, &records, true).await.unwrap();
    assert_eq!((before.uploaded, before.downloaded), (0, 1));
    assert_eq!(std::fs::read(dir.join("Game.state1")).unwrap(), b"REMOTE1");
    assert!(!dir.join("Game.state2").exists());
    let b = backups(d.path());
    assert_eq!(b.len(), 1);
    assert_eq!(std::fs::read(&b[0]).unwrap(), b"OLD1");
}

//! Syncing around a play session against a mock RomM.

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
        ext: "srm",
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

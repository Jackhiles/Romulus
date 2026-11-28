//! API client tests against a mock RomM server. Response bodies follow the RomM 5.4.1
//! OpenAPI schemas (docs/reference).

use romm_api::{
    Client, Credentials, DeviceInfo, DevicePoll, Error, RomQuery, normalize_server_url,
};
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client(server: &MockServer) -> Client {
    Client::new(normalize_server_url(&server.uri()).unwrap(), "romulus-test").unwrap()
}

fn rom(id: i64, platform_id: i64, name: &str) -> serde_json::Value {
    json!({
        "id": id, "platform_id": platform_id, "name": name, "fs_name": format!("{name}.sfc"),
        "fs_name_no_ext": name, "fs_size_bytes": 1024, "summary": null, "alternative_names": [],
        "path_cover_small": format!("/assets/romm/resources/roms/{platform_id}/{id}/cover/small.png"),
        "path_cover_large": null, "ra_id": null, "updated_at": "2026-11-20T10:00:00",
        "regions": ["USA"], "igdb_id": 99
    })
}

#[tokio::test]
async fn heartbeat_reads_version() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/heartbeat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "SYSTEM": {"VERSION": "5.4.1", "GIT_BRANCH": null, "SHOW_SETUP_WIZARD": false},
            "FILESYSTEM": {}, "EMULATION": {}
        })))
        .mount(&s)
        .await;
    let hb = client(&s).await.heartbeat().await.unwrap();
    assert_eq!(hb.system.version, "5.4.1");
}

#[tokio::test]
async fn device_pairing_flow() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/init"))
        .and(body_string_contains("\"requested_scopes\""))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "device_code": "dc-secret", "user_code": "ABCD-1234",
            "verification_path": "/pair/device",
            "verification_path_complete": "/pair/device?user_code=ABCD-1234",
            "expires_in": 600, "interval": 5
        })))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({"detail": "authorization_pending"})),
        )
        .up_to_n_times(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "rmm_abc", "device_id": "dev-1", "scopes": ["roms.read"], "expires_at": null
        })))
        .mount(&s)
        .await;

    let c = client(&s).await;
    let info = DeviceInfo {
        client_device_identifier: "id",
        name: "Deck",
        client: "Romulus",
        platform: "SteamOS",
        client_version: "0.1.0",
    };
    let init = c.device_auth_init(&info).await.unwrap();
    assert_eq!(init.user_code, "ABCD-1234");
    assert_eq!(
        c.verification_url(&init),
        format!("{}/pair/device?user_code=ABCD-1234", s.uri())
    );
    assert_eq!(
        c.device_auth_poll(&init.device_code).await.unwrap(),
        DevicePoll::Pending
    );
    match c.device_auth_poll(&init.device_code).await.unwrap() {
        DevicePoll::Approved(cr) => {
            assert_eq!(cr.access_token(), "rmm_abc");
            assert_eq!(cr.device_id(), Some("dev-1"));
        }
        other => panic!("expected approval, got {other:?}"),
    }
}

#[tokio::test]
async fn device_poll_maps_denied_and_expired() {
    let s = MockServer::start().await;
    Mock::given(path("/api/auth/device/token"))
        .and(body_string_contains("denied-code"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"detail": "access_denied"})))
        .mount(&s)
        .await;
    Mock::given(path("/api/auth/device/token"))
        .and(body_string_contains("old-code"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"detail": "expired_token"})))
        .mount(&s)
        .await;
    let c = client(&s).await;
    assert_eq!(
        c.device_auth_poll("denied-code").await.unwrap(),
        DevicePoll::Denied
    );
    assert_eq!(
        c.device_auth_poll("old-code").await.unwrap(),
        DevicePoll::Expired
    );
}

#[tokio::test]
async fn password_login_then_authorized_requests() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .and(body_string_contains("grant_type=password"))
        .and(body_string_contains("username=jack"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "acc1", "refresh_token": "ref1", "token_type": "bearer", "expires": 900, "refresh_expires": 604800
        })))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/platforms"))
        .and(header("authorization", "Bearer acc1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "slug": "snes", "fs_slug": "snes", "name": "Super Nintendo", "display_name": "SNES",
             "custom_name": null, "rom_count": 2, "updated_at": "2026-11-20T10:00:00", "firmware": []}
        ])))
        .mount(&s)
        .await;
    let c = client(&s).await;
    let cr = c.password_login("jack", "pw").await.unwrap();
    assert!(matches!(cr, Credentials::OAuth { .. }));
    let p = c.platforms(None).await.unwrap();
    assert_eq!(p[0].label(), "SNES");
    assert_eq!(p[0].extra["firmware"], json!([]));
}

#[tokio::test]
async fn wrong_password_is_unauthorized() {
    let s = MockServer::start().await;
    Mock::given(path("/api/token"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"detail": "Invalid username or password"})),
        )
        .mount(&s)
        .await;
    assert!(matches!(
        client(&s).await.password_login("a", "b").await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn refreshes_once_on_401_and_notifies() {
    let s = MockServer::start().await;
    Mock::given(path("/api/roms/identifiers"))
        .and(header("authorization", "Bearer old"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&s)
        .await;
    Mock::given(path("/api/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=r1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new", "token_type": "bearer", "expires": 900
        })))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(path("/api/roms/identifiers"))
        .and(header("authorization", "Bearer new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([1, 2, 3])))
        .mount(&s)
        .await;

    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen2 = seen.clone();
    let c = client(&s)
        .await
        .with_credentials_listener(std::sync::Arc::new(move |cr: &Credentials| {
            *seen2.lock().unwrap() = Some(cr.clone());
        }));
    c.set_credentials(Some(Credentials::OAuth {
        access_token: "old".into(),
        refresh_token: Some("r1".into()),
        expires_at: 0,
    }))
    .await;
    assert_eq!(c.rom_ids().await.unwrap(), vec![1, 2, 3]);
    // The refresh response had no new refresh token, so the old one is kept.
    let stored = seen.lock().unwrap().clone().unwrap();
    match stored {
        Credentials::OAuth {
            access_token,
            refresh_token,
            ..
        } => {
            assert_eq!(access_token, "new");
            assert_eq!(refresh_token.as_deref(), Some("r1"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn client_token_is_not_refreshed() {
    let s = MockServer::start().await;
    Mock::given(path("/api/roms/identifiers"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&s)
        .await;
    Mock::given(path("/api/token"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&s)
        .await;
    let c = client(&s).await;
    c.set_credentials(Some(Credentials::ClientToken {
        token: "rmm_x".into(),
        device_id: None,
        expires_at: None,
    }))
    .await;
    assert!(matches!(c.rom_ids().await, Err(Error::Unauthorized)));
}

#[tokio::test]
async fn rom_page_query_and_decoding() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/roms"))
        .and(query_param("platform_ids", "4"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "0"))
        .and(query_param("updated_after", "2026-11-01T00:00:00Z"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [rom(10, 4, "Alpha"), rom(11, 4, "Beta")], "total": 2, "limit": 500, "offset": 0,
            "char_index": {}, "rom_id_index": [], "filter_values": {}
        })))
        .mount(&s)
        .await;
    let q = RomQuery {
        platform_ids: vec![4],
        updated_after: Some(time::macros::datetime!(2026-11-01 0:00 UTC)),
        limit: 500,
        with_total: true,
        ..Default::default()
    };
    let page = client(&s).await.roms(&q).await.unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[1].title(), "Beta");
    assert_eq!(page.items[0].extra["igdb_id"], 99);
}

#[tokio::test]
async fn server_errors_carry_detail() {
    let s = MockServer::start().await;
    Mock::given(path("/api/roms/5"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"detail": "boom"})))
        .mount(&s)
        .await;
    match client(&s).await.rom_detail(5).await {
        Err(Error::Http {
            status: 500,
            detail,
        }) => assert_eq!(detail, "boom"),
        other => panic!("{other:?}"),
    }
}

fn file(id: i64, name: &str) -> romm_api::RomFile {
    romm_api::RomFile {
        id,
        file_name: name.into(),
        file_path: "roms/psx/Game".into(),
        file_size_bytes: 10,
        is_top_level: true,
        category: Some("game".into()),
        sha1_hash: None,
    }
}

#[tokio::test]
async fn download_encodes_name_and_resumes_with_range() {
    let s = MockServer::start().await;
    // The '#' and spaces must arrive as one encoded path segment.
    Mock::given(method("GET"))
        .and(path("/api/roms/7/files/content/Game%20%231%20(USA).chd"))
        .and(header("range", "bytes=4-"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 4-9/10")
                .set_body_bytes(b"456789".to_vec()),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/roms/7/files/content/Game%20%231%20(USA).chd"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"0123456789".to_vec()))
        .mount(&s)
        .await;
    let c = client(&s).await;
    let f = file(7, "Game #1 (USA).chd");

    let d = c.download_file(&f, 4).await.unwrap();
    assert!(d.partial);
    assert_eq!(d.total, Some(10));
    assert_eq!(d.response.bytes().await.unwrap().as_ref(), b"456789");

    let d = c.download_file(&f, 0).await.unwrap();
    assert!(!d.partial);
    assert_eq!(d.total, Some(10));
}

#[tokio::test]
async fn reads_rom_files_and_reports_play() {
    let s = MockServer::start().await;
    Mock::given(path("/api/roms/1653"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1653, "fs_name": "Metal Gear Solid (USA) (Rev 1)", "fs_path": "roms/psx",
            "platform_fs_slug": "psx", "has_simple_single_file": false,
            "files": [{"id": 10616, "file_name": "Metal Gear Solid (USA) (Rev 1).m3u",
                       "file_path": "roms/psx/Metal Gear Solid (USA) (Rev 1)", "file_size_bytes": 90,
                       "is_top_level": true, "category": "game", "sha1_hash": "5ae4"}],
            "summary": "ignored"
        })))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/play-sessions"))
        .and(body_string_contains("\"device_id\":\"dev-1\""))
        .and(body_string_contains("\"duration_ms\":60000"))
        .and(body_string_contains(
            "\"start_time\":\"2026-11-27T10:00:00Z\"",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"created": 1})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/roms/1653/props"))
        .and(query_param("update_last_played", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&s)
        .await;

    let c = client(&s).await;
    let r = c.rom_files(1653).await.unwrap();
    assert_eq!(r.files[0].category.as_deref(), Some("game"));
    let start = time::macros::datetime!(2026-11-27 10:00 UTC);
    c.report_play_sessions(
        Some("dev-1"),
        &[romm_api::PlaySession {
            rom_id: 1653,
            start_time: start,
            end_time: start + time::Duration::minutes(1),
            duration_ms: 60_000,
        }],
    )
    .await
    .unwrap();
    c.mark_played(1653).await.unwrap();
}

#[tokio::test]
async fn saves_list_download_upload_and_conflict() {
    let s = MockServer::start().await;
    let save = json!({"id": 2, "rom_id": 1653, "file_name": "MGS [2026-10-25_17-11-46].srm",
        "emulator": "retroarch-swanstation", "slot": "autosave", "content_hash": "3490c8fc",
        "file_size_bytes": 131072, "updated_at": "2026-10-25T17:11:46", "origin_device_id": "dev-x"});
    Mock::given(method("GET"))
        .and(path("/api/saves"))
        .and(query_param("rom_id", "1653"))
        .and(query_param("device_id", "dev-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([save])))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/saves/2/content"))
        .and(query_param("device_id", "dev-1"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"SRAM".to_vec()))
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/saves/2/downloaded"))
        .and(body_string_contains("\"content_hash\":\"abc\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(save.clone()))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/saves"))
        .and(query_param("slot", "autosave"))
        .and(query_param("overwrite", "false"))
        .and(body_string_contains(
            "name=\"saveFile\"; filename=\"MGS.srm\"",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(save.clone()))
        .up_to_n_times(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/saves"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"detail": "Slot has a newer save since your last sync"})),
        )
        .mount(&s)
        .await;

    let c = client(&s).await;
    let list = c.saves(1653, Some("dev-1")).await.unwrap();
    assert_eq!(list[0].emulator.as_deref(), Some("retroarch-swanstation"));
    assert_eq!(c.download_save(2, Some("dev-1")).await.unwrap(), b"SRAM");
    c.confirm_save_download(2, "dev-1", "abc").await.unwrap();
    let up = romm_api::SaveUpload {
        rom_id: 1653,
        emulator: "retroarch-swanstation",
        slot: "autosave",
        device_id: Some("dev-1"),
        content_hash: "abc",
        file_name: "MGS.srm",
        bytes: b"SRAM2".to_vec(),
        overwrite: false,
    };
    assert_eq!(c.upload_save(&up).await.unwrap().id, 2);
    assert!(c.upload_save(&up).await.unwrap_err().is_conflict());
}

#[tokio::test]
async fn lists_and_downloads_firmware() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/firmware"))
        .and(query_param("platform_id", "4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 9, "platform_id": 4, "file_name": "SCPH-1001 (US).bin", "file_size_bytes": 4,
            "is_verified": true, "md5_hash": "abc", "sha1_hash": null, "crc_hash": "x", "file_path": "bios/psx"
        }])))
        .mount(&s)
        .await;
    Mock::given(path("/api/firmware/9/content/SCPH-1001%20(US).bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"BIOS".to_vec()))
        .mount(&s)
        .await;
    let c = client(&s).await;
    let fw = c.firmware(Some(4)).await.unwrap();
    assert!(fw[0].is_verified);
    assert_eq!(c.download_firmware(&fw[0]).await.unwrap(), b"BIOS");
}

#[tokio::test]
async fn reads_retroachievements_progress() {
    let s = MockServer::start().await;
    Mock::given(path("/api/users/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "username": "jack", "ra_username": "jackra",
            "ra_progression": {"total": 1, "results": [{"rom_ra_id": 11244, "max_possible": 112, "num_awarded": 2,
              "num_awarded_hardcore": 0, "most_recent_awarded_date": null, "highest_award_kind": null,
              "earned_achievements": [{"id": "87992", "date": "2026-10-09 02:54:14"}]}]}
        })))
        .mount(&s)
        .await;
    let c = client(&s).await;
    let (user, p) = c.ra_progression().await.unwrap().unwrap();
    assert_eq!(user, "jackra");
    assert_eq!(p.results[0].earned_achievements[0].id, "87992");
}

#[tokio::test]
async fn updates_play_status() {
    let s = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/roms/42/props"))
        .and(body_string_contains("\"now_playing\":true"))
        .and(body_string_contains("\"status\":null"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"id": 1, "rom_id": 42, "now_playing": true, "backlogged": false, "status": null, "rating": 0}),
        ))
        .expect(1)
        .mount(&s)
        .await;
    let c = client(&s).await;
    let u = c
        .update_rom_user(42, &json!({"now_playing": true, "status": null}))
        .await
        .unwrap();
    assert_eq!(u["now_playing"], true);
}

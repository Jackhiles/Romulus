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

# RomM API: what Romulus uses

Source of truth: `docs/reference/romm-openapi-5.4.1.json`, generated from the RomM
**5.4.1** tag (latest stable on 2026-11-26) by loading its FastAPI app and calling
`app.openapi()`. It is the same document the server serves at `/openapi.json`
(and renders at `/api/docs`).

**Still to do:** compare it against the user's own instance (`GET /api/heartbeat` gives
the version). If their server is older or newer, regenerate the snapshot from
`<server>/openapi.json` and diff it before coding against an endpoint.

All paths are under `/api`. Auth is `Authorization: Bearer <token>` (OAuth2 scopes) or HTTP
Basic.

## Auth

| Purpose | Endpoint | Notes |
|---|---|---|
| Server version and features | `GET /heartbeat` | No auth. `SYSTEM`, `FILESYSTEM`, `EMULATION`, ... |
| **Device pairing (preferred)** | `POST /auth/device/init` | Body `{client_device_identifier, name, client, platform, client_version, requested_scopes[]}` returns `{device_code, user_code, verification_path, verification_path_complete, expires_in, interval}` |
| | `POST /auth/device/token` | Body `{device_code}`, polled every `interval` s; returns `{access_token, device_id, scopes, expires_at}` once the user approves it in the RomM web UI |
| Pair code from web UI | `POST /client-tokens/exchange` | Body `{code}` returns a client token |
| Username + password | `POST /token` | OAuth2 password grant (form-encoded), returns access + refresh tokens; `grant_type=refresh_token` refreshes |

Device pairing means nobody types a password with the Deck's on-screen keyboard: Romulus
shows a code and a QR code, the user approves on their phone, and we get a scoped token plus
a `device_id`. Username/password stays as a fallback.

Scopes Romulus asks for: `me.read roms.read platforms.read assets.read assets.write
firmware.read roms.user.read roms.user.write collections.read devices.read devices.write`.

## Library (Phase 1)

| Purpose | Endpoint | Notes |
|---|---|---|
| Platforms | `GET /platforms?updated_after=` | `PlatformSchema` includes `rom_count`, `fs_slug`, `firmware[]`, `updated_at` |
| ROM list | `GET /roms?platform_ids=&updated_after=&limit=&offset=&order_by=&with_total=` | Paged (`limit` up to 10000). Returns `{items: SimpleRomSchema[], total, limit, offset}` |
| All visible ROM ids | `GET /roms/identifiers` | `int[]`, used to detect deletions |
| Platform ids | `GET /platforms/identifiers` | same, for platforms |
| ROM detail | `GET /roms/{id}` | Full `DetailedRomSchema` |
| Search | `GET /roms?search_term=` | Local FTS covers most of it; server search is the fallback |
| Covers | `path_cover_small` / `path_cover_large` on the ROM | Paths served by the RomM web server; fetched with auth headers |
| Collections | `GET /collections`, `/collections/smart`, `/collections/virtual` | `updated_after` supported |

**Incremental refresh**: (1) `GET /platforms?updated_after=T`, (2) `GET /roms?updated_after=T`
paged, upsert, (3) `GET /roms/identifiers`, delete local rows not in the set, (4) set
`T = server time at step 1`. Step 3 is a single integer array, cheap even for large
libraries.

## Play (Phase 2)

| Purpose | Endpoint | Notes |
|---|---|---|
| ROM files | `GET /roms/{id}/files` | `RomFileSchema[]` with sizes and hashes |
| Download | `GET /roms/{id}/content/{file_name}?file_ids=` | Single file, or zip + .m3u for multi-file ROMs; `HEAD` gives the size for the disk check; Range support to be tested for resume |
| Last played | `PUT /roms/{id}/props?update_last_played=true` | |
| Play status | `PUT /roms/{id}/props` with `RomUserData` (`now_playing`, `backlogged`, `status`, `rating` 0-10) | Returns `RomUserSchema`; the list schema already carries it as `rom_user`. Needs `roms.user.write` |
| Manuals | `GET /assets/romm/resources/<path_manual>` | `has_manual`/`path_manual` are in the list schema (`roms/2/1/manual/1.pdf`); 5,340 of 6,600 games had one (2026-11-28) |
| Playtime | `POST /play-sessions` | `{device_id, sessions:[{rom_id, start_time, end_time, duration_ms}]}` |
| Register this Deck | `POST /devices` | Returns `device_id` (device pairing already returns one) |

## Sync (Phase 3)

| Purpose | Endpoint | Notes |
|---|---|---|
| **Negotiate** | `POST /sync/negotiate` | We send `{device_id, saves:[{rom_id, file_name, slot, emulator, content_hash, updated_at, file_size_bytes}], rom_ids}`. The server answers `operations[]` with `action` (upload/download/conflict/no_op/delete), `reason`, `server_content_hash`, `server_updated_at` |
| Finish | `POST /sync/sessions/{id}/complete` | |
| Saves | `GET /saves?rom_id=&device_id=`, `POST /saves?rom_id=&emulator=&slot=&device_id=&content_hash=` (multipart), `GET /saves/{id}/content`, `POST /saves/{id}/downloaded` | |
| States | `GET /states?rom_id=`, `POST /states?rom_id=&emulator=` (multipart `stateFile`), `GET /states/{id}/content` | No hash/device fields in 5.4.1, so Romulus tracks them itself (md5 on the Deck, `id@updated_at` on RomM). Every upload is a new state named `Game [2026-01-15 23-09-20-955].state`; older ones stay. Romulus only uses states whose `emulator` is its own (the web player writes e.g. `pcsx_rearmed`) |
| Collections | `GET /collections`, `GET /collections/smart`, `POST /collections?is_favorite=true` (multipart `name`), `POST`/`DELETE /collections/{id}/roms` `{rom_ids}` | Favourites is the collection with `is_favorite`. Changing one needs the `collections.write` scope, which pairings before 0.6 didn't ask for |
| Firmware | `GET /firmware?platform_id=`, `GET /firmware/{id}/content/{file_name}` | `FirmwareSchema` has `md5/sha1/crc` and `is_verified` |
| Save layout hint | `save_target`, `save_target_layout` on the ROM | To look into: may tell us where the server expects saves to live |

The server's negotiation is advice. Romulus still runs its own conflict check (`sync`
module), backs up before every overwrite, and never acts on a server `delete` for local
saves without a backup.

## RetroAchievements (Phase 4)

`merged_ra_metadata` and `ra_id` on the ROM, `has_ra` filter on `/roms`. Per-user progress
is to be checked on a live server. Fallback: RA web API.

## Focus audio (Phase 4)

`youtube_video_id`, `path_video` and `has_soundtrack` on the ROM;
`GET /roms/{id}/soundtracks/metadata`.

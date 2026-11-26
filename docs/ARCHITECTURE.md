# Romulus architecture

Status: **Phase 1 built** (approved 2026-11-26). Phases 2-4 below are still plans.
`docs/DECK_CHECKLIST.md` lists what still needs checking on a real Deck.

## Overview

```
┌──────────────────────────── Tauri window (fullscreen, 1280×800) ───────────────────────────┐
│  ui/  Svelte 5 + TypeScript                                                                │
│   focus manager · virtual grid · screens · footer button hints · toasts                    │
│        ▲ events (gamepad, sync progress, download progress)        │ invoke() commands      │
└────────┼───────────────────────────────────────────────────────────┼───────────────────────┘
         │                                                           ▼
┌─────────────────────────────────── Rust core (src-tauri) ───────────────────────────────────┐
│ commands.rs   thin Tauri command layer: validates input, calls crates, maps errors         │
│                                                                                            │
│ romm-api      typed HTTP client (reqwest), auth/token refresh, paging, downloads          │
│ library-cache SQLite (sqlx), migrations, incremental refresh, FTS search, image cache     │
│ launcher      RetroDECK detection, ROM placement, flatpak launch, process watch, playtime │
│ sync          save/state/BIOS sync, per-system adapters, conflict engine, backups         │
│ updater       GitHub Releases self-update for the AppImage (KlikCore2 pattern)            │
│ input         gilrs gamepad reader, forwards normalized button events to the UI           │
│ platform      paths (XDG), keyring, logging, Steam shortcut (one, for Romulus itself)     │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

A Cargo workspace keeps the modules apart. Each one is its own crate and can be tested
without Tauri:

```
Cargo.toml                 workspace
crates/romm-api/           no Tauri, no SQLite
crates/library-cache/      depends on romm-api types only
crates/updater/
crates/launcher/           (Phase 2)
crates/sync/               (Phase 3) pure conflict engine + IO adapters
src-tauri/                 app shell: commands, input, config, secrets, logging, steam
ui/                        Svelte app (Vite)
tools/mock-romm/           fake RomM server for development and end-to-end tests
docs/
```

## Frontend: Svelte 5

Recommendation: **Svelte 5** rather than React. It compiles to small, direct DOM updates
with no virtual-DOM diffing, which matters on WebKitGTK on the Deck's APU. It also has
fine-grained reactivity (runes), so moving focus across a 2,000-item grid re-renders two
tiles, not the grid.

- Routing: a small in-house screen stack (push/pop with B = back). No URL router.
- Grid: a custom virtualized grid. Only visible rows plus one row of overscan are in the
  DOM. Covers are requested for the visible index range, which the grid already knows, so
  no `IntersectionObserver` is needed.
- Styling: plain CSS with custom properties. Animations limited to `transform` and
  `opacity`. No `backdrop-filter` or large blurs, which are expensive on WebKitGTK.

## Input

**Early warning (as requested):** the browser Gamepad API in **WebKitGTK** (the webview Tauri
uses on Linux) is the weakest part of this stack. It depends on how WebKitGTK was built
(libmanette), only reports after a user gesture in some versions, and has had bugs on
Linux. Bundling WebKitGTK in an AppImage makes this depend on the build machine.

Plan: **read the gamepad in Rust with `gilrs`** (evdev). In Gaming Mode, Steam Input exposes
the Deck as a virtual Xbox-style pad, which gilrs reads reliably. The Rust side sends
normalized events (`nav_up/down/left/right`, `a`, `b`, `x`, `y`, `lb`, `rb`, `lt`, `rt`,
`start`, `select`) with key repeat (300 ms delay, 80 ms rate) to the UI. The web Gamepad API
stays as a fallback behind a setting, and the keyboard maps to the same actions for
development.

Phase 1, task 1 is a spike that proves this on real Deck hardware before anything else
depends on it. If WebKitGTK rendering also turns out to be poor (scroll jank at 60 fps with
virtualization), I'll stop and report it, with alternatives (for example a native Rust UI).

Focus model: one `FocusManager` holds a tree of focus zones (header, grid, detail actions,
footer). Arrow moves inside a zone go by grid geometry. Moves across zones use explicit
neighbours, not DOM spatial guessing. The focused element scrolls into view with a short
eased animation.

## Data and cache (`library-cache`)

SQLite via `rusqlite` (bundled, with FTS5) at `~/.local/share/romulus/library-<hash of server
URL>.db`, WAL mode. One database per server. rusqlite was picked over sqlx: queries are
small and synchronous, run on `spawn_blocking`, and there is no async driver to set up.

Tables (first cut):
- `servers(id, url, user, romm_version, last_sync_at)`
- `platforms(id, server_id, slug, fs_slug, name, rom_count, logo_path, updated_at, json)`
- `roms(id, server_id, platform_id, name, sort_key, fs_name, size, cover_small, cover_large,
  updated_at, ra_id, json)` plus an FTS5 table on `name`, `alternative_names`
- `rom_files(rom_id, file_id, name, size, sha1)`
- `local_installs(rom_id, path, installed_at, size)` (Phase 2)
- `play_sessions(rom_id, start, end, duration_ms, uploaded)` (Phase 2)
- `sync_records(rom_id, kind, slot, file_name, local_hash, server_hash, server_id,
  synced_at)` (Phase 3)
- `meta(key, value)` for schema version and cursors

The full server JSON is kept in `json` so new fields do not need migrations until we filter
or sort on them.

**Refresh** (background task, also on demand with a button):
1. `GET /api/platforms?updated_after=T`, then upsert.
2. `GET /api/roms?updated_after=T&limit=500` paged, then upsert in one transaction per page.
3. `GET /api/roms/identifiers` and `/api/platforms/identifiers`, then delete rows the server
   no longer lists.
4. Save the new cursor `T`: the newest `updated_at` the server returned, minus 10 minutes
   of overlap. Using the server's own timestamps means the Deck's clock doesn't matter;
   the overlap only causes a few rows to be written twice. The UI shows "Last synced 3 min
   ago".

The first sync of a big library pages 500 ROMs at a time and the UI fills in live. Later
refreshes usually move only a few rows.

**Images**: `~/.cache/romulus/covers/<hash of server path>.<ext>`. RomM's cover paths carry
`?ts=<updated_at>`, so a changed cover gets a new file. The Rust side fetches with the auth
header; the UI shows the file through Tauri's asset protocol. At most 6 requests run at
once, newest first, and requests for tiles that scrolled away are dropped. An LRU limit
(default 1 GB) prunes old covers after each sync. RomM's `small` covers are already
thumbnail size, so nothing is resized.

## RomM client (`romm-api`)

- `reqwest` with rustls, one shared client, gzip, 15 s connect timeout.
- Endpoints and types come from `docs/reference/romm-openapi-*.json` and are written by hand
  for only what we use (see `docs/ROMM_API.md`). A test checks that every path we call
  exists in the snapshot.
- Auth: device pairing first (`/api/auth/device/init` → QR + code on screen → poll
  `/api/auth/device/token`), password grant (`/api/token`) as fallback. 401 → one refresh
  attempt → re-login screen.
- Version gate: `/api/heartbeat` on connect. Below the minimum supported version, show a
  clear message instead of failing in odd ways.
- Tests run against a `wiremock` mock server with fixtures recorded from the OpenAPI
  schema.

## Launcher (Phase 2)

- RetroDECK detection: read RetroDECK's config file
  (`~/.var/app/net.retrodeck.retrodeck/config/retrodeck/`, exact file and keys to be checked
  on a Deck) for `rdhome`/roms/bios/saves/states paths. Fall back to `~/retrodeck`, then
  SD-card mounts under `/run/media/`. The user can override this in settings.
- Folder per platform: RomM `fs_slug` → RetroDECK system folder, via a mapping table checked
  on a Deck (the names mostly match ES-DE's: `snes`, `psx`, `gba`, ...).
- Download to `<roms>/<system>/<file>.part`, resumable with HTTP Range if the server
  supports it, renamed on success, hash-checked against `rom_files.sha1` when present.
  `statvfs` checks free space first.
- Launch: `flatpak run net.retrodeck.retrodeck "<path>"` through `tokio::process`. Track
  the pid tree (the flatpak wrapper exits early, so watch `flatpak ps` / the
  `bwrap` child) and record a play session on exit.

## Sync (Phase 3)

- Pure, fully tested conflict engine:
  `decide(local: Option<FileState>, server: Option<FileState>, last_synced: Option<SyncRecord>) -> Action`
  where `Action ∈ {NoOp, Upload, Download, Conflict{..}}`. Rule: only one side changed
  since `last_synced` → that side wins; both changed → `Conflict` → ask the user. Before any
  overwrite, the losing copy goes to
  `~/.local/share/romulus/backups/<rom>/<timestamp>-<file>`. Nothing is ever deleted
  without a backup.
- RomM's `/api/sync/negotiate` is called and its operations are compared with our engine.
  Where they disagree, we take the safer action (back up and ask).
- `SaveAdapter` trait per system/emulator: where saves and states live, how they are named,
  which files belong to one game. First adapters: RetroArch cores (`saves/<system>/` and
  `states/<system>/` in RetroDECK), then PCSX2, Dolphin, PPSSPP, DuckStation, as needed.
- BIOS: `GET /api/firmware?platform_id=` → download → check md5/sha1 → copy into the bios
  folder (backing up any file it would replace) → per-platform ready/missing status.

## Updater

Mirrors KlikCore2's `launcher/update.rs`:
- On start (and from settings), `GET https://api.github.com/repos/<owner>/Romulus/releases/latest`.
  If `tag_name` is newer than `CARGO_PKG_VERSION` (numeric major.minor.patch, leading `v`
  ignored) and the release has `Romulus-x86_64.AppImage`, show "Update available" with the
  release notes.
- Tags with a `-` (`v0.3.0-test1`) are published as pre-releases, which `releases/latest`
  never returns, so test builds do not reach users.
- Install: download to `<$APPIMAGE>.download`, reject a suspiciously small file, `chmod 755`,
  rename the running AppImage to `Romulus.old.AppImage`, rename the new file into place
  (rolling back if that fails), then restart. The `.old` copy is deleted on the next start.
  Also, unlike KlikCore2: the release workflow publishes a `.sha256` and the updater checks
  it.
- Only when `$APPIMAGE` is set. Dev builds (running from `target/`) never update.
- Uses the app's existing `reqwest` client instead of shelling out to `curl`.
- "What's new" after updating: fetch `releases/tags/v<version>` notes on first start of a
  new version.
- The release workflow is the same shape as KlikCore2's `release.yml`: on a `v*` tag, build on
  `ubuntu-22.04` (old glibc for SteamOS compatibility), test, `tauri build --bundles appimage`,
  rename to the asset name the updater expects, upload with `softprops/action-gh-release`.

(Tauri's own `tauri-plugin-updater` also supports AppImage, but it needs a signing key pair
and a `latest.json` manifest. The KlikCore2 approach is simpler and already proven on your
Deck, so it's the default. We can switch later if signed updates become important.)

## Steam integration

One shortcut for Romulus itself, added from settings ("Add Romulus to Steam"), using the
same binary `shortcuts.vdf` approach as KlikCore2's `steam_shortcuts.rs`: Steam must be
closed, artwork goes in `grid/`, and the shortcut points at `$APPIMAGE`. Per-game shortcuts
are a non-goal.

## Cross-cutting

- **Config**: `~/.config/romulus/config.toml` (server URL, RetroDECK path override, audio,
  update channel). `APP_NAME` / `APP_ID` constants in one place (`src-tauri/src/app.rs` and
  `ui/src/lib/app.ts`, generated from one source).
- **Secrets**: `keyring` crate (Secret Service). Risk: in Gaming Mode the Secret Service
  daemon (KWallet) may be locked or missing. Fallback: a token file with mode `0600` in the
  config dir, with a visible notice in settings. To verify on a Deck in Phase 1.
- **Logging**: `tracing` + `tracing-appender`, daily rolling file in
  `~/.local/state/romulus/logs/`, level set in settings. "Copy debug info" copies version,
  SteamOS version, RomM version, RetroDECK paths and the last 200 log lines (tokens redacted)
  to the clipboard.
- **Errors**: `thiserror` in crates, mapped to user-facing messages at the command layer.

## Phase 1 task breakdown

Each task is one or more small commits. All done except the on-Deck spike (task 2), which
needs hardware: see `docs/DECK_CHECKLIST.md`.

1. **Scaffold**: Cargo workspace, Tauri 2 app, Svelte 5 + Vite + TS, lint/format (clippy,
   rustfmt, eslint, prettier), CI on push (test + build AppImage), `APP_NAME` constant.
2. **Deck spike (go/no-go)**: fullscreen window, gilrs input → UI events, a 2,000-tile
   virtual grid with dummy covers. Build an AppImage and run it in Gaming Mode. Check:
   controller works with Steam Input, 60 fps scrolling, startup time. *Report the result
   to you before going on.*
3. **Logging + config + paths** (XDG dirs, file logging, config load/save).
4. **romm-api crate**: heartbeat, device-pairing auth, password auth, token refresh, platforms,
   roms (paged, `updated_after`), identifiers, rom detail, cover fetch. Mock-server tests.
   Test that every path we call exists in the OpenAPI snapshot.
5. **Keyring storage** with the 0600 file fallback.
6. **library-cache crate**: schema + migrations, upserts, deletion by identifiers, cursor,
   FTS5 search, cover cache with LRU. Tests with an in-memory DB and the mock server.
7. **UI shell**: screen stack, FocusManager, footer button hints, toasts, theme.
8. **Screens**: Login (QR/code pairing + password fallback), Settings, Platform grid, Game
   grid (sort A–Z / recent), Game detail (cover, summary, metadata, screenshots), Search
   (on-screen-keyboard friendly: Steam's keyboard via Steam+X, plus letter-jump with the
   triggers using `char_index`).
9. **Background refresh** with "last synced" indicator and manual refresh (Y on platform
   screen).
10. **Updater** (KlikCore2 pattern) + release workflow publishing `Romulus-x86_64.AppImage`
    and its `.sha256`.
11. **Copy debug info** in settings.
12. **"Add Romulus to Steam"** (single shortcut).

Phase 1 is done when: a fresh Deck can install the AppImage, pair with RomM, browse and
search the whole library with the controller only, reopen to an instant cached view, and
update itself from a GitHub release.

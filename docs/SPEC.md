# Romulus: product spec

Romulus is a fast, controller-first RomM client for the Steam Deck. The name is a working
name and lives in one constant (`APP_NAME`, see CLAUDE.md).

## Goal

A fullscreen "big picture" app that browses a self-hosted RomM library, downloads games on
demand, launches them through RetroDECK, and keeps saves, save states and BIOS in sync with
the RomM server. It replaces decky-romm-sync, which is slow to sync and a bit clunky.

## Core design principles

1. **No bulk sync.** Never register one Steam shortcut per game. The app has its own UI and
   is added to Steam once, as a single non-Steam shortcut.
2. **Local cache first.** Library metadata lives in SQLite. Refreshes are incremental
   (`updated_after` plus ID-set comparison), never full rescans. The UI renders instantly
   from cache and updates in the background.
3. **Lazy everything.** Fetch cover art at thumbnail size only for items on screen and cache
   it to disk. Download ROMs only when the user presses Play.
4. **Controller-first.** Every action is reachable with the Deck's gamepad. Touch and mouse
   are secondary. Button hints (A/B/X/Y/bumpers/triggers) are shown in the footer.

## Tech stack

- Tauri 2, TypeScript, Svelte 5 frontend, Rust backend for file IO, sync and process launching.
- SQLite (rusqlite, bundled with FTS5).
- Gamepad input, with Steam Input passing through (see ARCHITECTURE.md, "Input").
- Target: SteamOS (Linux x86_64). AppImage first, Flatpak later.
- The AppImage updates itself from GitHub Releases, the same way KlikCore2 does.
- If Tauri's gamepad handling or rendering are poor on the Deck, raise it in Phase 1 rather
  than working around it silently.

## RomM integration

- Talk directly to the RomM REST API. Endpoints are taken from the server's OpenAPI schema,
  never guessed. A snapshot for RomM 5.4.1 is in `docs/reference/`, with notes in
  `docs/ROMM_API.md`. At runtime, read `/api/heartbeat` to check the server version.
- Config: server URL and credentials. Tokens are stored in the system keyring, not in
  plaintext.
- Needs: platforms, ROM list per platform, ROM detail and metadata, covers and screenshots,
  file download, saves, states, firmware/BIOS, and RetroAchievements data if the server
  exposes it.
- Each user uses their own RomM account, because saves are per user.

## RetroDECK integration

- Launch: `flatpak run net.retrodeck.retrodeck "<rom path>"`. RetroDECK picks the emulator
  for each system, so Romulus does not keep its own core mapping.
- Detect the RetroDECK folder (default `~/retrodeck`, may be on the SD card) from its config,
  and let the user override it in settings. Confirm the real layout of roms/bios/saves/states
  on a Deck before hardcoding anything.
- Watch the launched process, and start a save sync when it exits.

## Features by phase

### Phase 1: browsing (MVP)
- Login/settings screen, platform grid, game grid with covers, game detail page, search.
- Full gamepad navigation with focus states and smooth scrolling.
- SQLite cache with incremental refresh and a visible "last synced" indicator.
- Self-update from GitHub Releases (check on start, install on confirm).

### Phase 2: play
- Download a ROM to the right RetroDECK system folder, with a progress UI, resume support
  and a disk space check.
- Launch through RetroDECK. Track playtime and recently played.
- Library management: downloaded badge, delete local copy.

### Phase 3: sync
- Saves and save states: two-way sync with RomM on launch and on exit. Conflicts: compare
  timestamps and hashes, always back up the losing copy, and ask the user if both sides
  changed.
- BIOS: pull firmware per platform, verify hashes, place it in RetroDECK's bios folder, and
  show "BIOS ready/missing" per platform.
- Per-emulator file layouts (RetroArch cores vs standalone emulators) go through a
  per-system adapter, so no format is assumed.

### Phase 4: extras
- RetroAchievements: progress and badges on the game page. Use RomM's RA data if available,
  otherwise the RA API. Make sure RetroDECK/RetroArch has RA credentials (document how, or
  write them with the user's consent).
- Focus audio: a short clip for the focused game, with fade in/out, a debounce so fast
  scrolling does not spam audio, a global mute and a volume setting. Sources, in order:
  video/audio previews from RomM metadata, then a user-provided folder of audio matched by
  game name. Copyrighted soundtracks are never scraped.
- Discovery mode (swipe-style "what should I play"), favorites and collections.
- Later, optional: streaming through Sunshine/Moonlight for systems RetroDECK cannot run well.

## Non-goals (for now)

Per-game Steam shortcuts, a desktop-only UI, Windows/macOS builds, hosting or distributing ROMs.

## Engineering requirements

- Modules stay separate: `romm-api`, `library-cache`, `launcher`, `sync`, `ui` (plus
  `updater`).
- Tests for the sync conflict logic and the API client (against a mock server).
- Never delete user save data; always back up before overwriting.
- Log to a file with levels; settings has a "copy debug info" button.
- Commit in small steps with clear messages.

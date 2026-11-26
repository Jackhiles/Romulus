# Romulus: project conventions

Controller-first RomM client for the Steam Deck. Read `docs/SPEC.md` (what) and
`docs/ARCHITECTURE.md` (how) before changing anything substantial.

## Ground rules

- **Never guess RomM endpoints.** Check `docs/reference/romm-openapi-*.json` and
  `docs/ROMM_API.md`. If an endpoint is not in the snapshot, regenerate the snapshot from the
  server's `/openapi.json`; do not invent it.
- **Never delete user save data.** Every overwrite or removal of a save, state or BIOS file
  first copies it to the backups folder. No exceptions, including in tests and tools.
- **No per-game Steam shortcuts.** Romulus is added to Steam once.
- **Cache first.** UI reads from SQLite; network work happens in the background and updates
  the cache.
- **Controller first.** Every new UI action needs a gamepad binding and a footer hint.
- RetroDECK paths come from detection or settings, never hardcoded literals in logic.
- The app name lives only in `APP_NAME`/`APP_ID` (Rust: `src-tauri/src/app.rs`, TS:
  `ui/src/lib/app.ts`).

## Layout

- `crates/romm-api`, `crates/library-cache`, `crates/updater` (later `crates/launcher`,
  `crates/sync`): Rust library crates, no Tauri dependency.
- `src-tauri/`: Tauri shell (commands, gamepad input, config, secrets, logging, Steam
  shortcut).
- `ui/`: Svelte 5 + TypeScript frontend. `ui/src/lib` has the logic (grid math, input
  routing, API wrappers), `ui/src/screens` one component per screen.
- `tools/mock-romm/`: fake RomM server (`python3 tools/mock-romm/server.py`) for
  development and end-to-end checks.

## Code style

- Rust: stable toolchain, `cargo fmt`, `cargo clippy -- -D warnings`. Errors via `thiserror`
  in crates; no `unwrap()` outside tests. Async with tokio. Logging with `tracing`; never
  log tokens or passwords.
- TS/Svelte: strict TypeScript, Svelte 5 runes, prettier + eslint. Animate only `transform`
  and `opacity`.
- Keep Tauri commands thin; logic belongs in the crates where it can be unit tested.
- Svelte effects: never read state synchronously in an `$effect` that the effect itself
  writes (directly or through a promise callback that replaces it); wrap such reads in
  `untrack`. A loop like that floods Rust with commands and freezes rendering.
- Only the top screen is mounted. Anything a screen must remember across Back goes on its
  stack entry (`entry.index`, `entry.sort`, `entry.query`).

## Tests

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `npm --prefix ui run check` and `npm --prefix ui test` must pass before committing
  (CI runs the same).
- API client: tests against a `wiremock` mock server.
- Sync conflict engine: table-driven tests covering every (local, server, last-synced) case.

## Git

- Small commits with clear imperative messages (`romm-api: add paged rom listing`).
- Release: bump `version` in the root `Cargo.toml` (tauri.conf.json takes it from there),
  tag `vX.Y.Z`. Tags with a `-` are pre-releases and are ignored by the updater.

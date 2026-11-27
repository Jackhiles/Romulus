# Romulus

A fast, controller-first [RomM](https://github.com/rommapp/romm) client for the Steam Deck.
Browse your self-hosted RomM library from a fullscreen "big picture" app that is added to
Steam once, and (in later phases) download games on demand, launch them through RetroDECK,
and keep saves, states and BIOS in sync.

Status: **Phase 1 (browsing) is done.** Downloading and playing are Phase 2. See
[docs/SPEC.md](docs/SPEC.md) for the plan and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
for how it works.

## Install on the Steam Deck

1. In Desktop Mode, download `Romulus-x86_64.AppImage` from the
   [latest release](https://github.com/Jackhiles/Romulus/releases/latest) into
   `~/Applications`, and make it executable (right-click → Properties → Permissions →
   "Is executable").
2. Start it, enter your RomM server address and approve the pairing code on your phone
   (or sign in with your username and password).
3. Settings (☰) → **Add to Steam**. This closes Steam, adds one shortcut and starts Steam
   again.
4. Back in Gaming Mode, start Romulus from Non-Steam in your library. Use the default
   gamepad layout (or "Gamepad" in Steam Input); Romulus reads the controller directly.

Romulus updates itself: when a new release is out it shows a message on start, and you
install it from Settings.

## Controls

| Button | Action |
|---|---|
| D-pad / left stick | Move. In a system's game list, left / right switch to the previous / next system |
| A | Open / select |
| B | Back |
| X | Sync library (systems screen), newest first / A–Z (games) |
| Y | Search |
| ⧉ (Select) | Change layout: carousel / list / grid (systems), list / grid (games, remembered per system) |
| LB / RB | Page up / down; previous / next game on the detail page |
| LT / RT | Jump to previous / next letter |
| ☰ (Start) | Settings |

## Where files go

| What | Where |
|---|---|
| Settings | `~/.config/romulus/config.toml` |
| Library cache | `~/.local/share/romulus/library-*.db` |
| Covers | `~/.cache/romulus/covers/` (pruned at 1 GB) |
| Logs | `~/.local/state/romulus/logs/` |
| Credentials | `~/.config/romulus/credentials.json` (readable only by you) |

## Development

Requirements: Rust (stable), Node 22, and the Tauri Linux libraries
(`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libudev-dev libdbus-1-dev`).

```sh
npm --prefix ui ci
# A fake RomM server with 1500 games (any password works, pairing auto-approves):
python3 tools/mock-romm/server.py --port 8099 &
ui/node_modules/.bin/tauri dev           # then use http://localhost:8099 as the server
cargo test --workspace && npm --prefix ui test
ui/node_modules/.bin/tauri build --bundles appimage
```

Keyboard works for development: arrows, Enter (A), Esc (B), `x`, `y`, `q`/`e` (LB/RB),
`[`/`]` (LT/RT), Tab (⧉), F1 (☰).

The UI can also run in a plain browser without the Rust side, against a fixture folder
(`library.json`, `assets.json`, `assets/`; see `ui/src/dev/mockTauri.ts`) served with CORS:
`VITE_MOCK=http://127.0.0.1:1421 npm --prefix ui run dev`. The mock is dev-only and never
part of a build.

Releasing: bump `version` in the root `Cargo.toml`, commit, and push a tag `vX.Y.Z`, or
start Actions → Release → Run workflow with the tag name (it creates the tag). The release
workflow builds the AppImage and publishes it with a checksum. Tags with a dash
(`v0.2.0-test1`) become pre-releases, which the updater ignores.

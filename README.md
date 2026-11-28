# Romulus

A fast, controller-first [RomM](https://github.com/rommapp/romm) client for the Steam Deck.
Browse your self-hosted RomM library from a fullscreen "big picture" app that is added to
Steam once, and (in later phases) download games on demand, launch them through RetroDECK,
and keep saves, states and BIOS in sync.

Status: **browsing, downloading, playing through RetroDECK and save sync work on a Deck.**
Saves sync for RetroArch systems (PS1 with SwanStation, N64, GBA, NDS, ...), PS1 with
DuckStation (interchangeable with SwanStation's), PS2 (PCSX2), PSP (PPSSPP), GameCube
and Wii (Dolphin) and Wii U (Cemu); save
states sync too, between devices using the same emulator. Your RomM collections and
Favourites show on the systems screen. BIOS files come from RomM; RetroAchievements
progress shows per game; games play their soundtrack (from RomM or your own music
folder). See
[docs/SPEC.md](docs/SPEC.md) for the plan and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
for how it works.

## Install on the Steam Deck

1. In Desktop Mode, download `Romulus-x86_64.AppImage` from the
   [latest release](https://github.com/Jackhiles/Romulus/releases/latest) into
   `~/Applications`, and make it executable (right-click → Properties → Permissions →
   "Is executable").
2. Start it, enter your RomM server address and approve the pairing code on your phone
   (or sign in with your username and password).
3. Settings (☰) → **Add to Steam**. This closes Steam, adds one shortcut (with its library
   artwork: capsule, banner, hero, logo and icon) and starts Steam again. Artwork you set
   yourself in Steam is kept; a shortcut added by an older version gets the missing
   images the next time Romulus starts.
4. Back in Gaming Mode, start Romulus from Non-Steam in your library. Use the default
   gamepad layout (or "Gamepad" in Steam Input); Romulus reads the controller directly.

Romulus updates itself: when a new release is out it shows a message on start, and you
install it from Settings.

## Controls

| Button | Action |
|---|---|
| D-pad / left stick | Move. In a system's game list, left / right switch to the previous / next system (up / down in the carousel) |
| A | Open / select |
| B | Back |
| X | Sync library (systems screen), newest first / A–Z (games) |
| Y | Search |
| ⧉ (Select) | Change layout: carousel / list / grid (systems), list / grid / big covers / carousel (games, remembered per system) |
| LB / RB | Page up / down; previous / next game on the detail page |
| LT / RT | Jump to previous / next letter; on a game's page, switch between its versions |
| RT (systems screen) | Surprise me: a random game (Select on its page picks another) |
| LT (systems screen) | Discover: flick through random games. A opens one, Y keeps it (adds it to Favourites), X skips, Select picks from the whole library, this Deck or games never played here |
| Y (game page) | RetroAchievements list |
| ☰ (Start, game page) | Add to / remove from Favourites (on RomM) |
| ☰ (Start) | Settings |

## Where files go

| What | Where |
|---|---|
| Settings | `~/.config/romulus/config.toml` |
| Library cache | `~/.local/share/romulus/library-*.db` |
| Covers | `~/.cache/romulus/covers/` (pruned at 1 GB) |
| Logs | `~/.local/state/romulus/logs/` |
| Save backups | `~/.local/share/romulus/backups/<game id>/` (every save Romulus replaces; restore from Settings → Save backups); replaced save states in `backups/states/<game id>/` |
| Your own game music | `~/.local/share/romulus/music/` (a file named like the game, e.g. `Katamari Damacy.mp3`) |
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

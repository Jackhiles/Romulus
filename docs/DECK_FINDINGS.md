# What a real Deck looks like

Gathered over SSH from the developer's Steam Deck on 2026-11-26 (SteamOS 3.9.5, RetroDECK
0.10.10b, RomM 5.4.1). Phase 2 and 3 code is built against this, not against guesses.

## Romulus in Gaming Mode

Environment Steam gives the process (started from its non-Steam shortcut):
`SteamDeck=1`, `SteamGamepadUI=1`, `XDG_CURRENT_DESKTOP=gamescope`, `DISPLAY=:1`,
`APPIMAGE=~/Applications/Romulus-x86_64.AppImage`, no `DBUS_SESSION_BUS_ADDRESS` secret
service.

- gilrs sees `Steam Virtual Gamepad (Microsoft X-Box 360 pad 0)` in Gaming Mode and
  `Valve Steam Deck` in Desktop Mode.
- KWallet answers in Desktop Mode but is "not activatable" in Gaming Mode. Credentials
  saved in Desktop Mode were unreadable in Gaming Mode, so 0.1.1 keeps them in
  `~/.config/romulus/credentials.json` (0600) only.
- "Add to Steam" from Desktop Mode worked; Steam starts the AppImage directly.

## RomM server behaviour

- 4,321 ROMs on 23 platforms. A `/api/roms` page of 500 is about 7.6 MB of JSON
  (`ss_metadata` and `igdb_metadata` are most of each item).
- Response time varies a lot under load: the same 100-item page took 3 s, 7 s and more
  than 60 s at different moments. 0.1.0's fixed 60 s read timeout and restart-from-zero
  sync never finished a first sync. 0.1.1: 180 s read timeout, 250-item pages, retries
  with backoff, a checkpoint after each page, and missing ids fetched one by one.

## RetroDECK paths

The source of truth is `~/.var/app/net.retrodeck.retrodeck/config/retrodeck/retrodeck.json`
(not the older `retrodeck.cfg`), object `paths`:

| Key | Value here |
|---|---|
| `rd_home_path` | `/run/media/deck/<card label>/retrodeck` |
| `roms_path` | `<rd_home>/roms` |
| `bios_path` | `<rd_home>/bios` |
| `saves_path` | `<rd_home>/saves` |
| `states_path` | `<rd_home>/states` |
| `sdcard` | `/run/media/deck/<card label>` |

The card label contains a space ("1TB SD"), so paths must never go through a shell
unquoted.

## ROM folders

RetroDECK has 173 system folders under `roms/`. RomM `fs_slug` → folder for this library:

| RomM | ROMs | RetroDECK folder |
|---|---|---|
| gba, gb, gbc, psx, n64, arcade, nes, c64, psp, switch, atari2600, psvita, wii, ps2, nds, wiiu, ps3, xbox | 1166 … 1 | same name |
| dc | 87 | `dreamcast` |
| ngc | 37 | `gc` |
| 3ds | 0 | `n3ds` |
| win | 18 | none (not a RetroDECK system) |
| ps4 | 1 | none |

Multi-disc games are folders named like the playlist: `roms/psx/Parasite eve.m3u/`.

## Saves and states (Phase 3)

RetroDECK keeps per-emulator layouts under `saves/` and `states/`:

- RetroArch cores: `saves/<system>/<game>.srm`; for directory-based games the folder is
  named after the content folder (`saves/Parasite eve.m3u/Parasite eve.srm`).
- DuckStation: `saves/psx/duckstation/memcards/`
- PCSX2: `saves/ps2/pcsx2/memcards/` (folder memory cards)
- Dolphin / PrimeHack: `saves/gc/dolphin/{US,EU,JP}/`, `saves/gc/primehack/…`
- PPSSPP: `saves/PSP/PPSSPP-SA/`
- melonDS: `saves/nds/melonds/`; Azahar (3DS): `saves/n3ds/azahar/{nand,sdmc}`
- RPCS3: `saves/ps3/rpcs3/`; Vita3K: `saves/psvita/vita3k/`

Another sync tool (decky-romm-sync) has already written into these folders:
`romm-save-<id>-<name> [<timestamp>].srm` files and `.romm-backup/` folders, plus the
user's own `.manual-backup/` folders. Phase 3 must leave all of them alone and must not
treat them as saves to upload.

## BIOS

`bios/` holds 7,700+ files already (PS1 and PS2 BIOS sets, Dolphin, PPSSPP, MAME
folders, …). RomM has firmware for psx (17), ps2 (7), switch (5), xbox (3), gba (1). BIOS
placement must check hashes against what is already there and never overwrite without a
backup.

## Launching and performance (0.3.1, 2026-11-27)

- Host programs started from the AppImage must not inherit its environment:
  `LD_LIBRARY_PATH` pointing at the bundled GLib made `flatpak` fail at once
  ("symbol lookup error ... g_once_init_leave_pointer", exit 127). `launcher::env` strips
  every path into an AppImage mount. With that, `flatpak run net.retrodeck.retrodeck
  "<game>.m3u"` starts the game in Gaming Mode and returns when it is quit.
- Play sessions reach RomM (`POST /api/play-sessions`, 201).
- Frame times while scrolling: 18–29 fps with WebKitGTK's compatible renderer, about
  60 fps (p95 under 26 ms) with the DMA-BUF renderer, which is now the default.
- A copy started by the self-updater inherits the previous copy's environment, so
  settings carried in environment variables must be set or cleared explicitly at start.

## Save sync (0.4.0)

- RetroDECK's default emulators: SwanStation (RetroArch) for `psx`, PCSX2 standalone for
  `ps2` (first `<command>` in the bundled `es_systems.xml`; no overrides in this Deck's
  gamelists).
- RetroArch: `savefile_directory` is RetroDECK's `saves`, `sort_savefiles_by_content_enable
  = true`, so a game's battery save is `saves/<game's folder>/<game>.srm` (for a single
  file in `roms/psx/`, `saves/psx/<game>.srm`).
- PCSX2: slot 1 is the folder card `saves/ps2/pcsx2/memcards/Memcard.ps2/`; each game's
  saves are folders named after its serial (`BASLUS-21008Opt`, ...). `_pcsx2_index` files
  are rewritten on access.
- RomM knows `save_target` for 434/436 PS1 games (`SLUS-00404`, `file-prefix`) and 13/15
  PS2 games (`BASLUS-21008`, `folder-prefix`).
- decky-romm-sync's saves on RomM: slot `autosave`, emulator `retroarch-<core>` or `pcsx2`,
  PS2 as a zip of the game's card folders. RomM's `content_hash` of that zip matched one
  recomputed from its entries, which is how Romulus hashes card folders.
- A new PCSX2 folder card has an empty `_pcsx2_superblock` (PCSX2 creates it that way) and
  counts as unformatted: PCSX2 ignores the card's folders, so restored saves don't show,
  and Katamari Damacy only reported "no data" instead of offering to format. Writing the
  superblock a PS2 writes when formatting an 8 MB card (`sync::layout::formatted_superblock`)
  fixed it on the Deck: PCSX2 logged `FolderMcd: Indexing slot 0 with filter "SLUS-21008"`
  and the restored save loaded. Romulus now does this before syncing, only for a missing
  or empty superblock, backing up the old file.

## More emulators (save sync)

- RetroDECK's defaults here: PSP PPSSPP (standalone), GameCube/Wii Dolphin, 3DS Azahar,
  Vita Vita3K, Switch Ryubing, PS3 RPCS3, Xbox xemu; N64, GBA, NDS and Dreamcast use
  RetroArch cores.
- PPSSPP: `config/ppsspp/PSP/SAVEDATA` is a link to `saves/PSP/PPSSPP-SA`; each game's
  saves are folders starting with its ID. RomM has `save_target` (`ULUS10567`,
  `folder-prefix`) for all 84 PSP games, so PSP saves sync like PCSX2's card folders.
- DuckStation: per-title memory cards (`Card1Type = PerGameTitle`) in
  `saves/psx/duckstation/memcards`; named by DuckStation's own title database, so not
  synced yet (SwanStation is the PS1 default).
- BIOS: RomM's 23 PS1/PS2 files are all in `bios/` and match RomM's md5; GBA's
  `gba_bios.bin` is missing; Switch and Xbox firmware need their own layouts.

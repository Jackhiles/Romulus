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

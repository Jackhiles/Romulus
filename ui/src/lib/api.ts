// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface Status {
  app_name: string
  version: string
  server_url: string | null
  signed_in: boolean
  credential_store: 'file' | null
  rom_count: number
  last_sync_at: number | null
  refreshing: boolean
  updatable: boolean
  fullscreen: boolean
  in_steam: boolean
  gaming_mode: boolean
}

export interface Platform {
  id: number
  slug: string
  fs_slug: string
  name: string
  rom_count: number
  logo: string | null
}

export interface Rom {
  id: number
  platform_id: number
  title: string
  fs_name: string
  size_bytes: number
  cover_small: string | null
  cover_large: string | null
  updated_at: string | null
}

export interface RomDetail extends Rom {
  data: Record<string, any>
}

export interface PairInfo {
  device_code: string
  user_code: string
  url: string
  expires_in: number
  interval: number
}

export interface Release {
  version: string
  url: string
  sha256_url: string | null
  notes: string
}

export interface Settings {
  server_url: string | null
  username: string | null
  log_level: string
  check_updates: boolean
  fullscreen: boolean
  cover_cache_mb: number
  web_gamepad: boolean
  fast_renderer: boolean
  save_sync: boolean
  systems_view: SystemsView
  games_view: Record<string, GamesView>
}

export type SystemsView = 'carousel' | 'list' | 'grid'
export type GamesView = 'list' | 'grid'

export type Sort = 'title' | 'newest'

export type RefreshEvent =
  | { stage: 'platforms' }
  | { stage: 'roms'; done: number; total: number | null }
  | { stage: 'cleanup' }
  | { stage: 'done'; stats: { full: boolean; roms_updated: number; roms_removed: number } }
  | { stage: 'error'; message: string }

export const api = {
  status: () => invoke<Status>('status'),
  setServer: (url: string) => invoke<{ url: string; version: string }>('set_server', { url }),
  pairStart: () => invoke<PairInfo>('pair_start'),
  pairPoll: (deviceCode: string) =>
    invoke<'pending' | 'slow_down' | 'approved' | 'denied' | 'expired'>('pair_poll', { deviceCode }),
  passwordLogin: (username: string, password: string) => invoke<void>('password_login', { username, password }),
  signOut: () => invoke<void>('sign_out'),
  platforms: () => invoke<Platform[]>('platforms'),
  roms: (platformId: number, order: Sort) => invoke<Rom[]>('roms', { platformId, order }),
  rom: (id: number) => invoke<RomDetail | null>('rom', { id }),
  search: (query: string) => invoke<Rom[]>('search', { query }),
  collection: (kind: 'recent' | 'installed') => invoke<Rom[]>('collection', { kind }),
  refreshLibrary: () => invoke<boolean>('refresh_library'),
  checkUpdate: () => invoke<Release | null>('check_update'),
  installUpdate: () => invoke<void>('install_update'),
  restartApp: () => invoke<void>('restart_app'),
  whatsNew: () => invoke<string | null>('whats_new'),
  settings: () => invoke<Settings>('settings'),
  updateSettings: (patch: Partial<Settings>) => invoke<void>('update_settings', { patch }),
  setGamesView: (platform: string, view: GamesView) => invoke<void>('set_games_view', { platform, view }),
  platformCovers: (perPlatform: number) =>
    invoke<Record<string, string[]>>('platform_covers', { perPlatform }),
  debugInfo: () => invoke<string>('debug_info'),
  openKeyboard: () => invoke<void>('open_keyboard'),
  addToSteam: () => invoke<string>('add_to_steam'),
  quit: () => invoke<void>('quit'),
}

export function onRefresh(f: (e: RefreshEvent) => void): Promise<UnlistenFn> {
  return listen<RefreshEvent>('refresh', (e) => f(e.payload))
}

export function onUpdateProgress(f: (done: number, total: number | null) => void): Promise<UnlistenFn> {
  return listen<{ done: number; total: number | null }>('update-progress', (e) => f(e.payload.done, e.payload.total))
}

// ---- assets (covers, screenshots) ------------------------------------------------------
//
// Each request goes through Rust (auth header, disk cache). At most a few run at once,
// and requests for tiles that scrolled away before their turn are dropped.

const urls = new Map<string, string>()
const inflight = new Map<string, Promise<string>>()
const queue: { path: string; wanted: () => boolean; resolve: (u: string) => void; reject: (e: unknown) => void }[] = []
let running = 0
const MAX_PARALLEL = 6

function pump() {
  while (running < MAX_PARALLEL && queue.length > 0) {
    // Newest first: the tiles the user is looking at now.
    const job = queue.pop()!
    if (!job.wanted()) {
      inflight.delete(job.path)
      job.reject(new Error('cancelled'))
      continue
    }
    running++
    invoke<string>('asset', { path: job.path })
      .then((file) => {
        const u = convertFileSrc(file)
        urls.set(job.path, u)
        job.resolve(u)
      })
      .catch(job.reject)
      .finally(() => {
        inflight.delete(job.path)
        running--
        pump()
      })
  }
}

/** URL for a server asset path, or null if it is not loaded yet. */
export function assetUrlCached(path: string | null): string | null {
  return path ? (urls.get(path) ?? null) : null
}

/** Load a server asset. `wanted` is checked just before the request starts. */
export function assetUrl(path: string, wanted: () => boolean = () => true): Promise<string> {
  const hit = urls.get(path)
  if (hit) return Promise.resolve(hit)
  const existing = inflight.get(path)
  if (existing) return existing
  const p = new Promise<string>((resolve, reject) => {
    queue.push({ path, wanted, resolve, reject })
  })
  inflight.set(path, p)
  pump()
  return p
}

// ---- Phase 2: installs and playing --------------------------------------------------

export type InstallStatus =
  | { state: 'installed'; size_bytes: number; path: string }
  | { state: 'downloading' }
  | { state: 'not_installed' }
  | { state: 'unavailable'; reason: string }

export type DownloadEvent =
  | { stage: 'progress'; rom_id: number; done: number; total: number }
  | { stage: 'done'; rom_id: number }
  | { stage: 'cancelled'; rom_id: number }
  | { stage: 'failed'; rom_id: number; message: string }

export type GameEvent =
  | { stage: 'started'; rom_id: number }
  | { stage: 'ended'; rom_id: number; duration_ms: number; save: string | null }
  | { stage: 'failed'; rom_id: number; message: string }

export type SaveChoice = 'keep_local' | 'use_server'

export interface SaveSyncStatus {
  syncs: boolean
  emulator: string | null
  note: string | null
  synced_at: string | null
  pending: boolean
}

export interface SaveConflict {
  local_modified: string | null
  server_updated: string
  server_device: string | null
}

export type PlayStart = { state: 'started'; save: string | null; warning: string | null } | { state: 'conflict'; info: SaveConflict }

export type BiosState = 'present' | 'missing' | 'different' | 'unsupported'

export interface BiosSystem {
  platform_id: number
  name: string
  slug: string
  supported: boolean
  files: { id: number; name: string; size_bytes: number; verified: boolean; state: BiosState }[]
}

export const bios = {
  status: () => invoke<BiosSystem[]>('bios_status'),
  download: (platformId: number) =>
    invoke<{ installed: number; failed: string[] }>('download_bios', { platformId }),
}

export const games = {
  status: (romId: number) => invoke<InstallStatus>('install_status', { romId }),
  installedIds: () => invoke<number[]>('installed_ids'),
  download: (romId: number) => invoke<void>('download_rom', { romId }),
  cancel: (romId: number) => invoke<void>('cancel_download', { romId }),
  remove: (romId: number) => invoke<void>('remove_rom', { romId }),
  play: (romId: number, choice?: SaveChoice) => invoke<PlayStart>('play_rom', { romId, choice: choice ?? null }),
  saveSyncStatus: (romId: number) => invoke<SaveSyncStatus | null>('save_sync_status', { romId }),
}

export function onDownload(f: (e: DownloadEvent) => void): Promise<UnlistenFn> {
  return listen<DownloadEvent>('download', (e) => f(e.payload))
}

export function onGame(f: (e: GameEvent) => void): Promise<UnlistenFn> {
  return listen<GameEvent>('game', (e) => f(e.payload))
}

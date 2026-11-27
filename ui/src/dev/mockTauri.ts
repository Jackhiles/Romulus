// Development only: stands in for the Rust backend so the UI can run in a normal
// browser. Enabled by `VITE_MOCK=<fixture url>` with `npm run dev`; never part of a
// build. The fixture is `library.json` ({platforms, roms: [{...row, data}]}) plus
// `assets.json` (server path → file name) and the files under `assets/`.
//
// Keyboard: arrows, Enter (A), Esc (B), x, y, q/e (LB/RB), [/] (LT/RT), Tab (Select),
// F1 (Start).

interface FixtureRom {
  id: number
  platform_id: number
  title: string
  updated_at: string | null
  cover_small: string | null
  data: Record<string, unknown>
  [k: string]: unknown
}

const base = String(import.meta.env.VITE_MOCK).replace(/\/$/, '')
const lib: { platforms: { id: number; slug: string }[]; roms: FixtureRom[] } = await (
  await fetch(`${base}/library.json`)
).json()
const assets: Record<string, string> = await (await fetch(`${base}/assets.json`)).json()

const settings = {
  server_url: 'https://romm.example/',
  username: null,
  log_level: 'info',
  check_updates: false,
  fullscreen: false,
  cover_cache_mb: 1024,
  web_gamepad: false,
  fast_renderer: true,
  save_sync: true,
  music: true,
  music_volume: 40,
  systems_view: 'carousel',
  games_view: {} as Record<string, string>,
}
const sortKey = (t: string) => t.toLowerCase().replace(/^(the|a|an)\s+/, '')
const row = (r: FixtureRom) => {
  const { data: _data, ...rest } = r
  return rest
}

// Events: listeners registered through `plugin:event|listen`, fired by emit().
const callbacks = new Map<number, (e: unknown) => void>()
const listeners: { event: string; handler: number }[] = []
function emit(event: string, payload: unknown) {
  for (const l of listeners) if (l.event === event) callbacks.get(l.handler)?.({ event, id: 0, payload })
}

// Device state: one game starts installed, downloads take a few seconds.
const installed = new Set<number>([lib.roms[0]?.id])
const conflicted = new Set<number>()
const downloading = new Map<number, ReturnType<typeof setInterval>>()

const handlers: Record<string, (a: any) => unknown> = {
  install_status: ({ romId }) =>
    downloading.has(romId)
      ? { state: 'downloading' }
      : installed.has(romId)
        ? { state: 'installed', size_bytes: 1, path: '/mock' }
        : { state: 'not_installed' },
  installed_ids: () => [...installed],
  download_rom: ({ romId }) => {
    const total = 700 * 1024 * 1024
    let done = 0
    downloading.set(
      romId,
      setInterval(() => {
        done = Math.min(total, done + total / 16)
        emit('download', { stage: 'progress', rom_id: romId, done, total })
        if (done >= total) {
          clearInterval(downloading.get(romId))
          downloading.delete(romId)
          installed.add(romId)
          emit('download', { stage: 'done', rom_id: romId })
        }
      }, 250),
    )
  },
  cancel_download: ({ romId }) => {
    clearInterval(downloading.get(romId))
    downloading.delete(romId)
    emit('download', { stage: 'cancelled', rom_id: romId })
  },
  remove_rom: ({ romId }) => installed.delete(romId),
  play_rom: ({ romId, choice }) => {
    // The first play of each game reports a save conflict, to preview the dialog.
    if (!choice && !conflicted.has(romId)) {
      conflicted.add(romId)
      return {
        state: 'conflict',
        info: { local_modified: '2026-11-27T21:14:00Z', server_updated: '2026-11-26T19:28:01', server_device: 'dev' },
      }
    }
    emit('game', { stage: 'started', rom_id: romId })
    setTimeout(() => emit('game', { stage: 'ended', rom_id: romId, duration_ms: 95 * 60000, save: 'uploaded' }), 2500)
    return { state: 'started', save: choice === 'use_server' ? 'downloaded' : null, warning: null }
  },
  collection: ({ kind }) =>
    kind === 'installed'
      ? lib.roms.filter((r) => installed.has(r.id)).map(row)
      : [lib.roms[40], lib.roms[120], lib.roms[200], lib.roms[7]].filter(Boolean).map(row),
  downloads: () => [...downloading.keys()].map((id, i) => ({ rom_id: id, title: lib.roms.find((r) => r.id === id)?.title ?? '?', platform: 'Arcade', state: i === 0 ? 'active' : 'queued', done: 1e8, total: 7e8 })),
  random_rom: () => row(lib.roms[Math.floor(Math.random() * lib.roms.length)]),
  preview_music: () => false,
  stop_music: () => null,
  achievements: () => ({ total: 3, earned: 1, earned_hardcore: 0, points_total: 30, points_earned: 10, list: [
    { title: 'Piece Be With You', description: 'Clear the first area', points: 10, badge: null, earned: true, earned_at: '2026-10-09 02:54:14' },
    { title: 'Wise Guy', description: 'Talk to everyone', points: 10, badge: null, earned: false, earned_at: null },
    { title: 'Finisher', description: 'Beat the game', points: 10, badge: null, earned: false, earned_at: null },
  ] }),
  bios_status: () => [
    { platform_id: 1, name: 'PlayStation', slug: 'psx', supported: true, files: [
      { id: 1, name: 'scph1001.bin', size_bytes: 524288, verified: true, state: 'present' },
      { id: 2, name: 'scph5501.bin', size_bytes: 524288, verified: true, state: 'missing' },
      { id: 3, name: 'scph7001.bin', size_bytes: 524288, verified: true, state: 'different' },
    ] },
    { platform_id: 2, name: 'Nintendo Switch', slug: 'switch', supported: false, files: [
      { id: 4, name: 'prod.keys', size_bytes: 9000, verified: false, state: 'unsupported' },
    ] },
  ],
  download_bios: () => ({ installed: 1, failed: [] }),
  save_sync_status: () => ({ syncs: true, emulator: 'retroarch-swanstation', note: null, synced_at: new Date(Date.now() - 300000).toISOString(), pending: false }),
  status: () => ({
    app_name: 'Romulus',
    version: 'dev',
    server_url: settings.server_url,
    signed_in: true,
    credential_store: 'file',
    rom_count: lib.roms.length,
    last_sync_at: Math.floor(Date.now() / 1000) - 600,
    refreshing: false,
    updatable: false,
    fullscreen: false,
    in_steam: true,
    gaming_mode: false,
  }),
  settings: () => settings,
  update_settings: ({ patch }) => Object.assign(settings, patch),
  set_games_view: ({ platform, view }) => (settings.games_view[platform] = view),
  platforms: () => lib.platforms,
  platform_covers: ({ perPlatform }) => {
    const out: Record<number, string[]> = {}
    for (const r of lib.roms) {
      if (!r.cover_small || !assets[r.cover_small]) continue
      const l = (out[r.platform_id] ??= [])
      if (l.length < perPlatform) l.push(r.cover_small)
    }
    return out
  },
  roms: ({ platformId, order }) => {
    const l = lib.roms.filter((r) => r.platform_id === platformId).map(row)
    return order === 'newest'
      ? l.sort((a, b) => b.id - a.id)
      : l.sort((a, b) => sortKey(a.title).localeCompare(sortKey(b.title)))
  },
  rom: ({ id }) => {
    const r = lib.roms.find((x) => x.id === id)
    return r ? { ...row(r), data: r.data } : null
  },
  search: ({ query }) => {
    const words = String(query).toLowerCase().split(/\W+/).filter(Boolean)
    if (!words.length) return []
    return lib.roms
      .filter((r) => words.every((w) => r.title.toLowerCase().split(/\W+/).some((t) => t.startsWith(w))))
      .slice(0, 200)
      .map(row)
  },
  asset: ({ path }) => {
    const f = assets[path]
    if (!f) throw new Error('not in fixture')
    return `${base}/assets/${f}`
  },
  refresh_library: () => false,
  whats_new: () => null,
  check_update: () => null,
  open_keyboard: () => null,
  log_ui: ({ level, message }) => console[level === 'error' ? 'error' : 'warn'](message),
  debug_info: () => 'mock',
  'plugin:event|listen': ({ event, handler }) => {
    listeners.push({ event, handler })
    return listeners.length
  },
  'plugin:event|unlisten': () => null,
}

let cb = 1
;(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
;(window as any).__TAURI_INTERNALS__ = {
  transformCallback: (f: (e: unknown) => void) => {
    callbacks.set(cb, f)
    return cb++
  },
  convertFileSrc: (p: string) => p,
  invoke: async (cmd: string, args: unknown) => {
    const h = handlers[cmd]
    if (!h) {
      console.warn('mock: unhandled command', cmd)
      return null
    }
    // A little latency, like the real IPC.
    await new Promise((r) => setTimeout(r, 4))
    return h(args)
  },
}
console.info(`mock backend: ${lib.platforms.length} platforms, ${lib.roms.length} roms`)

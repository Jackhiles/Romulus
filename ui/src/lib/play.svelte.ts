// What is on the device, what is downloading, and what is running. Shared by the lists,
// the detail page and the "playing" overlay; fed by `download` and `game` events.
import { listen } from '@tauri-apps/api/event'
import { games, onDownload, onGame } from './api'
import { size } from './format'
import { toast } from './toast.svelte'

export const device = $state({
  installed: new Set<number>(),
  /** Running and waiting downloads; `queued` is the place in line while waiting. */
  downloads: {} as Record<number, { done: number; total: number; queued?: number }>,
  playing: null as number | null,
  playingTitle: '',
  /** Bumped when an install changes, so screens can re-check a game. */
  revision: 0,
})

let started = false

/** Read which games are on the device (after signing in). */
export function loadInstalled() {
  games
    .installedIds()
    .then((ids) => (device.installed = new Set(ids)))
    .catch(() => {})
}

/** Listen for download and game events (once). */
export function initDevice() {
  if (started) return
  started = true
  onDownload((e) => {
    switch (e.stage) {
      case 'queued':
        // Position 1 starts right away; others wait.
        device.downloads[e.rom_id] = { done: 0, total: 0, queued: e.position > 1 ? e.position : undefined }
        break
      case 'progress':
        device.downloads[e.rom_id] = { done: e.done, total: e.total }
        break
      case 'done':
        delete device.downloads[e.rom_id]
        device.installed = new Set([...device.installed, e.rom_id])
        device.revision++
        toast('Download finished. Press A to play.')
        break
      case 'cancelled':
        delete device.downloads[e.rom_id]
        device.revision++
        toast('Download stopped. Downloading again continues where it left off.')
        break
      case 'failed':
        delete device.downloads[e.rom_id]
        device.revision++
        toast(`Download failed: ${e.message}`, 'error', 7000)
        break
    }
  }).catch(() => {})
  listen<{ uploaded: number; conflicts: number }>('save-sync', (e) => {
    const { uploaded, conflicts } = e.payload
    if (uploaded) toast(`${uploaded} save${uploaded === 1 ? '' : 's'} uploaded to RomM.`)
    if (conflicts)
      toast(
        `${conflicts} game${conflicts === 1 ? ' has' : 's have'} newer saves on both this Deck and RomM; you'll be asked when you play.`,
        'error',
        7000,
      )
    device.revision++
  }).catch(() => {})
  onGame((e) => {
    switch (e.stage) {
      case 'started':
        device.playing = e.rom_id
        break
      case 'ended': {
        device.playing = null
        const min = Math.round(e.duration_ms / 60000)
        const played = e.duration_ms >= 30000 ? `Played for ${min < 1 ? 'under a minute' : `${min} min`}. ` : ''
        const save = saveMessage(e.save, 'after')
        const states = e.states ? ` ${e.states === 1 ? 'A save state' : `${e.states} save states`} uploaded.` : ''
        if (played || save || states)
          toast(`${played}${save}${states}`.trim(), e.save?.startsWith('failed') || e.save === 'conflict' ? 'error' : 'info', 6000)
        break
      }
      case 'failed':
        device.playing = null
        toast(`Could not start the game: ${e.message}`, 'error', 7000)
        break
    }
  }).catch(() => {})
}

export function startDownload(romId: number) {
  device.downloads[romId] = { done: 0, total: 0 }
  games.download(romId).catch((e) => {
    delete device.downloads[romId]
    toast(String(e), 'error', 6000)
  })
}

export function removed(romId: number) {
  const s = new Set(device.installed)
  s.delete(romId)
  device.installed = s
  device.revision++
}

/** Text for a save sync result from the backend (null: nothing to say). */
export function saveMessage(note: string | null, when: 'before' | 'after'): string {
  if (!note) return ''
  if (note === 'downloaded') return 'Your latest save was restored from RomM.'
  if (note === 'uploaded') return when === 'after' ? 'Save uploaded to RomM.' : 'Your save was uploaded to RomM.'
  if (note === 'offline') return "RomM couldn't be reached; your save will sync next time."
  if (note === 'conflict')
    return 'Another device saved this game while you played. Your save is kept; you choose which to use next time.'
  return `Save sync problem: ${note.replace(/^failed: /, '')}`
}

/** Downloads running or waiting. */
export function downloadCount(): number {
  return Object.keys(device.downloads).length
}

export function progressText(p: { done: number; total: number; queued?: number }): string {
  if (p.queued) return 'Waiting in the download queue'
  if (!p.total) return 'Starting…'
  return `${size(p.done)} of ${size(p.total)} · ${Math.floor((p.done / p.total) * 100)}%`
}

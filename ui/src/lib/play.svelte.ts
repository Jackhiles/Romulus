// What is on the device, what is downloading, and what is running. Shared by the lists,
// the detail page and the "playing" overlay; fed by `download` and `game` events.
import { games, onDownload, onGame } from './api'
import { size } from './format'
import { toast } from './toast.svelte'

export const device = $state({
  installed: new Set<number>(),
  downloads: {} as Record<number, { done: number; total: number }>,
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
        toast('Download stopped. It will continue where it left off next time.')
        break
      case 'failed':
        delete device.downloads[e.rom_id]
        device.revision++
        toast(`Download failed: ${e.message}`, 'error', 7000)
        break
    }
  }).catch(() => {})
  onGame((e) => {
    switch (e.stage) {
      case 'started':
        device.playing = e.rom_id
        break
      case 'ended': {
        device.playing = null
        const min = Math.round(e.duration_ms / 60000)
        if (e.duration_ms >= 30000) toast(`Played for ${min < 1 ? 'under a minute' : `${min} min`}.`)
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

export function progressText(p: { done: number; total: number }): string {
  if (!p.total) return 'Starting…'
  return `${size(p.done)} of ${size(p.total)} · ${Math.floor((p.done / p.total) * 100)}%`
}

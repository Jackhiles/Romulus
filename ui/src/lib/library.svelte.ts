// Shared library sync state: last sync time, refresh progress, and a revision number
// that screens watch to reload from the cache when a refresh brings changes.
import { api, onRefresh } from './api'
import { device } from './play.svelte'
import { toast } from './toast.svelte'

export const library = $state({
  lastSync: null as number | null,
  refreshing: false,
  progress: '',
  revision: 0,
  needsSignIn: false,
})

let started = false

export function initLibrary(onUnauthorized: () => void) {
  if (started) return
  started = true
  onRefresh((e) => {
    switch (e.stage) {
      case 'platforms':
        library.refreshing = true
        library.progress = 'Syncing platforms…'
        break
      case 'roms':
        library.refreshing = true
        library.progress = e.total ? `Syncing games ${e.done} / ${e.total}` : `Syncing games ${e.done}`
        // Show games as they arrive on a first sync.
        if (e.done > 0) library.revision++
        break
      case 'cleanup':
        library.progress = 'Finishing…'
        break
      case 'done':
        library.refreshing = false
        library.lastSync = Math.floor(Date.now() / 1000)
        if (e.stats.roms_updated > 0 || e.stats.roms_removed > 0) library.revision++
        break
      case 'error':
        library.refreshing = false
        if (e.message === 'unauthorized') {
          library.needsSignIn = true
          toast('Your RomM sign-in expired. Please sign in again.', 'error', 6000)
          onUnauthorized()
        } else {
          // Usually RomM is down or the Deck is offline: try again later by itself.
          toast(`Sync failed (${e.message}). Trying again in 5 minutes.`, 'error', 6000)
          scheduleRefresh(RETRY_MS)
        }
        break
    }
  }).catch(() => {})
}

const RETRY_MS = 5 * 60_000
const PERIODIC_MS = 30 * 60_000
let timer: ReturnType<typeof setTimeout> | undefined

/** Sync again after `ms` (replacing any sync already scheduled). Skipped while a game
 *  runs; tried again later instead. */
function scheduleRefresh(ms: number) {
  clearTimeout(timer)
  timer = setTimeout(() => {
    if (device.playing !== null) scheduleRefresh(RETRY_MS)
    else refresh()
  }, ms)
}

/** Start a background sync. Progress and completion arrive as events (which can come
 *  before this call returns, so the events alone drive `library.refreshing`). */
export async function refresh() {
  // The next periodic sync counts from now; an error reschedules sooner.
  scheduleRefresh(PERIODIC_MS)
  try {
    await api.refreshLibrary()
  } catch (e) {
    toast(String(e), 'error')
  }
}

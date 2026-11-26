// Shared library sync state: last sync time, refresh progress, and a revision number
// that screens watch to reload from the cache when a refresh brings changes.
import { api, onRefresh } from './api'
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
          toast(`Sync failed: ${e.message}`, 'error', 6000)
        }
        break
    }
  }).catch(() => {})
}

/** Start a background sync. Progress and completion arrive as events (which can come
 *  before this call returns, so the events alone drive `library.refreshing`). */
export async function refresh() {
  try {
    await api.refreshLibrary()
  } catch (e) {
    toast(String(e), 'error')
  }
}

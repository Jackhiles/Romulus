// The focused game's cached details and first screenshot, loaded a moment after focus
// settles so holding a direction doesn't query every game it passes.
import { api, assetUrl, type RomDetail } from './api'

const details = new Map<number, RomDetail | null>()
const MAX = 400

async function load(id: number): Promise<RomDetail | null> {
  if (details.has(id)) return details.get(id) ?? null
  const d = await api.rom(id).catch(() => null)
  if (details.size >= MAX) details.delete(details.keys().next().value!)
  details.set(id, d)
  return d
}

export function screenshots(d: RomDetail | null): string[] {
  const s = d?.data?.merged_screenshots
  return Array.isArray(s) ? s.filter((p: unknown): p is string => typeof p === 'string' && p.startsWith('/')) : []
}

export class FocusedDetail {
  id = $state<number | null>(null)
  detail = $state<RomDetail | null>(null)
  /** Local URL of the first screenshot, once loaded. */
  shot = $state<string | null>(null)
  #timer: ReturnType<typeof setTimeout> | undefined
  #delay: number

  constructor(delay = 110) {
    this.#delay = delay
  }

  set(id: number | null | undefined) {
    const want = id ?? null
    if (want === this.id) return
    this.id = want
    clearTimeout(this.#timer)
    if (want === null) {
      this.detail = null
      this.shot = null
      return
    }
    const cached = details.get(want)
    if (cached !== undefined) this.detail = cached
    this.#timer = setTimeout(async () => {
      const d = await load(want)
      if (this.id !== want) return
      this.detail = d
      const path = screenshots(d)[0]
      if (!path) {
        this.shot = null
        return
      }
      const url = await assetUrl(path, () => this.id === want).catch(() => null)
      if (this.id === want) this.shot = url
    }, cached !== undefined ? 0 : this.#delay)
  }

  destroy() {
    clearTimeout(this.#timer)
  }
}

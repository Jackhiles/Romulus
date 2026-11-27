export function ago(unixSeconds: number | null, now = Date.now()): string {
  if (!unixSeconds) return 'never'
  const s = Math.max(0, Math.round(now / 1000 - unixSeconds))
  if (s < 45) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h} h ago`
  const d = Math.round(h / 24)
  return `${d} d ago`
}

export function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = bytes / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`
}

/** A stable hue (0-359) for a string, for placeholder art and per-system tints. */
export function hueOf(s: string): number {
  let h = 7
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) % 360
  return h
}

export interface Facts {
  year: string | null
  developer: string | null
  genre: string | null
  players: string | null
  /** 0-5 in half steps, from RomM's 0-100 average rating. */
  rating: number | null
}

const first = (v: unknown, n = 1): string | null =>
  Array.isArray(v) && v.length ? v.filter((x) => typeof x === 'string').slice(0, n).join(', ') || null : null

/** Display facts from a cached RomM ROM (`metadatum` holds the merged metadata). */
export function gameFacts(data: Record<string, any> | null | undefined): Facts {
  const m = data?.metadatum ?? {}
  let year: string | null = null
  const t = m.first_release_date
  if (typeof t === 'number' && t > 0) {
    // RomM stores milliseconds; older entries may be seconds.
    year = String(new Date(t > 1e11 ? t : t * 1000).getUTCFullYear())
  }
  const r = m.average_rating
  return {
    year,
    developer: first(m.developers) ?? first(m.companies) ?? first(m.publishers),
    genre: first(m.genres, 2),
    players: typeof m.player_count === 'string' && m.player_count ? m.player_count : null,
    rating: typeof r === 'number' && r > 0 ? Math.round(Math.min(r, 100) / 10) / 2 : null,
  }
}

/** The bracketed tags in a ROM file name, for telling versions apart:
 *  "Ace Combat 2 (USA) (Disc 1).chd" → "USA · Disc 1". */
export function fileTags(fsName: string): string {
  const tags = fsName.match(/[([][^)\]]+[)\]]/g) ?? []
  return tags.map((t) => t.slice(1, -1).trim()).filter(Boolean).join(' · ')
}

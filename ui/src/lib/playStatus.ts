// The user's RomM data for a game (`rom_user`): now playing, backlog, status and a 0-10
// rating, plus storage totals. Pure, unit tested.

export type PlayStatus = 'incomplete' | 'finished' | 'completed_100' | 'retired' | 'never_playing'

export interface RomUser {
  now_playing: boolean
  backlogged: boolean
  status: PlayStatus | null
  /** 0-10 (half stars). */
  rating: number
}

export const NO_USER: RomUser = { now_playing: false, backlogged: false, status: null, rating: 0 }

/** In the order the game menu cycles through them. */
export const STATUSES: (PlayStatus | null)[] = [null, 'incomplete', 'finished', 'completed_100', 'retired', 'never_playing']

export const STATUS_LABEL: Record<PlayStatus, string> = {
  incomplete: 'Unfinished',
  finished: 'Finished',
  completed_100: 'Completed 100%',
  retired: 'Gave up',
  never_playing: 'Not for me',
}

export function statusLabel(s: PlayStatus | null): string {
  return s ? STATUS_LABEL[s] : 'None'
}

/** RomM's `rom_user` (or nothing) as a RomUser. */
export function readUser(v: unknown): RomUser {
  if (!v || typeof v !== 'object') return { ...NO_USER }
  const o = v as Record<string, unknown>
  const status = STATUSES.includes(o.status as PlayStatus) ? (o.status as PlayStatus | null) : null
  const rating = typeof o.rating === 'number' ? Math.min(Math.max(Math.round(o.rating), 0), 10) : 0
  return { now_playing: o.now_playing === true, backlogged: o.backlogged === true, status, rating }
}

export function nextStatus(s: PlayStatus | null, delta: 1 | -1): PlayStatus | null {
  const i = STATUSES.indexOf(s)
  return STATUSES[(i + delta + STATUSES.length) % STATUSES.length]
}

export function nextRating(r: number, delta: 1 | -1): number {
  return Math.min(Math.max(r + delta, 0), 10)
}

/** A 0-10 rating as five stars in half steps ("★★★½☆"), or "Not rated". */
export function stars(r: number): string {
  if (r <= 0) return 'Not rated'
  const full = Math.floor(r / 2)
  const half = r % 2 === 1
  return '★'.repeat(full) + (half ? '½' : '') + '☆'.repeat(5 - full - (half ? 1 : 0))
}

/** Short tags for a game page. */
export function userTags(u: RomUser): string[] {
  const out: string[] = []
  if (u.now_playing) out.push('Now playing')
  if (u.backlogged) out.push('Backlog')
  if (u.status) out.push(STATUS_LABEL[u.status])
  if (u.rating > 0) out.push(stars(u.rating))
  return out
}

export interface SystemUsage {
  platform: string
  bytes: number
  games: number
}

/** Space used per system, largest first. */
export function usageBySystem(games: { platform: string; disk_bytes: number }[]): SystemUsage[] {
  const m = new Map<string, SystemUsage>()
  for (const g of games) {
    const u = m.get(g.platform) ?? { platform: g.platform, bytes: 0, games: 0 }
    u.bytes += g.disk_bytes
    u.games++
    m.set(g.platform, u)
  }
  return [...m.values()].sort((a, b) => b.bytes - a.bytes || a.platform.localeCompare(b.platform))
}

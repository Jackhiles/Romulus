// Automatic collections shown before the real systems, like ES-DE's: "Continue playing"
// and "On this Deck". They use negative ids so they can't clash with RomM platforms.
import { api, type Platform, type Rom, type Sort } from './api'

export const RECENT = -1
export const INSTALLED = -2

const KIND: Record<number, 'recent' | 'installed'> = { [RECENT]: 'recent', [INSTALLED]: 'installed' }

export function isCollection(id: number): boolean {
  return id in KIND
}

/** Games of a system or collection. Collections keep their own order unless sorting
 *  "On this Deck" by newest. */
export async function gamesOf(id: number, order: Sort): Promise<Rom[]> {
  if (!isCollection(id)) return api.roms(id, order)
  const list = await api.collection(KIND[id])
  return id === INSTALLED && order === 'newest' ? [...list].sort((a, b) => b.id - a.id) : list
}

export interface Systems {
  list: Platform[]
  /** A few cover paths per system or collection. */
  covers: Record<string, string[]>
}

/** Collections (when not empty) followed by the platforms. */
export async function systemsWithCollections(perCover = 6): Promise<Systems> {
  const [platforms, covers, recent, installed] = await Promise.all([
    api.platforms(),
    api.platformCovers(perCover),
    api.collection('recent').catch(() => [] as Rom[]),
    api.collection('installed').catch(() => [] as Rom[]),
  ])
  const virtual: Platform[] = []
  const add = (id: number, slug: string, name: string, roms: Rom[]) => {
    if (!roms.length) return
    virtual.push({ id, slug, fs_slug: slug, name, rom_count: roms.length, logo: null })
    covers[id] = roms.map((r) => r.cover_small).filter((c): c is string => !!c).slice(0, perCover)
  }
  add(RECENT, 'recent', 'Continue playing', recent)
  add(INSTALLED, 'installed', 'On this Deck', installed)
  return { list: [...virtual, ...platforms], covers }
}

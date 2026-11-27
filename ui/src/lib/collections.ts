// Collections shown before the real systems, like ES-DE's: the automatic "Continue
// playing" and "On this Deck", then RomM's Favourites and the user's RomM collections.
// They use negative ids so they can't clash with RomM platforms.
import { api, collections, type Platform, type Rom, type RommCollection, type Sort } from './api'

export const RECENT = -1
export const INSTALLED = -2

const KIND: Record<number, 'recent' | 'installed'> = { [RECENT]: 'recent', [INSTALLED]: 'installed' }

/** RomM collection ids map below these, so regular and smart ones can't clash. */
const REGULAR_BASE = -1000
const SMART_BASE = -1_000_000_000

/** The negative id a RomM collection gets in the systems list. */
export function collectionId(c: Pick<RommCollection, 'id' | 'smart'>): number {
  return (c.smart ? SMART_BASE : REGULAR_BASE) - c.id
}

/** The RomM collection behind a negative id, if it is one. */
export function rommCollection(id: number): { smart: boolean; id: number } | null {
  if (id <= SMART_BASE) return { smart: true, id: SMART_BASE - id }
  if (id <= REGULAR_BASE) return { smart: false, id: REGULAR_BASE - id }
  return null
}

export function isCollection(id: number): boolean {
  return id < 0
}

/** Games of a system or collection. Collections keep their own order unless sorting
 *  "On this Deck" by newest. */
export async function gamesOf(id: number, order: Sort): Promise<Rom[]> {
  if (!isCollection(id)) return api.roms(id, order)
  const romm = rommCollection(id)
  const list = romm ? await collections.games(romm.smart, romm.id) : await api.collection(KIND[id])
  return order === 'newest' && (id === INSTALLED || romm) ? [...list].sort((a, b) => b.id - a.id) : list
}

export interface Systems {
  list: Platform[]
  /** A few cover paths per system or collection. */
  covers: Record<string, string[]>
}

/** Collections (when not empty) followed by the platforms. */
export async function systemsWithCollections(perCover = 6): Promise<Systems> {
  const [platforms, covers, recent, installed, romm] = await Promise.all([
    api.platforms(),
    api.platformCovers(perCover),
    api.collection('recent').catch(() => [] as Rom[]),
    api.collection('installed').catch(() => [] as Rom[]),
    collections.list().catch(() => [] as RommCollection[]),
  ])
  const rommGames = await Promise.all(
    romm.map((c) => collections.games(c.smart, c.id).catch(() => [] as Rom[])),
  )
  const virtual: Platform[] = []
  const add = (id: number, slug: string, name: string, roms: Rom[]) => {
    if (!roms.length) return
    virtual.push({ id, slug, fs_slug: slug, name, rom_count: roms.length, logo: null })
    covers[id] = roms.map((r) => r.cover_small).filter((c): c is string => !!c).slice(0, perCover)
  }
  add(RECENT, 'recent', 'Continue playing', recent)
  add(INSTALLED, 'installed', 'On this Deck', installed)
  romm.forEach((c, i) => {
    const name = c.is_favorite ? 'Favourites' : c.name
    add(collectionId(c), c.is_favorite ? 'favourites' : name.toLowerCase().slice(0, 12), name, rommGames[i])
  })
  return { list: [...virtual, ...platforms], covers }
}

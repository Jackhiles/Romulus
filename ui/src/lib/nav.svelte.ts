// Screen stack: push to go deeper, B pops. Only the top screen is mounted; each entry
// keeps the screen's position (focused index, sort, query) so going back restores it.
import type { Sort } from './api'

interface Saved {
  index?: number
  sort?: Sort
  query?: string
}

export type Screen = Saved &
  (
    | { name: 'setup' }
    | { name: 'platforms' }
    | { name: 'games'; platformId: number; platformName: string; platformSlug: string }
    | { name: 'detail'; romId: number; siblings?: number[]; surprise?: boolean }
    | { name: 'search' }
    | { name: 'settings' }
    | { name: 'bios' }
    | { name: 'downloads' }
    | { name: 'backups' }
    | { name: 'achievements'; romId: number; title: string }
  )

export const nav = $state({ stack: [] as Screen[] })

export function current(): Screen | undefined {
  return nav.stack[nav.stack.length - 1]
}

export function push(s: Screen) {
  nav.stack.push(s)
}

export function back(): boolean {
  if (nav.stack.length <= 1) return false
  nav.stack.pop()
  return true
}

export function reset(s: Screen) {
  nav.stack.length = 0
  nav.stack.push(s)
}

// Games screen layouts and the carousel's geometry (pure, unit tested).

export type GamesView = 'list' | 'grid' | 'covers' | 'carousel'

/** In the order Select cycles through them. */
export const GAMES_VIEWS: GamesView[] = ['list', 'grid', 'covers', 'carousel']

export const GAMES_VIEW_LABEL: Record<GamesView, string> = {
  list: 'List',
  grid: 'Grid',
  covers: 'Big covers',
  carousel: 'Carousel',
}

export function isGamesView(v: unknown): v is GamesView {
  return GAMES_VIEWS.includes(v as GamesView)
}

export function nextGamesView(v: GamesView): GamesView {
  return GAMES_VIEWS[(GAMES_VIEWS.indexOf(v) + 1) % GAMES_VIEWS.length]
}

/** Indices within `radius` of `index`, clamped to `count` items. */
export function around(index: number, count: number, radius: number): number[] {
  const out: number[] = []
  for (let i = Math.max(0, index - radius); i <= Math.min(count - 1, index + radius); i++) out.push(i)
  return out
}

/** Horizontal offset of a carousel cover `d` places from the focused one: the focused
 *  cover gets extra room, the rest overlap a little. */
export function flowOffset(d: number, first = 265, rest = 200): number {
  if (d === 0) return 0
  return Math.sign(d) * (first + (Math.abs(d) - 1) * rest)
}

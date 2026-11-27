import { describe, expect, it } from 'vitest'
import { GAMES_VIEWS, around, flowOffset, isGamesView, nextGamesView } from './views'

describe('games views', () => {
  it('cycles through every view and back', () => {
    let v = GAMES_VIEWS[0]
    const seen = new Set([v])
    for (let i = 0; i < GAMES_VIEWS.length; i++) seen.add((v = nextGamesView(v)))
    expect(seen.size).toBe(GAMES_VIEWS.length)
    expect(v).toBe(nextGamesView(GAMES_VIEWS[GAMES_VIEWS.length - 1]))
  })
  it('rejects unknown saved views', () => {
    expect(isGamesView('covers')).toBe(true)
    expect(isGamesView('wall')).toBe(false)
    expect(isGamesView(undefined)).toBe(false)
  })
})

describe('carousel', () => {
  it('mounts only the covers near focus', () => {
    expect(around(0, 100, 3)).toEqual([0, 1, 2, 3])
    expect(around(50, 100, 2)).toEqual([48, 49, 50, 51, 52])
    expect(around(99, 100, 2)).toEqual([97, 98, 99])
    expect(around(0, 0, 3)).toEqual([])
  })
  it('spaces covers symmetrically with room around the focused one', () => {
    expect(flowOffset(0)).toBe(0)
    expect(flowOffset(1)).toBe(265)
    expect(flowOffset(-2)).toBe(-465)
    expect(flowOffset(3) - flowOffset(2)).toBe(flowOffset(2) - flowOffset(1))
  })
})

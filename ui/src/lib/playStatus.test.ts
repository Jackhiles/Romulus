import { describe, expect, it } from 'vitest'
import { NO_USER, STATUSES, nextRating, nextStatus, readUser, stars, usageBySystem, userTags } from './playStatus'

describe('rom user data', () => {
  it('reads what RomM sends, defensively', () => {
    expect(readUser(null)).toEqual(NO_USER)
    expect(readUser({ now_playing: true, backlogged: false, status: 'finished', rating: 7 })).toEqual({
      now_playing: true,
      backlogged: false,
      status: 'finished',
      rating: 7,
    })
    expect(readUser({ status: 'bogus', rating: 99 })).toEqual({ ...NO_USER, rating: 10 })
  })
  it('cycles statuses through "none" and clamps ratings', () => {
    let s = nextStatus(null, 1)
    expect(s).toBe('incomplete')
    for (let i = 1; i < STATUSES.length; i++) s = nextStatus(s, 1)
    expect(s).toBe(null)
    expect(nextStatus(null, -1)).toBe('never_playing')
    expect(nextRating(10, 1)).toBe(10)
    expect(nextRating(0, -1)).toBe(0)
  })
  it('shows ratings as half stars and lists tags', () => {
    expect(stars(0)).toBe('Not rated')
    expect(stars(7)).toBe('★★★½☆')
    expect(stars(10)).toBe('★★★★★')
    expect(userTags({ now_playing: true, backlogged: true, status: 'completed_100', rating: 2 })).toEqual([
      'Now playing',
      'Backlog',
      'Completed 100%',
      '★☆☆☆☆',
    ])
  })
})

describe('storage', () => {
  it('totals space per system, largest first', () => {
    expect(
      usageBySystem([
        { platform: 'PlayStation', disk_bytes: 700 },
        { platform: 'Game Boy', disk_bytes: 1 },
        { platform: 'PlayStation', disk_bytes: 600 },
      ]),
    ).toEqual([
      { platform: 'PlayStation', bytes: 1300, games: 2 },
      { platform: 'Game Boy', bytes: 1, games: 1 },
    ])
  })
})

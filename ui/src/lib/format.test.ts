import { describe, expect, it } from 'vitest'
import { ago, fileTags, gameFacts, hueOf, size } from './format'

describe('format', () => {
  it('ago', () => {
    const now = 1_000_000_000_000
    expect(ago(null, now)).toBe('never')
    expect(ago(now / 1000 - 10, now)).toBe('just now')
    expect(ago(now / 1000 - 600, now)).toBe('10 min ago')
    expect(ago(now / 1000 - 7200, now)).toBe('2 h ago')
    expect(ago(now / 1000 - 86400 * 3, now)).toBe('3 d ago')
  })
  it('size', () => {
    expect(size(512)).toBe('512 B')
    expect(size(1536)).toBe('1.5 KB')
    expect(size(700 * 1024 * 1024)).toBe('700 MB')
  })
})

describe('gameFacts', () => {
  it('reads RomM metadata', () => {
    const f = gameFacts({
      metadatum: {
        first_release_date: 725760000000,
        developers: ['Konami', 'Other'],
        genres: ['Sport', 'Racing', 'Puzzle'],
        player_count: '1-2',
        average_rating: 73,
      },
    })
    expect(f).toEqual({ year: '1992', developer: 'Konami', genre: 'Sport, Racing', players: '1-2', rating: 3.5 })
  })
  it('falls back and tolerates missing data', () => {
    expect(gameFacts({ metadatum: { companies: ['Sega'], first_release_date: 725760000 } }).developer).toBe('Sega')
    expect(gameFacts({ metadatum: { first_release_date: 725760000 } }).year).toBe('1992')
    expect(gameFacts(null)).toEqual({ year: null, developer: null, genre: null, players: null, rating: null })
  })
  it('hueOf is stable', () => {
    expect(hueOf('snes')).toBe(hueOf('snes'))
    expect(hueOf('snes')).not.toBe(hueOf('gba'))
  })
})

describe('fileTags', () => {
  it('collects bracketed tags', () => {
    expect(fileTags('Ace Combat 2 (USA) (Disc 1).chd')).toBe('USA · Disc 1')
    expect(fileTags('Game [!] (Europe) (Rev 1).zip')).toBe('! · Europe · Rev 1')
    expect(fileTags('Plain.zip')).toBe('')
  })
})

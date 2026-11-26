import { describe, expect, it } from 'vitest'
import { jumpLetter, letterOf, page, scrollFor, step } from './grid'

describe('step', () => {
  // 10 items, 4 columns:
  // 0 1 2 3
  // 4 5 6 7
  // 8 9
  it('moves within rows and wraps left/right', () => {
    expect(step(0, 'left', 4, 10)).toBe(0)
    expect(step(3, 'right', 4, 10)).toBe(4)
    expect(step(4, 'left', 4, 10)).toBe(3)
    expect(step(9, 'right', 4, 10)).toBe(9)
  })
  it('moves vertically and clamps to a short last row', () => {
    expect(step(1, 'down', 4, 10)).toBe(5)
    expect(step(6, 'down', 4, 10)).toBe(9)
    expect(step(9, 'down', 4, 10)).toBe(9)
    expect(step(2, 'up', 4, 10)).toBe(2)
    expect(step(9, 'up', 4, 10)).toBe(5)
  })
  it('handles empty grids', () => {
    expect(step(0, 'down', 4, 0)).toBe(0)
  })
})

describe('page', () => {
  it('moves by pages and clamps', () => {
    expect(page(0, 1, 4, 3, 100)).toBe(12)
    expect(page(95, 1, 4, 3, 100)).toBe(99)
    expect(page(5, -1, 4, 3, 100)).toBe(0)
  })
})

describe('letters', () => {
  const titles = ['Aladdin', 'Asterix', 'The Battle', 'Contra', 'Castlevania', '1942', 'Zelda']
  it('groups ignoring articles and non-letters', () => {
    expect(letterOf('The Legend of Zelda')).toBe('L')
    expect(letterOf('1942')).toBe('#')
    expect(letterOf('Évasion')).toBe('E')
  })
  it('jumps forward to the next group', () => {
    expect(jumpLetter(titles, 0, 1)).toBe(2)
    expect(jumpLetter(titles, 2, 1)).toBe(3)
    expect(jumpLetter(titles, 6, 1)).toBe(6)
  })
  it('jumps back to the group start, then the previous group', () => {
    expect(jumpLetter(titles, 4, -1)).toBe(3)
    expect(jumpLetter(titles, 3, -1)).toBe(2)
    expect(jumpLetter(titles, 1, -1)).toBe(0)
    expect(jumpLetter(titles, 0, -1)).toBe(0)
  })
})

describe('scrollFor', () => {
  it('only scrolls when the row nears an edge', () => {
    expect(scrollFor(0, 100, 500, 0, 2000)).toBe(0)
    expect(scrollFor(2, 100, 500, 0, 2000)).toBe(0)
    expect(scrollFor(5, 100, 500, 0, 2000)).toBe(160)
    expect(scrollFor(1, 100, 500, 300, 2000)).toBe(40)
    expect(scrollFor(19, 100, 500, 0, 2000)).toBe(1500)
  })
})

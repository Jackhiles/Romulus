import { describe, expect, it } from 'vitest'
import { INSTALLED, RECENT, collectionId, isCollection, rommCollection } from './collections'

describe('collection ids', () => {
  it('maps RomM collections to negative ids and back', () => {
    for (const c of [
      { id: 1, smart: false },
      { id: 987654, smart: false },
      { id: 1, smart: true },
      { id: 42, smart: true },
    ]) {
      const id = collectionId(c)
      expect(isCollection(id)).toBe(true)
      expect(rommCollection(id)).toEqual(c)
    }
  })

  it('keeps the automatic collections and platforms apart', () => {
    expect(rommCollection(RECENT)).toBeNull()
    expect(rommCollection(INSTALLED)).toBeNull()
    expect(isCollection(5)).toBe(false)
    expect(collectionId({ id: 0, smart: false })).toBeLessThan(INSTALLED)
  })
})

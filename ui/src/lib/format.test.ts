import { describe, expect, it } from 'vitest'
import { ago, size } from './format'

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

import { describe, expect, it } from 'vitest'
import { follow } from './ease'

describe('follow', () => {
  it('moves further on a long frame and settles exactly', () => {
    const short = follow(0, 100, 8)
    const long = follow(0, 100, 50)
    expect(short).toBeGreaterThan(0)
    expect(long).toBeGreaterThan(short)
    expect(follow(99.8, 100, 16)).toBe(100)
    expect(follow(0, 100, 0)).toBe(0)
  })
  it('two half frames land where one whole frame does', () => {
    expect(follow(follow(0, 100, 10), 100, 10)).toBeCloseTo(follow(0, 100, 20), 6)
  })
})

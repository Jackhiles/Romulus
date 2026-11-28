import { describe, expect, it } from 'vitest'
import { clampPage, fitScale } from './manual'

describe('manual reader', () => {
  it('fits a page to the screen or its width', () => {
    // A portrait page on the Deck's 1280×700 reading area.
    expect(fitScale(600, 800, 1280, 700, 'page')).toBeCloseTo(0.875)
    expect(fitScale(600, 800, 1280, 700, 'width')).toBeCloseTo(2.1333, 3)
    // A wide page is limited by the width either way.
    expect(fitScale(1600, 600, 1280, 700, 'page')).toBeCloseTo(0.8)
    expect(fitScale(0, 0, 1280, 700, 'page')).toBe(1)
  })
  it('keeps page numbers in range', () => {
    expect(clampPage(0, 20)).toBe(1)
    expect(clampPage(25, 20)).toBe(20)
    expect(clampPage(3, 0)).toBe(1)
  })
})

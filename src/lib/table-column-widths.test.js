import { describe, expect, it, vi } from 'vitest'
import { AUTO_FIT_CHARS, AUTO_FIT_MAX, AUTO_FIT_MIN, fitColumnWidth } from './table-column-widths.js'

// Monospace, 8px a character: what the grid's font measures like.
const measure = (/** @type {string} */ s) => s.length * 8

describe('fitColumnWidth', () => {
  it('fits short values with padding on both sides', () => {
    expect(fitColumnWidth({ headerW: 40, values: ['1783659094883', '1784'], measure, padX: 10 })).toBe(13 * 8 + 20)
  })

  it('keeps the header readable when the values are narrower', () => {
    expect(fitColumnWidth({ headerW: 150, values: ['1', '2'], measure, padX: 10 })).toBe(170)
  })

  it('never goes past the cap, however long the text', () => {
    const essay = 'word '.repeat(5000)
    expect(fitColumnWidth({ headerW: 30, values: [essay], measure, padX: 10 })).toBe(AUTO_FIT_MAX)
  })

  it('never goes under the floor', () => {
    expect(fitColumnWidth({ headerW: 8, values: ['t'], measure, padX: 4 })).toBe(AUTO_FIT_MIN)
  })

  it('measures at most AUTO_FIT_CHARS of a value, and only its first line', () => {
    const spy = vi.fn(measure)
    fitColumnWidth({ headerW: 10, values: ['x'.repeat(10_000), 'ab\ncdefghij'], measure: spy, padX: 0, max: 10_000 })
    expect(spy.mock.calls[0][0].length).toBe(AUTO_FIT_CHARS)
    expect(spy.mock.calls[1][0]).toBe('ab')
  })

  it('stops reading once the cap is reached', () => {
    const spy = vi.fn(measure)
    const values = Array.from({ length: 150 }, () => 'y'.repeat(60))
    fitColumnWidth({ headerW: 10, values, measure: spy, padX: 10 })
    expect(spy).toHaveBeenCalledTimes(1)
  })

  it('skips empty and null cells', () => {
    expect(fitColumnWidth({ headerW: 50, values: [null, '', undefined, 'abcdefghij'], measure, padX: 10 })).toBe(100)
  })
})

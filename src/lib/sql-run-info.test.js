import { describe, expect, it } from 'vitest'
import { formatDuration, formatRunInfo } from './sql-run-info.js'

describe('formatDuration', () => {
  it('reads milliseconds, seconds and minutes', () => {
    expect(formatDuration(0)).toBe('0ms')
    expect(formatDuration(478.4)).toBe('478ms')
    expect(formatDuration(1234)).toBe('1.23s')
    expect(formatDuration(12_340)).toBe('12.3s')
    expect(formatDuration(125_000)).toBe('2m 5s')
  })

  it('says nothing without a duration', () => {
    expect(formatDuration(null)).toBe('')
    expect(formatDuration(undefined)).toBe('')
    expect(formatDuration(-1)).toBe('')
    expect(formatDuration(NaN)).toBe('')
  })
})

describe('formatRunInfo', () => {
  it('joins the time and what came back', () => {
    expect(formatRunInfo({ ms: 478, rows: 12 })).toBe('478ms · 12 rows')
    expect(formatRunInfo({ ms: 5, rows: 1 })).toBe('5ms · 1 row')
    expect(formatRunInfo({ ms: 35, rows: null, affected: 3 })).toBe('35ms · 3 affected')
    expect(formatRunInfo({ ms: 2100, rows: 1_250_000 })).toBe('2.10s · 1.3M rows')
  })

  it('leaves out what it was not told', () => {
    expect(formatRunInfo({ ms: 12 })).toBe('12ms')
    expect(formatRunInfo({ rows: 0 })).toBe('0 rows')
    expect(formatRunInfo({})).toBe('')
  })
})

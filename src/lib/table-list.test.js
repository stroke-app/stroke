import { describe, it, expect } from 'vitest'
import { compareCreated, formatCompactCount, formatTableRowCount, normalizeTableRowCount } from './table-list.js'

describe('formatCompactCount', () => {
  it('shows the placeholder while a count is still resolving', () => {
    expect(formatCompactCount(null)).toBe('…')
  })
  it('shows an em dash for non-numeric input', () => {
    expect(formatCompactCount(undefined)).toBe('—')
    expect(formatCompactCount('nope')).toBe('—')
  })
  it('renders sub-thousand counts with locale grouping', () => {
    expect(formatCompactCount(0)).toBe('0')
    expect(formatCompactCount(999)).toBe('999')
  })
  it('compacts thousands with one optional decimal', () => {
    expect(formatCompactCount(1000)).toBe('1k')
    expect(formatCompactCount(1500)).toBe('1.5k')
    expect(formatCompactCount(10_000)).toBe('10k')
    expect(formatCompactCount(999_000)).toBe('999k')
  })
  it('compacts millions', () => {
    expect(formatCompactCount(1_000_000)).toBe('1M')
    expect(formatCompactCount(1_200_000)).toBe('1.2M')
  })
  it('moves to the next unit when rounding reaches it', () => {
    expect(formatCompactCount(999_499)).toBe('999k')
    expect(formatCompactCount(999_999)).toBe('1M')
    expect(formatCompactCount(999_999_999)).toBe('1B')
    expect(formatCompactCount(2_500_000_000)).toBe('2.5B')
  })
  it('preserves the sign for negatives', () => {
    expect(formatCompactCount(-1500)).toBe('-1.5k')
  })
  it('accepts numeric strings', () => {
    expect(formatCompactCount('2500')).toBe('2.5k')
  })
})

describe('formatTableRowCount', () => {
  it('delegates to the compact formatter', () => {
    expect(formatTableRowCount(1500)).toBe(formatCompactCount(1500))
  })
})

describe('normalizeTableRowCount', () => {
  it('maps the -1 "not counted yet" sentinel (and any negative) to null', () => {
    expect(normalizeTableRowCount(-1)).toBeNull()
    expect(normalizeTableRowCount(-999)).toBeNull()
  })
  it('maps non-numeric input to null', () => {
    expect(normalizeTableRowCount('abc')).toBeNull()
    expect(normalizeTableRowCount(undefined)).toBeNull()
  })
  it('passes through a real non-negative count', () => {
    expect(normalizeTableRowCount(0)).toBe(0)
    expect(normalizeTableRowCount('42')).toBe(42)
  })
})

describe('compareCreated', () => {
  const sortNames = (list) => [...list].sort(compareCreated).map((t) => t.name)
  it('orders by creation time where the engine keeps one, ties by name', () => {
    expect(sortNames([
      { name: 'b', createdAt: '2024-03-01 10:00:00' },
      { name: 'a', createdAt: '2024-03-01 10:00:00' },
      { name: 'old', createdAt: '2023-01-01 00:00:00' },
    ])).toEqual(['old', 'a', 'b'])
  })
  it('falls back to creation order, and puts tables with neither last', () => {
    expect(sortNames([
      { name: 'views_later', createOrder: 300 },
      { name: 'unknown' },
      { name: 'first', createOrder: 12 },
    ])).toEqual(['first', 'views_later', 'unknown'])
    // A view without CREATE_TIME (MySQL) sorts after the tables that have one.
    expect(sortNames([{ name: 'v', createdAt: null }, { name: 't', createdAt: '2024-01-01 00:00:00' }])).toEqual(['t', 'v'])
  })
})

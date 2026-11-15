import { describe, it, expect, vi } from 'vitest'
vi.mock('$lib/chart-utils.js', () => ({ buildOption: () => ({ series: [] }) }))
import { kpiOf, formatKpi, agoText, createQueue } from './dashboard-live.js'

describe('kpiOf', () => {
  it('reads one number, with a label beside it', () => {
    expect(kpiOf([{ name: 'count' }], [[42]])).toEqual({ value: 42, label: 'count' })
    expect(kpiOf([{ name: 'status' }, { name: 'n' }], [['paid', '17']])).toEqual({ value: 17, label: 'paid' })
  })
  it('leaves real charts alone', () => {
    expect(kpiOf([{ name: 'a' }], [[1], [2]])).toBeNull()
    expect(kpiOf([{ name: 'a' }, { name: 'b' }], [[1, 2]])).toBeNull()
  })
})

describe('formatKpi / agoText', () => {
  it('formats', () => {
    expect(formatKpi(1234)).toBe('1,234')
    expect(formatKpi(2_500_000)).toBe('2.5M')
    expect(agoText(1000, 1000 + 125_000)).toBe('2m ago')
  })
})

describe('createQueue', () => {
  it('never runs more than the limit at once', async () => {
    const run = createQueue(2)
    let live = 0, peak = 0
    const task = () => run(async () => { live++; peak = Math.max(peak, live); await new Promise((r) => setTimeout(r, 5)); live-- })
    await Promise.all([task(), task(), task(), task(), task()])
    expect(peak).toBe(2)
  })
})

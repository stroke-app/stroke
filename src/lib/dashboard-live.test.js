import { describe, it, expect, vi } from 'vitest'
vi.mock('$lib/chart-utils.js', async (orig) => ({ ...(await orig()), buildOption: (/** @type {any} */ o) => ({ type: o.type, x: o.xCol, y: o.yCol }) }))
import { kpiOf, formatKpi, agoText, createQueue, queryView } from './dashboard-live.js'

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

describe('queryView', () => {
  const r = (/** @type {any[]} */ columns, /** @type {unknown[][]} */ rows) => ({ columns, rows })
  it('draws one value as a number', () => {
    const v = queryView(r([{ name: 'count' }], [[42]]), 'auto', true)
    expect(v.kpi).toEqual({ value: 42, label: 'count' })
  })
  it('draws a time series as a line and categories as bars', () => {
    const series = r([{ name: 'day', dataType: 'date' }, { name: 'signups', dataType: 'integer' }], [['2026-11-01', 3], ['2026-11-02', 5]])
    expect(queryView(series, 'auto', true).option).toEqual({ type: 'line', x: 'day', y: 'signups' })
    const cats = r([{ name: 'status', dataType: 'text' }, { name: 'n', dataType: 'bigint' }], [['paid', 3], ['open', 5]])
    expect(queryView(cats, undefined, true).option).toEqual({ type: 'bar', x: 'status', y: 'n' })
    expect(queryView(cats, 'area', true).option).toEqual({ type: 'area', x: 'status', y: 'n' })
  })
  it('falls back to a table when there is nothing to plot', () => {
    const v = queryView(r([{ name: 'email', dataType: 'text' }, { name: 'name', dataType: 'text' }], [['a@b.c', 'A']]), 'auto', true)
    expect(v.table).toEqual({ columns: ['email', 'name'], rows: [['a@b.c', 'A']] })
    expect(queryView(r([{ name: 'n' }], [[1]]), 'table', true).table?.rows).toEqual([[1]])
  })
})

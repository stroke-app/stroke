import { describe, it, expect } from 'vitest'
import { chartRows } from './ai-chart-data.js'

const rows = [{ month: 'Jan', total: 3 }, { month: 'Feb', total: 5 }]

describe('chartRows', () => {
  it('passes row objects through', () => {
    expect(chartRows(rows)).toBe(rows)
  })

  it('reads the array out of a JSON string', () => {
    expect(chartRows(JSON.stringify(rows))).toEqual(rows)
    expect(chartRows('not json')).toEqual([])
  })

  it('takes execute_sql\'s own { columns, rows } result', () => {
    expect(chartRows({ columns: [{ name: 'month' }, { name: 'total' }], rows: [['Jan', 3], ['Feb', 5]] })).toEqual(rows)
    expect(chartRows({ columns: ['month', 'total'], rows })).toEqual(rows)
  })

  it('names array rows by a header row, the spec columns, or the axes', () => {
    expect(chartRows([['month', 'total'], ['Jan', 3], ['Feb', 5]])).toEqual(rows)
    expect(chartRows([['Jan', 3], ['Feb', 5]], { x_col: 'month', y_col: 'total' })).toEqual(rows)
  })

  it('zips one array per column into rows', () => {
    expect(chartRows({ month: ['Jan', 'Feb'], total: [3, 5] })).toEqual(rows)
  })

  it('gives nothing for what cannot be charted', () => {
    expect(chartRows(undefined)).toEqual([])
    expect(chartRows(42)).toEqual([])
    expect(chartRows([1, 2, 3])).toEqual([])
    expect(chartRows({ total: 5 })).toEqual([])
  })
})

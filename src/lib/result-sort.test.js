import { describe, it, expect } from 'vitest'
import { sortRowsByColumn, temporalMicros, compareDecimalText, keyKindOf } from './result-sort.js'

const col = (rows, type, desc = false) => sortRowsByColumn(rows.map((v) => [v]), 0, type, desc).map((r) => r[0])

describe('sortRowsByColumn', () => {
  it('sorts timestamps by instant whatever their fraction length', () => {
    // Numeric collation read ".500" and ".123456" as 500 and 123456.
    expect(col(['2024-01-02 10:00:00.500 UTC', '2024-01-02 10:00:00.123456 UTC', '2024-01-02 09:59:59 UTC'], 'timestamptz')).toEqual([
      '2024-01-02 09:59:59 UTC',
      '2024-01-02 10:00:00.123456 UTC',
      '2024-01-02 10:00:00.500 UTC',
    ])
  })

  it('sorts dates, NULLs last in both directions', () => {
    expect(col(['2024-10-01', null, '1999-12-31', '2024-02-29'], 'date', true)).toEqual(['2024-10-01', '2024-02-29', '1999-12-31', null])
    expect(col(['2024-10-01', null, '1999-12-31'], 'date')).toEqual(['1999-12-31', '2024-10-01', null])
  })

  it('sorts numeric text exactly, past float precision', () => {
    expect(col(['10', '9.5', '-2', '12345678901234567890.1', '12345678901234567890.01', '0.00', '-10.5'], 'numeric')).toEqual([
      '-10.5', '-2', '0.00', '9.5', '10', '12345678901234567890.01', '12345678901234567890.1',
    ])
  })

  it('sorts integers and floats by value, text naturally', () => {
    expect(col([10, 9, null, -1], 'int8')).toEqual([-1, 9, 10, null])
    expect(col([1.5, -0.5, 1e10], 'float8', true)).toEqual([1e10, 1.5, -0.5])
    expect(col(['file10', 'File2', 'file1'], 'text')).toEqual(['file1', 'File2', 'file10'])
  })

  it('keeps arrival order for equal keys', () => {
    const rows = [[1, 'a'], [0, 'b'], [1, 'c'], [0, 'd']]
    expect(sortRowsByColumn(rows, 0, 'int4', false).map((r) => r[1])).toEqual(['b', 'd', 'a', 'c'])
  })
})

describe('helpers', () => {
  it('reads offsets and times of day', () => {
    expect(temporalMicros('2024-01-02 10:00:00+05:45')).toBe(temporalMicros('2024-01-02 04:15:00 UTC'))
    expect(temporalMicros('00:00:01.5')).toBe(1_500_000)
    expect(Number.isNaN(temporalMicros('soon'))).toBe(true)
  })

  it('compares decimal text exactly', () => {
    expect(compareDecimalText('0.10', '0.1')).toBe(0)
    expect(compareDecimalText('-1', '-2')).toBeGreaterThan(0)
    expect(keyKindOf('varchar(...)')).toBe('text')
    expect(keyKindOf('_int4')).toBe('text')
  })
})

import { describe, it, expect } from 'vitest'
import { errorSpan, lineColumn, cleanErrorMessage } from './sql-error-range.js'

describe('errorSpan', () => {
  it('underlines the token at the position', () => {
    const sql = 'SELECT * FROM no_such_table_here'
    const s = errorSpan(sql, 15)
    expect(sql.slice(s.from, s.to)).toBe('no_such_table_here')
  })

  it('underlines the last token when it failed at the end of input', () => {
    const sql = 'SELECT * FROM ats_stats where'
    const s = errorSpan(sql, sql.length + 1)
    expect(sql.slice(s.from, s.to)).toBe('where')
    const t = 'SELECT * FROM t WHERE  ;'
    expect(t.slice(errorSpan(t, t.length + 1).from, errorSpan(t, t.length + 1).to)).toBe('WHERE')
  })

  it('falls back to the first line without a position', () => {
    expect(errorSpan('SELECT 1\nFROM x', null)).toEqual({ from: 0, to: 8 })
  })
})

describe('helpers', () => {
  it('finds line and column', () => {
    expect(lineColumn('SELECT *\nFROM x', 9)).toEqual({ line: 2, column: 1 })
    expect(lineColumn('SELECT 1', 7)).toEqual({ line: 1, column: 8 })
  })

  it('strips the backend wrapping from a message', () => {
    expect(cleanErrorMessage('Error: Query failed: error returned from database: syntax error at end of input')).toBe('syntax error at end of input')
    expect(cleanErrorMessage('Statement 2 failed: error returned from database: relation "x" does not exist')).toBe('relation "x" does not exist')
  })
})

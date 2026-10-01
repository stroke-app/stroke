import { describe, it, expect } from 'vitest'
import { lintSql } from './sql-statements.js'

const messages = (sql) => lintSql(sql).map((d) => d.message)

describe('lintSql', () => {
  it('does not flag a single query without a semicolon', () => {
    expect(lintSql('SELECT * FROM users_table')).toEqual([])
  })

  it('flags two queries separated only by a blank line', () => {
    const sql = 'SELECT * FROM users_table\n\nSELECT * FROM posts'
    const d = lintSql(sql)
    expect(d).toHaveLength(1)
    expect(sql.slice(d[0].start, d[0].end)).toBe('SELECT')
    expect(d[0].start).toBe(sql.lastIndexOf('SELECT'))
  })

  it('does not flag a terminated statement, a CTE, a subquery or a UNION arm', () => {
    expect(lintSql('SELECT 1;\n\nSELECT 2')).toEqual([])
    expect(lintSql('WITH x AS (\n  SELECT 1\n)\n\nSELECT * FROM x')).toEqual([])
    expect(lintSql('SELECT * FROM t WHERE id IN (\n\n  SELECT id FROM u\n)')).toEqual([])
    expect(lintSql('SELECT 1\nUNION ALL\n\nSELECT 2')).toEqual([])
  })

  it('still reports unterminated strings', () => {
    expect(messages("SELECT 'abc")[0]).toMatch(/Unterminated string/)
  })
})

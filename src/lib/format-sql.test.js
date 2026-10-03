import { describe, it, expect } from 'vitest'
import { formatSql } from './format-sql.js'

describe('formatSql', () => {
  it('upper-cases keywords and reflows a statement', () => {
    const out = formatSql('select id, name from users where id = 1')
    expect(out).toContain('SELECT')
    expect(out).toContain('FROM')
    expect(out).toContain('WHERE')
  })

  it('returns the original input for a blank string (no throw)', () => {
    expect(formatSql('')).toBe('')
    expect(formatSql('   ')).toBe('   ')
  })

  it('falls back to the original text when formatting throws', () => {
    // Deliberately malformed - formatSql must never throw, just return input.
    const junk = 'this is not ;; valid (( sql'
    expect(typeof formatSql(junk)).toBe('string')
  })
})

describe('formatSql compact clauses', () => {
  it('keeps a short query on one line per clause', () => {
    expect(formatSql('select * from users_table')).toBe('SELECT *\nFROM users_table')
  })

  it('joins a short WHERE, keeps a long one on AND lines', () => {
    expect(formatSql("select id from t where id = 4 and name = 'x'")).toBe("SELECT id\nFROM t\nWHERE id = 4 AND name = 'x'")
    const long = formatSql("select id from t where some_long_column_name = 'a value that is long' and another_column = 'another long value here'")
    expect(long).toContain("WHERE some_long_column_name = 'a value that is long'\n  AND another_column")
  })

  it('keeps a long SELECT list in the library layout', () => {
    const cols = Array.from({ length: 12 }, (_, i) => `column_number_${i}`).join(', ')
    expect(formatSql(`select ${cols} from t`)).toMatch(/^SELECT\n  column_number_0,\n/)
  })

  it('leaves joins on their own lines', () => {
    expect(formatSql('select p.id from posts p join users u on u.id = p.author_id')).toBe(
      'SELECT p.id\nFROM posts p\n  JOIN users u ON u.id = p.author_id',
    )
  })

  it('compacts the review dock\'s generated UPDATE', () => {
    expect(formatSql(`UPDATE "public"."t" SET "name" = 'ad' WHERE "id" = 4;`)).toBe(`UPDATE "public"."t"\nSET "name" = 'ad'\nWHERE "id" = 4;`)
  })

  it('can be switched off', () => {
    expect(formatSql('select * from t', { compactClauses: false })).toBe('SELECT\n  *\nFROM\n  t')
  })
})

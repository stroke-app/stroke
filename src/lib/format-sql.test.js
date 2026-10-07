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

describe('formatSql compact layout', () => {
  it('keeps a statement that fits on one line', () => {
    expect(formatSql('select * from users_table')).toBe('SELECT * FROM users_table')
    expect(formatSql('SELECT * FROM public.campaigns;')).toBe('SELECT * FROM public.campaigns;')
    expect(formatSql(`UPDATE "public"."t" SET "name" = 'ad' WHERE "id" = 4;`)).toBe(`UPDATE "public"."t" SET "name" = 'ad' WHERE "id" = 4;`)
  })

  it('breaks a long statement into clauses, each on its keyword\'s line', () => {
    const out = formatSql("select u.id, u.name, count(o.id) as orders from users u left join orders o on o.user_id = u.id and o.status = 'paid' where u.created_at > now() - interval '30 days' and (u.role = 'a' or u.role = 'b') group by u.id, u.name order by orders desc limit 20;")
    expect(out).toBe([
      'SELECT u.id, u.name, count(o.id) AS orders',
      'FROM users u',
      "LEFT JOIN orders o ON o.user_id = u.id AND o.status = 'paid'",
      "WHERE u.created_at > now() - interval '30 days'",
      "  AND (u.role = 'a' OR u.role = 'b')",
      'GROUP BY u.id, u.name',
      'ORDER BY orders DESC',
      'LIMIT 20;',
    ].join('\n'))
  })

  it('starts every join on its own line, level with FROM', () => {
    expect(formatSql('select p.id from posts p join users u on u.id = p.author_id')).toBe(
      'SELECT p.id\nFROM posts p\nJOIN users u ON u.id = p.author_id',
    )
  })

  it('keeps a long SELECT list one column per line', () => {
    const cols = Array.from({ length: 12 }, (_, i) => `column_number_${i}`).join(', ')
    expect(formatSql(`select ${cols} from t`)).toMatch(/^SELECT\n  column_number_0,\n  column_number_1,\n/)
  })

  it('opens a CTE beside WITH and gives the main query its own chance at one line', () => {
    expect(formatSql("with recent as (select * from orders where created_at > now() - interval '7 days' and status = 'paid') select user_id, count(*) from recent group by user_id")).toBe([
      'WITH recent AS (',
      '  SELECT *',
      '  FROM orders',
      "  WHERE created_at > now() - interval '7 days' AND status = 'paid'",
      ')',
      'SELECT user_id, count(*) FROM recent GROUP BY user_id',
    ].join('\n'))
  })

  it('inlines a short subquery and indents a long one under its bracket', () => {
    expect(formatSql('select * from users where id in (select user_id from orders where total > 100)'))
      .toBe('SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE total > 100)')
    expect(formatSql('select * from users where id in (select user_id from orders where total > 100 and status = 1 and created_at > now())')).toBe([
      'SELECT *',
      'FROM users',
      'WHERE id IN (',
      '  SELECT user_id',
      '  FROM orders',
      '  WHERE total > 100 AND status = 1 AND created_at > now()',
      ')',
    ].join('\n'))
  })

  it('fills a long value list instead of a line per value', () => {
    const ids = Array.from({ length: 40 }, (_, i) => 1000 + i).join(', ')
    const lines = formatSql(`select * from t where id in (${ids})`).split('\n')
    expect(lines[2]).toBe('WHERE id IN (')
    expect(lines.length).toBeLessThan(10)
    expect(lines.every((l) => l.length <= 80)).toBe(true)
  })

  it('keeps a CASE with several branches on its lines, a one-branch CASE inline', () => {
    const out = formatSql("select id, case when total > 100 then 'big' when total > 10 then 'mid' else 'small' end as size, case when x then 1 end as flag from orders")
    expect(out).toContain("  CASE\n    WHEN total > 100 THEN 'big'\n    WHEN total > 10 THEN 'mid'\n    ELSE 'small'\n  END AS size,\n")
    expect(out).toContain('  CASE WHEN x THEN 1 END AS flag\n')
  })

  it('keeps a table definition one column per line', () => {
    expect(formatSql('create table t (id serial primary key, name text not null, created_at timestamptz default now());')).toBe(
      'CREATE TABLE t (\n  id serial PRIMARY KEY,\n  name text NOT NULL,\n  created_at timestamptz DEFAULT now()\n);',
    )
  })

  it('never joins a line past a line comment', () => {
    expect(formatSql('-- top\nselect a from t -- trailing\nwhere x = 1')).toBe('-- top\nSELECT a\nFROM t -- trailing\nWHERE x = 1')
  })

  it('keeps statements apart', () => {
    expect(formatSql('select a from t1; select b from t2;')).toBe('SELECT a FROM t1;\n\nSELECT b FROM t2;')
  })

  it('formats in the connection\'s dialect', () => {
    expect(formatSql('select `id` from `users` where `id` = 1', undefined, 'mysql')).toBe('SELECT `id` FROM `users` WHERE `id` = 1')
    expect(formatSql('select top 10 [id] from [dbo].[users]', undefined, 'mssql')).toBe('SELECT TOP 10 [id] FROM [dbo].[users]')
  })

  it('can be switched off', () => {
    expect(formatSql('select * from t', { compactClauses: false })).toBe('SELECT\n  *\nFROM\n  t')
  })
})

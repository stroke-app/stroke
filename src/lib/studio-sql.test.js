import { describe, expect, it } from 'vitest'
import { studioEngine, studioPageSql, studioQuote } from './studio-sql.js'
import { setOdbcDialect } from './sql-ident.js'

describe('studio share engines', () => {
  it('shares every SQL engine and no other', () => {
    expect(studioEngine('cockroachdb')).toBe('postgres')
    expect(studioEngine('mariadb')).toBe('mysql')
    expect(studioEngine('d1')).toBe('sqlite')
    for (const t of ['mssql', 'oracle', 'duckdb', 'clickhouse', 'spanner', 'bigquery', 'snowflake', 'odbc']) expect(studioEngine(t)).toBe(t)
    for (const t of ['redis', 'mongodb', 'dynamodb', 'elasticsearch', 'firestore']) expect(studioEngine(t)).toBeNull()
  })

  it('quotes names the way each engine reads them', () => {
    expect(studioQuote('postgres', 'a"b')).toBe('"a""b"')
    expect(studioQuote('mysql', 'a`b')).toBe('`a``b`')
    expect(studioQuote('bigquery', 'orders')).toBe('`orders`')
    expect(studioQuote('mssql', 'a]b')).toBe('[a]]b]')
    setOdbcDialect({ quote: '`', namespace: 'catalog' })
    expect(studioQuote('odbc', 't')).toBe('`t`')
    setOdbcDialect({ quote: '"', namespace: 'schema' })
  })

  it('pages the way each engine does', () => {
    const base = 'SELECT * FROM t'
    expect(studioPageSql('postgres', base, '', 50, 100)).toBe('SELECT * FROM t LIMIT 50 OFFSET 100')
    expect(studioPageSql('mssql', base, '', 50, 0)).toBe('SELECT * FROM t ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY')
    expect(studioPageSql('mssql', base, ' ORDER BY [id] ASC', 50, 0)).toBe('SELECT * FROM t ORDER BY [id] ASC OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY')
    expect(studioPageSql('oracle', base, '', 10, 20)).toBe('SELECT * FROM t OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY')
    expect(studioPageSql('odbc', base, '', 10, 0, true)).toBe('SELECT * FROM t ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY')
  })
})

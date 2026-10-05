import { describe, expect, it } from 'vitest'
import { needsQuotes, quoteName, tableRef } from './sql-ident.js'

describe('needsQuotes', () => {
  it('leaves plain names bare', () => {
    expect(needsQuotes('users', 'postgres')).toBe(false)
    expect(needsQuotes('order_items', 'mysql')).toBe(false)
    expect(needsQuotes('_tmp2', 'sqlite')).toBe(false)
  })

  it('quotes words reserved on every engine', () => {
    for (const engine of ['postgres', 'mysql', 'sqlite', 'mssql', 'clickhouse']) {
      expect(needsQuotes('select', engine), engine).toBe(true)
      expect(needsQuotes('Order', engine), engine).toBe(true)
    }
  })

  it("follows each engine's own list", () => {
    // USER is reserved in Postgres and SQL Server, a plain name in MySQL and SQLite.
    expect(needsQuotes('user', 'postgres')).toBe(true)
    expect(needsQuotes('user', 'mssql')).toBe(true)
    expect(needsQuotes('user', 'mysql')).toBe(false)
    expect(needsQuotes('user', 'sqlite')).toBe(false)
    expect(needsQuotes('public', 'postgres')).toBe(false)
    expect(needsQuotes('public', 'mssql')).toBe(true)
    expect(needsQuotes('rank', 'mysql')).toBe(true)
    // No list of its own: every list applies.
    expect(needsQuotes('user', 'clickhouse')).toBe(true)
  })

  it('quotes what Postgres would fold, and only there', () => {
    expect(needsQuotes('Products', 'postgres')).toBe(true)
    expect(needsQuotes('Products', 'cockroachdb')).toBe(true)
    expect(needsQuotes('Products', 'mysql')).toBe(false)
    expect(needsQuotes('Products', 'clickhouse')).toBe(false)
  })

  it('quotes names that are not plain identifiers', () => {
    expect(needsQuotes('my table', 'mysql')).toBe(true)
    expect(needsQuotes('2024_sales', 'postgres')).toBe(true)
    expect(needsQuotes('price$usd', 'postgres')).toBe(true)
  })
})

describe('quoteName', () => {
  it("quotes each engine's way and doubles the closing quote", () => {
    expect(quoteName('a"b', 'postgres')).toBe('"a""b"')
    expect(quoteName('a`b', 'mysql')).toBe('`a``b`')
    expect(quoteName('a]b', 'mssql')).toBe('[a]]b]')
  })

  it('follows the mode', () => {
    expect(quoteName('users', 'postgres', 'auto')).toBe('users')
    expect(quoteName('user', 'postgres', 'auto')).toBe('"user"')
    expect(quoteName('users', 'postgres', 'always')).toBe('"users"')
    expect(quoteName('My Table', 'postgres', 'never')).toBe('My Table')
  })
})

describe('tableRef', () => {
  it('qualifies by default', () => {
    expect(tableRef({ schema: 'public', table: 'users', engine: 'postgres', mode: 'auto' })).toBe('public.users')
    expect(tableRef({ schema: 'public', table: 'users', engine: 'postgres' })).toBe('"public"."users"')
  })

  it("leaves out only the engine's default schema when qualifying is off", () => {
    expect(tableRef({ schema: 'public', table: 'users', engine: 'postgres', mode: 'auto', qualify: false })).toBe('users')
    expect(tableRef({ schema: 'billing', table: 'users', engine: 'postgres', mode: 'auto', qualify: false })).toBe('billing.users')
    expect(tableRef({ schema: 'dbo', table: 'users', engine: 'mssql', mode: 'auto', qualify: false })).toBe('users')
    expect(tableRef({ schema: 'shop', table: 'users', engine: 'mysql', mode: 'auto', qualify: false })).toBe('shop.users')
    expect(tableRef({ schema: 'main', table: 't', engine: 'sqlite', mode: 'auto', qualify: false })).toBe('t')
    expect(tableRef({ schema: 'other', table: 't', engine: 'sqlite', mode: 'auto', qualify: false })).toBe('other.t')
  })

  it('takes a bare table when there is no schema', () => {
    expect(tableRef({ schema: '', table: 'order', engine: 'sqlite', mode: 'auto' })).toBe('"order"')
  })
})

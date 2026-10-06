import { describe, it, expect } from 'vitest'
import { explainSqlError, suggestNames, bareName, writeIdent, tablesIn } from './sql-error-explain.js'

describe('explainSqlError', () => {
  it('says "table" for a Postgres relation', () => {
    expect(explainSqlError('relation "alembic_versio" does not exist')).toMatchObject({
      kind: 'table', title: 'No table named', name: 'alembic_versio',
    })
  })

  it('keeps the qualifier Postgres reported', () => {
    expect(explainSqlError('relation "public.orderz" does not exist').name).toBe('public.orderz')
  })

  it('reads Postgres columns, quoted, qualified and of a relation', () => {
    expect(explainSqlError('column "emial" does not exist')).toMatchObject({ kind: 'column', name: 'emial' })
    expect(explainSqlError('column u.emial does not exist')).toMatchObject({ kind: 'column', name: 'u.emial' })
    expect(explainSqlError('column "emial" of relation "users" does not exist')).toMatchObject({ kind: 'column', name: 'emial' })
  })

  it('reads MySQL, SQLite, SQL Server, DuckDB and ClickHouse wording', () => {
    expect(explainSqlError("1146 (42S02): Table 'shop.orderz' doesn't exist")).toMatchObject({ kind: 'table', name: 'orderz' })
    expect(explainSqlError("1054 (42S22): Unknown column 'emial' in 'field list'")).toMatchObject({ kind: 'column', name: 'emial' })
    expect(explainSqlError('(code: 1) no such table: orderz')).toMatchObject({ kind: 'table', name: 'orderz' })
    expect(explainSqlError('no such column: emial')).toMatchObject({ kind: 'column', name: 'emial' })
    expect(explainSqlError("Invalid object name 'dbo.orderz'.")).toMatchObject({ kind: 'table', name: 'dbo.orderz' })
    expect(explainSqlError('Catalog Error: Table with name orderz does not exist!')).toMatchObject({ kind: 'table', name: 'orderz' })
    expect(explainSqlError('Code: 60. DB::Exception: Table default.orderz does not exist.')).toMatchObject({ kind: 'table', name: 'default.orderz' })
  })

  it('names the token a syntax error is at, and the early end', () => {
    expect(explainSqlError('syntax error at or near "FORM"')).toMatchObject({ kind: 'syntax', title: 'Syntax error near', name: 'FORM' })
    expect(explainSqlError('syntax error at end of input')).toMatchObject({ kind: 'syntax', title: 'The statement ends too early', name: '' })
    expect(explainSqlError("1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near 'FORM users' at line 1"))
      .toMatchObject({ kind: 'syntax', name: 'FORM' })
    expect(explainSqlError("You have an error in your SQL syntax; check the manual for the right syntax to use near '' at line 1"))
      .toMatchObject({ kind: 'syntax', title: 'The statement ends too early', name: '' })
  })

  it('falls back to the message itself', () => {
    expect(explainSqlError('permission denied for table secrets\nDETAIL: nope')).toEqual({
      kind: 'other', title: 'permission denied for table secrets', name: '', detail: 'DETAIL: nope',
    })
  })
})

describe('suggestNames', () => {
  const tables = ['alembic_version', 'alembic_versions_old', 'users', 'Orders', 'order_items']

  it('finds the typo', () => {
    expect(suggestNames('alembic_versio', tables, 'postgres')[0]).toBe('alembic_version')
  })

  it('puts a case-only match first, quoted on Postgres', () => {
    expect(suggestNames('orders', tables, 'postgres')[0]).toBe('"Orders"')
    expect(suggestNames('orders', tables, 'mysql')[0]).toBe('Orders')
  })

  it('compares the bare name of a qualified one', () => {
    expect(suggestNames('public.userz', tables, 'postgres')).toContain('users')
  })

  it('counts a swapped pair as one typo', () => {
    expect(suggestNames('FORM', ['FROM', 'FOR', 'FORMAT'], '')[0]).toBe('FROM')
  })

  it('stays quiet when nothing is close', () => {
    expect(suggestNames('invoices', tables, 'postgres')).toEqual([])
    expect(suggestNames('', tables, 'postgres')).toEqual([])
  })

  it('caps the list', () => {
    expect(suggestNames('a', ['b', 'c', 'd', 'e'], 'postgres', 2).length).toBeLessThanOrEqual(2)
  })
})

describe('helpers', () => {
  it('bareName strips qualifiers and quotes', () => {
    expect(bareName('public.orders')).toBe('orders')
    expect(bareName('"public"."My.Table"')).toBe('My.Table')
    expect(bareName('`shop`.`orders`')).toBe('orders')
  })

  it('writeIdent quotes only what needs it', () => {
    expect(writeIdent('orders', 'postgres')).toBe('orders')
    expect(writeIdent('Orders', 'postgres')).toBe('"Orders"')
    expect(writeIdent('order items', 'mysql')).toBe('`order items`')
    expect(writeIdent('Orders', 'sqlite')).toBe('Orders')
  })

  it('tablesIn lists the tables a statement touches', () => {
    expect(tablesIn('SELECT u.id FROM public.users u JOIN "Orders" o ON o.user_id = u.id')).toEqual(['users', 'Orders'])
    expect(tablesIn('update accounts set x = 1')).toEqual(['accounts'])
  })
})

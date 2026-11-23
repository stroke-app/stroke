import { describe, it, expect } from 'vitest'

describe('CREATE TABLE starters', () => {
  it('SQL Server and ClickHouse each have their own, in their own syntax', async () => {
    const { SQL_SNIPPETS } = await import('./sql-complete-data.js')
    const forFam = (/** @type {string} */ f) => SQL_SNIPPETS.find((s) => s.name === 'CREATE TABLE' && s.only?.includes(f))
    expect(forFam('mssql')?.body).toContain('IDENTITY(1,1)')
    expect(forFam('clickhouse')?.body).toContain('ENGINE = MergeTree')
  })
})

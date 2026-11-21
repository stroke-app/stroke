import { describe, expect, it } from 'vitest'
import { genDelete, genSelectFields, genSelectStar } from './sql-generate.js'

const base = {
  schema: 'public',
  table: 'users',
  columns: [{ name: 'id' }, { name: 'Email' }, { name: 'user' }],
  primaryKey: ['id'],
}

describe('Generate SQL names', () => {
  it('writes names the way the grid does without a quote setting', () => {
    expect(genSelectStar({ ...base, dialect: 'postgres' })).toBe('SELECT *\nFROM "public"."users"\nLIMIT 100;')
    expect(genDelete({ ...base, dialect: 'mysql' })).toBe('DELETE FROM `public`.`users`\nWHERE `id` = :id;')
  })

  it('quotes only what needs it when asked', () => {
    expect(genSelectFields({ ...base, dialect: 'postgres', quote: 'auto' })).toBe(
      'SELECT\n  id,\n  "Email",\n  "user"\nFROM public.users\nLIMIT 100;',
    )
  })

  it('leaves the default schema out when qualifying is off', () => {
    expect(genSelectStar({ ...base, dialect: 'postgres', quote: 'auto', qualify: false })).toBe('SELECT *\nFROM users\nLIMIT 100;')
    expect(genSelectStar({ ...base, schema: 'billing', dialect: 'postgres', quote: 'auto', qualify: false })).toBe(
      'SELECT *\nFROM billing.users\nLIMIT 100;',
    )
  })

  it('never qualifies where the grid does not (the SQLite family)', () => {
    expect(genSelectStar({ ...base, schema: 'main', dialect: 'sqlite', quote: 'always' })).toBe('SELECT *\nFROM "users"\nLIMIT 100;')
  })
})

describe('genUpsert on GoogleSQL engines', () => {
  it("writes Spanner's INSERT OR UPDATE and offers none on BigQuery", async () => {
    const { genUpsert } = await import('./sql-generate.js')
    const base = { schema: 'default', table: 'users', columns: [{ name: 'id' }, { name: 'name' }], primaryKey: ['id'] }
    expect(genUpsert({ ...base, dialect: /** @type {any} */ ('spanner') })).toBe('INSERT OR UPDATE INTO `users` (\n  `id`,\n  `name`\n) VALUES (\n  :id,\n  :name\n);')
    expect(genUpsert({ ...base, schema: 'shop', dialect: /** @type {any} */ ('bigquery') })).toBe(null)
  })
})

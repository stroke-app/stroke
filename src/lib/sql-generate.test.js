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

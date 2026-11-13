import { describe, expect, it } from 'vitest'
import { buildRowDeleteStatements, buildRowUpdateStatements, buildUpdateStatements } from './dml-preview.js'

/** @type {import('./dml-preview.js').DmlContext} */
const ctx = {
  dialect: 'postgres',
  schema: 'public',
  table: 'users',
  columns: [{ name: 'id' }, { name: 'name' }, { name: 'email' }, { name: 'age' }],
  primaryKey: ['id'],
}
const rows = [
  [1, 'Ann', 'ann@x.io', 30],
  [2, "O'Neil", null, 41],
]

describe('buildRowUpdateStatements', () => {
  it('writes one UPDATE per row with every changed column, sorted by name', () => {
    const edits = [
      { rowIdx: 0, colIdx: 2, value: 'a@x.io' },
      { rowIdx: 1, colIdx: 3, value: 42 },
      { rowIdx: 0, colIdx: 1, value: 'Annie' },
    ]
    expect(buildRowUpdateStatements(edits, rows, ctx)).toEqual([
      `UPDATE "public"."users" SET "email" = 'a@x.io', "name" = 'Annie' WHERE "id" = 1;`,
      `UPDATE "public"."users" SET "age" = 42 WHERE "id" = 2;`,
    ])
    // The per-cell builder still writes one statement per edit.
    expect(buildUpdateStatements(edits, rows, ctx)).toHaveLength(3)
  })

  it('keeps the last value when a cell is staged twice, and renders NULL', () => {
    const edits = [
      { rowIdx: 1, colIdx: 1, value: 'x' },
      { rowIdx: 1, colIdx: 1, value: null },
    ]
    expect(buildRowUpdateStatements(edits, rows, ctx)).toEqual([
      `UPDATE "public"."users" SET "name" = NULL WHERE "id" = 2;`,
    ])
  })

  it('addresses composite keys and marks tables without one', () => {
    const composite = { ...ctx, primaryKey: ['id', 'email'] }
    expect(buildRowUpdateStatements([{ rowIdx: 1, colIdx: 3, value: 1 }], rows, composite)).toEqual([
      `UPDATE "public"."users" SET "age" = 1 WHERE "id" = 2 AND "email" IS NULL;`,
    ])
    expect(buildRowUpdateStatements([{ rowIdx: 0, colIdx: 3, value: 1 }], rows, { ...ctx, primaryKey: [] })).toEqual([
      `UPDATE "public"."users" SET "age" = 1 WHERE /* no primary key */;`,
    ])
  })
})

describe('buildRowDeleteStatements', () => {
  it('deletes key by key', () => {
    expect(buildRowDeleteStatements([0, 1], rows, ctx)).toEqual([
      `DELETE FROM "public"."users" WHERE "id" = 1;`,
      `DELETE FROM "public"."users" WHERE "id" = 2;`,
    ])
  })
})

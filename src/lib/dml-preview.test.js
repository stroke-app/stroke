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

describe('GoogleSQL previews (Spanner, BigQuery)', () => {
  it('backticks names, escapes with backslashes, and leaves Spanner\'s default schema unnamed', () => {
    const spanner = { ...ctx, dialect: /** @type {const} */ ('spanner'), schema: 'default' }
    expect(buildRowUpdateStatements([{ rowIdx: 1, colIdx: 1, value: "O'Neil \\ co" }], rows, spanner)).toEqual([
      "UPDATE `users` SET `name` = 'O\\'Neil \\\\ co' WHERE `id` = 2;",
    ])
    const bq = { ...ctx, dialect: /** @type {const} */ ('bigquery'), schema: 'shop' }
    expect(buildRowDeleteStatements([0], rows, bq)).toEqual(['DELETE FROM `shop`.`users` WHERE `id` = 1;'])
  })
})

describe('ODBC previews', () => {
  it("use the driver's own quote", async () => {
    const { setOdbcDialect } = await import('./sql-ident.js')
    setOdbcDialect({ quote: '`', namespace: 'catalog' })
    const odbcCtx = { ...ctx, dialect: /** @type {const} */ ('odbc'), schema: 'shop' }
    expect(buildRowDeleteStatements([0], rows, odbcCtx)).toEqual(['DELETE FROM `shop`.`users` WHERE `id` = 1;'])
    setOdbcDialect({ quote: '"', namespace: 'flat' })
    expect(buildRowDeleteStatements([0], rows, odbcCtx)).toEqual(['DELETE FROM "users" WHERE "id" = 1;'])
  })
})

describe('PostgreSQL-dialect Spanner previews', () => {
  it('quote and escape the PostgreSQL way, and GoogleSQL is untouched', async () => {
    const { setSpannerDialect } = await import('./sql-ident.js')
    const spanner = { ...ctx, dialect: /** @type {const} */ ('spanner'), schema: 'public' }
    setSpannerDialect('postgresql')
    expect(buildRowUpdateStatements([{ rowIdx: 1, colIdx: 1, value: "O'Neil" }], rows, spanner)).toEqual([
      `UPDATE "users" SET "name" = 'O''Neil' WHERE "id" = 2;`,
    ])
    setSpannerDialect('googlesql')
    expect(buildRowUpdateStatements([{ rowIdx: 1, colIdx: 1, value: "O'Neil" }], rows, { ...spanner, schema: 'default' })).toEqual([
      "UPDATE `users` SET `name` = 'O\\'Neil' WHERE `id` = 2;",
    ])
  })
})

describe('sqlInList', () => {
  it('quotes per dialect, keeps first-seen order, drops duplicates and NULLs', async () => {
    const { sqlInList } = await import('./dml-preview.js')
    expect(sqlInList(["O'Neil", 'b', "O'Neil", null, 'a'], 'postgres')).toEqual({ text: "('O''Neil', 'b', 'a')", count: 3, nulls: 1 })
    expect(sqlInList([3, 1, 3, 2], 'mysql')).toEqual({ text: '(3, 1, 2)', count: 3, nulls: 0 })
    expect(sqlInList(['a\\b'], 'mysql').text).toBe("('a\\\\b')")
    expect(sqlInList([null, undefined], 'sqlite')).toEqual({ text: '', count: 0, nulls: 2 })
  })
})

describe('SQL Server Unicode strings', () => {
  it('prefixes N only when the text is not ASCII', async () => {
    const { sqlLiteral } = await import('./dml-preview.js')
    expect(sqlLiteral('Zoë', 'mssql')).toBe("N'Zoë'")
    expect(sqlLiteral('東京', 'mssql')).toBe("N'東京'")
    expect(sqlLiteral("O'Neil", 'mssql')).toBe("'O''Neil'")
    expect(sqlLiteral('東京', 'postgres')).toBe("'東京'")
  })
})

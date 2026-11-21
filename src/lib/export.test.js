import { describe, it, expect } from 'vitest'

describe('rowsToSql for an engine', () => {
  it("writes MySQL's backticks and 1/0, and stays ANSI without one", async () => {
    const { rowsToSql } = await import('./export.js')
    const cols = [{ name: 'id' }, { name: 'ok' }, { name: 'note' }]
    const rows = [[1, true, "it's"]]
    expect(rowsToSql(cols, rows, 'products', 'mysql')).toBe("INSERT INTO `products` (`id`, `ok`, `note`) VALUES (1, 1, 'it''s');")
    expect(rowsToSql(cols, rows, 'products')).toBe(`INSERT INTO "products" ("id", "ok", "note") VALUES (1, TRUE, 'it''s');`)
  })
})

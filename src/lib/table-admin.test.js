import { describe, expect, it } from 'vitest'
import { dropObjectSql, supportsCascade, tableDialect, truncateTableSql } from './table-admin.js'

// Same cases as the `tests` module in src-tauri/src/db/admin.rs.
describe('table-admin', () => {
  it('maps every connection type to the dialect it is spelled in', () => {
    expect(tableDialect('mariadb')).toBe('mysql')
    expect(tableDialect('cockroachdb')).toBe('postgres')
    expect(tableDialect('d1')).toBe('sqlite')
    expect(tableDialect('libsql')).toBe('sqlite')
    expect(tableDialect('mssql')).toBe('mssql')
    expect(tableDialect('redis')).toBeNull()
    expect(tableDialect('posthog')).toBeNull()
  })

  it('spells a table drop per dialect', () => {
    expect(dropObjectSql('postgres', 'public', 'users')).toBe('DROP TABLE "public"."users"')
    expect(dropObjectSql('postgres', 'public', 'users', { cascade: true })).toBe('DROP TABLE "public"."users" CASCADE')
    expect(dropObjectSql('mysql', 'shop', 'order items', { cascade: true })).toBe('DROP TABLE `shop`.`order items`')
    expect(dropObjectSql('mssql', 'dbo', 'a]b', { cascade: true })).toBe('DROP TABLE [dbo].[a]]b]')
    expect(dropObjectSql('sqlite', 'main', 'x"y', { cascade: true })).toBe('DROP TABLE "x""y"')
    expect(dropObjectSql('duckdb', 'main', 'events', { cascade: true })).toBe('DROP TABLE "events" CASCADE')
    expect(dropObjectSql('clickhouse', 'default', 'hits')).toBe('DROP TABLE `default`.`hits`')
  })

  it('names views by their kind', () => {
    expect(dropObjectSql('postgres', 's', 'v', { kind: 'view', cascade: true })).toBe('DROP VIEW "s"."v" CASCADE')
    expect(dropObjectSql('postgres', 's', 'm', { kind: 'materialized_view' })).toBe('DROP MATERIALIZED VIEW "s"."m"')
    expect(dropObjectSql('mysql', 'shop', 'v', { kind: 'view' })).toBe('DROP VIEW `shop`.`v`')
    expect(dropObjectSql('clickhouse', 'db', 'mv', { kind: 'materialized_view' })).toBe('DROP VIEW `db`.`mv`')
    expect(() => dropObjectSql('mysql', 'shop', 'm', { kind: 'materialized_view' })).toThrow()
  })

  it('falls back to DELETE where there is no TRUNCATE', () => {
    expect(truncateTableSql('mysql', 'shop', 't')).toBe('TRUNCATE TABLE `shop`.`t`')
    expect(truncateTableSql('mssql', 'dbo', 't')).toBe('TRUNCATE TABLE [dbo].[t]')
    expect(truncateTableSql('sqlite', 'main', 't')).toBe('DELETE FROM "t"')
    expect(truncateTableSql('duckdb', 'main', 't')).toBe('DELETE FROM "t"')
  })

  it('offers CASCADE only where it does something', () => {
    expect(supportsCascade('postgres')).toBe(true)
    expect(supportsCascade('duckdb')).toBe(true)
    expect(supportsCascade('mysql')).toBe(false)
    expect(supportsCascade('mssql')).toBe(false)
  })
})

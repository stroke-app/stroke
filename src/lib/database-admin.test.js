import { describe, it, expect } from 'vitest'
import {
  dbAdminKind,
  dbActionBlocker,
  canDbAction,
  quoteDb,
  validateDbName,
  createDatabaseSql,
  renameDatabaseSql,
  duplicateDatabaseSql,
  dropDatabaseSql,
  terminateSessionsSql,
  sessionCountSql,
  canForceDrop,
  databaseInfoSql,
  databaseInfoRows,
} from './database-admin.js'

const pg = /** @type {any} */ ({ type: 'postgres', database: 'app' })
const mysql = /** @type {any} */ ({ type: 'mysql', database: 'app' })

describe('dbAdminKind', () => {
  it('maps the engine families onto a dialect', () => {
    expect(dbAdminKind(pg)).toBe('postgres')
    expect(dbAdminKind({ ...pg, type: 'cockroachdb' })).toBe('postgres')
    expect(dbAdminKind(mysql)).toBe('mysql')
    expect(dbAdminKind({ ...mysql, type: 'mariadb' })).toBe('mysql')
    expect(dbAdminKind({ ...pg, type: 'mssql' })).toBe('mssql')
    expect(dbAdminKind({ ...pg, type: 'clickhouse' })).toBe('clickhouse')
  })
  it('rules out engines and providers with no server-level databases', () => {
    expect(dbAdminKind(null)).toBeNull()
    expect(dbAdminKind({ ...pg, type: 'sqlite' })).toBeNull()
    expect(dbAdminKind({ ...pg, type: 'd1' })).toBeNull()
    expect(dbAdminKind({ ...pg, type: 'duckdb' })).toBeNull()
    expect(dbAdminKind({ ...pg, type: 'redis' })).toBeNull()
    expect(dbAdminKind({ ...pg, type: 'posthog' })).toBeNull()
    // A Neon connection is postgres, but its sibling databases are API refs.
    expect(dbAdminKind({ ...pg, provider: 'neon' })).toBeNull()
  })
})

describe('dbActionBlocker', () => {
  it('allows every action on another database', () => {
    for (const action of /** @type {const} */ (['rename', 'duplicate', 'drop', 'terminate'])) {
      expect(canDbAction(action, pg, { isCurrent: false })).toBe(true)
    }
  })
  it('refuses to rename, copy or drop the connected database', () => {
    expect(dbActionBlocker('rename', pg, { isCurrent: true })).toMatch(/connected to/)
    expect(dbActionBlocker('drop', pg, { isCurrent: true })).toMatch(/connected to/)
    expect(dbActionBlocker('duplicate', pg, { isCurrent: true })).toMatch(/while a session is connected/)
  })
  it('says what MySQL cannot express at all, and copies the rest', () => {
    expect(dbActionBlocker('rename', mysql, { isCurrent: false })).toMatch(/no RENAME DATABASE/)
    expect(canDbAction('duplicate', mysql, { isCurrent: false })).toBe(true)
    expect(canDbAction('duplicate', mysql, { isCurrent: true })).toBe(true)
    expect(canDbAction('drop', mysql, { isCurrent: false })).toBe(true)
  })
  it('copies and drops on SQL Server and ClickHouse, but never the connected database', () => {
    for (const type of ['mssql', 'clickhouse']) {
      const conn = /** @type {any} */ ({ type, database: 'app' })
      for (const action of /** @type {const} */ (['rename', 'duplicate', 'drop'])) {
        expect(canDbAction(action, conn, { isCurrent: false })).toBe(true)
      }
      expect(dbActionBlocker('drop', conn, { isCurrent: true })).toMatch(/connected to/)
      expect(canDbAction('terminate', conn)).toBe(false)
    }
  })
  it('knows where the engine or host has no such statement', () => {
    const cockroach = /** @type {any} */ ({ type: 'cockroachdb', database: 'app' })
    expect(dbActionBlocker('duplicate', cockroach)).toMatch(/no database templates/)
    expect(canDbAction('drop', cockroach)).toBe(true)
    const ps = /** @type {any} */ ({ type: 'mysql', host: 'aws.connect.psdb.cloud', database: 'app' })
    expect(dbActionBlocker('drop', ps)).toMatch(/PlanetScale/)
    expect(canDbAction('info', ps)).toBe(true)
  })
  it('blocks everything on a connection with no manageable databases', () => {
    expect(dbActionBlocker('drop', { ...pg, type: 'sqlite' })).toMatch(/no server-level databases/)
  })
})

describe('validateDbName', () => {
  it('rejects empty, over-long and unquotable names', () => {
    expect(validateDbName('  ')).toMatch(/required/)
    expect(validateDbName('x'.repeat(64))).toMatch(/63/)
    expect(validateDbName('bad"name')).toMatch(/quotes/)
    expect(validateDbName('bad`name')).toMatch(/quotes/)
  })
  it('catches a collision case-insensitively', () => {
    expect(validateDbName('App', ['app'])).toMatch(/already exists/)
    expect(validateDbName('app_2', ['app'])).toBe('')
  })
})

describe('statement builders', () => {
  it('quotes identifiers per dialect and escapes the quote character', () => {
    expect(quoteDb('postgres', 'my db')).toBe('"my db"')
    expect(quoteDb('mysql', 'my db')).toBe('`my db`')
    expect(quoteDb('postgres', 'a"b')).toBe('"a""b"')
    expect(quoteDb('mysql', 'a`b')).toBe('`a``b`')
  })

  it('builds CREATE DATABASE with only the options that were set', () => {
    expect(createDatabaseSql('postgres', { name: 'app' })).toBe('CREATE DATABASE "app"')
    const full = createDatabaseSql('postgres', {
      name: 'app', owner: 'nees', encoding: 'UTF8', lcCollate: 'en_US.utf8', template: 'template0', connectionLimit: 10,
    })
    expect(full).toContain(`ENCODING 'UTF8'`)
    expect(full).toContain('TEMPLATE template0')
    expect(full).toContain('OWNER "nees"')
    expect(full).toContain('CONNECTION LIMIT 10')
    expect(createDatabaseSql('mysql', { name: 'app', encoding: 'utf8mb4', lcCollate: 'utf8mb4_unicode_ci' }))
      .toBe('CREATE DATABASE `app` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci')
  })

  it('renames per dialect, and refuses on MySQL', () => {
    expect(renameDatabaseSql('postgres', 'old', 'new')).toBe('ALTER DATABASE "old" RENAME TO "new"')
    expect(renameDatabaseSql('mssql', 'old', 'new')).toBe('ALTER DATABASE [old] MODIFY NAME = [new]')
    expect(renameDatabaseSql('clickhouse', 'old', 'new')).toBe('RENAME DATABASE `old` TO `new`')
    expect(() => renameDatabaseSql('mysql', 'old', 'new')).toThrow()
  })

  it('previews a copy: one statement on Postgres, the outline elsewhere', () => {
    expect(duplicateDatabaseSql('postgres', 'src', 'copy')).toBe('CREATE DATABASE "copy" WITH TEMPLATE "src"')
    expect(duplicateDatabaseSql('mysql', 'src', 'copy')).toMatch(/^CREATE DATABASE `copy`[\s\S]*FROM `src`\.<table>/)
    expect(duplicateDatabaseSql('clickhouse', 'src', 'copy')).toContain('CREATE TABLE `copy`.<table> AS `src`.<table>')
    expect(duplicateDatabaseSql('mssql', 'src', 'copy')).toMatch(/^BACKUP DATABASE \[src\][\s\S]*RESTORE DATABASE \[copy\]/)
  })

  it('creates SQL Server and ClickHouse databases with the server defaults', () => {
    expect(createDatabaseSql('mssql', { name: 'a]b' })).toBe('CREATE DATABASE [a]]b]')
    expect(createDatabaseSql('clickhouse', { name: 'app' })).toBe('CREATE DATABASE `app`')
  })

  it('forces a drop on Postgres and SQL Server only', () => {
    expect(dropDatabaseSql('mssql', 'app')).toBe('DROP DATABASE [app]')
    expect(dropDatabaseSql('mssql', 'app', { force: true })).toBe('ALTER DATABASE [app] SET SINGLE_USER WITH ROLLBACK IMMEDIATE;\nDROP DATABASE [app]')
    expect(dropDatabaseSql('clickhouse', 'app', { force: true })).toBe('DROP DATABASE `app`')
    expect(canForceDrop(pg)).toBe(true)
    expect(canForceDrop({ ...pg, type: 'cockroachdb' })).toBe(false)
    expect(canForceDrop({ ...pg, type: 'mssql' })).toBe(true)
    expect(canForceDrop(mysql)).toBe(false)
    expect(sessionCountSql('mssql', 'app')).toContain(`DB_ID('app')`)
    expect(sessionCountSql('mysql', 'app')).toBe('')
  })

  it('drops with FORCE only where it exists and only when asked', () => {
    expect(dropDatabaseSql('postgres', 'app')).toBe('DROP DATABASE "app"')
    expect(dropDatabaseSql('postgres', 'app', { force: true })).toBe('DROP DATABASE "app" WITH (FORCE)')
    expect(dropDatabaseSql('mysql', 'app', { force: true })).toBe('DROP DATABASE `app`')
  })

  it('escapes the name as a literal where it is compared, not quoted', () => {
    expect(terminateSessionsSql('postgres', "o'brien")).toContain(`datname = 'o''brien'`)
    expect(databaseInfoSql('postgres', "o'brien")).toContain(`d.datname = 'o''brien'`)
    expect(databaseInfoSql('mysql', 'app')).toContain(`s.SCHEMA_NAME = 'app'`)
  })
})

describe('databaseInfoRows', () => {
  it('returns nothing for an empty result', () => {
    expect(databaseInfoRows('postgres', null)).toEqual([])
    expect(databaseInfoRows('postgres', { rows: [] })).toEqual([])
  })
  it('labels the Postgres columns and reads -1 as unlimited', () => {
    const rows = databaseInfoRows('postgres', {
      rows: [['app', 'nees', 'UTF8', 'en_US.utf8', 'en_US.utf8', -1, true, '8192 kB', 3, null]],
    })
    expect(rows.find((r) => r.label === 'Connection limit')?.value).toBe('Unlimited')
    expect(rows.find((r) => r.label === 'Accepts connections')?.value).toBe('Yes')
    expect(rows.find((r) => r.label === 'Size on disk')?.value).toBe('8192 kB')
    expect(rows.find((r) => r.label === 'Comment')?.value).toBe('-')
  })
  it('formats the MySQL size, which comes back as bytes', () => {
    const rows = databaseInfoRows('mysql', { rows: [['app', 'utf8mb4', 'utf8mb4_unicode_ci', 12, 5_242_880]] })
    expect(rows.find((r) => r.label === 'Size')?.value).toBe('5 MB')
    expect(rows.find((r) => r.label === 'Tables')?.value).toBe('12')
  })
  it('labels the SQL Server and ClickHouse columns', () => {
    const ms = databaseInfoRows('mssql', { rows: [['app', 'sa', 'SQL_Latin1_General_CP1_CI_AS', 'ONLINE', 'FULL', 160, '2026-10-05 10:00:00', 16_777_216, 2]] })
    expect(ms.find((r) => r.label === 'Size on disk')?.value).toBe('16 MB')
    expect(ms.find((r) => r.label === 'Recovery model')?.value).toBe('FULL')
    // ClickHouse sends 64-bit counts as strings.
    const ch = databaseInfoRows('clickhouse', { rows: [['app', 'Atomic', '', '3', '1200', '2048']] })
    expect(ch.find((r) => r.label === 'Size')?.value).toBe('2 kB')
    expect(ch.find((r) => r.label === 'Comment')?.value).toBe('-')
  })
})

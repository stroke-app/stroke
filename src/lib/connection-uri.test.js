import { describe, it, expect } from 'vitest'
import {
  parseMssqlUri,
  parseConnectionUri,
  detectConnectionUri,
  parseRedisUri,
  parseLibsqlUri,
} from './connection-uri.js'

describe('parseMssqlUri', () => {
  it('reads the semicolon form Prisma and JDBC actually write', () => {
    // A studio pointed at SQL Server hands us exactly this shape.
    expect(
      parseMssqlUri('sqlserver://db.example.com:1433;database=app;user=sa;password=p@ss;encrypt=true'),
    ).toEqual({
      host: 'db.example.com',
      port: '1433',
      database: 'app',
      user: 'sa',
      password: 'p@ss',
      encrypt: true,
      trustCert: true,
    })
  })

  it('defaults the port and honours trustServerCertificate=false', () => {
    const parsed = parseMssqlUri('sqlserver://localhost;database=master;trustServerCertificate=false')
    expect(parsed).toMatchObject({ host: 'localhost', port: '1433', database: 'master', trustCert: false })
  })

  it('still reads the URL form', () => {
    expect(parseMssqlUri('sqlserver://sa:pw@10.0.0.5:1444/reporting')).toMatchObject({
      host: '10.0.0.5', port: '1444', database: 'reporting', user: 'sa', password: 'pw',
    })
  })

  it('rejects something that is not a connection URI', () => {
    expect(parseMssqlUri('just some text')).toEqual({ error: 'Expected a sqlserver:// connection URI' })
  })
})

describe('parseConnectionUri', () => {
  it('routes remote provider URLs to usable fields', () => {
    expect(parseConnectionUri('postgres', 'postgresql://u:p@ep-x.eu-central-1.aws.neon.tech/neondb?sslmode=require'))
      .toMatchObject({ host: 'ep-x.eu-central-1.aws.neon.tech', database: 'neondb', user: 'u', ssl: true })
    expect(parseConnectionUri('mysql', 'mysql://user:pw@aws.connect.psdb.cloud/app'))
      .toMatchObject({ host: 'aws.connect.psdb.cloud', database: 'app', user: 'user' })
  })
})

describe('detectConnectionUri', () => {
  it('reads the engine off the scheme', () => {
    expect(detectConnectionUri('postgres://u:p@h:5432/db')?.type).toBe('postgres')
    expect(detectConnectionUri('postgresql://h/db')?.type).toBe('postgres')
    expect(detectConnectionUri('mysql://h/db')?.type).toBe('mysql')
    expect(detectConnectionUri('mariadb://h/db')?.type).toBe('mariadb')
    expect(detectConnectionUri('cockroachdb://h/db')?.type).toBe('cockroachdb')
    expect(detectConnectionUri('sqlserver://h:1433;database=x')?.type).toBe('mssql')
    expect(detectConnectionUri('clickhouse://h:8123')?.type).toBe('clickhouse')
    expect(detectConnectionUri('rediss://cache:6379')?.type).toBe('redis')
    expect(detectConnectionUri('libsql://db.example.dev')?.type).toBe('libsql')
  })

  it('names the provider a host identifies, so the caller can offer its sign-in', () => {
    expect(detectConnectionUri('postgresql://u:p@ep-cool-1.eu-central-1.aws.neon.tech/main')).toMatchObject({
      type: 'postgres',
      provider: 'neon',
    })
    expect(detectConnectionUri('postgres://u:p@aws-0-ap-south-1.pooler.supabase.com:5432/postgres')).toMatchObject({
      provider: 'supabase',
    })
    expect(detectConnectionUri('mysql://u:p@aws.connect.psdb.cloud/app')).toMatchObject({
      type: 'mysql',
      provider: 'planetscale',
    })
    expect(detectConnectionUri('libsql://app-org.turso.io')).toMatchObject({ provider: 'turso' })
    expect(detectConnectionUri('prisma+postgres://accelerate.prisma-data.net/?api_key=x')).toMatchObject({
      provider: 'prisma-postgres',
    })
  })

  it('is not fooled by credentials that contain an @', () => {
    // The host is what follows the LAST @, or a password like "p@ss" renames it.
    expect(detectConnectionUri('postgres://user:p@ss@db.neon.tech/main')).toMatchObject({ provider: 'neon' })
  })

  it('unwraps a jdbc: prefix', () => {
    expect(detectConnectionUri('jdbc:postgresql://h/db')?.type).toBe('postgres')
    expect(detectConnectionUri('jdbc:mysql://h/db')?.type).toBe('mysql')
  })

  it('takes a bare file path as SQLite, and :memory: as the in-memory engine', () => {
    expect(detectConnectionUri('/Users/a/app.db')?.type).toBe('sqlite')
    expect(detectConnectionUri('./local.sqlite3')?.type).toBe('sqlite')
    expect(detectConnectionUri('data/warehouse.duckdb')?.type).toBe('duckdb')
    expect(detectConnectionUri(':memory:')?.type).toBe('sqlite-memory')
  })

  it('returns null when nothing in the string says which engine', () => {
    expect(detectConnectionUri('')).toBeNull()
    expect(detectConnectionUri('   ')).toBeNull()
    expect(detectConnectionUri('just some text')).toBeNull()
    expect(detectConnectionUri('a sentence with app.db in it')).toBeNull()
  })

  it('hands back the uriType that parseConnectionUri expects', () => {
    const d = detectConnectionUri('mysql://root@127.0.0.1:3306/app')
    expect(d?.uriType).toBe('mysql')
    expect(parseConnectionUri(d.uriType, 'mysql://root@127.0.0.1:3306/app')).toMatchObject({ host: '127.0.0.1' })
  })
})

describe('parseRedisUri', () => {
  it('reads host, port, database index and TLS', () => {
    expect(parseRedisUri('rediss://cache.example.com:6380/3')).toMatchObject({
      host: 'cache.example.com',
      port: '6380',
      db: '3',
      tls: true,
    })
  })

  it('accepts a password with no username, which Redis allows', () => {
    // The Postgres parser reads `:pw` as a username, which is why Redis cannot
    // share it.
    expect(parseRedisUri('redis://:s3cret@localhost:6379/0')).toMatchObject({
      user: '',
      password: 's3cret',
    })
  })

  it('fills the defaults a bare host leaves out', () => {
    expect(parseRedisUri('redis://localhost')).toMatchObject({ port: '6379', db: '0', tls: false })
  })

  it('ignores a path that is not a database index', () => {
    expect(parseRedisUri('redis://h/not-a-number')).toMatchObject({ db: '0' })
  })

  it('reports a non-Redis string instead of guessing', () => {
    expect(parseRedisUri('postgres://h/db')).toMatchObject({ error: expect.stringContaining('Expected') })
    expect(parseRedisUri('')).toBeNull()
  })
})

describe('parseLibsqlUri', () => {
  it('splits the auth token out of the query string', () => {
    expect(parseLibsqlUri('libsql://app-org.turso.io?authToken=abc')).toEqual({
      url: 'libsql://app-org.turso.io',
      authToken: 'abc',
    })
  })

  it('accepts the snake_case spelling too', () => {
    expect(parseLibsqlUri('https://db.turso.io?auth_token=xyz')).toMatchObject({ authToken: 'xyz' })
  })

  it('keeps a URL that carries no token', () => {
    expect(parseLibsqlUri('libsql://db.example.dev')).toEqual({
      url: 'libsql://db.example.dev',
      authToken: '',
    })
  })

  it('reports anything that is not a URL', () => {
    expect(parseLibsqlUri('nonsense')).toMatchObject({ error: expect.stringContaining('Expected') })
  })
})

describe('parseConnectionUri dispatch', () => {
  it('routes redis and libsql to their own parsers', () => {
    expect(parseConnectionUri('redis', 'redis://h:6379/2')).toMatchObject({ db: '2' })
    expect(parseConnectionUri('libsql', 'libsql://h?authToken=t')).toMatchObject({ authToken: 't' })
  })
})

describe('passwords with URL-special characters', () => {
  it('decodes escapes in a correctly encoded URL', async () => {
    const { parsePostgresUri } = await import('./connection-uri.js')
    const r = parsePostgresUri('postgres://lms-usr:Lm$$pR0D54%25@172.31.33.32:5432/lms-prod')
    expect(r).toMatchObject({ user: 'lms-usr', password: 'Lm$$pR0D54%', host: '172.31.33.32', database: 'lms-prod' })
  })

  it('keeps a raw, unencoded password whole', async () => {
    const { parsePostgresUri, parseMysqlUri, parseRedisUri } = await import('./connection-uri.js')
    expect(parsePostgresUri('postgres://u:p%ss@h:5432/d')).toMatchObject({ password: 'p%ss', host: 'h', database: 'd' })
    expect(parsePostgresUri('postgres://u:p#w?x@h/d')).toMatchObject({ password: 'p#w?x', host: 'h', database: 'd' })
    expect(parsePostgresUri('postgres://u:pa@ss@h/d')).toMatchObject({ password: 'pa@ss', host: 'h' })
    expect(parsePostgresUri('postgres://u:a/b@h:5432/d')).toMatchObject({ password: 'a/b', host: 'h', port: '5432' })
    expect(parseMysqlUri('mysql://u:x%y#z@h:3306/d')).toMatchObject({ password: 'x%y#z', host: 'h', database: 'd' })
    expect(parseRedisUri('rediss://default:p%q@h:6380')).toMatchObject({ password: 'p%q', host: 'h', port: '6380', tls: true })
  })
})

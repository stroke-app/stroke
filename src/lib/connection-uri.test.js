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

describe('MongoDB strings', () => {
  it('are recognised, Atlas SRV included', () => {
    expect(detectConnectionUri('mongodb://u:p@localhost:27017/shop')).toEqual({ type: 'mongodb', uriType: 'mongodb' })
    expect(detectConnectionUri('mongodb+srv://u:p@cluster0.ab12.mongodb.net/?retryWrites=true')).toEqual({ type: 'mongodb', uriType: 'mongodb' })
  })
})

describe('detectConnectionUri: file paths', () => {
  it('takes absolute paths with spaces (Windows home folders)', () => {
    expect(detectConnectionUri('C:\\Users\\Jane Doe\\Documents\\app.db')?.type).toBe('sqlite')
    expect(detectConnectionUri('/home/jane/My Data/app.duckdb')?.type).toBe('duckdb')
    expect(detectConnectionUri('~/My Data/app.sqlite')?.type).toBe('sqlite')
    expect(detectConnectionUri('\\\\server\\share\\team data.db')?.type).toBe('sqlite')
  })
  it('still leaves a relative phrase with spaces alone', () => {
    expect(detectConnectionUri('open my app.db')).toBeNull()
    expect(detectConnectionUri('app.db')?.type).toBe('sqlite')
  })
})

describe('parseConnectionUri: a bare database path fills the file field', () => {
  it('takes the path the field asks for, not only sqlite://', () => {
    expect(parseConnectionUri('sqlite', '/tmp/app.db')).toEqual({ filePath: '/tmp/app.db' })
    expect(parseConnectionUri('sqlite', 'C:\\Users\\Jane Doe\\app.db')).toEqual({ filePath: 'C:\\Users\\Jane Doe\\app.db' })
  })
  it('drops the quotes Windows "Copy as path" adds', () => {
    expect(parseConnectionUri('sqlite', '"C:\\Users\\Jane Doe\\app.db"')).toEqual({ filePath: 'C:\\Users\\Jane Doe\\app.db' })
    expect(detectConnectionUri('"C:\\data\\x.duckdb"')?.type).toBe('duckdb')
  })
})

describe('Oracle connection strings', () => {
  it('reads oracle:// URLs as host, port and service name', () => {
    expect(parseConnectionUri('oracle', 'oracle://scott:t%40ger@db.example.com:1522/ORCLPDB1')).toEqual({
      oracle: true, host: 'db.example.com', port: '1522', serviceName: 'ORCLPDB1', sid: '', connectString: '', user: 'scott', password: 't@ger',
    })
  })

  it('reads the JDBC thin forms: service, SID and descriptor', () => {
    expect(parseConnectionUri('oracle', 'jdbc:oracle:thin:@//h:1521/FREEPDB1')).toMatchObject({ host: 'h', port: '1521', serviceName: 'FREEPDB1', sid: '' })
    expect(parseConnectionUri('oracle', 'jdbc:oracle:thin:hr/pw@h:1521:XE')).toMatchObject({ host: 'h', sid: 'XE', serviceName: '', user: 'hr', password: 'pw' })
    const desc = '(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=h)(PORT=1521))(CONNECT_DATA=(SERVICE_NAME=s)))'
    expect(parseConnectionUri('oracle', `jdbc:oracle:thin:@${desc}`)).toMatchObject({ connectString: desc, host: '' })
  })

  it('is recognised from the paste bar, JDBC prefix included', () => {
    expect(detectConnectionUri('oracle://u:p@h:1521/svc')).toEqual({ type: 'oracle', uriType: 'oracle' })
    expect(detectConnectionUri('jdbc:oracle:thin:@//h:1521/svc')).toEqual({ type: 'oracle', uriType: 'oracle' })
  })
})

describe('detectConnectionUri: everything people paste', () => {
  const d = (s) => detectConnectionUri(s)
  it('takes the value out of a .env line', () => {
    expect(d('DATABASE_URL="postgresql://u:p@db:5432/app"')).toMatchObject({ type: 'postgres', uri: 'postgresql://u:p@db:5432/app' })
    expect(d("export REDIS_URL='rediss://:pw@x.upstash.io:6379'")).toMatchObject({ type: 'redis', provider: 'upstash', uriType: 'redis' })
  })
  it('drops ORM driver suffixes', () => {
    expect(d('postgresql+psycopg2://u:p@h/db')).toMatchObject({ type: 'postgres', uri: 'postgresql://u:p@h/db' })
    expect(d('mysql+pymysql://u:p@h/db')).toMatchObject({ type: 'mysql', uri: 'mysql://u:p@h/db' })
    expect(d('mongodb+srv://u:p@c.mongodb.net/app')).toMatchObject({ type: 'mongodb' })
  })
  it('parses Redis, Valkey and libSQL with their own parsers, not Postgres', () => {
    expect(d('redis://:pw@h:6379/2')).toMatchObject({ type: 'redis', uriType: 'redis' })
    expect(d('valkey://h:6379')).toMatchObject({ type: 'valkey', uriType: 'redis', uri: 'redis://h:6379' })
    expect(d('libsql://db-org.turso.io?authToken=x')).toMatchObject({ type: 'libsql', uriType: 'libsql' })
    expect(d('https://db-org.turso.io')).toMatchObject({ type: 'libsql' })
  })
  it('reads key/value strings', () => {
    const pg = d("host=db.example.com port=5433 dbname=shop user=me password='p w' sslmode=require")
    expect(pg).toMatchObject({ type: 'postgres' })
    expect(parseConnectionUri('postgres', pg.uri)).toMatchObject({ host: 'db.example.com', port: '5433', database: 'shop', user: 'me', password: 'p w' })
    const ms = d('Server=tcp:sql.example.com,1433;Database=app;User Id=sa;Password=Secret1!;Encrypt=True;')
    expect(ms).toMatchObject({ type: 'mssql' })
    expect(parseConnectionUri('mssql', ms.uri)).toMatchObject({ host: 'sql.example.com', port: '1433', database: 'app', user: 'sa', password: 'Secret1!' })
  })
  it('knows the NoSQL and HTTP endpoints', () => {
    expect(d('couchdb://admin:pw@localhost:5984')).toMatchObject({ type: 'couchdb' })
    expect(parseConnectionUri('couchdb', 'http://admin:pw@localhost:5984')).toEqual({ couch: true, url: 'http://localhost:5984', user: 'admin', password: 'pw' })
    expect(d('cassandra://u:p@h1:9042,h2:9042/shop')).toMatchObject({ type: 'cassandra' })
    expect(parseConnectionUri('cassandra', 'cassandra://u:p@h1:9042,h2:9042/shop')).toEqual({ cql: true, user: 'u', password: 'p', hosts: 'h1:9042,h2:9042', keyspace: 'shop' })
    expect(d('http://localhost:8123')).toMatchObject({ type: 'clickhouse' })
    expect(d('https://example.com')).toBeNull()
  })
  it('recognises cloud hosts', () => {
    expect(d('mysql://u:p@gateway01.ap-northeast-1.prod.aws.tidbcloud.com:4000/test')).toMatchObject({ type: 'mysql', provider: 'tidb' })
    expect(d('postgres://u:p@eu-central-1.db.thenile.dev/db')).toMatchObject({ provider: 'nile' })
    expect(d('postgresql://u:p@monorail.proxy.rlwy.net:5432/railway')).toMatchObject({ type: 'postgres', provider: 'railway' })
    expect(d('jdbc:sqlserver://h:1433;databaseName=x')).toMatchObject({ type: 'mssql' })
    expect(d('jdbc:oracle:thin:@//h:1521/FREEPDB1')).toMatchObject({ type: 'oracle' })
  })
  it('still says no to junk', () => {
    expect(d('adsf')).toBeNull()
  })
})

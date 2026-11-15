import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { parseEnv, findConnection, resolveFileUri, engineOf, redact } from '../src/find.js'
import { appLink } from '../src/app.js'
import { splitStatements, cell, openSqlite } from '../src/engines.js'
import { startBridge, studioUrl } from '../src/bridge.js'

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'strokedb-'))

test('parseEnv: export, quotes, comments, references', () => {
  const env = parseEnv(`
# comment
export HOST=db.local
USER="me"
PASS='p#ss \${HOST}'
DATABASE_URL=postgres://\${USER}:x@\${HOST}/app # trailing
EMPTY=
`)
  assert.equal(env.HOST, 'db.local')
  assert.equal(env.PASS, 'p#ss ${HOST}')
  assert.equal(env.DATABASE_URL, 'postgres://me:x@db.local/app')
  assert.equal(env.EMPTY, '')
})

test('findConnection: argument, then env, then .env files, then a .db file', () => {
  const dir = tmp()
  assert.equal(findConnection({ cwd: dir, env: {} }), null)

  fs.writeFileSync(path.join(dir, 'dev.sqlite'), '')
  assert.equal(findConnection({ cwd: dir, env: {} })?.uri, path.join(dir, 'dev.sqlite'))

  fs.writeFileSync(path.join(dir, '.env'), 'DATABASE_URL=postgres://a@h/one\n')
  fs.writeFileSync(path.join(dir, '.env.local'), 'DATABASE_URL=postgres://a@h/two\n')
  const fromFile = findConnection({ cwd: dir, env: {} })
  assert.equal(fromFile?.uri, 'postgres://a@h/two')
  assert.equal(fromFile?.source, 'DATABASE_URL in .env.local')

  assert.equal(findConnection({ cwd: dir, env: { POSTGRES_URL: 'postgres://env/x' } })?.uri, 'postgres://env/x')
  assert.equal(findConnection({ arg: 'mysql://u@h/d', cwd: dir, env: {} })?.source, 'argument')
  assert.throws(() => findConnection({ arg: 'nope.db', cwd: dir, env: {} }))
})

test('file: URLs and bare paths resolve against the project', () => {
  assert.equal(resolveFileUri('file:./dev.db', '/p'), path.resolve('/p', 'dev.db'))
  assert.equal(resolveFileUri('file:dev.db?mode=ro', '/p'), path.resolve('/p', 'dev.db'))
  assert.equal(resolveFileUri('data/app.sqlite', '/p'), path.resolve('/p', 'data/app.sqlite'))
  assert.equal(resolveFileUri('postgres://h/db', '/p'), 'postgres://h/db')
})

test('engineOf and redact', () => {
  assert.equal(engineOf('postgresql://h/d'), 'postgres')
  assert.equal(engineOf('mariadb://h/d'), 'mysql')
  assert.equal(engineOf('/x/app.db'), 'sqlite')
  assert.equal(engineOf('mongodb://h'), null)
  assert.equal(redact('postgres://me:s3cr@t@host/db'), 'postgres://me:****@host/db')
  assert.equal(redact('postgres://host/db'), 'postgres://host/db')
})

test('appLink round-trips the URL and name', () => {
  const link = appLink('stroke', 'postgres://me:p&ss@h/db?sslmode=require', 'my app')
  const u = new URL(link)
  assert.equal(u.protocol, 'stroke:')
  assert.equal(u.host, 'open')
  assert.equal(u.searchParams.get('url'), 'postgres://me:p&ss@h/db?sslmode=require')
  assert.equal(u.searchParams.get('name'), 'my app')
})

test('splitStatements respects quotes and comments', () => {
  assert.deepEqual(splitStatements(`select ';'; -- a;b\nselect 2; /* ; */ select "x;y"`), [`select ';'`, 'select 2', 'select "x;y"'])
})

test('cell makes values JSON-safe', () => {
  assert.equal(cell(10n), '10')
  assert.equal(cell(new Date(0)), '1970-01-01T00:00:00.000Z')
  assert.equal(cell(Buffer.from([1, 255])), '\\x01ff')
  assert.deepEqual(cell({ a: 1n }), { a: '1' })
})

test('sqlite engine and bridge, end to end', async () => {
  const file = path.join(tmp(), 'app.db')
  const db = await openSqlite(file)
  await db.query(`create table users (id integer primary key, name text not null); insert into users (name) values ('ada'), ('bob')`)
  assert.deepEqual(await db.tables(), [{ schema: 'main', name: 'users', kind: 'table', rows: 2 }])
  const cols = await db.columns('main', 'users')
  assert.deepEqual(cols.map((c) => [c.name, c.primaryKey, c.nullable]), [['id', true, true], ['name', false, false]])

  const origin = 'https://stroke.click'
  const bridge = await startBridge({ engine: db, name: 'proj', origins: [origin], token: 'secret-token' })
  const base = `http://127.0.0.1:${bridge.port}`
  /** @param {string} p @param {unknown} body @param {Record<string, string>} [headers] */
  const call = (p, body, headers = {}) =>
    fetch(base + p, { method: 'POST', headers: { origin, authorization: 'Bearer secret-token', 'content-type': 'application/json', ...headers }, body: JSON.stringify(body) })
  try {
    const rows = await (await call('/rows', { schema: 'main', table: 'users', limit: 1, offset: 1, sort: { column: 'name', dir: 'desc' } })).json()
    assert.deepEqual(rows.columns, ['id', 'name'])
    assert.deepEqual(rows.rows, [[1, 'ada']])
    assert.equal(rows.total, 2)

    const q = await (await call('/query', { sql: 'select count(*) as n from users' })).json()
    assert.deepEqual(q.rows, [[2]])
    const bad = await call('/query', { sql: 'select * from nope' })
    assert.equal(bad.status, 400)
    assert.match((await bad.json()).error, /no such table/)

    const info = await (await call('/info', {})).json()
    assert.equal(info.name, 'proj')

    assert.equal((await call('/tables', {}, { authorization: 'Bearer wrong-token!' })).status, 401)
    assert.equal((await call('/tables', {}, { origin: 'https://evil.test' })).status, 403)

    const pre = await fetch(base + '/tables', { method: 'OPTIONS', headers: { origin, 'access-control-request-private-network': 'true' } })
    assert.equal(pre.status, 204)
    assert.equal(pre.headers.get('access-control-allow-private-network'), 'true')
    assert.equal(pre.headers.get('access-control-allow-origin'), origin)
  } finally {
    await bridge.close()
    await db.close()
  }
})

test('bridge rejects a foreign Host (DNS rebinding)', async () => {
  const http = await import('node:http')
  const bridge = await startBridge({ engine: /** @type {any} */ ({}), name: 'x', token: 't' })
  try {
    const status = await new Promise((resolve, reject) => {
      const req = http.request({ host: '127.0.0.1', port: bridge.port, path: '/tables', method: 'POST', headers: { host: `evil.test:${bridge.port}`, origin: 'https://stroke.click' } }, (res) => {
        res.resume()
        resolve(res.statusCode)
      })
      req.on('error', reject)
      req.end('{}')
    })
    assert.equal(status, 421)
  } finally {
    await bridge.close()
  }
})

test('studioUrl keeps the token in the fragment', () => {
  const u = new URL(studioUrl({ studio: 'https://stroke.click/studio', port: 4321, token: 'abc' }))
  assert.equal(u.pathname, '/studio')
  assert.equal(u.search, '')
  const f = new URLSearchParams(u.hash.slice(1))
  assert.equal(f.get('bridge'), 'http://127.0.0.1:4321')
  assert.equal(f.get('token'), 'abc')
})

test('sqlite: structure, where, and edits by primary key', async () => {
  const db = await openSqlite(path.join(tmp(), 'edit.db'))
  try {
    await db.query(`create table teams (id integer primary key, name text);
      create table users (id integer primary key, email text not null default 'x', team integer references teams(id));
      create unique index users_email on users(email);
      insert into teams (name) values ('core');
      insert into users (email, team) values ('a@x.io', 1), ('b@x.io', 1), ('c@x.io', null)`)
    const t = await db.tables()
    assert.equal(t.find((x) => x.name === 'users')?.rows, 3)

    const s = await db.structure('main', 'users')
    assert.equal(s.columns.find((c) => c.name === 'email')?.default, "'x'")
    assert.ok(s.indexes.some((i) => i.name === 'users_email' && i.unique && i.columns[0] === 'email'))
    assert.deepEqual(s.foreignKeys.map((f) => [f.columns[0], f.refTable, f.refColumns[0]]), [['team', 'teams', 'id']])

    // A key that matches more than one row changes nothing.
    await assert.rejects(db.update({ schema: 'main', table: 'users', key: { team: 1 }, set: { team: null } }), /2 rows/)
    assert.equal((await db.query('select count(*) from users where team = 1')).rows[0][0], 2)

    const filtered = await db.rows({ schema: 'main', table: 'users', limit: 10, offset: 0, where: 'team is not null' })
    assert.equal(filtered.total, 2)

    assert.equal((await db.update({ schema: 'main', table: 'users', key: { id: 2 }, set: { email: 'bob@x.io', team: null } })).rowCount, 1)
    assert.equal((await db.insert({ schema: 'main', table: 'users', values: { email: 'd@x.io' } })).rowCount, 1)
    assert.equal((await db.remove({ schema: 'main', table: 'users', keys: [{ id: 1 }, { id: 3 }] })).rowCount, 2)
    const left = await db.query('select id, email, team from users order by id')
    assert.deepEqual(left.rows, [[2, 'bob@x.io', null], [4, 'd@x.io', null]])

    await assert.rejects(db.update({ schema: 'main', table: 'users', key: {}, set: { email: 'z' } }), /primary key/)
  } finally {
    await db.close()
  }
})

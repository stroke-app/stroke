import { describe, it, expect } from 'vitest'
import {
  parseWrite, lex, pgReadMeta, sqliteReadMeta, mysqlReadMeta, captureSql, sizeSql, insertReturningSql,
  revertUpdateSql, revertDeleteSql, revertInsertSql, runWithUndo, describeUndo, revertSummary,
} from './sql-undo.js'

describe('parseWrite', () => {
  it('reads a plain UPDATE', () => {
    expect(parseWrite('UPDATE users SET name = \'x\', "Email" = lower(email) WHERE id > 3;')).toMatchObject({
      kind: 'update', table: { text: 'users' }, alias: '', rowRef: 'users',
      setColumns: [{ name: 'name', quoted: false }, { name: 'Email', quoted: true }],
      tail: 'WHERE id > 3',
    })
  })

  it('keeps aliases, ONLY and qualified names', () => {
    expect(parseWrite('update only public."Users" as u set active = false where u.id = 1')).toMatchObject({
      kind: 'update', only: true, table: { text: 'public."Users"' }, alias: 'u', rowRef: 'u',
    })
    expect(parseWrite('DELETE FROM app.orders o WHERE o.total < 0')).toMatchObject({
      kind: 'delete', table: { text: 'app.orders' }, alias: 'o', tail: 'WHERE o.total < 0',
    })
  })

  it('reads multi-column SET targets', () => {
    expect(parseWrite('UPDATE t SET (a, "B") = (1, 2), c = 3')?.setColumns.map((c) => c.name)).toEqual(['a', 'B', 'c'])
  })

  it('does not end a clause inside strings, comments or parentheses', () => {
    const p = parseWrite("-- note\nUPDATE t SET v = 'WHERE x' /* WHERE */, w = (SELECT 1 WHERE true) WHERE id = 2")
    expect(p?.setColumns.map((c) => c.name)).toEqual(['v', 'w'])
    expect(p?.tail).toBe('WHERE id = 2')
  })

  it('counts INSERT rows', () => {
    expect(parseWrite("INSERT INTO t (a, b) VALUES (1, '(x)'), (2, 'y')")).toMatchObject({ kind: 'insert', insertRows: 2 })
    expect(parseWrite('INSERT INTO t DEFAULT VALUES')).toMatchObject({ kind: 'insert', insertRows: 1 })
    expect(parseWrite('INSERT INTO t (a) VALUES (1) ON CONFLICT DO NOTHING')).toMatchObject({ kind: 'insert', insertRows: 1 })
  })

  it('leaves alone what it cannot undo', () => {
    for (const sql of [
      'SELECT 1',
      'UPDATE t SET a = 1 FROM u WHERE u.id = t.id',
      'DELETE FROM t USING u WHERE u.id = t.id',
      'UPDATE t SET a = 1 RETURNING *',
      'DELETE FROM t WHERE CURRENT OF c',
      'INSERT INTO t SELECT * FROM u',
      'INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET a = 2',
      'INSERT INTO t (a) VALUES (1) RETURNING id',
      'INSERT OR REPLACE INTO t (a) VALUES (1)',
      'WITH x AS (SELECT 1) UPDATE t SET a = 1',
      'UPDATE t SET a = 1; DELETE FROM t',
    ]) expect(parseWrite(sql), sql).toBeNull()
  })

  it('lexes E strings and dollar quotes as single tokens', () => {
    expect(lex("E'a\\'b' $x$ ; $x$ \"q\"\"r\"").map((t) => t.t)).toEqual(['string', 'string', 'ident'])
  })
})

const pgMeta = /** @type {import('./sql-undo.js').TableMeta} */ (pgReadMeta([
  ['id', 'bigint', 'false', 'true', 'true', 'false', 'true', 'r', 'public.users'],
  ['name', 'text', 'false', 'false', 'false', 'false', 'true', 'r', 'public.users'],
  ['slug', 'text', 'true', 'false', 'false', 'false', 'true', 'r', 'public.users'],
]))

describe('Postgres SQL', () => {
  it('reads the catalog rows', () => {
    expect(pgMeta).toMatchObject({ qualified: 'public.users', triggers: false, cascades: true })
    expect(pgMeta.pk.map((c) => c.name)).toEqual(['id'])
    expect(pgReadMeta([['a', 'int', 'false', 'false', 'false', 'false', 'false', 'v', 'public.v']])).toBe('only tables can be reverted')
  })

  it('captures with the database quoting each value', () => {
    const plan = /** @type {import('./sql-undo.js').WritePlan} */ (parseWrite('UPDATE users u SET name = upper(name) WHERE u.id < 10'))
    expect(captureSql(plan, [pgMeta.columns[0], pgMeta.columns[1]], 'postgres'))
      .toBe('SELECT quote_nullable(u."id"), quote_nullable(u."name") FROM users AS u WHERE u.id < 10 FOR UPDATE')
    expect(sizeSql(plan, pgMeta, [pgMeta.columns[0]], 'postgres'))
      .toBe('SELECT count(*)::text, COALESCE(sum(COALESCE(pg_column_size(u."id"), 0)), 0)::text FROM users AS u WHERE u.id < 10')
  })

  it('returns inserted keys through a CTE', () => {
    const plan = /** @type {import('./sql-undo.js').WritePlan} */ (parseWrite("INSERT INTO users (name) VALUES ('a')"))
    expect(insertReturningSql(plan, pgMeta)).toBe("WITH __ins AS (\nINSERT INTO users (name) VALUES ('a')\nRETURNING quote_nullable(\"id\") AS k0\n) SELECT k0 FROM __ins")
  })

  it('reverts an UPDATE only where the written value is still there', () => {
    const [sql] = revertUpdateSql(pgMeta, [pgMeta.columns[1]], [{ key: ["'1'"], before: ["'old'"], after: ["'NEW'"] }], 'postgres')
    expect(sql).toContain('"name" = __v.o0::text')
    expect(sql).toContain('__t."id" = __v.k0::bigint')
    expect(sql).toContain('__t."name"::text IS NOT DISTINCT FROM __v.a0')
  })

  it('restores deleted rows, overriding an always-identity key', () => {
    const cols = pgMeta.columns.filter((c) => !c.generated)
    const [sql] = revertDeleteSql(pgMeta, cols, [["'1'", "'a'"]], 'postgres')
    expect(sql).toBe('INSERT INTO public.users ("id", "name") OVERRIDING SYSTEM VALUE VALUES\n  (\'1\', \'a\')\nON CONFLICT DO NOTHING;')
  })

  it('chunks long reverts', () => {
    const keys = Array.from({ length: 1201 }, (_, i) => [`'${i}'`])
    expect(revertInsertSql(pgMeta, keys, 'postgres')).toHaveLength(3)
  })
})

describe('SQLite SQL', () => {
  const plan = /** @type {import('./sql-undo.js').WritePlan} */ (parseWrite('UPDATE notes SET body = 1'))
  it('falls back to the rowid without a declared key', () => {
    const meta = /** @type {import('./sql-undo.js').TableMeta} */ (sqliteReadMeta([['body', 'TEXT', 0, 0]], [['table', 0]], plan))
    expect(meta.pk.map((c) => c.name)).toEqual(['rowid'])
    expect(captureSql(plan, [...meta.pk, meta.columns[0]], 'sqlite')).toBe('SELECT quote(notes.rowid), quote(notes."body") FROM notes')
    expect(revertUpdateSql(meta, [meta.columns[0]], [{ key: ['3'], before: ["'a'"], after: ['1'] }], 'sqlite')[0])
      .toBe('UPDATE notes SET "body" = \'a\' WHERE rowid = 3 AND "body" IS 1;')
  })
})

describe('runWithUndo', () => {
  /** A fake database: answers by the shape of each query. */
  function fakeIo(/** @type {Record<string, any>} */ over = {}) {
    /** @type {string[]} */
    const log = []
    const io = {
      log,
      inspect: async (/** @type {string} */ sql) => {
        log.push(`inspect ${sql.slice(0, 20)}`)
        if (sql.includes('pg_class')) return { rows: [['id', 'integer', 'false', 'false', 'true', 'false', 'false', 'r', 'public.t'], ['v', 'text', 'false', 'false', 'false', 'false', 'false', 'r', 'public.t']] }
        return { rows: [[over.count ?? '2', '10']] }
      },
      begin: async () => { log.push('begin') },
      exec: async (/** @type {string} */ sql) => {
        log.push(`exec ${sql.slice(0, 24)}`)
        if (sql.includes('FOR UPDATE')) return { rows: [["'1'", "'a'"], ["'2'", "'b'"]] }
        if (sql.startsWith('SELECT quote_nullable("id")')) return { rows: [["'1'", "'A'"], ["'2'", "'B'"]] }
        return { rows: [] }
      },
      run: async (/** @type {string} */ sql) => {
        log.push(`run ${sql}`)
        if (over.runFails) throw new Error('Query failed: boom')
        return { columns: [], rows: [], rowCount: over.affected ?? 2, message: '2 rows affected (not committed yet)' }
      },
      commit: async () => { log.push('commit') },
      rollback: async () => { log.push('rollback') },
    }
    return io
  }

  it('keeps an undo copy of an UPDATE', async () => {
    const io = fakeIo()
    const out = await runWithUndo(/** @type {any} */ (parseWrite('UPDATE t SET v = upper(v)')), 'postgres', io)
    expect('undo' in out && out.undo).toMatchObject({ kind: 'update', table: 'public.t', rows: 2, columns: ['v'] })
    expect('result' in out && out.result.message).toBeNull()
    expect(io.log.at(-1)).toBe('commit')
  })

  it('rolls back and asks for the ordinary run when the write fails', async () => {
    const io = fakeIo({ runFails: true })
    expect(await runWithUndo(/** @type {any} */ (parseWrite('UPDATE t SET v = 1')), 'postgres', io)).toEqual({ fallback: true, note: '' })
    expect(io.log.at(-1)).toBe('rollback')
  })

  it('keeps the write but no copy when the counts disagree', async () => {
    const out = await runWithUndo(/** @type {any} */ (parseWrite('UPDATE t SET v = 1')), 'postgres', fakeIo({ affected: 3 }))
    expect(out).toMatchObject({ undo: null, note: 'the rows it changed could not be read exactly' })
  })

  it('skips the copy for big writes and key changes', async () => {
    expect(await runWithUndo(/** @type {any} */ (parseWrite('DELETE FROM t')), 'postgres', fakeIo({ count: '20000' })))
      .toEqual({ fallback: true, note: 'it changes over 10,000 rows' })
    expect(await runWithUndo(/** @type {any} */ (parseWrite('UPDATE t SET id = 5')), 'postgres', fakeIo()))
      .toEqual({ fallback: true, note: 'it changes the primary key' })
  })

  it('lets Stop through', async () => {
    const io = fakeIo()
    io.inspect = async () => { throw new Error('Query cancelled') }
    await expect(runWithUndo(/** @type {any} */ (parseWrite('UPDATE t SET v = 1')), 'postgres', io)).rejects.toThrow('cancelled')
  })
})

describe('describing', () => {
  const u = /** @type {import('./sql-undo.js').UndoRecord} */ ({ id: 'x', dialect: 'postgres', kind: 'update', table: 'public.users', rows: 6, columns: ['name', 'email'], statements: [], warnings: [], sql: '', at: 0 })
  it('says what a revert does', () => {
    expect(describeUndo(u)).toMatchObject({ title: 'Revert UPDATE', body: 'Puts back the previous name and email in 6 rows of public.users.', action: 'Revert 6 rows', destructive: false })
    expect(revertSummary(u, 6)).toEqual({ ok: true, title: 'Reverted 6 rows in public.users', description: '' })
    expect(revertSummary(u, 4)).toMatchObject({ ok: false, title: 'Reverted 4 of 6 rows in public.users' })
  })
})

describe('schema changes', () => {
  it('reads the CREATEs it can undo', () => {
    expect(parseWrite('CREATE TABLE test (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, "column" text NOT NULL);'))
      .toMatchObject({ kind: 'ddl', ddl: 'create', objectType: 'TABLE', name: { text: 'test' } })
    expect(parseWrite('create unique index if not exists users_email on public.users (lower(email))'))
      .toMatchObject({ ddl: 'create', objectType: 'INDEX', name: { text: 'users_email' }, table: { text: 'public.users' } })
    expect(parseWrite('CREATE MATERIALIZED VIEW app.totals AS SELECT 1')).toMatchObject({ objectType: 'MATERIALIZED VIEW', name: { text: 'app.totals' } })
    expect(parseWrite('CREATE UNLOGGED TABLE scratch (a int)')).toMatchObject({ objectType: 'TABLE' })
  })

  it('reads single ALTER TABLE actions', () => {
    expect(parseWrite('ALTER TABLE users ADD COLUMN nickname text DEFAULT \'\'')).toMatchObject({ ddl: 'add-column', column: { name: 'nickname' }, table: { text: 'users' } })
    expect(parseWrite('alter table only app.items add "Qty2" int')).toMatchObject({ ddl: 'add-column', column: { name: 'Qty2', quoted: true } })
    expect(parseWrite('ALTER TABLE users RENAME TO members')).toMatchObject({ ddl: 'rename-table', to: { name: 'members' } })
    expect(parseWrite('ALTER TABLE users RENAME COLUMN name TO full_name')).toMatchObject({ ddl: 'rename-column', column: { name: 'name' }, to: { name: 'full_name' } })
  })

  it('leaves alone what it cannot undo', () => {
    for (const sql of [
      'CREATE OR REPLACE VIEW v AS SELECT 1',
      'CREATE TEMP TABLE t (a int)',
      'CREATE INDEX ON t (a)',
      'CREATE INDEX CONCURRENTLY i ON t (a)',
      'CREATE SCHEMA AUTHORIZATION bob',
      'CREATE FUNCTION f() RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql',
      'ALTER TABLE t ADD CONSTRAINT c CHECK (a > 0)',
      'ALTER TABLE t ADD PRIMARY KEY (a)',
      'ALTER TABLE t ADD a int, ADD b int',
      'ALTER TABLE t DROP COLUMN a',
      'ALTER TABLE t RENAME CONSTRAINT a TO b',
      'DROP TABLE t',
    ]) expect(parseWrite(sql), sql).toBeNull()
  })

  it('describes a schema revert in its own words', () => {
    const u = /** @type {import('./sql-undo.js').UndoRecord} */ ({ id: 'x', dialect: 'postgres', kind: 'ddl', table: 'public.test', rows: 0, columns: [], statements: ['DROP TABLE public.test;'], warnings: [], sql: '', at: 0,
      words: { title: 'Revert this CREATE TABLE?', body: 'Drops the table public.test it created.', note: 'n', done: 'Dropped table public.test', action: 'Drop table', destructive: true } })
    expect(describeUndo(u).title).toBe('Revert this CREATE TABLE?')
    expect(describeUndo(u).action).toBe('Drop table')
    expect(revertSummary(u, 0)).toEqual({ ok: true, title: 'Dropped table public.test', description: '' })
  })
})

describe('MySQL', () => {
  const meta = /** @type {import('./sql-undo.js').TableMeta} */ (mysqlReadMeta([
    ['id', 'bigint', 'bigint', 'PRI', 'auto_increment', 'BASE TABLE', 'InnoDB', 0, 1, '`t`.`w`'],
    ['s', 'varchar(20)', 'varchar', '', '', 'BASE TABLE', 'InnoDB', 0, 1, '`t`.`w`'],
    ['b', 'blob', 'blob', '', '', 'BASE TABLE', 'InnoDB', 0, 1, '`t`.`w`'],
    ['bt', 'bit(5)', 'bit', '', '', 'BASE TABLE', 'InnoDB', 0, 1, '`t`.`w`'],
  ]))

  it('reads the catalog and refuses tables that cannot roll back', () => {
    expect(meta).toMatchObject({ qualified: '`t`.`w`', cascades: true })
    expect(mysqlReadMeta([['id', 'int', 'int', 'PRI', '', 'BASE TABLE', 'MyISAM', 0, 0, '`t`.`m`']])).toBe('only InnoDB tables can be reverted, and this one is MyISAM')
  })

  it('captures values as hex the sql_mode cannot misread, and quotes names with backticks', () => {
    const plan = /** @type {import('./sql-undo.js').WritePlan} */ (parseWrite('UPDATE `w` SET s = 1, b = 2, bt = 3 WHERE id = 1'))
    const sql = captureSql(plan, meta.columns, 'mysql')
    expect(sql).toContain("IF(`w`.`id` IS NULL, 'NULL', CAST(`w`.`id` AS CHAR))")
    expect(sql).toContain("CONCAT('CONVERT(X''', HEX(CAST(`w`.`s` AS CHAR)), ''' USING utf8mb4)')")
    expect(sql).toContain("CONCAT('X''', HEX(`w`.`b`), '''')")
    expect(sql).toContain('CAST(`w`.`bt` + 0 AS CHAR)')
    expect(sql.endsWith('FOR UPDATE')).toBe(true)
  })

  it('reverts with backticks, a byte-exact guard and a no-op on taken keys', () => {
    const [upd] = revertUpdateSql(meta, [meta.columns[1], meta.columns[3]], [{ key: ['1'], before: ["CONVERT(X'61' USING utf8mb4)", '5'], after: ["CONVERT(X'62' USING utf8mb4)", '6'] }], 'mysql')
    expect(upd).toBe("UPDATE `t`.`w` SET `s` = CONVERT(X'61' USING utf8mb4), `bt` = 5 WHERE `id` = 1 AND HEX(CAST(`s` AS CHAR)) <=> HEX(CAST(CONVERT(X'62' USING utf8mb4) AS CHAR)) AND `bt` + 0 <=> 6;")
    const [del] = revertDeleteSql(meta, meta.columns, [['1', "CONVERT(X'61' USING utf8mb4)", "X''", '0']], 'mysql')
    expect(del).toBe("INSERT INTO `t`.`w` (`id`, `s`, `b`, `bt`) VALUES\n  (1, CONVERT(X'61' USING utf8mb4), X'', 0)\nON DUPLICATE KEY UPDATE `id` = `id`;")
  })
})

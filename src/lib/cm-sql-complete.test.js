import { describe, it, expect } from 'vitest'
import { EditorState, EditorSelection } from '@codemirror/state'
import { CompletionContext } from '@codemirror/autocomplete'
import { sql, PostgreSQL } from '@codemirror/lang-sql'
import { sqlCompletionSource, completionIsTypedOut } from './cm-sql-complete.js'
import { SQL_SNIPPETS } from './sql-complete-data.js'

/** @type {import('./sql-complete-data.js').SqlSchemaHints} */
const hints = {
  schemas: ['public', 'auth'],
  activeSchema: 'public',
  tables: ['users_table', 'posts', 'post_tags', 'Orders', 'order'],
  columnsByTable: {
    users_table: [{ name: 'id', type: 'int4' }, { name: 'name', type: 'text' }, 'createdAt'],
    'public.posts': ['id', 'title', 'author_id'],
    post_tags: ['post_id', 'tag'],
  },
  enumValues: { mood: ['happy', 'sad'] },
}
const source = sqlCompletionSource(() => hints, () => 'postgres')

/** Labels in the order the list shows them (the source filters and sorts). */
function complete(doc, { pos = doc.length, explicit = false } = {}) {
  const state = EditorState.create({ doc, extensions: [sql({ dialect: PostgreSQL })] })
  const r = source(new CompletionContext(state, pos, explicit))
  if (!r) return null
  return { from: r.from, labels: r.options.map((o) => o.label), options: r.options }
}

describe('sqlCompletionSource', () => {
  it('fetches the columns of a table the statement names, then suggests them', async () => {
    let live = { tables: ['playing_with_neon'], columnsByTable: {} }
    const calls = []
    live.loadColumns = async (tables) => {
      calls.push(tables)
      live = { ...live, columnsByTable: { playing_with_neon: [{ name: 'id', type: 'int4' }, { name: 'name', type: 'text' }] } }
    }
    const src = sqlCompletionSource(() => live, () => 'postgres')
    const doc = 'UPDATE playing_with_neon SET n'
    const state = EditorState.create({ doc, extensions: [sql({ dialect: PostgreSQL })] })
    const r = await src(new CompletionContext(state, doc.length, false))
    expect(calls).toEqual([['playing_with_neon']])
    expect(r?.options[0].label).toBe('name')
  })

  it('completes a selected snippet field as an empty slot', () => {
    const doc = 'UPDATE users_table SET column = value'
    const from = doc.indexOf('column')
    const state = EditorState.create({
      doc,
      selection: { anchor: from, head: from + 'column'.length },
      extensions: [sql({ dialect: PostgreSQL })],
    })
    const r = source(new CompletionContext(state, from + 'column'.length, true))
    expect(r?.from).toBe(from)
    expect(r?.to).toBe(from + 'column'.length)
    expect(r?.options.slice(0, 3).map((o) => o.label)).toEqual(['id', 'name', 'createdAt'])
  })

  it('stays closed on a selected snippet field until something is typed', () => {
    const doc = 'SELECT * FROM posts'
    const state = EditorState.create({ doc, selection: { anchor: 7, head: 8 }, extensions: [sql({ dialect: PostgreSQL })] })
    expect(source(new CompletionContext(state, 8, false))).toBeNull()
  })

  it('asks for a SELECT snippet\'s table before its columns', () => {
    for (const s of SQL_SNIPPETS.filter((s) => /^SELECT .*\$\{\d+:[^}]*\}.* FROM \$\{/.test(s.body))) {
      expect(s.body, s.name).toMatch(/ FROM \$\{1:/)
    }
  })

  it('suggests tables after FROM', () => {
    expect(complete('SELECT * FROM pos')?.labels.slice(0, 2)).toEqual(['posts', 'post_tags'])
  })

  it('lists columns as soon as a quote opens in SET, in table order', () => {
    const r = complete('UPDATE "public"."users_table"\nSET\n  "')
    expect(r?.labels.slice(0, 3)).toEqual(['id', 'name', 'createdAt'])
  })

  it('treats a blank line as the end of the query above', () => {
    const r = complete('SELECT * FROM users_table\n\nsel')
    expect(r?.labels[0]).toBe('SELECT')
    expect(r?.labels).toContain('SELECT … FROM')
    expect(r?.labels).not.toContain('users_table')
  })

  it('never matches scattered letters, but does match word starts', () => {
    expect(complete('SELECT sel FROM users_table', { pos: 10 })?.labels ?? []).not.toContain('users_table')
    const r = complete('SELECT at FROM users_table', { pos: 9 })
    expect(r?.labels).toContain('createdAt')
  })

  it('finds snippets by name and by their short alias', () => {
    expect(complete('ups')?.labels[0]).toBe('INSERT … ON CONFLICT (upsert)')
    expect(complete('ins')?.labels).toContain('INSERT INTO … VALUES')
  })

  it('ranks the referenced table\'s columns first', () => {
    const r = complete('SELECT i FROM posts', { pos: 8 })
    expect(r?.labels[0]).toBe('id')
    expect(r?.options.find((o) => o.label === 'id')?.detail).toBe('posts')
  })

  it('resolves an alias after a dot', () => {
    expect(complete('SELECT p. FROM posts p', { pos: 9 })?.labels).toEqual(['id', 'title', 'author_id'])
  })

  it('offers a schema\'s tables after "schema".', () => {
    expect(complete('SELECT * FROM public.')?.labels).toEqual(['users_table', 'posts', 'post_tags', 'Orders', 'order'])
  })

  it('puts UPDATE first at the start, with its snippets after it', () => {
    const labels = complete('Up')?.labels ?? []
    expect(labels[0]).toBe('UPDATE')
    expect(labels).toContain('UPDATE … SET … WHERE')
    expect(labels).not.toContain('UPPER')
  })

  it('puts SET first after the UPDATE target, WHERE after an assignment', () => {
    expect(complete('UPDATE "users_table" S')?.labels[0]).toBe('SET')
    expect(complete(`UPDATE "users_table" SET "name" = 'x' W`)?.labels[0]).toBe('WHERE')
  })

  it('keeps columns first right after SELECT, FROM first after a column', () => {
    expect(complete('SELECT * FROM posts; SELECT t')?.labels[0]).not.toBe('FROM')
    expect(complete('SELECT title f')?.labels[0]).toBe('FROM')
  })

  it('offers functions with their signature', () => {
    const count = complete('SELECT cou')?.options.find((o) => o.label === 'count')
    expect(count?.detail).toBe('count(*)')
  })

  it('offers nothing inside a string or a comment', () => {
    expect(complete("SELECT * FROM posts WHERE title = 'po")).toBeNull()
    expect(complete('-- FROM pos')).toBeNull()
  })

  it('stays closed on whitespace until asked', () => {
    expect(complete('SELECT * FROM ')).toBeNull()
    expect(complete('SELECT * FROM ', { explicit: true })?.labels[0]).toBe('users_table')
  })
})

/** The same, on another engine. */
function completeOn(dialect, doc, { pos = doc.length, explicit = false } = {}) {
  const src = sqlCompletionSource(() => hints, () => dialect)
  const state = EditorState.create({ doc, selection: { anchor: pos }, extensions: [sql({ dialect: PostgreSQL })] })
  const r = src(new CompletionContext(state, pos, explicit))
  if (!r) return null
  return { from: r.from, labels: r.options.map((o) => o.label), options: r.options, state }
}

/** Run a name option's apply against a stand-in view; returns the new text. */
function accept(r, label) {
  const c = r.options.find((o) => o.label === label)
  let state = r.state
  const view = { get state() { return state }, dispatch: (/** @type {any} */ tr) => { state = state.update(tr).state } }
  c.apply(view, c, r.from, r.state.doc.length)
  return state.doc.toString()
}

describe('data types', () => {
  it('suggests types in a column definition, ALTER ... TYPE, CAST and ::', () => {
    for (const doc of [
      'CREATE TABLE t (id varc',
      'ALTER TABLE users_table ALTER COLUMN name TYPE varc',
      'ALTER TABLE users_table ADD COLUMN nick varc',
      'SELECT CAST(name AS varc',
      'SELECT name::varc',
    ]) {
      expect(complete(doc)?.labels[0], doc).toBe('varchar')
    }
  })

  it('writes the length as a field to type over', () => {
    const v = complete('CREATE TABLE t (id varc')?.options.find((o) => o.label === 'varchar')
    expect(v?.detail).toBe('varchar(255)')
    expect(typeof v?.apply).toBe('function')
  })

  it('opens the type list right after ::, and ranks everyday types first', () => {
    const r = complete('SELECT name::')
    expect(r?.labels.slice(0, 3)).toEqual(['text', 'varchar', 'integer'])
    expect(r?.labels).not.toContain('id')
  })

  it('writes the type in capitals when capitals are typed', () => {
    const r = complete('CREATE TABLE t (id VARC')
    expect(r?.labels[0]).toBe('VARCHAR')
    expect(r?.options[0].detail).toBe('VARCHAR(255)')
  })

  it('offers the engine\'s own types', () => {
    expect(completeOn('mysql', 'CREATE TABLE t (id tiny')?.labels).toEqual(expect.arrayContaining(['tinyint', 'tinytext', 'tinyblob']))
    expect(completeOn('mysql', 'CREATE TABLE t (id time')?.labels).not.toContain('timestamptz')
    expect(completeOn('mssql', 'ALTER TABLE t ADD c nvar')?.labels).toEqual(['nvarchar', 'nvarchar(max)'])
    expect(completeOn('sqlite', 'CREATE TABLE t (id ', { explicit: true })?.labels[0]).toBe('integer')
    expect(completeOn('duckdb', 'CREATE TABLE t (id huge')?.labels[0]).toBe('hugeint')
  })

  it('keeps ClickHouse type case whatever is typed', () => {
    expect(completeOn('clickhouse', 'CREATE TABLE t (id uint6')?.labels[0]).toBe('UInt64')
    expect(completeOn('clickhouse', 'CREATE TABLE t (id DATETIME')?.labels).toContain('DateTime64')
  })

  it('offers the schema\'s enum types on Postgres', () => {
    const mood = complete('ALTER TABLE users_table ADD COLUMN feeling moo')?.options.find((o) => o.label === 'mood')
    expect(mood?.detail).toBe('enum')
  })
})

describe('DDL and engines', () => {
  it('offers ALTER TABLE\'s actions and a column\'s constraints, not table names', () => {
    expect(complete('ALTER TABLE users_table AD')?.labels[0]).toBe('ADD')
    expect(complete('ALTER TABLE users_table ADD CO')?.labels[0]).toBe('COLUMN')
    expect(complete('CREATE TABLE t (id int NO')?.labels[0]).toBe('NOT')
    expect(complete('CREATE TABLE t (id int ', { explicit: true })?.labels).not.toContain('users_table')
  })

  it('offers tables, not columns, after TRUNCATE', () => {
    expect(complete('TRUNCATE pos')?.labels[0]).toBe('posts')
  })

  it('offers COALESCE once, as the function', () => {
    const labels = complete('SELECT coa')?.labels ?? []
    expect(labels.filter((l) => l.toLowerCase() === 'coalesce')).toEqual(['coalesce'])
  })

  it('matches keywords from their start only', () => {
    expect(complete('SELECT * FROM users_table WHERE na')?.labels).toEqual(['name'])
  })

  it('offers each engine only the functions and snippets it runs', () => {
    expect(complete('SELECT jsonb_s')?.labels).toContain('jsonb_set')
    expect(completeOn('mysql', 'SELECT jsonb_s')).toBeNull()
    expect(completeOn('mysql', 'SELECT group_c')?.labels).toContain('group_concat')
    expect(completeOn('mssql', 'SELECT getd')?.labels).toContain('getdate')
    // One CREATE TABLE per engine, written for it.
    for (const d of ['postgres', 'mysql', 'sqlite', 'mssql', 'clickhouse', 'duckdb']) {
      expect(completeOn(d, 'ct')?.labels.filter((l) => l === 'CREATE TABLE'), d).toEqual(['CREATE TABLE'])
    }
    const mysqlUpsert = completeOn('mysql', 'ups')?.labels ?? []
    expect(mysqlUpsert).toContain('INSERT … ON DUPLICATE KEY (upsert)')
    expect(mysqlUpsert).not.toContain('INSERT … ON CONFLICT (upsert)')
    expect(completeOn('mssql', 'sell')?.labels).toContain('SELECT TOP …')
    expect(completeOn('duckdb', 'expl')?.labels ?? []).not.toContain('EXPLAIN ANALYZE')
  })

  it('quotes a mixed-case name only where the engine folds case', () => {
    expect(accept(completeOn('postgres', 'SELECT * FROM ord'), 'Orders')).toBe('SELECT * FROM "Orders"')
    expect(accept(completeOn('mssql', 'SELECT * FROM ord'), 'Orders')).toBe('SELECT * FROM Orders')
    expect(accept(completeOn('mysql', 'SELECT * FROM ord'), 'Orders')).toBe('SELECT * FROM Orders')
    expect(accept(completeOn('mysql', 'SELECT * FROM ord'), 'order')).toBe('SELECT * FROM `order`')
  })
})

describe('completionIsTypedOut', () => {
  it('lets Enter break the line on a word already typed out', () => {
    const r = completeOn('postgres', 'SELECT * FROM users_table')
    expect(completionIsTypedOut(r.state, r.options.find((o) => o.label === 'users_table'))).toBe(true)
    const kw = completeOn('postgres', 'select * from users_table wher')
    expect(completionIsTypedOut(kw.state, { label: 'WHERE', type: 'keyword' })).toBe(false)
    const typed = completeOn('postgres', 'select * from users_table where')
    expect(completionIsTypedOut(typed.state, { label: 'WHERE', type: 'keyword' })).toBe(true)
  })

  it('still accepts where taking it writes something', () => {
    const quoted = completeOn('postgres', 'SELECT * FROM Orders')
    expect(completionIsTypedOut(quoted.state, quoted.options.find((o) => o.label === 'Orders'))).toBe(false)
    const fn = completeOn('postgres', 'SELECT count')
    expect(completionIsTypedOut(fn.state, fn.options.find((o) => o.label === 'count'))).toBe(false)
    const schema = completeOn('postgres', 'SELECT * FROM public', { explicit: true })
    expect(completionIsTypedOut(schema.state, schema.options.find((o) => o.label === 'public'))).toBe(false)
  })
})

describe('completion in a trigger', () => {
  const triggerHints = {
    activeSchema: 'main',
    tables: ['user', 'account'],
    columnsByTable: { user: ['id', 'email', 'updated_at'], account: ['id', 'user_id'] },
  }
  const src = sqlCompletionSource(() => triggerHints, () => 'sqlite')

  it("offers the trigger table's columns after NEW.", () => {
    const doc = 'CREATE TRIGGER t AFTER UPDATE ON user FOR EACH ROW BEGIN\n  UPDATE account SET user_id = NEW.'
    const state = EditorState.create({ doc, extensions: [sql()] })
    const r = src(new CompletionContext(state, doc.length, false))
    expect(r?.options.map((o) => o.label)).toEqual(['id', 'email', 'updated_at'])
  })

  it("offers a Postgres trigger function's table columns after NEW.", () => {
    const pgHints = { activeSchema: 'public', tables: ['users'], columnsByTable: { users: ['id', 'updated_at'] } }
    const pgSrc = sqlCompletionSource(() => pgHints, () => 'postgres')
    const fn = 'CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$\nBEGIN\n  NEW.'
    const doc = `${fn}\nEND;\n$$;\n\nCREATE TRIGGER trg BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION set_updated_at();`
    const state = EditorState.create({ doc, extensions: [sql({ dialect: PostgreSQL })] })
    const r = pgSrc(new CompletionContext(state, fn.length, false))
    expect(r?.options.map((o) => o.label)).toEqual(['id', 'updated_at'])
  })

  it('writes a picked table into every copy of a linked template field', () => {
    const doc = 'CREATE TRIGGER t AFTER UPDATE ON table_name FOR EACH ROW BEGIN\n  UPDATE table_name SET x = 1;\nEND;'
    const a = doc.indexOf('table_name')
    const b = doc.indexOf('table_name', a + 1)
    let state = EditorState.create({
      doc,
      selection: EditorSelection.create([EditorSelection.range(a, a + 10), EditorSelection.range(b, b + 10)], 0),
      extensions: [sql(), EditorState.allowMultipleSelections.of(true)],
    })
    const r = src(new CompletionContext(state, a + 10, true))
    const pick = r?.options.find((o) => o.label === 'user')
    const view = { get state() { return state }, dispatch: (/** @type {any} */ spec) => { state = state.update(spec).state } }
    const apply = /** @type {any} */ (pick?.apply)
    apply(view, pick, r?.from, r?.to)
    expect(state.doc.toString()).toBe('CREATE TRIGGER t AFTER UPDATE ON user FOR EACH ROW BEGIN\n  UPDATE user SET x = 1;\nEND;')
    expect(state.selection.ranges.length).toBe(2)
  })
})

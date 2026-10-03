import { describe, it, expect } from 'vitest'
import { EditorState } from '@codemirror/state'
import { CompletionContext } from '@codemirror/autocomplete'
import { sql, PostgreSQL } from '@codemirror/lang-sql'
import { sqlCompletionSource } from './cm-sql-complete.js'
import { SQL_SNIPPETS } from './sql-complete-data.js'

/** @type {import('./sql-complete-data.js').SqlSchemaHints} */
const hints = {
  schemas: ['public', 'auth'],
  activeSchema: 'public',
  tables: ['users_table', 'posts', 'post_tags'],
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
    expect(complete('SELECT * FROM public.')?.labels).toEqual(['users_table', 'posts', 'post_tags'])
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

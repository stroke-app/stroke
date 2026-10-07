import { describe, it, expect } from 'vitest'
import { suggest, keystrokesFor } from './terminal-complete.js'

const hints = {
  tables: ['users', 'user_sessions', 'messages', 'Orders'],
  columnsByTable: {
    users: [{ name: 'id', type: 'uuid' }, { name: 'email', type: 'text' }, { name: 'created_at', type: 'timestamptz' }],
    'public.messages': ['id', 'body', 'user_id'],
  },
}
const labels = (r) => r.items.map((i) => i.label)

describe('suggest', () => {
  it('translates a MySQL command typed into psql', () => {
    const r = suggest({ client: 'psql', line: 'show databases' })
    expect(r.items[0]).toMatchObject({ insert: '\\l', kind: 'translate', replaceLine: true, run: true })
    expect(suggest({ client: 'psql', line: 'describe users;' }).items[0].insert).toBe('\\d users')
    expect(suggest({ client: 'psql', line: 'use chatbot-dev' }).items[0].insert).toBe('\\c chatbot-dev')
  })

  it('translates psql commands typed into mysql, where \\d means delimiter', () => {
    expect(suggest({ client: 'mariadb', line: '\\dt' }).items[0].insert).toBe('SHOW TABLES;')
    expect(suggest({ client: 'mysql', line: '\\d users' }).items[0].insert).toBe('DESCRIBE users;')
    expect(suggest({ client: 'sqlite3', line: 'show tables' }).items[0].insert).toBe('.tables')
  })

  it('does not translate inside a statement that is still open', () => {
    expect(suggest({ client: 'psql', line: 'show tables', statement: 'select 1' }).items.every((i) => i.kind !== 'translate')).toBe(true)
  })

  it('lists psql meta-commands with what they do, and tables after \\d', () => {
    const r = suggest({ client: 'psql', line: '\\d' })
    expect(r.token).toBe('\\d')
    expect(r.items.find((i) => i.label === '\\dt')?.detail).toBe('List tables')
    expect(labels(suggest({ client: 'psql', line: '\\d us', hints }))).toEqual(['users', 'user_sessions'])
  })

  it('offers tables after FROM and a table\'s columns after its alias', () => {
    expect(labels(suggest({ client: 'psql', line: 'select * from me', hints }))).toEqual(['messages'])
    const r = suggest({ client: 'psql', line: 'select u.e', statement: 'select * from users u where', hints })
    expect(r).toMatchObject({ token: 'e' })
    expect(labels(r)).toEqual(['email'])
  })

  it('offers the referenced tables\' columns, then keywords in the typed case', () => {
    const r = suggest({ client: 'psql', line: 'select * from users where cr', hints })
    expect(r.items[0]).toMatchObject({ label: 'created_at', kind: 'column' })
    expect(labels(suggest({ client: 'psql', line: 'sel' }))).toContain('select')
    expect(labels(suggest({ client: 'psql', line: 'SEL' }))).toContain('SELECT')
  })

  it('offers only statement openers for the first word, and nothing for one letter', () => {
    expect(labels(suggest({ client: 'psql', line: 'ins' }))).toEqual(['insert into'])
    expect(labels(suggest({ client: 'psql', line: 'de' }))).toEqual(['delete from'])
    expect(suggest({ client: 'psql', line: 'l' }).items).toEqual([])
    expect(suggest({ client: 'psql', line: 'select l' }).items).toEqual([])
  })

  it('gives a line that is not SQL no SQL suggestions', () => {
    expect(suggest({ client: 'psql', line: 'le', statement: 'ls' }).items).toEqual([])
  })

  it('puts an exact match first so Enter runs it, and hides a list of only exact matches', () => {
    const r = suggest({ client: 'psql', line: '\\dt' })
    expect(r.items[0]).toMatchObject({ label: '\\dt', exact: true })
    expect(r.items[1].label).toBe('\\dt+')
    expect(suggest({ client: 'psql', line: '\\conninfo' }).items).toEqual([])
  })

  it('maps shell habits', () => {
    expect(suggest({ client: 'psql', line: 'ls' }).items[0].insert).toBe('\\dt')
    expect(suggest({ client: 'psql', line: 'cd chatbot-dev' }).items[0].insert).toBe('\\c chatbot-dev')
    expect(keystrokesFor(suggest({ client: 'psql', line: 'clear' }).items[0], '')).toBe('\x05\x15\x0c')
  })

  it('clears the Windows console by running cls, where the client can', () => {
    const r = suggest({ client: 'psql', line: 'cls', editor: 'console' })
    expect(r.items[0]).toMatchObject({ label: '\\! cls', run: true })
    expect(keystrokesFor(r.items[0], '', 'console')).toBe('\x1b[F\x1b[1;5H\\! cls\r')
    expect(suggest({ client: 'mysql', line: 'clear', editor: 'console' }).items).toEqual([])
  })

  it('quotes names that need it', () => {
    expect(suggest({ client: 'psql', line: 'select * from Or', hints }).items[0].insert).toBe('"Orders"')
    expect(suggest({ client: 'mysql', line: 'select * from Or', hints }).items[0].insert).toBe('`Orders`')
  })

  it('stays quiet inside strings, on numbers and once the word is complete', () => {
    expect(suggest({ client: 'psql', line: "select 'us", hints }).items).toEqual([])
    expect(suggest({ client: 'psql', line: 'select 12', hints }).items).toEqual([])
    expect(suggest({ client: 'psql', line: 'select * from users', hints }).items.map((i) => i.label)).not.toContain('users')
  })

  it('completes redis-cli commands on the first word only', () => {
    expect(labels(suggest({ client: 'redis-cli', line: 'hg' }))).toEqual(['HGETALL', 'HGET'])
    expect(suggest({ client: 'redis-cli', line: 'GET us' }).items).toEqual([])
  })
})

describe('keystrokesFor', () => {
  it('types the rest of a word, retypes a word whose case changed, replaces a line', () => {
    expect(keystrokesFor({ label: 'users', insert: 'users', kind: 'table' }, 'us')).toBe('ers')
    expect(keystrokesFor({ label: 'SELECT', insert: 'SELECT', kind: 'keyword' }, 'sel')).toBe('\x7f\x7f\x7fSELECT')
    expect(keystrokesFor({ label: '\\l', insert: '\\l', kind: 'translate', replaceLine: true, run: true }, '')).toBe('\x05\x15\\l\r')
    expect(keystrokesFor({ label: '\\l', insert: '\\l', kind: 'translate', replaceLine: true, run: true }, '', 'console')).toBe('\x1b[F\x1b[1;5H\\l\r')
  })
})

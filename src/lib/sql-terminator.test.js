import { describe, expect, it } from 'vitest'
import { isStatementSnippet, snippetEndsStatement, wantsTerminator } from './sql-terminator.js'

describe('wantsTerminator', () => {
  it('offers the ; once a statement reads finished', () => {
    for (const sql of [
      'SELECT * FROM users',
      'SELECT id, email FROM users WHERE id = 1',
      "SELECT * FROM users WHERE name = 'ada'",
      'SELECT count(*) FROM orders',
      'SELECT 1',
      'UPDATE tickets SET status = 2 WHERE id IN (SELECT id FROM stale)',
      'DELETE FROM sessions WHERE expires_at < now()',
      'INSERT INTO t (a, b) VALUES (1, 2)',
      'SELECT * FROM users ORDER BY created_at DESC LIMIT 10',
      'CREATE TABLE t (\n  id int PRIMARY KEY\n)',
      "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; $$ LANGUAGE sql",
      'COMMIT',
      'begin',
      'VACUUM',
      'SELECT * FROM users -- the admins',
      'SELECT *\nFROM users\nWHERE id = 1',
    ]) expect(wantsTerminator(sql), sql).toBe(true)
  })

  it('stays out of a statement that is still being written', () => {
    for (const sql of [
      '',
      'SELECT',
      'SELECT *',
      'SELECT * FROM',
      'SELECT * FROM users WHERE',
      'SELECT * FROM users WHERE id =',
      'SELECT a, b,',
      'SELECT * FROM users u JOIN',
      'SELECT * FROM users u JOIN orders o ON',
      'SELECT * FROM users WHERE id = 1 AND',
      'SELECT * FROM users ORDER BY',
      'UPDATE t SET',
      'INSERT INTO t (a, b',
      "SELECT 'unterminated",
      'SELECT * FROM t /* open comment',
      'SELECT * FROM users;',
      'CREATE FUNCTION f() AS $$ SELECT 1',
      'DROP TABLE IF EXISTS',
      'users',
      'WHERE id = 1',
    ]) expect(wantsTerminator(sql), JSON.stringify(sql)).toBe(false)
  })
})

describe('statement snippets', () => {
  it('tells a statement from a clause', () => {
    expect(isStatementSnippet('SELECT ${2:*} FROM ${1:table}')).toBe(true)
    expect(isStatementSnippet('INSERT INTO ${1:table} (${2:columns}) VALUES (${3:values})')).toBe(true)
    expect(isStatementSnippet('CREATE TABLE ${1:name} (\n  id int\n)')).toBe(true)
    expect(isStatementSnippet("PRAGMA table_info('${1:table}')")).toBe(true)
    expect(isStatementSnippet('JOIN ${1:table} ${2:t} ON ${2:t}.${3:id} = ${4:other}.${5:id}')).toBe(false)
    expect(isStatementSnippet('ORDER BY ${1:column} ${2:DESC}')).toBe(false)
    expect(isStatementSnippet('CASE WHEN ${1:condition} THEN ${2:result} ELSE ${3:default} END')).toBe(false)
    // Already terminated: the transaction snippet ends with COMMIT;
    expect(isStatementSnippet('BEGIN;\n\n${1}\n\nCOMMIT;')).toBe(false)
  })

  it('closes the statement only when nothing follows on the line', () => {
    expect(snippetEndsStatement('')).toBe(true)
    expect(snippetEndsStatement('   ')).toBe(true)
    expect(snippetEndsStatement(')')).toBe(false)
    expect(snippetEndsStatement(' WHERE id = 1')).toBe(false)
  })
})

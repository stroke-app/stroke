import { describe, expect, it } from 'vitest'
import { applyAutoLimit, topLevelWords, wantsLimit } from './sql-auto-limit.js'

describe('topLevelWords', () => {
  it('reads words outside parentheses, strings and comments only', () => {
    expect(topLevelWords("SELECT a FROM (SELECT 1 LIMIT 2) s WHERE b = 'limit' -- limit\n/* LIMIT */")).toEqual([
      'SELECT', 'A', 'FROM', 'S', 'WHERE', 'B',
    ])
    expect(topLevelWords('SELECT $$ LIMIT $$, "limit", `limit`, [limit]')).toEqual(['SELECT'])
  })
})

describe('wantsLimit', () => {
  it('takes plain queries', () => {
    expect(wantsLimit('SELECT * FROM users')).toBe(true)
    expect(wantsLimit('select id from users where x in (select y from z limit 3);')).toBe(true)
    expect(wantsLimit('WITH a AS (SELECT 1) SELECT * FROM a')).toBe(true)
    expect(wantsLimit('WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM t) SELECT n FROM t')).toBe(true)
    expect(wantsLimit('SELECT 1 UNION SELECT 2')).toBe(true)
  })

  it('leaves statements that already limit', () => {
    expect(wantsLimit('SELECT * FROM t LIMIT 10')).toBe(false)
    expect(wantsLimit('SELECT * FROM t OFFSET 5')).toBe(false)
    expect(wantsLimit('SELECT * FROM t FETCH FIRST 5 ROWS ONLY')).toBe(false)
    expect(wantsLimit('SELECT TOP 5 * FROM t')).toBe(false)
    expect(wantsLimit('SELECT * FROM t LIMIT 5 BY user_id')).toBe(false)
  })

  it('leaves statements that write, lock or end in a clause LIMIT must precede', () => {
    expect(wantsLimit('SELECT * INTO backup FROM t')).toBe(false)
    expect(wantsLimit('WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x')).toBe(false)
    expect(wantsLimit('WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x')).toBe(true)
    expect(wantsLimit('SELECT * FROM t FOR UPDATE')).toBe(false)
    expect(wantsLimit('SELECT * FROM t LOCK IN SHARE MODE')).toBe(false)
    expect(wantsLimit('SELECT * FROM t FORMAT JSON')).toBe(false)
    expect(wantsLimit('SELECT * FROM t SETTINGS max_threads = 2')).toBe(false)
  })

  it('leaves everything that is not a query', () => {
    for (const sql of ['INSERT INTO t VALUES (1)', 'UPDATE t SET a = 1', 'EXPLAIN SELECT 1', 'SHOW TABLES', 'FROM t SELECT a', '(SELECT 1)', '']) {
      expect(wantsLimit(sql), sql).toBe(false)
    }
  })

  it('does not count a FROM ... FOR inside a function call', () => {
    expect(wantsLimit('SELECT substring(name FROM 1 FOR 3) FROM users')).toBe(true)
  })
})

describe('applyAutoLimit', () => {
  it('adds the limit on its own line, before the semicolon', () => {
    expect(applyAutoLimit('SELECT * FROM users;', 100, 'postgres')).toEqual({ sql: 'SELECT * FROM users\nLIMIT 100;', changed: true })
    expect(applyAutoLimit('SELECT * FROM users -- all of them', 50, 'mysql').sql).toBe('SELECT * FROM users -- all of them\nLIMIT 50')
  })

  it('changes only the statements that want it and keeps the text between them', () => {
    const sql = '-- report\nSELECT * FROM a;\n\nUPDATE b SET x = 1;\nSELECT * FROM c LIMIT 3;\nSELECT 1'
    expect(applyAutoLimit(sql, 10, 'sqlite').sql).toBe(
      '-- report\nSELECT * FROM a\nLIMIT 10;\n\nUPDATE b SET x = 1;\nSELECT * FROM c LIMIT 3;\nSELECT 1\nLIMIT 10',
    )
  })

  it('is off at 0 and on SQL Server', () => {
    expect(applyAutoLimit('SELECT 1', 0, 'postgres')).toEqual({ sql: 'SELECT 1', changed: false })
    expect(applyAutoLimit('SELECT 1', 100, 'mssql')).toEqual({ sql: 'SELECT 1', changed: false })
  })
})

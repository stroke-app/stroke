import { describe, it, expect } from 'vitest'
import { lintSql, splitSqlStatements } from './sql-statements.js'

const messages = (sql) => lintSql(sql).map((d) => d.message)

describe('lintSql', () => {
  it('does not flag a single query without a semicolon', () => {
    expect(lintSql('SELECT * FROM users_table')).toEqual([])
  })

  it('flags two queries separated only by a blank line', () => {
    const sql = 'SELECT * FROM users_table\n\nSELECT * FROM posts'
    const d = lintSql(sql)
    expect(d).toHaveLength(1)
    expect(sql.slice(d[0].start, d[0].end)).toBe('SELECT')
    expect(d[0].start).toBe(sql.lastIndexOf('SELECT'))
  })

  it('flags a statement left without its ; when the buffer holds several', () => {
    const sql = 'SELECT * FROM ai_settings;\n\nSELECT * FROM ats_stats where'
    const d = lintSql(sql)
    expect(d.map((x) => x.message)).toEqual(["Missing ';' at the end of this statement"])
    expect(sql.slice(d[0].start, d[0].end)).toBe('where')
    expect(lintSql('SELECT 1;\n\nSELECT 2;')).toEqual([])
  })

  it('does not flag a terminated statement, a CTE, a subquery or a UNION arm', () => {
    expect(lintSql('SELECT 1;\n\nSELECT 2;')).toEqual([])
    expect(lintSql('WITH x AS (\n  SELECT 1\n)\n\nSELECT * FROM x')).toEqual([])
    expect(lintSql('SELECT * FROM t WHERE id IN (\n\n  SELECT id FROM u\n)')).toEqual([])
    expect(lintSql('SELECT 1\nUNION ALL\n\nSELECT 2')).toEqual([])
  })

  it('still reports unterminated strings', () => {
    expect(messages("SELECT 'abc")[0]).toMatch(/Unterminated string/)
  })
})

describe('splitSqlStatements: routine and trigger bodies', () => {
  const texts = (sql) => splitSqlStatements(sql).map((st) => st.text)

  it('keeps an SQLite trigger body in one statement', () => {
    const sql = 'CREATE TRIGGER t AFTER INSERT ON a\nBEGIN\n  UPDATE b SET n = n + 1;\n  INSERT INTO c VALUES (1);\nEND;\nSELECT 1;'
    const parts = texts(sql)
    expect(parts).toHaveLength(2)
    expect(parts[0].endsWith('END;')).toBe(true)
    expect(parts[1]).toBe('SELECT 1;')
  })

  it('counts nested blocks, CASE and END IF / END LOOP in a MySQL procedure', () => {
    const body = [
      'CREATE DEFINER=`root`@`%` PROCEDURE p(IN x INT)',
      'BEGIN',
      '  DECLARE i INT DEFAULT 0;',
      '  IF x > 0 THEN SET i = 1; END IF;',
      '  l: LOOP SET i = i + 1; IF i > 3 THEN LEAVE l; END IF; END LOOP l;',
      '  CASE x WHEN 1 THEN SELECT 1; ELSE SELECT 2; END CASE;',
      "  SELECT CASE WHEN i > 2 THEN 'a' ELSE 'b' END;",
      '  BEGIN SELECT i; END;',
      'END;',
    ].join('\n')
    const parts = texts(`${body}\nCALL p(1);`)
    expect(parts).toHaveLength(2)
    expect(parts[1]).toBe('CALL p(1);')
  })

  it('treats BEGIN TRAN in a T-SQL body as a statement, not a block', () => {
    expect(texts('CREATE OR ALTER PROCEDURE dbo.p AS BEGIN BEGIN TRAN; UPDATE t SET a = 1; COMMIT; END; SELECT 1;')).toHaveLength(2)
  })

  it('still splits transactions, CASE expressions and tables named event', () => {
    expect(texts('BEGIN; UPDATE t SET a = CASE WHEN b THEN 1 ELSE 2 END; COMMIT;')).toEqual([
      'BEGIN;',
      'UPDATE t SET a = CASE WHEN b THEN 1 ELSE 2 END;',
      'COMMIT;',
    ])
    expect(texts('CREATE TABLE event (id int); CREATE TABLE b (x int);')).toHaveLength(2)
    expect(texts('CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $$ BEGIN RETURN 1; END $$; SELECT f();')).toHaveLength(2)
  })

  it('does not warn about a missing ; between the statements of a body', () => {
    const sql = 'CREATE TRIGGER t AFTER INSERT ON a\nBEGIN\n  UPDATE b SET n = 1;\n\n  DELETE FROM c;\nEND;\n\nSELECT 1;'
    expect(messages(sql)).toEqual([])
  })
})

describe('lintSql fixes', () => {
  /** Apply the fix of the first diagnostic matching `message`. */
  const fixed = (text, message) => {
    const d = lintSql(text).find((x) => x.message.startsWith(message))
    expect(d?.fix, `${message} in ${JSON.stringify(text)}`).toBeDefined()
    const f = /** @type {import('./sql-statements.js').SqlFix} */ (d?.fix)
    return text.slice(0, f.from) + f.insert + text.slice(f.to)
  }

  it('adds the missing ; at the end of a statement', () => {
    expect(fixed('SELECT 1;\n\nSELECT * FROM bot_appearance', "Missing ';' at the end")).toBe('SELECT 1;\n\nSELECT * FROM bot_appearance;')
  })
  it('adds the ; between two statements after the last word of the first', () => {
    expect(fixed('SELECT * FROM a -- note\n\nSELECT 2;', "Missing ';' - this starts")).toBe('SELECT * FROM a; -- note\n\nSELECT 2;')
  })
  it('closes a string at the end of its line', () => {
    expect(fixed("SELECT 'abc\nFROM t;", 'Unterminated string')).toBe("SELECT 'abc'\nFROM t;")
  })
  it('closes a comment and a dollar quote at the end', () => {
    expect(fixed('SELECT 1 /* note', 'Unclosed block comment')).toBe('SELECT 1 /* note */')
    expect(fixed('CREATE FUNCTION f() AS $$ SELECT 1', 'Unterminated dollar-quoted')).toBe('CREATE FUNCTION f() AS $$ SELECT 1$$')
  })
  it('puts the missing ) at the end of its statement, before the ;', () => {
    expect(fixed('SELECT count(* FROM t;', 'Unclosed parenthesis')).toBe('SELECT count(* FROM t);')
    expect(fixed('SELECT (1 + 2', 'Unclosed parenthesis')).toBe('SELECT (1 + 2)')
  })
  it('removes a stray )', () => {
    expect(fixed('SELECT 1);', 'Unmatched closing')).toBe('SELECT 1;')
  })
})

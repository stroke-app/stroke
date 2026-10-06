import { describe, it, expect } from 'vitest'
import { sqlCompletionContext as ctx } from './sql-complete-context.js'

describe('sqlCompletionContext', () => {
  it('offers statement keywords at the start', () => {
    expect(ctx('Up')).toMatchObject({ kind: 'statement', prefix: 'Up', from: 0 })
    expect(ctx('Up').next).toContain('UPDATE')
  })

  it('offers tables right after UPDATE / FROM / INTO', () => {
    expect(ctx('UPDATE us')).toMatchObject({ kind: 'tables', prefix: 'us' })
    expect(ctx('DELETE FROM ')).toMatchObject({ kind: 'tables', prefix: '' })
    expect(ctx('INSERT INTO "')).toMatchObject({ kind: 'tables', quote: '"', prefix: '' })
  })

  it('offers SET after the UPDATE target', () => {
    const c = ctx('UPDATE "public"."users_table"\nS')
    expect(c).toMatchObject({ kind: 'keywords', clause: 'UPDATE', prefix: 'S' })
    expect(c.next).toEqual(['SET'])
    expect(c.tables).toEqual(['users_table'])
  })

  it('offers columns inside an empty quote in SET', () => {
    const text = 'UPDATE "public"."users_table"\nSET\n  "'
    expect(ctx(text)).toMatchObject({ kind: 'columns', quote: '"', prefix: '', from: text.length })
  })

  it('keeps the typed part of a quoted name as the prefix', () => {
    expect(ctx('UPDATE "t" SET "na')).toMatchObject({ kind: 'columns', quote: '"', prefix: 'na' })
  })

  it('offers WHERE after an assignment, AND/OR after a condition', () => {
    expect(ctx(`UPDATE "t" SET "name" = 'ad' W`).next).toContain('WHERE')
    expect(ctx('UPDATE "t" SET "a" = 1 WHERE "id" = 4 A')).toMatchObject({ kind: 'columns', clause: 'WHERE' })
  })

  it('qualifies after a dot', () => {
    expect(ctx('UPDATE "public".')).toMatchObject({ kind: 'qualified', qualifier: 'public' })
  })

  it('INSERT column list, then VALUES', () => {
    expect(ctx('INSERT INTO "t" ("a", ')).toMatchObject({ kind: 'columns', clause: 'INTO' })
    expect(ctx('INSERT INTO "t" ("a") V').next).toContain('VALUES')
  })

  it('offers nothing inside a string, a comment or a number', () => {
    expect(ctx("UPDATE t SET a = 'x")).toBeNull()
    expect(ctx('-- a note')).toBeNull()
    expect(ctx('UPDATE t SET a = 4')).toBeNull()
  })

  it('knows where a data type goes', () => {
    expect(ctx('CREATE TABLE t (id va')).toMatchObject({ kind: 'types', prefix: 'va' })
    expect(ctx('CREATE TABLE IF NOT EXISTS s.t (\n  id int,\n  name va')).toMatchObject({ kind: 'types' })
    expect(ctx('CREATE TABLE t (price numeric(10, 2), name t')).toMatchObject({ kind: 'types' })
    expect(ctx('SELECT CAST(x AS ')).toMatchObject({ kind: 'types' })
    expect(ctx('SELECT x::')).toMatchObject({ kind: 'types', prefix: '' })
    expect(ctx('SELECT x::tim')).toMatchObject({ kind: 'types', prefix: 'tim' })
    expect(ctx('SELECT CONVERT(')).toMatchObject({ kind: 'types' })
    expect(ctx('ALTER TABLE users ADD COLUMN age ')).toMatchObject({ kind: 'types' })
    expect(ctx('ALTER TABLE users ADD age ')).toMatchObject({ kind: 'types' })
    expect(ctx('ALTER TABLE users ALTER COLUMN email TYPE ')).toMatchObject({ kind: 'types', next: [] })
    expect(ctx('ALTER TABLE users ALTER COLUMN email SET DATA TYPE ')).toMatchObject({ kind: 'types' })
    expect(ctx('ALTER TABLE t MODIFY COLUMN c ')).toMatchObject({ kind: 'types' })
    expect(ctx('ALTER TABLE t CHANGE COLUMN a b ')).toMatchObject({ kind: 'types' })
    // An alias is not a type.
    expect(ctx('SELECT a AS ').kind).not.toBe('types')
  })

  it('offers TYPE beside the types after ALTER COLUMN name', () => {
    const c = ctx('ALTER TABLE users ALTER COLUMN email ')
    expect(c.kind).toBe('types')
    expect(c.next).toContain('TYPE')
  })

  it('offers the statement\'s own words around a column definition, no names', () => {
    expect(ctx('CREATE TABLE t (')).toMatchObject({ kind: 'ddl' })
    expect(ctx('CREATE TABLE t (').next).toContain('PRIMARY')
    const after = ctx('CREATE TABLE t (id int ')
    expect(after.kind).toBe('ddl')
    expect(after.next).toEqual(expect.arrayContaining(['NOT', 'DEFAULT', 'REFERENCES']))
    expect(ctx('CREATE TABLE t (id int REFERENCES ')).toMatchObject({ kind: 'tables' })
    const refCols = ctx('CREATE TABLE t (id int REFERENCES users (')
    expect(refCols.kind).toBe('columns')
    expect(refCols.tables).toContain('users')
  })

  it('reads ALTER TABLE actions', () => {
    expect(ctx('ALTER TABLE users ').next).toEqual(expect.arrayContaining(['ADD', 'DROP', 'ALTER', 'RENAME']))
    expect(ctx('ALTER TABLE users ')).toMatchObject({ kind: 'ddl' })
    expect(ctx('ALTER TABLE users ADD ').next).toContain('COLUMN')
    expect(ctx('ALTER TABLE users DROP COLUMN ')).toMatchObject({ kind: 'columns', tables: ['users'] })
    expect(ctx('ALTER TABLE users ALTER COLUMN ')).toMatchObject({ kind: 'columns', tables: ['users'] })
  })

  it('offers tables after TRUNCATE and CREATE INDEX ... ON, keywords after the name', () => {
    expect(ctx('TRUNCATE ')).toMatchObject({ kind: 'tables' })
    expect(ctx('TRUNCATE users ')).toMatchObject({ kind: 'ddl', next: ['CASCADE', 'RESTRICT'] })
    expect(ctx('DROP TABLE users ')).toMatchObject({ kind: 'ddl', next: ['CASCADE', 'RESTRICT'] })
    expect(ctx('CREATE INDEX i ON ')).toMatchObject({ kind: 'tables' })
    expect(ctx('CREATE INDEX i ON users (')).toMatchObject({ kind: 'columns', tables: ['users'] })
  })

  it('reads doubled quotes as escapes and starts over after a semicolon', () => {
    expect(ctx(`UPDATE t SET a = 'it''s' W`).next).toContain('WHERE')
    expect(ctx('UPDATE "t" SET "a" = 1;\nDEL')).toMatchObject({ kind: 'statement', prefix: 'DEL' })
  })
})

describe('sqlCompletionContext in a trigger', () => {
  const head = 'CREATE TRIGGER trg_after_update '
  it('offers the timing, then the events, then ON', () => {
    expect(ctx(head)).toMatchObject({ kind: 'ddl', next: ['BEFORE', 'AFTER', 'INSTEAD', 'ON'] })
    expect(ctx(`${head}AFTER `)).toMatchObject({ kind: 'ddl', next: ['INSERT', 'UPDATE', 'DELETE'] })
    expect(ctx(`${head}AFTER UPDATE `)).toMatchObject({ kind: 'ddl', next: ['ON', 'OR', 'OF'] })
  })

  it('offers tables after ON', () => {
    expect(ctx(`${head}AFTER UPDATE ON `)?.kind).toBe('tables')
    expect(ctx(`${head}AFTER UPDATE ON us`)).toMatchObject({ kind: 'tables', prefix: 'us' })
  })

  it('offers FOR EACH ROW and the body after the table', () => {
    expect(ctx(`${head}AFTER UPDATE ON user `)?.next).toContain('FOR')
    expect(ctx(`${head}AFTER UPDATE ON user FOR EACH `)?.next).toEqual(['ROW', 'STATEMENT'])
  })

  it('keeps the trigger table across the statements of its body', () => {
    const body = `${head}AFTER UPDATE ON user FOR EACH ROW BEGIN\n  SELECT 1;\n  UPDATE account SET a = NEW.`
    expect(ctx(body)).toMatchObject({ kind: 'qualified', qualifier: 'NEW', rowTable: 'user' })
  })

  it('reads a body statement by its own rules', () => {
    const c = ctx(`${head}AFTER UPDATE ON user FOR EACH ROW BEGIN\n  UPDATE `)
    expect(c?.kind).toBe('tables')
  })

  it('names the function being written', () => {
    expect(ctx('CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger AS $$ BEGIN NEW.')?.routine).toBe('set_updated_at')
  })
})

describe('sqlCompletionContext grammar', () => {
  const texts = (/** @type {any} */ c) => c.phrases.map((/** @type {any} */ p) => p.text)

  it('says what follows DROP, and when it is certain', () => {
    expect(ctx('DROP ')).toMatchObject({ kind: 'ddl', eager: true })
    expect(texts(ctx('DROP TABLE '))).toEqual(['IF EXISTS'])
    expect(ctx('DROP TABLE ')).toMatchObject({ kind: 'tables', eager: false })
    expect(ctx('DROP TABLE IF ')).toMatchObject({ kind: 'ddl', eager: true, next: [] })
    expect(ctx('DROP TABLE IF EXISTS ')).toMatchObject({ kind: 'tables', eager: true })
    expect(ctx('DROP SCHEMA IF EXISTS ')).toMatchObject({ kind: 'ddl', names: 'schemas' })
    expect(texts(ctx('DROP TABLE users ')).slice(0, 2)).toEqual(['CASCADE', 'RESTRICT'])
  })

  it('treats a CREATE\'s name as new', () => {
    expect(ctx('CREATE TABLE ')).toMatchObject({ kind: 'ddl', eager: false })
    expect(texts(ctx('CREATE TABLE '))).toEqual(['IF NOT EXISTS'])
    expect(ctx('CREATE TABLE IF NOT ')).toMatchObject({ eager: true })
    expect(texts(ctx('CREATE UNIQUE INDEX i '))).toEqual(['ON'])
  })

  it('names the table whose columns go here', () => {
    expect(ctx('ALTER TABLE app.users DROP COLUMN ')).toMatchObject({ kind: 'columns', columnsOf: 'users', eager: true })
    expect(ctx('INSERT INTO "Orders" (')).toMatchObject({ kind: 'columns', columnsOf: 'Orders' })
    expect(ctx('UPDATE t SET a = 1 WHERE b ')).toMatchObject({ columnsOf: null })
  })

  it('reads the case the statement is written in', () => {
    expect(ctx('drop table ').lower).toBe(true)
    expect(ctx('DROP TABLE ').lower).toBe(false)
  })

  it('leaves quotes and dotted names alone', () => {
    expect(ctx('DROP TABLE "').phrases).toEqual([])
    expect(ctx('DROP TABLE public.').phrases).toEqual([])
  })
})

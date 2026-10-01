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

  it('reads doubled quotes as escapes and starts over after a semicolon', () => {
    expect(ctx(`UPDATE t SET a = 'it''s' W`).next).toContain('WHERE')
    expect(ctx('UPDATE "t" SET "a" = 1;\nDEL')).toMatchObject({ kind: 'statement', prefix: 'DEL' })
  })
})

import { describe, it, expect } from 'vitest'
import { mergePrisma, mergeDrizzle, counts, toMs, migrationQueries, isMissingTable, rowsOf } from './migrations.js'

const file = (name, checksum, when = null) => ({ name, sql: `-- ${name}`, checksum, when })

describe('mergePrisma', () => {
  const rows = [
    { migration_name: '20240101_init', checksum: 'aaa', started_at: '2024-01-01T00:00:00Z', finished_at: '2024-01-01T00:00:02Z', rolled_back_at: null, logs: null, applied_steps_count: 1 },
    { migration_name: '20240102_posts', checksum: 'bbb', started_at: '2024-01-02T00:00:00Z', finished_at: null, rolled_back_at: null, logs: 'relation "users" does not exist', applied_steps_count: 0 },
    { migration_name: '20240103_tags', checksum: 'ccc', started_at: '2024-01-03T00:00:00Z', finished_at: null, rolled_back_at: '2024-01-03T01:00:00Z', logs: null, applied_steps_count: 0 },
  ]
  it('reads applied, failed and rolled back runs, and pending files', () => {
    const list = mergePrisma(rows, [file('20240101_init', 'aaa'), file('20240102_posts', 'bbb'), file('20240103_tags', 'ccc'), file('20240104_likes', 'ddd')])
    expect(list.map((m) => [m.name, m.status])).toEqual([
      ['20240101_init', 'applied'], ['20240102_posts', 'failed'], ['20240103_tags', 'rolled-back'], ['20240104_likes', 'pending'],
    ])
    expect(list[0].durationMs).toBe(2000)
    expect(list[1].logs).toContain('does not exist')
    expect(counts(list)).toEqual({ applied: 1, pending: 1, failed: 1, rolledBack: 1, changed: 0 })
  })
  it('flags a file edited after it ran, and rows with no file', () => {
    const list = mergePrisma(rows.slice(0, 1), [file('20240101_init', 'zzz')])
    expect(list[0].changed).toBe(true)
    expect(mergePrisma(rows.slice(0, 1), []).at(0)?.missingFile).toBe(true)
    // No folder: nothing is "missing".
    expect(mergePrisma(rows.slice(0, 1), null).at(0)?.missingFile).toBe(false)
  })
  it('a retry that succeeded counts as applied', () => {
    const retry = { ...rows[1], started_at: '2024-01-02T02:00:00Z', finished_at: '2024-01-02T02:00:01Z', logs: null }
    const [m] = mergePrisma([rows[1], retry], null)
    expect(m.status).toBe('applied')
    expect(m.attempts).toBe(2)
  })
})

describe('mergeDrizzle', () => {
  const rows = [{ id: 1, hash: 'h0', created_at: '1700000001000' }, { id: 2, hash: 'h1', created_at: 1700000002000 }]
  it('applied up to the newest created_at, pending after', () => {
    const list = mergeDrizzle(rows, [file('0000_init', 'h0', 1700000001000), file('0001_posts', 'h1', 1700000002000), file('0002_tags', 'h2', 1700000003000)])
    expect(list.map((m) => [m.name, m.status])).toEqual([['0000_init', 'applied'], ['0001_posts', 'applied'], ['0002_tags', 'pending']])
    expect(list[1].appliedAt).toBe(1700000002000)
  })
  it('flags an edited file and rows the project no longer has', () => {
    const list = mergeDrizzle(rows, [file('0000_init', 'edited', 1700000001000)])
    expect(list[0]).toMatchObject({ status: 'applied', changed: true })
    expect(list.at(-1)).toMatchObject({ missingFile: true, status: 'applied' })
  })
  it('lists the rows alone when no folder is set', () => {
    const list = mergeDrizzle(rows, null)
    expect(list).toHaveLength(2)
    expect(list.every((m) => m.status === 'applied')).toBe(true)
  })
})

describe('helpers', () => {
  it('reads timestamps the way each engine returns them', () => {
    expect(toMs('2024-01-01 00:00:00')).toBe(Date.UTC(2024, 0, 1))
    expect(toMs(1700000000)).toBe(1700000000000)
    expect(toMs('1700000000000')).toBe(1700000000000)
    expect(toMs(null)).toBeNull()
  })
  it('quotes per engine and tries the schema first', () => {
    expect(migrationQueries('prisma', 'postgres', 'app')[0]).toContain('FROM "app"."_prisma_migrations"')
    expect(migrationQueries('prisma', 'mysql', null)[0]).toContain('FROM `_prisma_migrations`')
    expect(migrationQueries('drizzle', 'postgres', null)[0]).toContain('"drizzle"."__drizzle_migrations"')
    expect(migrationQueries('drizzle', 'sqlite', null)).toHaveLength(1)
  })
  it('knows a missing table from a real error', () => {
    expect(isMissingTable('relation "_prisma_migrations" does not exist')).toBe(true)
    expect(isMissingTable('no such table: __drizzle_migrations')).toBe(true)
    expect(isMissingTable('permission denied')).toBe(false)
  })
  it('turns a result into objects', () => {
    expect(rowsOf({ columns: [{ name: 'a' }, { name: 'b' }], rows: [[1, 2]] })).toEqual([{ a: 1, b: 2 }])
  })
})

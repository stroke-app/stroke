import { describe, expect, it } from 'vitest'
import { createOps, ordered, stage, toOps, toPattern, ttlLabel, ttlOps, unstage } from './redis-staging.js'

/** @param {string} key @param {string} cell @param {string[][]} ops */
const item = (key, cell, ops = [['X', key]]) => ({ key, binary: false, cell, label: cell, ops })

describe('stage', () => {
  it('replaces an edit to the same cell in place', () => {
    let l = stage([], item('a', 'value', [['SET', 'a', '1']]))
    l = stage(l, item('b', 'ttl'))
    l = stage(l, item('a', 'value', [['SET', 'a', '2']]))
    expect(l).toHaveLength(2)
    expect(l[0].ops).toEqual([['SET', 'a', '2']])
  })

  it('a delete drops other edits to the key and blocks new ones', () => {
    let l = stage([], item('a', 'value'))
    l = stage(l, item('a', 'row'))
    expect(l.map((s) => s.cell)).toEqual(['row'])
    expect(stage(l, item('a', 'ttl'))).toBe(l)
  })

  it('deleting a staged create removes it entirely', () => {
    const l = stage(stage([], item('n', 'create')), item('n', 'row'))
    expect(l).toEqual([])
  })

  it('unstage removes one cell or the whole key', () => {
    const l = stage(stage([], item('a', 'value')), item('a', 'ttl'))
    expect(unstage(l, 'a', 'ttl').map((s) => s.cell)).toEqual(['value'])
    expect(unstage(l, 'a')).toEqual([])
  })
})

describe('ordered / toOps', () => {
  it('sends creates, edits, renames, deletes in that order', () => {
    let l = stage([], item('a', 'row'))
    l = stage(l, item('b', 'key', [['RENAME', 'b', 'c']]))
    l = stage(l, item('b', 'value', [['SET', 'b', 'v']]))
    l = stage(l, item('n', 'create', [['SET', 'n', '1'], ['EXPIRE', 'n', '5']]))
    expect(ordered(l).map((s) => s.cell)).toEqual(['create', 'value', 'key', 'row'])
    const { ops, owner } = toOps(l)
    expect(ops.map((o) => o.args[0])).toEqual(['SET', 'EXPIRE', 'SET', 'RENAME', 'X'])
    expect(owner[0]).toBe(owner[1])
  })
})

describe('ttlOps', () => {
  it('reads durations and blanks', () => {
    expect(ttlOps('k', '')?.ops).toEqual([['PERSIST', 'k']])
    expect(ttlOps('k', '90')?.seconds).toBe(90)
    expect(ttlOps('k', '15m')?.seconds).toBe(900)
    expect(ttlOps('k', '1.5h')?.seconds).toBe(5400)
    expect(ttlOps('k', '2d')?.ops).toEqual([['EXPIRE', 'k', '172800']])
    expect(ttlOps('k', 'soon')).toBeNull()
  })
})

describe('createOps', () => {
  it('builds the first write plus an optional expiry', () => {
    expect(createOps('h', 'hash', { value: 'v', field: 'f', ttl: 60 })).toEqual([
      ['HSET', 'h', 'f', 'v'],
      ['EXPIRE', 'h', '60'],
    ])
    expect(createOps('z', 'zset', { value: 'm' })).toEqual([['ZADD', 'z', '0', 'm']])
  })
})

describe('toPattern / ttlLabel', () => {
  it('wraps plain text as a substring glob', () => {
    expect(toPattern('user')).toBe('*user*')
    expect(toPattern('user:*')).toBe('user:*')
    expect(toPattern('  ')).toBe('')
  })

  it('counts a TTL down from fetch time', () => {
    expect(ttlLabel(-1, 0, 5000)).toBe('')
    expect(ttlLabel(90_000, 0, 30_000)).toBe('1m')
    expect(ttlLabel(1000, 0, 2000)).toBe('expired')
  })
})

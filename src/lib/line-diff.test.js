import { describe, expect, it } from 'vitest'
import { diffLines, pairWordDiffs, wordDiff } from './line-diff.js'

const sig = (/** @type {any[]} */ d) => d.map((l) => ({ same: ' ', add: '+', remove: '-' })[l.type] + l.content).join('\n')

describe('diffLines', () => {
  it('finds a changed column in a table', () => {
    const a = ['CREATE TABLE t (', '  id int,', '  price numeric(10,2),', '  name text', ');']
    const b = ['CREATE TABLE t (', '  id int,', '  price numeric(12,2),', '  name text', ');']
    expect(sig(diffLines(a, b))).toBe(' CREATE TABLE t (\n   id int,\n-  price numeric(10,2),\n+  price numeric(12,2),\n   name text\n );')
  })

  it('ignores a trailing comma that comes with a new last column', () => {
    const a = ['CREATE TABLE t (', '  id int', ');']
    const b = ['CREATE TABLE t (', '  id int,', '  email text', ');']
    expect(diffLines(a, b).map((l) => l.type)).toEqual(['same', 'same', 'add', 'same'])
  })

  it('numbers lines on both sides', () => {
    const d = diffLines(['x', 'a', 'y'], ['x', 'b', 'c', 'y'])
    expect(d.map((l) => [l.type, l.lineA ?? null, l.lineB ?? null])).toEqual([
      ['same', 1, 1], ['remove', 2, null], ['add', null, 2], ['add', null, 3], ['same', 3, 4],
    ])
  })

  it('handles empty sides', () => {
    expect(diffLines([], ['a']).map((l) => l.type)).toEqual(['add'])
    expect(diffLines(['a'], []).map((l) => l.type)).toEqual(['remove'])
    expect(diffLines([], [])).toEqual([])
  })

  it('is an edit script that rebuilds both sides', () => {
    let seed = 7
    const r = () => (seed = (seed * 16807) % 2147483647) / 2147483647
    for (let n = 0; n < 40; n++) {
      const a = Array.from({ length: Math.floor(r() * 30) }, () => 'l' + Math.floor(r() * 6))
      const b = Array.from({ length: Math.floor(r() * 30) }, () => 'l' + Math.floor(r() * 6))
      const d = diffLines(a, b)
      expect(d.filter((l) => l.type !== 'add').map((l) => l.content)).toEqual(a)
      expect(d.filter((l) => l.type !== 'remove').map((l) => l.content)).toEqual(b)
    }
  })

  it('stays fast on a large schema with a few edits', () => {
    const a = Array.from({ length: 20000 }, (_, i) => `  col_${i} text,`)
    const b = a.slice()
    b[5000] = '  col_5000 integer,'
    b.splice(12000, 0, '  added_col uuid,')
    const t = performance.now()
    const d = diffLines(a, b)
    expect(performance.now() - t).toBeLessThan(200)
    expect(d.filter((l) => l.type !== 'same').length).toBe(3)
  })
})

describe('wordDiff', () => {
  it('marks only the part that changed', () => {
    const w = wordDiff('  price numeric(10,2),', '  price numeric(12,2),')
    expect(w.before.filter((p) => p.changed).map((p) => p.text)).toEqual(['10'])
    expect(w.after.filter((p) => p.changed).map((p) => p.text)).toEqual(['12'])
  })

  it('pairs removals with the additions that follow', () => {
    const d = diffLines(['a int', 'b text'], ['a bigint', 'b text'])
    const m = pairWordDiffs(d)
    expect(m.get(0)?.find((p) => p.changed)?.text).toBe('int')
    expect(m.get(1)?.find((p) => p.changed)?.text).toBe('bigint')
  })
})

import { describe, it, expect } from 'vitest'
import { layoutJson, layoutJsonText } from './json-layout.js'

describe('layoutJson', () => {
  it('round-trips: whitespace only', () => {
    const v = { a: 1, b: [1, 2, { c: 'x', d: null }], e: { f: [true, false] }, g: 'q"uote\n', h: [], i: {} }
    expect(JSON.parse(layoutJson(v))).toEqual(v)
  })

  it('keeps short containers on one line', () => {
    expect(layoutJson({ a: 1, b: [1, 2] })).toBe('{"a": 1, "b": [1, 2]}')
  })

  it('one key per line when an object is too wide', () => {
    const v = { name: 'x'.repeat(60), city: 'y'.repeat(60) }
    expect(layoutJson(v)).toBe(`{\n  "name": "${'x'.repeat(60)}",\n  "city": "${'y'.repeat(60)}"\n}`)
  })

  it('packs a huge scalar array into short lines', () => {
    const data = Array.from({ length: 263870 }, (_, i) => i % 256)
    const v = { size: data.length, buffer: { type: 'Buffer', data } }
    const out = layoutJson(v)
    const lines = out.split('\n')
    expect(Math.max(...lines.map((l) => l.length))).toBeLessThanOrEqual(100)
    // About a line per hundred characters, not one per number.
    expect(lines.length).toBeLessThan(out.length / 60)
    expect(JSON.parse(out)).toEqual(v)
  })

  it('is fast on a megabyte-scale value', () => {
    const v = { buffer: { data: Array.from({ length: 300000 }, (_, i) => i % 256) }, rows: Array.from({ length: 5000 }, (_, i) => ({ id: i, name: `n${i}`, tags: ['a', 'b'] })) }
    const t0 = performance.now()
    layoutJson(v)
    expect(performance.now() - t0).toBeLessThan(1500)
  })

  it('an array of objects gets one item per line', () => {
    const v = [{ id: 1, name: 'a'.repeat(50) }, { id: 2, name: 'b'.repeat(50) }]
    expect(layoutJson(v, { width: 80 })).toBe(`[\n  {"id": 1, "name": "${'a'.repeat(50)}"},\n  {"id": 2, "name": "${'b'.repeat(50)}"}\n]`)
  })
})

describe('layoutJsonText', () => {
  it('lays out JSON text and leaves anything else', () => {
    expect(layoutJsonText('{"a":1}')).toBe('{"a": 1}')
    expect(layoutJsonText('hello')).toBeNull()
    expect(layoutJsonText('{broken')).toBeNull()
  })
})

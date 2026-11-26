import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import { savedDiagrams, saveDiagram, deleteDiagram, switchDiagramsConnection } from './saved-diagrams.js'

describe('diagram delete undo', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
    })
    switchDiagramsConnection('a')
  })

  it('puts a deleted diagram back at its index, once', () => {
    saveDiagram('one', 'graph TD')
    saveDiagram('two', 'graph TD')
    saveDiagram('three', 'graph TD')
    const names = () => get(savedDiagrams).map((d) => d.name)
    const before = names()
    const mid = get(savedDiagrams)[1]
    const undo = deleteDiagram(mid.id)
    expect(names()).toHaveLength(2)
    expect(undo?.()).toBe(true)
    expect(names()).toEqual(before)
    expect(undo?.()).toBe(false)
    expect(deleteDiagram('missing')).toBe(null)
  })

  it('will not restore into another connection', () => {
    saveDiagram('one', 'graph TD')
    const undo = deleteDiagram(get(savedDiagrams)[0].id)
    switchDiagramsConnection('b')
    expect(undo?.()).toBe(false)
    expect(get(savedDiagrams)).toEqual([])
  })
})

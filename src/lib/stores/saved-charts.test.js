import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('$lib/sync/changes.js', () => ({ notifyLocalChange: () => {} }))

const { savedCharts, saveChart, deleteChart, switchChartsConnection } = await import('./saved-charts.js')
const { get } = await import('svelte/store')

/** @param {string} name */
const chart = (name) => ({ name, group: 'Default', connectionId: 'a', sql: 'select 1', config: /** @type {any} */ ({ type: 'bar' }) })

describe('chart delete undo', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
    })
    switchChartsConnection('a')
  })

  it('puts a deleted chart back at its index, once', () => {
    saveChart(chart('one'))
    const mid = saveChart(chart('two'))
    saveChart(chart('three'))
    const undo = deleteChart(mid)
    expect(get(savedCharts).map((c) => c.name)).toEqual(['one', 'three'])
    expect(undo?.()).toBe(true)
    expect(get(savedCharts).map((c) => c.name)).toEqual(['one', 'two', 'three'])
    expect(undo?.()).toBe(false)
    expect(deleteChart('missing')).toBe(null)
  })

  it('will not restore into another connection', () => {
    const id = saveChart(chart('one'))
    const undo = deleteChart(id)
    switchChartsConnection('b')
    expect(undo?.()).toBe(false)
    expect(get(savedCharts)).toEqual([])
  })
})

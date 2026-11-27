import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('$lib/sync/changes.js', () => ({ notifyLocalChange: () => {} }))

const { dashboards, activeDashboardId, createDashboard, deleteDashboard, addQueryToDashboard, removeChartFromDashboard, switchDashboardsConnection } = await import('./dashboards.js')
const { get } = await import('svelte/store')

describe('dashboard delete undo', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
    })
    switchDashboardsConnection('a')
  })

  it('puts a deleted dashboard back in its place, active again', () => {
    createDashboard('One')
    const two = createDashboard('Two')
    createDashboard('Three')
    activeDashboardId.set(two.id)
    const undo = deleteDashboard(two.id)
    expect(get(dashboards).map((d) => d.name)).toEqual(['One', 'Three'])
    expect(get(activeDashboardId)).toBe(null)
    expect(undo?.()).toBe(true)
    expect(get(dashboards).map((d) => d.name)).toEqual(['One', 'Two', 'Three'])
    expect(get(activeDashboardId)).toBe(two.id)
    expect(undo?.()).toBe(false)
    expect(deleteDashboard('missing')).toBe(null)
  })

  it('will not restore into another connection', () => {
    const d = createDashboard('One')
    const undo = deleteDashboard(d.id)
    switchDashboardsConnection('b')
    expect(undo?.()).toBe(false)
    expect(get(dashboards)).toEqual([])
  })

  it('puts a removed card back at its index', () => {
    const d = createDashboard('One')
    addQueryToDashboard(d.id, { sql: 'select 1', name: 'a' })
    addQueryToDashboard(d.id, { sql: 'select 2', name: 'b' })
    addQueryToDashboard(d.id, { sql: 'select 3', name: 'c' })
    const items = () => get(dashboards)[0].items.map((i) => i.query?.name)
    const mid = get(dashboards)[0].items[1].id
    const undo = removeChartFromDashboard(d.id, mid)
    expect(items()).toEqual(['a', 'c'])
    expect(undo?.()).toBe(true)
    expect(items()).toEqual(['a', 'b', 'c'])
  })
})

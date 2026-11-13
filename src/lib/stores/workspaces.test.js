import { describe, it, expect, beforeEach } from 'vitest'
import { get } from 'svelte/store'

// A minimal localStorage for the store (the test environment has none).
const mem = new Map()
globalThis.localStorage = /** @type {any} */ ({
  getItem: (k) => (mem.has(k) ? mem.get(k) : null),
  setItem: (k, v) => mem.set(k, String(v)),
  removeItem: (k) => mem.delete(k),
  clear: () => mem.clear(),
})

const ws = await import('./workspaces.js')

describe('workspaces', () => {
  beforeEach(() => {
    mem.clear()
    ws.workspaces.set(ws.loadWorkspaces())
    ws.activeWorkspaceId.set(ws.DEFAULT_WORKSPACE_ID)
  })

  it('always has the default workspace, first', () => {
    expect(get(ws.workspaces).map((w) => w.id)).toEqual([ws.DEFAULT_WORKSPACE_ID])
    mem.set('stroke:workspaces', JSON.stringify([{ id: 'x', name: 'Office', color: 'nope' }]))
    const list = ws.loadWorkspaces()
    expect(list.map((w) => w.id)).toEqual([ws.DEFAULT_WORKSPACE_ID, 'x'])
    expect(list[1].color).toBe('gray')
  })

  it('creates, renames, recolours and persists', () => {
    const w = ws.createWorkspace('  Office ', 'green')
    expect(w.name).toBe('Office')
    ws.updateWorkspace(w.id, { name: 'Work', color: 'violet' })
    const saved = JSON.parse(mem.get('stroke:workspaces'))
    expect(saved.find((x) => x.id === w.id)).toMatchObject({ name: 'Work', color: 'violet' })
    ws.updateWorkspace(w.id, { name: '   ' })
    expect(get(ws.workspaces).find((x) => x.id === w.id)?.name).toBe('Work')
  })

  it('deleting moves connections out first and falls back to default', () => {
    const w = ws.createWorkspace('Client', 'red')
    ws.setActiveWorkspace(w.id)
    /** @type {string[]} */
    const moved = []
    ws.deleteWorkspace(w.id, (id) => moved.push(id))
    expect(moved).toEqual([w.id])
    expect(get(ws.workspaces).some((x) => x.id === w.id)).toBe(false)
    expect(get(ws.activeWorkspaceId)).toBe(ws.DEFAULT_WORKSPACE_ID)
    ws.deleteWorkspace(ws.DEFAULT_WORKSPACE_ID, () => moved.push('default'))
    expect(moved).toEqual([w.id])
  })

  it('filters connections; unknown or missing workspace means default', () => {
    const w = ws.createWorkspace('Office', 'blue')
    const list = get(ws.workspaces)
    const conns = [{ id: 'a' }, { id: 'b', workspaceId: w.id }, { id: 'c', workspaceId: 'gone' }]
    expect(ws.inWorkspace(conns, ws.DEFAULT_WORKSPACE_ID, list).map((c) => c.id)).toEqual(['a', 'c'])
    expect(ws.inWorkspace(conns, w.id, list).map((c) => c.id)).toEqual(['b'])
  })

  it('setActiveWorkspace ignores unknown ids', () => {
    ws.setActiveWorkspace('nope')
    expect(get(ws.activeWorkspaceId)).toBe(ws.DEFAULT_WORKSPACE_ID)
  })
})

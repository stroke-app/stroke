import { describe, it, expect, beforeEach, vi } from 'vitest'

// The store mirrors every change to disk through the backend; record those
// writes instead of calling Tauri.
const writes = /** @type {string[]} */ ([])
let diskPayload = /** @type {string | null} */ (null)
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (/** @type {string} */ cmd, /** @type {any} */ args) => {
    if (cmd === 'connections_store_write') { writes.push(args.json); diskPayload = args.json; return null }
    if (cmd === 'connections_store_read') return diskPayload
    throw new Error(`unexpected command ${cmd}`)
  }),
}))
vi.mock('$lib/stores/sql-draft.js', () => ({ saveSqlDraft: () => {} }))

/** Minimal in-memory localStorage for the node test environment. */
function installLocalStorage() {
  const map = new Map()
  globalThis.localStorage = /** @type {any} */ ({
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => { map.set(k, String(v)) },
    removeItem: (k) => { map.delete(k) },
    clear: () => map.clear(),
    key: (i) => [...map.keys()][i] ?? null,
    get length() { return map.size },
  })
}

const flush = () => new Promise((r) => setTimeout(r, 0))

describe('saved connections', () => {
  beforeEach(() => {
    installLocalStorage()
    writes.length = 0
    diskPayload = null
    vi.resetModules()
  })

  it('does not bring back a deleted connection when it is re-saved', async () => {
    const { upsertConnection, removeConnection, loadSavedConnections } = await import('./connections.js')
    const conn = { id: 'a', type: 'postgres', name: 'prod' }
    upsertConnection(conn)
    removeConnection('a')
    // What the shell does on every connect/reconnect with the open connection.
    upsertConnection({ ...conn, lastConnectedAt: Date.now() })
    expect(loadSavedConnections().map((c) => c.id)).toEqual([])
  })

  it('keeps a Redis provider that serves Redis, drops a stale one', async () => {
    const store = await import('./connections.js')
    store.upsertConnection({ id: 'u', type: 'redis', name: 'Upstash · cache', provider: 'upstash', providerRef: 'db-1', host: 'x.upstash.io' })
    store.upsertConnection({ id: 's', type: 'redis', name: 'old', provider: 'supabase', db: 'postgres' })
    vi.resetModules()
    const next = await import('./connections.js')
    const byId = Object.fromEntries(next.loadSavedConnections().map((c) => [c.id, c]))
    expect(byId.u).toMatchObject({ provider: 'upstash', providerRef: 'db-1' })
    expect(byId.s.provider).toBeUndefined()
    expect(byId.s.db).toBe(0)
  })

  it('lets an explicit re-add revive a deleted id', async () => {
    const { upsertConnection, removeConnection, loadSavedConnections } = await import('./connections.js')
    const conn = { id: 'sample', type: 'sqlite', name: 'Sample Database' }
    upsertConnection(conn)
    removeConnection('sample')
    upsertConnection(conn, { revive: true })
    expect(loadSavedConnections().map((c) => c.id)).toEqual(['sample'])
  })

  it('clears the last-connection id when that connection is deleted', async () => {
    const { upsertConnection, removeConnection, setLastConnectionId, getLastConnectionId } = await import('./connections.js')
    upsertConnection({ id: 'a', type: 'postgres' })
    setLastConnectionId('a')
    removeConnection('a')
    expect(getLastConnectionId()).toBeNull()
  })

  it('writes the delete to disk and restores it from there on the next launch', async () => {
    const store = await import('./connections.js')
    store.upsertConnection({ id: 'a', type: 'postgres' })
    store.upsertConnection({ id: 'b', type: 'postgres' })
    store.removeConnection('a')
    await flush()
    const last = JSON.parse(writes.at(-1) ?? '{}')
    expect(last.connections.map((/** @type {any} */ c) => c.id)).toEqual(['b'])
    expect(last.deleted).toContain('a')

    // Next launch: the webview lost its storage (the WebView2 case), the file did not.
    installLocalStorage()
    vi.resetModules()
    const next = await import('./connections.js')
    await next.hydrateConnectionsFromDisk()
    expect(next.loadSavedConnections().map((c) => c.id)).toEqual(['b'])
    next.upsertConnection({ id: 'a', type: 'postgres' })
    expect(next.loadSavedConnections().map((c) => c.id)).toEqual(['b'])
  })
})

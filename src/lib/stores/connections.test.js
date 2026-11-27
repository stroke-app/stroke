import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// The store mirrors every change to disk through the backend; record those
// writes instead of calling Tauri.
const writes = /** @type {string[]} */ ([])
let diskPayload = /** @type {string | null} */ (null)
/** The keychain vault's connection-secrets entry, and whether it works. */
let vault = /** @type {string | null} */ (null)
let vaultBroken = false
/** Saved-connection ids whose set-aside pools were closed (Labs). */
const closedParked = /** @type {string[]} */ ([])
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (/** @type {string} */ cmd, /** @type {any} */ args) => {
    if (cmd === 'connections_store_write') { writes.push(args.json); diskPayload = args.json; return null }
    if (cmd === 'connections_store_read') return diskPayload
    if (cmd === 'close_parked') { closedParked.push(args.connection); return null }
    if (cmd === 'conn_secrets_read') { if (vaultBroken) throw new Error('no keychain'); return vault }
    if (cmd === 'conn_secrets_read_fresh') { if (vaultBroken) throw new Error('no keychain'); return vault }
    if (cmd === 'conn_secrets_merge') {
      // Same rules as merge_conn_secrets in secrets.rs.
      if (vaultBroken) throw new Error('no keychain')
      const map = vault ? JSON.parse(vault) : {}
      for (const [id, sec] of Object.entries(JSON.parse(args.set))) {
        if (sec && Object.keys(sec).length) map[id] = sec
        else delete map[id]
      }
      for (const id of args.removed) delete map[id]
      vault = Object.keys(map).length ? JSON.stringify(map) : null
      return null
    }
    throw new Error(`unexpected command ${cmd}`)
  }),
}))
vi.mock('$lib/stores/sql-draft.js', () => ({ saveSqlDraft: () => {}, saveSqlTabs: () => {} }))

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
    vault = null
    vaultBroken = false
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

describe('connection secrets in the keychain', () => {
  beforeEach(() => {
    installLocalStorage()
    writes.length = 0
    diskPayload = null
    vault = null
    vaultBroken = false
    vi.resetModules()
  })

  const legacy = [
    { id: 'pg', type: 'postgres', name: 'pg', host: 'h', password: 'hunter2' },
    { id: 'mg', type: 'mongodb', name: 'mg', uri: 'mongodb://u:p@h/db' },
    { id: 'lite', type: 'sqlite', name: 'lite', filePath: '/x.db' },
  ]

  it('lists connections before the keychain answers, and fills passwords for a connect once it has', async () => {
    vault = JSON.stringify({ pg: { password: 'hunter2' } })
    diskPayload = JSON.stringify({ connections: [{ id: 'pg', type: 'postgres', name: 'pg', host: 'h' }] })
    const store = await import('./connections.js')
    // What startup does: the list is in, the keychain read keeps going.
    await store.hydrateConnectionsFromDisk({ waitForSecrets: false })
    expect(store.loadSavedConnections().map((c) => c.id)).toEqual(['pg'])
    const ready = await store.withSecrets(store.loadSavedConnections()[0])
    expect(ready.password).toBe('hunter2')
    // A field the caller set wins over the stored one.
    expect((await store.withSecrets({ id: 'pg', password: 'typed' })).password).toBe('typed')
  })

  it('moves inline secrets to the keychain at startup and strips them from disk', async () => {
    localStorage.setItem('stroke:connections', JSON.stringify(legacy))
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    await flush()
    expect(JSON.parse(vault ?? '{}')).toEqual({ pg: { password: 'hunter2' }, mg: { uri: 'mongodb://u:p@h/db' } })
    // No write to disk ever held a secret.
    expect(writes.every((w) => !w.includes('hunter2'))).toBe(true)
    const raw = localStorage.getItem('stroke:connections') ?? ''
    expect(raw).not.toContain('hunter2')
    expect(raw).not.toContain('u:p@')
    expect(diskPayload).not.toContain('hunter2')
    // Callers still see whole connections.
    const loaded = store.loadSavedConnections()
    expect(loaded.find((c) => c.id === 'pg')?.password).toBe('hunter2')
    expect(loaded.find((c) => c.id === 'mg')?.uri).toBe('mongodb://u:p@h/db')
  })

  it('saves new secrets to the keychain only, and forgets deleted ones', async () => {
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    store.upsertConnection({ id: 'n', type: 'mysql', name: 'n', password: 's3cret' })
    await flush()
    expect(localStorage.getItem('stroke:connections')).not.toContain('s3cret')
    expect(JSON.parse(vault ?? '{}').n).toEqual({ password: 's3cret' })
    store.removeConnection('n')
    await flush()
    expect(JSON.parse(vault ?? '{}').n).toBeUndefined()
  })

  it("leaves the other track's passwords in the shared keychain", async () => {
    // Nightly saved "n" (with its password); this app (Stable) never had it.
    vault = JSON.stringify({ n: { password: 'nightly' }, gone: { password: 'x' } })
    diskPayload = JSON.stringify({ connections: [{ id: 's', type: 'postgres', name: 's', host: 'h' }], deleted: ['gone'] })
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    // Startup drops only a deleted id, not every id missing from this list.
    expect(JSON.parse(vault ?? '{}')).toEqual({ n: { password: 'nightly' } })
    // A save here merges, so "n" survives this app's writes too.
    store.upsertConnection({ id: 's', type: 'postgres', name: 's', host: 'h', password: 'stable' })
    await flush()
    expect(JSON.parse(vault ?? '{}')).toEqual({ n: { password: 'nightly' }, s: { password: 'stable' } })
  })

  it('picks up a connection the other track added, with its password', async () => {
    diskPayload = JSON.stringify({ connections: [{ id: 's', type: 'postgres', name: 's' }], deleted: [] })
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    expect(await store.refreshConnectionsFromDisk()).toBe(false)
    // Meanwhile Nightly adds "n" to the shared file and keychain.
    diskPayload = JSON.stringify({ connections: [{ id: 's', type: 'postgres', name: 's' }, { id: 'n', type: 'mysql', name: 'n' }], deleted: [] })
    vault = JSON.stringify({ n: { password: 'nightly' } })
    expect(await store.refreshConnectionsFromDisk()).toBe(true)
    const n = store.loadSavedConnections().find((c) => c.id === 'n')
    expect(n?.password).toBe('nightly')
    // Same list with keys in another order is not a change.
    diskPayload = JSON.stringify({ deleted: [], connections: [{ name: 's', type: 'postgres', id: 's' }, { name: 'n', id: 'n', type: 'mysql' }] })
    expect(await store.refreshConnectionsFromDisk()).toBe(false)
  })

  it('tells the disk copy about a revived connection', async () => {
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    store.upsertConnection({ id: 'sample', type: 'sqlite', name: 'Sample', filePath: '/s.db' })
    store.removeConnection('sample')
    store.upsertConnection({ id: 'sample', type: 'sqlite', name: 'Sample', filePath: '/s.db' }, { revive: true })
    await flush()
    const last = JSON.parse(writes.at(-1) ?? '{}')
    expect(last.revived).toEqual(['sample'])
    expect(last.deleted).toEqual([])
  })

  it('keeps secrets inline when the keychain is unavailable', async () => {
    vaultBroken = true
    localStorage.setItem('stroke:connections', JSON.stringify(legacy))
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    expect(localStorage.getItem('stroke:connections')).toContain('hunter2')
    store.upsertConnection({ id: 'n', type: 'mysql', name: 'n', password: 'p2' })
    expect(localStorage.getItem('stroke:connections')).toContain('p2')
    expect(store.loadSavedConnections().find((c) => c.id === 'pg')?.password).toBe('hunter2')
  })

  it('a keychain write that fails later puts the secrets back inline', async () => {
    const store = await import('./connections.js')
    await store.hydrateConnectionsFromDisk()
    vaultBroken = true
    store.upsertConnection({ id: 'n', type: 'mysql', name: 'n', password: 'keepme' })
    await flush(); await flush()
    expect(localStorage.getItem('stroke:connections')).toContain('keepme')
  })
})

// Labs, several connections at once: a pool set aside for a saved connection
// must not outlive an edit that points it somewhere else.
describe('set-aside pools on edit and delete', () => {
  beforeEach(() => {
    installLocalStorage()
    closedParked.length = 0
    vi.resetModules()
  })

  const base = { id: 'p', type: 'postgres', name: 'prod', host: 'db.one', port: 5432, user: 'me', password: 'x', database: 'app' }

  it('leaves them alone on the write-back of every connect and database switch', async () => {
    const { upsertConnection } = await import('./connections.js')
    upsertConnection(base)
    upsertConnection({ ...base, lastConnectedAt: 1, database: 'other', name: 'Prod', color: 'red' })
    // A live copy without its secret is not a password change.
    const { password: _, ...noSecret } = base
    upsertConnection(noSecret)
    await flush()
    expect(closedParked).toEqual([])
  })

  it('closes them when the server, login or password changes', async () => {
    const { upsertConnection } = await import('./connections.js')
    upsertConnection(base)
    upsertConnection({ ...base, host: 'db.two' })
    upsertConnection({ ...base, host: 'db.two', password: 'y' })
    upsertConnection({ ...base, host: 'db.two', password: 'y', ssh: { host: 'jump', username: 'me' } })
    await flush()
    expect(closedParked).toEqual(['p', 'p', 'p'])
  })

  it('closes them when the connection is deleted', async () => {
    const { upsertConnection, removeConnection } = await import('./connections.js')
    upsertConnection(base)
    removeConnection('p')
    await flush()
    expect(closedParked).toEqual(['p'])
  })
})

// Workspaces: a one-click connect (provider, Docker, local server, D1) builds a
// fresh row each time, and writing it over the saved one used to drop its
// workspace, colour and group, sending it back to Personal.
describe('workspace filing on save', () => {
  beforeEach(() => {
    installLocalStorage()
    vi.resetModules()
  })

  it('files a new connection under the workspace on screen', async () => {
    const { upsertConnection, loadSavedConnections } = await import('./connections.js')
    const ws = await import('./workspaces.js')
    ws.activeWorkspaceId.set('work')
    upsertConnection({ id: 'n', type: 'postgres', name: 'neon-db', host: 'ep.neon.tech' })
    expect(loadSavedConnections()[0].workspaceId).toBe('work')
  })

  it('leaves a new connection in the default workspace when that is on screen', async () => {
    const { upsertConnection, loadSavedConnections } = await import('./connections.js')
    upsertConnection({ id: 'n', type: 'postgres', name: 'x' })
    expect(loadSavedConnections()[0].workspaceId).toBeUndefined()
  })

  it('keeps the workspace, colour and group when a fresh row is written over a saved one', async () => {
    const { upsertConnection, loadSavedConnections } = await import('./connections.js')
    upsertConnection({ id: 'local-pg', type: 'postgres', name: 'pg', workspaceId: 'work', color: 'red', group: 'Local' })
    // What connectionFor / providerToSaved hand back on the next connect.
    upsertConnection({ id: 'local-pg', type: 'postgres', name: 'pg', host: '127.0.0.1', lastConnectedAt: 5 })
    const row = loadSavedConnections()[0]
    expect([row.workspaceId, row.color, row.group, row.lastConnectedAt]).toEqual(['work', 'red', 'Local', 5])
  })

  it('lets the edit form move or clear them by naming the field', async () => {
    const { upsertConnection, loadSavedConnections } = await import('./connections.js')
    upsertConnection({ id: 'a', type: 'postgres', name: 'a', workspaceId: 'work', color: 'red' })
    upsertConnection({ id: 'a', type: 'postgres', name: 'a', workspaceId: undefined, color: 'blue' })
    const row = loadSavedConnections()[0]
    expect([row.workspaceId, row.color]).toEqual([undefined, 'blue'])
  })

  it('stores a synced row exactly as it came', async () => {
    const { upsertConnection, loadSavedConnections } = await import('./connections.js')
    const ws = await import('./workspaces.js')
    ws.activeWorkspaceId.set('work')
    upsertConnection({ id: 'a', type: 'postgres', name: 'a', workspaceId: 'home', color: 'red' })
    upsertConnection({ id: 'a', type: 'postgres', name: 'a' }, { exact: true })
    upsertConnection({ id: 'b', type: 'postgres', name: 'b' }, { exact: true })
    const [a, b] = loadSavedConnections()
    expect([a.workspaceId, a.color, b.workspaceId]).toEqual([undefined, undefined, undefined])
  })
})

// Undo over confirm: a delete from the UI hides the row at once, but holds the
// purge of its saved work until the undo window closes.
describe('undoable delete', () => {
  beforeEach(() => {
    installLocalStorage()
    vi.resetModules()
    vi.useFakeTimers()
  })
  afterEach(() => vi.useRealTimers())

  const conn = { id: 'u', type: 'postgres', name: 'prod', host: 'db', workspaceId: 'w' }

  it('brings the row and its saved work back inside the window', async () => {
    const store = await import('./connections.js')
    store.upsertConnection(conn)
    localStorage.setItem('stroke:saved-charts:u', '[1]')
    store.removeConnection('u', { undoable: true })
    expect(store.loadSavedConnections().map((c) => c.id)).toEqual([])
    vi.advanceTimersByTime(store.UNDO_REMOVE_MS - 1)
    expect(store.restoreConnection('u')).toBe(true)
    expect(store.loadSavedConnections().map((c) => c.id)).toEqual(['u'])
    expect(localStorage.getItem('stroke:saved-charts:u')).toBe('[1]')
    // Undone means really back: a later write does not treat it as deleted.
    store.upsertConnection({ ...conn, name: 'prod 2' })
    expect(store.loadSavedConnections()[0].name).toBe('prod 2')
  })

  it('purges once the window closes, and Undo then does nothing', async () => {
    const store = await import('./connections.js')
    store.upsertConnection(conn)
    localStorage.setItem('stroke:saved-charts:u', '[1]')
    store.removeConnection('u', { undoable: true })
    expect(localStorage.getItem('stroke:saved-charts:u')).toBe('[1]')
    vi.advanceTimersByTime(store.UNDO_REMOVE_MS)
    expect(localStorage.getItem('stroke:saved-charts:u')).toBeNull()
    expect(store.restoreConnection('u')).toBe(false)
    expect(store.loadSavedConnections()).toEqual([])
  })

  it('flushes an open window at once', async () => {
    const store = await import('./connections.js')
    store.upsertConnection(conn)
    localStorage.setItem('stroke:saved-charts:u', '[1]')
    store.removeConnection('u', { undoable: true })
    store.flushPendingRemovals()
    expect(localStorage.getItem('stroke:saved-charts:u')).toBeNull()
  })

  it('purges at once when not undoable (a delete from sync)', async () => {
    const store = await import('./connections.js')
    store.upsertConnection(conn)
    localStorage.setItem('stroke:saved-charts:u', '[1]')
    store.removeConnection('u')
    expect(localStorage.getItem('stroke:saved-charts:u')).toBeNull()
    expect(store.restoreConnection('u')).toBe(false)
  })
})

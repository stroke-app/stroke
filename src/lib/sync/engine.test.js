import { describe, it, expect, beforeEach } from 'vitest'
import { createSyncEngine, hashData, stableStringify, resetSyncBookkeeping, META_KEY } from './engine.js'
import { recordDeletion, onLocalChange, notifyLocalChange, withLocalChangesMuted, loadTombstones } from './changes.js'

/** A Storage-like map. */
function memStorage() {
  const m = new Map()
  return /** @type {any} */ ({
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => m.set(k, String(v)),
    removeItem: (k) => m.delete(k),
    clear: () => m.clear(),
    _m: m,
  })
}

/**
 * The server side of Appendix B, with plaintext standing in for ciphertext
 * (the Rust side encrypts; the engine never sees ciphertext).
 */
function fakeServer() {
  /** @type {Map<string, any>} */
  const recs = new Map()
  let seq = 0
  let pageSize = 500
  const calls = { pull: 0, push: 0, pushed: /** @type {any[]} */ ([]) }
  /** @param {string} cmd @param {any} args */
  async function invoke(cmd, args) {
    if (cmd === 'sync_pull') {
      calls.pull++
      const all = [...recs.values()].filter((r) => r.seq > args.since).sort((a, b) => a.seq - b.seq)
      const page = all.slice(0, pageSize)
      return { records: page.map((r) => ({ ...r })), next_since: page.at(-1)?.seq ?? args.since, more: all.length > page.length }
    }
    if (cmd === 'sync_push') {
      calls.push++
      const accepted = []
      const conflicts = []
      for (const r of args.records) {
        calls.pushed.push(r)
        const key = `${r.kind}:${r.id}`
        const cur = recs.get(key)?.version ?? 0
        if (r.version === cur + 1) {
          seq++
          recs.set(key, { ...r, plaintext: r.deleted ? null : r.plaintext, seq, updated_at: seq * 1000 })
          accepted.push({ id: r.id, version: r.version, seq })
        } else {
          conflicts.push(recs.get(key) ? { ...recs.get(key) } : { id: r.id, kind: r.kind, version: 0, deleted: true, plaintext: null, updated_at: 0, seq: 0 })
        }
      }
      return { accepted, conflicts }
    }
    throw new Error(`unexpected ${cmd}`)
  }
  return { invoke, recs, calls, setPageSize: (n) => (pageSize = n) }
}

/** One device: its own storage, stores and clock. */
function device(server, name) {
  const storage = memStorage()
  let clock = 1_000_000
  const state = {
    workspace: new Map([['default', { id: 'default', name: 'Personal', color: 'blue', createdAt: 0 }]]),
    connection: new Map(),
    settings: new Map([['settings', { app: { theme: 'light' } }]]),
  }
  let copies = 0
  /** @param {'workspace'|'connection'|'settings'} kind */
  const adapter = (kind) => ({
    list: () => [...state[kind].entries()].map(([id, data]) => ({ id, data })),
    upsert: (data) => state[kind].set(kind === 'settings' ? 'settings' : data.id, data),
    remove: (id) => state[kind].delete(id),
    ...(kind === 'connection' && {
      conflictCopy: (data) => {
        const id = `${data.id}-copy${++copies}`
        state.connection.set(id, { ...data, id, name: `${data.name} (conflicted copy)` })
      },
    }),
  })
  const engine = createSyncEngine({
    invoke: server.invoke,
    adapters: { workspace: adapter('workspace'), connection: adapter('connection'), settings: adapter('settings') },
    storage,
    now: () => clock,
  })
  return {
    name,
    state,
    storage,
    tick: (ms = 1000) => (clock += ms),
    /** Run `fn` with this device's localStorage (tombstones live there). */
    as(fn) {
      globalThis.localStorage = storage
      return fn()
    },
    async sync() {
      globalThis.localStorage = storage
      return engine.run()
    },
  }
}

const conn = (id, extra = {}) => ({ id, type: 'postgres', name: id.toUpperCase(), host: 'db', password: 'secret', ...extra })

describe('hashing', () => {
  it('ignores key order and undefined', () => {
    expect(stableStringify({ b: 1, a: [1, { d: 2, c: undefined }] })).toBe('{"a":[1,{"d":2}],"b":1}')
    expect(hashData({ a: 1, b: 2 })).toBe(hashData({ b: 2, a: 1 }))
    expect(hashData({ a: 1 })).not.toBe(hashData({ a: 2 }))
    expect(hashData({ a: 1 })).toMatch(/^[0-9a-f]{16}$/)
  })
})

describe('change signals', () => {
  beforeEach(() => {
    globalThis.localStorage = memStorage()
  })

  it('notifies listeners, and stays quiet while sync applies remote data', () => {
    const seen = []
    const off = onLocalChange((k) => seen.push(k))
    notifyLocalChange('connection')
    withLocalChangesMuted(() => {
      notifyLocalChange('workspace')
      recordDeletion('connection', 'x')
    })
    expect(seen).toEqual(['connection'])
    expect(loadTombstones()).toEqual([])
    recordDeletion('workspace', 'w1')
    expect(loadTombstones().map((t) => t.id)).toEqual(['w1'])
    expect(seen).toEqual(['connection', 'workspace'])
    off()
  })
})

describe('sync engine', () => {
  /** @type {ReturnType<typeof fakeServer>} */ let server
  /** @type {ReturnType<typeof device>} */ let a
  /** @type {ReturnType<typeof device>} */ let b

  beforeEach(() => {
    server = fakeServer()
    a = device(server, 'A')
    b = device(server, 'B')
  })

  it('first device pushes everything; a new device takes it without conflict copies', async () => {
    a.state.workspace.set('w1', { id: 'w1', name: 'Office', color: 'red', createdAt: 5 })
    a.state.connection.set('c1', conn('c1', { workspaceId: 'w1' }))
    a.state.settings.set('settings', { app: { theme: 'dark' } })
    a.state.workspace.set('default', { id: 'default', name: 'Home', color: 'green', createdAt: 0 })
    const r = await a.sync()
    expect(r.pushed).toBe(4)
    expect(server.recs.size).toBe(4)
    expect(JSON.parse(server.recs.get('connection:c1').plaintext).data.password).toBe('secret')

    // B has fresh defaults: the server's copies win, nothing is pushed back.
    const rb = await b.sync()
    expect(rb.conflicts).toBe(0)
    expect(b.state.settings.get('settings')).toEqual({ app: { theme: 'dark' } })
    expect(b.state.workspace.get('default').name).toBe('Home')
    expect(b.state.connection.get('c1')).toEqual(conn('c1', { workspaceId: 'w1' }))
    expect(b.state.connection.size).toBe(1)
    expect(rb.pushed).toBe(0)
    expect(rb.items).toBe(4)

    // Steady state: nothing to send either way.
    const before = server.calls.push
    await a.sync()
    await b.sync()
    expect(server.calls.push).toBe(before)
  })

  it('carries edits across without echoing them back', async () => {
    a.state.connection.set('c1', conn('c1'))
    await a.sync()
    await b.sync()
    a.tick()
    a.state.connection.set('c1', conn('c1', { host: 'db2' }))
    expect((await a.sync()).pushed).toBe(1)
    expect(server.recs.get('connection:c1').version).toBe(2)
    const rb = await b.sync()
    expect(rb.pulled).toBe(1)
    expect(rb.pushed).toBe(0)
    expect(b.state.connection.get('c1').host).toBe('db2')
    expect((await a.sync()).pushed).toBe(0)
  })

  it('pushes deletions as tombstones and applies them elsewhere', async () => {
    a.state.connection.set('c1', conn('c1'))
    a.state.workspace.set('w1', { id: 'w1', name: 'X', color: 'gray', createdAt: 1 })
    await a.sync()
    await b.sync()
    expect(b.state.connection.has('c1')).toBe(true)

    a.state.connection.delete('c1')
    a.state.workspace.delete('w1')
    a.as(() => {
      recordDeletion('connection', 'c1')
      recordDeletion('workspace', 'w1')
    })
    const r = await a.sync()
    expect(r.pushed).toBe(2)
    const tomb = server.recs.get('connection:c1')
    expect(tomb.deleted).toBe(true)
    expect(tomb.plaintext).toBeNull()
    expect(tomb.version).toBe(2)
    expect(a.as(loadTombstones)).toEqual([])

    await b.sync()
    expect(b.state.connection.has('c1')).toBe(false)
    expect(b.state.workspace.has('w1')).toBe(false)
  })

  it('drops tombstones of records that never reached the server', async () => {
    a.as(() => recordDeletion('connection', 'never-synced'))
    await a.sync()
    expect(server.calls.pushed.some((r) => r.deleted)).toBe(false)
    expect(a.as(loadTombstones)).toEqual([])
  })

  it('a newer local edit wins a conflict; the losing connection is kept as a copy', async () => {
    a.state.connection.set('c1', conn('c1'))
    await a.sync()
    await b.sync()

    a.tick(1000)
    a.state.connection.set('c1', conn('c1', { host: 'from-a' }))
    b.tick(5000) // B edits later
    b.state.connection.set('c1', conn('c1', { host: 'from-b' }))
    // Make B notice its edit before A pushes (as the 2 s debounce would).
    await a.sync()
    const rb = await b.sync()
    expect(rb.conflicts).toBe(1)
    expect(b.state.connection.get('c1').host).toBe('from-b')
    const copy = [...b.state.connection.values()].find((c) => c.name.endsWith('(conflicted copy)'))
    expect(copy?.host).toBe('from-a')
    expect(server.recs.get('connection:c1').version).toBe(3)

    await a.sync()
    expect(a.state.connection.get('c1').host).toBe('from-b')
    expect([...a.state.connection.values()].some((c) => c.name === 'C1 (conflicted copy)')).toBe(true)
  })

  it('an older local edit loses; it is kept as a copy and the server version applied', async () => {
    a.state.connection.set('c1', conn('c1'))
    await a.sync()
    await b.sync()

    // B edits first and notices it (a sync with nothing new on the server).
    b.state.connection.set('c1', conn('c1', { host: 'old-b' }))
    b.tick(1000)
    // A edits later and pushes first.
    a.tick(60_000)
    a.state.connection.set('c1', conn('c1', { host: 'new-a' }))
    // B's push would conflict; its pull sees A's newer edit first.
    await a.sync()
    const rb = await b.sync()
    expect(rb.conflicts).toBe(1)
    expect(b.state.connection.get('c1').host).toBe('new-a')
    const copy = [...b.state.connection.values()].find((c) => c.name.endsWith('(conflicted copy)'))
    expect(copy?.host).toBe('old-b')
  })

  it('a stale server edit loses to a newer local one (settings: no copy)', async () => {
    a.state.settings.set('settings', { app: { theme: 'dark' } })
    await a.sync()
    await b.sync()
    b.tick(10_000)
    b.state.settings.set('settings', { app: { theme: 'nord' } })
    const rec = server.recs.get('settings:settings')
    server.recs.set('settings:settings', {
      ...rec,
      version: rec.version + 1,
      seq: 99,
      plaintext: JSON.stringify({ updated_at: 1, data: { app: { theme: 'old' } } }),
    })
    const rb = await b.sync()
    expect(rb.conflicts).toBe(1)
    expect(b.state.settings.get('settings')).toEqual({ app: { theme: 'nord' } })
    expect(server.recs.get('settings:settings').version).toBe(rec.version + 2)
    expect(JSON.parse(server.recs.get('settings:settings').plaintext).data.app.theme).toBe('nord')
  })

  it('a push conflict (another device pushed between pull and push) resolves and retries', async () => {
    a.state.connection.set('c1', conn('c1'))
    await a.sync()
    const orig = server.invoke
    let bumped = false
    const c = device({
      invoke: async (cmd, args) => {
        if (cmd === 'sync_push' && !bumped) {
          bumped = true
          const rec = server.recs.get('connection:c1')
          server.recs.set('connection:c1', {
            ...rec,
            version: rec.version + 1,
            seq: 500,
            plaintext: JSON.stringify({ updated_at: 5, data: conn('c1', { host: 'sneaky' }) }),
          })
        }
        return orig(cmd, args)
      },
    }, 'C')
    await c.sync() // takes c1 v1
    c.tick(100_000)
    c.state.connection.set('c1', conn('c1', { host: 'mine' }))
    bumped = false
    const r = await c.sync()
    expect(r.conflicts).toBe(1)
    expect(c.state.connection.get('c1').host).toBe('mine')
    expect([...c.state.connection.values()].find((x) => x.name.endsWith('(conflicted copy)'))?.host).toBe('sneaky')
    expect(server.recs.get('connection:c1').version).toBe(3)
    expect(JSON.parse(server.recs.get('connection:c1').plaintext).data.host).toBe('mine')
  })

  it('a deletion elsewhere loses to a newer local edit, which comes back', async () => {
    a.state.connection.set('c1', conn('c1'))
    await a.sync()
    await b.sync()
    a.state.connection.delete('c1')
    a.as(() => recordDeletion('connection', 'c1'))
    await a.sync() // tombstone at server time 2000ms-ish
    b.tick(10_000_000)
    b.state.connection.set('c1', conn('c1', { host: 'kept' }))
    await b.sync()
    expect(b.state.connection.get('c1').host).toBe('kept')
    expect(server.recs.get('connection:c1').deleted).toBe(false)
    await a.sync()
    expect(a.state.connection.get('c1').host).toBe('kept')
  })

  it('pages through /changes and skips undecryptable records', async () => {
    for (let i = 0; i < 5; i++) a.state.workspace.set(`w${i}`, { id: `w${i}`, name: `W${i}`, color: 'gray', createdAt: i })
    await a.sync()
    server.setPageSize(2)
    const bad = { id: 'zz', kind: 'workspace', version: 1, deleted: false, plaintext: null, seq: 1000, error: 'decryption failed' }
    const orig = server.invoke
    let first = true
    const engineB = device({ invoke: async (cmd, args) => {
      const r = await orig(cmd, args)
      if (cmd === 'sync_pull' && first) {
        first = false
        r.records.push(bad)
      }
      return r
    } }, 'B2')
    const r = await engineB.sync()
    expect(r.undecryptable).toBe(1)
    expect(engineB.state.workspace.size).toBe(6)
    expect(server.calls.pull).toBeGreaterThanOrEqual(4)
  })

  it('a placeholder conflict (server has no record, e.g. after a reset) restarts at version 1', async () => {
    a.state.connection.set('c1', conn('c1'))
    a.state.connection.set('c2', conn('c2'))
    await a.sync()
    server.recs.clear() // reset elsewhere wiped the records
    a.tick()
    a.state.connection.set('c1', conn('c1', { host: 'after-reset' }))
    a.state.connection.delete('c2')
    a.as(() => recordDeletion('connection', 'c2'))
    const r = await a.sync()
    expect(r.conflicts).toBe(0)
    expect(server.recs.get('connection:c1').version).toBe(1)
    expect(JSON.parse(server.recs.get('connection:c1').plaintext).data.host).toBe('after-reset')
    expect(server.recs.has('connection:c2')).toBe(false)
    expect(a.as(loadTombstones)).toEqual([])
  })

  it('ignores ids the server would reject', async () => {
    a.state.connection.set('stroke:sample-sqlite', conn('stroke:sample-sqlite'))
    await a.sync()
    expect(server.recs.size).toBe(2) // default workspace + settings
  })

  it('reset bookkeeping keeps only unsynced local edit times', async () => {
    a.state.connection.set('c1', conn('c1'))
    a.state.connection.set('c3', conn('c3'))
    await a.sync()
    a.tick()
    a.state.connection.set('c1', conn('c1', { host: 'edited' }))
    a.state.connection.set('c2', conn('c2'))
    // A failed push still records when the edits were seen.
    const offline = createSyncEngine({
      invoke: async (cmd, args) => (cmd === 'sync_pull' ? server.invoke(cmd, args) : Promise.reject(new Error('offline'))),
      adapters: {
        workspace: { list: () => [], upsert() {}, remove() {} },
        connection: { list: () => [...a.state.connection.entries()].map(([id, data]) => ({ id, data })), upsert() {}, remove() {} },
        settings: { list: () => [], upsert() {}, remove() {} },
      },
      storage: a.storage,
      now: () => 5_000_000,
    })
    globalThis.localStorage = a.storage
    await expect(offline.run()).rejects.toThrow('offline')
    resetSyncBookkeeping(a.storage)
    const kept = JSON.parse(a.storage.getItem(META_KEY))
    expect(Object.keys(kept).sort()).toEqual(['connection:c1', 'connection:c2'])
    expect(kept['connection:c1']).toEqual({ v: 0, h: null, lh: null, at: 5_000_000 })
  })
})

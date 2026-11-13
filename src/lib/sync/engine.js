// End-to-end encrypted sync engine (docs/accounts-and-sync.md, Appendix B).
//
// Rust (src-tauri/src/sync.rs) does the crypto and HTTP; this module decides
// what to send and what to apply. It is a pure core with injected store
// adapters so it can be tested without Tauri; `runtime.js` wires the real
// stores, the triggers and the status store.
//
// Per record (`<kind>:<id>`) it keeps, in localStorage:
//   v   the server version this device last saw (0 = never synced)
//   h   hash of the data at that version (null = deleted there)
//   lh  hash of the local data when last looked at
//   at  when the local data last changed (unix ms; 0 = before this device synced)
// A record is dirty when its local hash differs from `h`. A sync pulls first,
// applies what changed elsewhere, then pushes what is dirty.

import { loadTombstones, saveTombstones, withLocalChangesMuted } from './changes.js'

/** @typedef {import('./changes.js').SyncKind} SyncKind */
/** @typedef {{ v: number, h: string | null, lh?: string | null, at: number }} Meta */
/** @typedef {{ kind: SyncKind, id: string, data: any }} LocalRecord */
/**
 * @typedef {{ id: string, kind: SyncKind, version: number, deleted: boolean,
 *   plaintext: string | null, seq?: number, updated_at?: number | string | null, error?: string }} RemoteRecord
 */
/**
 * One synced kind. `list` returns every local record of the kind; `upsert` and
 * `remove` write remote data into the store (change signals are muted around
 * them); `conflictCopy` (connections only) saves a losing copy.
 * @typedef {{
 *   list: () => { id: string, data: any }[],
 *   upsert: (data: any) => void,
 *   remove: (id: string) => void,
 *   conflictCopy?: (data: any) => void,
 * }} KindAdapter
 */
/** @typedef {Record<SyncKind, KindAdapter>} Adapters */
/** @typedef {{ pulled: number, pushed: number, conflicts: number, undecryptable: number, items: number }} SyncReport */

export const META_KEY = 'stroke:sync:meta'
export const CURSOR_KEY = 'stroke:sync:cursor'
export const INIT_KEY = 'stroke:sync:initialized'
export const KINDS = /** @type {SyncKind[]} */ (['workspace', 'connection', 'settings'])
const ID_RE = /^[A-Za-z0-9_-]{1,128}$/
const PUSH_ROUNDS = 3

// ── Hashing ──────────────────────────────────────────────────────────────────

/** JSON with sorted keys, so the same data always hashes the same. @param {any} v @returns {string} */
export function stableStringify(v) {
  if (v === undefined) return 'null'
  if (v === null || typeof v !== 'object') return JSON.stringify(v) ?? 'null'
  if (Array.isArray(v)) return `[${v.map(stableStringify).join(',')}]`
  return `{${Object.keys(v)
    .filter((k) => v[k] !== undefined)
    .sort()
    .map((k) => `${JSON.stringify(k)}:${stableStringify(v[k])}`)
    .join(',')}}`
}

/** 64-bit FNV-1a (two 32-bit lanes) of the stable JSON, hex. @param {any} data */
export function hashData(data) {
  const s = stableStringify(data)
  let a = 0x811c9dc5
  let b = 0xcbf29ce4
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i)
    a = Math.imul(a ^ c, 0x01000193) >>> 0
    b = Math.imul(b ^ c ^ (a >>> 7), 0x01000193) >>> 0
  }
  return a.toString(16).padStart(8, '0') + b.toString(16).padStart(8, '0')
}

// ── Local bookkeeping ─────────────────────────────────────────────────────────

/** @param {Storage} storage @returns {Record<string, Meta>} */
function loadMeta(storage) {
  try {
    const m = JSON.parse(storage.getItem(META_KEY) ?? '{}')
    return m && typeof m === 'object' && !Array.isArray(m) ? m : {}
  } catch {
    return {}
  }
}

/** @param {Storage} storage @param {Record<string, Meta>} meta */
function saveMeta(storage, meta) {
  try { storage.setItem(META_KEY, JSON.stringify(meta)) } catch { /* next sync recomputes */ }
}

/** @param {Storage} storage */
function loadCursor(storage) {
  try { return Math.max(0, Number(storage.getItem(CURSOR_KEY)) || 0) } catch { return 0 }
}

/** @param {SyncKind} kind @param {string} id */
export const recordKey = (kind, id) => `${kind}:${id}`

/**
 * Server `updated_at` in ms (seconds are scaled up), for tombstones that carry
 * no plaintext.
 * @param {unknown} v
 */
function serverMs(v) {
  const n = typeof v === 'string' ? Date.parse(v) || Number(v) : Number(v)
  if (!Number.isFinite(n) || n <= 0) return 0
  return n < 1e12 ? n * 1000 : n
}

/**
 * Forget what this device knows about the server (a new key, another account),
 * keeping the change time of anything edited locally since its last sync so
 * those edits still compete fairly.
 * @param {Storage} storage
 */
export function resetSyncBookkeeping(storage) {
  const meta = loadMeta(storage)
  /** @type {Record<string, Meta>} */
  const kept = {}
  for (const [k, m] of Object.entries(meta)) {
    if (m.lh != null && m.lh !== m.h && m.at > 0) kept[k] = { v: 0, h: null, lh: null, at: m.at }
  }
  try {
    storage.setItem(META_KEY, JSON.stringify(kept))
    storage.removeItem(CURSOR_KEY)
    storage.removeItem(INIT_KEY)
  } catch { /* storage blocked */ }
  saveTombstones([])
}

/** Everything sync state, gone (sign-out). @param {Storage} storage */
export function clearSyncBookkeeping(storage) {
  try {
    storage.removeItem(META_KEY)
    storage.removeItem(CURSOR_KEY)
    storage.removeItem(INIT_KEY)
  } catch { /* storage blocked */ }
  saveTombstones([])
}

// ── Engine ────────────────────────────────────────────────────────────────────

/**
 * @param {{
 *   invoke: (cmd: string, args?: any) => Promise<any>,
 *   adapters: Adapters,
 *   storage?: Storage,
 *   now?: () => number,
 *   onApplied?: (kinds: Set<SyncKind>) => void,
 * }} deps
 */
export function createSyncEngine({ invoke, adapters, storage = globalThis.localStorage, now = () => Date.now(), onApplied }) {
  /** @returns {Map<string, LocalRecord>} */
  function collectLocal() {
    const out = new Map()
    for (const kind of KINDS) {
      for (const { id, data } of adapters[kind].list()) {
        if (typeof id === 'string' && ID_RE.test(id)) out.set(recordKey(kind, id), { kind, id, data })
      }
    }
    return out
  }

  /**
   * Note local edits since the last look, so each dirty record has a change
   * time to compare in a conflict. Before this device's first sync, changes
   * count as time 0: what is already on the server wins over fresh defaults.
   * @param {Record<string, Meta>} meta @param {Map<string, LocalRecord>} local
   */
  function observe(meta, local) {
    const initialized = storage.getItem(INIT_KEY) === '1'
    for (const [key, rec] of local) {
      const lh = hashData(rec.data)
      const m = meta[key]
      if (!m) meta[key] = { v: 0, h: null, lh, at: initialized ? now() : 0 }
      else if (m.lh !== lh) {
        m.lh = lh
        // Back to what the server has: not a change worth a time.
        m.at = lh === m.h ? m.at : initialized ? now() : 0
      }
    }
  }

  /**
   * Bring one server record into this device. Returns 'applied' | 'kept' | 'skipped'.
   * @param {RemoteRecord} rec @param {Record<string, Meta>} meta
   * @param {Map<string, LocalRecord>} local @param {Set<SyncKind>} touched
   */
  function resolve(rec, meta, local, touched) {
    const kind = rec.kind
    const adapter = adapters[kind]
    if (!adapter || !ID_RE.test(rec.id)) return 'skipped'
    const key = recordKey(kind, rec.id)
    const m = meta[key]
    // A conflict placeholder (version 0): the server has no such record (e.g.
    // after a reset). Start this record over at version 1; a pending delete of
    // it has nothing left to delete.
    if (rec.version === 0) {
      meta[key] = { v: 0, h: null, lh: m?.lh ?? null, at: m?.at ?? 0 }
      saveTombstones(loadTombstones().filter((t) => !(t.kind === kind && t.id === rec.id)))
      return 'skipped'
    }
    if (m && rec.version <= m.v) return 'skipped'

    /** @type {{ updated_at: number, data: any } | null} */
    let remote = null
    if (!rec.deleted) {
      try {
        remote = JSON.parse(rec.plaintext ?? '')
      } catch {
        return 'skipped'
      }
      if (!remote || typeof remote !== 'object' || remote.data == null) return 'skipped'
    }
    const remoteHash = remote ? hashData(remote.data) : null
    const remoteAt = remote ? Number(remote.updated_at) || 0 : serverMs(rec.updated_at)

    const tombs = loadTombstones()
    const tomb = tombs.find((t) => t.kind === kind && t.id === rec.id)
    const cur = local.get(key)
    const localHash = cur ? hashData(cur.data) : null
    const localDirty = tomb ? true : cur ? localHash !== (m?.h ?? null) : false
    const settle = () => {
      meta[key] = { v: rec.version, h: remoteHash, lh: remoteHash, at: remoteAt }
      if (tomb) saveTombstones(tombs.filter((t) => t !== tomb))
    }

    // Nothing to decide: same content, or both sides deleted.
    if ((cur && !tomb && localHash === remoteHash) || ((tomb || !cur) && !remote)) {
      settle()
      return 'skipped'
    }

    const localAt = tomb ? tomb.at : m?.at ?? 0
    if (!localDirty || remoteAt > localAt) {
      // The server's side wins. A losing connection edit is kept as a copy.
      // (At time 0 the local side was never edited here: nothing to keep.)
      if (localDirty && localAt > 0 && cur && remote && adapter.conflictCopy) withLocalChangesMuted(() => adapter.conflictCopy?.(cur.data))
      withLocalChangesMuted(() => (remote ? adapter.upsert(remote.data) : adapter.remove(rec.id)))
      if (remote) local.set(key, { kind, id: rec.id, data: remote.data })
      else local.delete(key)
      touched.add(kind)
      settle()
      return localDirty && localAt > 0 ? 'kept' : 'applied'
    }

    // This device's side wins: adopt the server version so the push lands on
    // top of it; the record stays dirty. A losing connection is kept as a copy.
    if (cur && remote && adapter.conflictCopy) {
      withLocalChangesMuted(() => adapter.conflictCopy?.(remote.data))
      touched.add(kind)
    }
    meta[key] = { v: rec.version, h: remoteHash, lh: localHash, at: localAt }
    return 'kept'
  }

  /** @param {Record<string, Meta>} meta @param {Map<string, LocalRecord>} local */
  function dirtyRecords(meta, local) {
    /** @type {{ id: string, kind: SyncKind, version: number, deleted: boolean, plaintext: string | null, hash: string | null }[]} */
    const out = []
    const tombs = loadTombstones()
    const keep = []
    for (const t of tombs) {
      const key = recordKey(t.kind, t.id)
      const m = meta[key]
      // Never reached the server, or already gone there: nothing to say.
      if (!m || !m.v || m.h == null || local.has(key)) continue
      keep.push(t)
      out.push({ id: t.id, kind: t.kind, version: m.v + 1, deleted: true, plaintext: null, hash: null })
    }
    if (keep.length !== tombs.length) saveTombstones(keep)
    for (const [key, rec] of local) {
      const m = meta[key]
      const hash = hashData(rec.data)
      if (m && m.v > 0 && m.h === hash) continue
      const at = m?.at || now()
      out.push({
        id: rec.id,
        kind: rec.kind,
        version: (m?.v ?? 0) + 1,
        deleted: false,
        plaintext: JSON.stringify({ updated_at: at, data: rec.data }),
        hash,
      })
    }
    return out
  }

  /** Pull, apply, push. @returns {Promise<SyncReport>} */
  async function run() {
    const meta = loadMeta(storage)
    let local = collectLocal()
    observe(meta, local)
    /** @type {Set<SyncKind>} */
    const touched = new Set()
    const report = { pulled: 0, pushed: 0, conflicts: 0, undecryptable: 0, items: 0 }

    try {
      // Pull everything newer than the cursor.
      let since = loadCursor(storage)
      for (;;) {
        const res = await invoke('sync_pull', { since })
        for (const rec of /** @type {RemoteRecord[]} */ (res?.records ?? [])) {
          if (rec.error) {
            report.undecryptable++
            continue
          }
          const r = resolve(rec, meta, local, touched)
          if (r !== 'skipped') report.pulled++
          if (r === 'kept') report.conflicts++
        }
        since = Math.max(since, Number(res?.next_since) || 0)
        saveMeta(storage, meta)
        try { storage.setItem(CURSOR_KEY, String(since)) } catch { /* re-pulls next time */ }
        if (!res?.more) break
      }
      // A conflict copy added records.
      if (touched.size) {
        local = collectLocal()
        observe(meta, local)
      }
      try { storage.setItem(INIT_KEY, '1') } catch { /* ignore */ }

      // Push what's dirty; a conflict is resolved and the loser pushed again.
      for (let round = 0; round < PUSH_ROUNDS; round++) {
        const dirty = dirtyRecords(meta, local)
        if (!dirty.length) break
        const res = await invoke('sync_push', {
          records: dirty.map(({ hash: _h, ...r }) => r),
        })
        for (const a of /** @type {{ id: string, version: number }[]} */ (res?.accepted ?? [])) {
          const sent = dirty.find((d) => d.id === a.id && d.version === a.version)
          if (!sent) continue
          const key = recordKey(sent.kind, sent.id)
          const prev = meta[key]
          meta[key] = { v: a.version, h: sent.hash, lh: sent.hash, at: prev?.at ?? 0 }
          if (sent.deleted) saveTombstones(loadTombstones().filter((t) => !(t.kind === sent.kind && t.id === sent.id)))
          report.pushed++
        }
        const conflicts = /** @type {RemoteRecord[]} */ (res?.conflicts ?? [])
        for (const rec of conflicts) {
          if (rec.error) {
            report.undecryptable++
            continue
          }
          if (rec.version > 0) report.conflicts++
          resolve(rec, meta, local, touched)
        }
        saveMeta(storage, meta)
        if (!conflicts.length) break
        local = collectLocal()
        observe(meta, local)
      }
    } finally {
      saveMeta(storage, meta)
      if (touched.size) onApplied?.(touched)
    }
    report.items = Object.values(meta).filter((m) => m.v > 0 && m.h != null).length
    return report
  }

  return { run, collectLocal }
}

// Local-change signals for sync (docs/accounts-and-sync.md, Appendix B).
//
// The stores that sync (connections, workspaces, settings, AI profiles) call
// `notifyLocalChange()` after they persist, and `recordDeletion()` when the
// user deletes something, so the engine can push a tombstone. This module has
// no imports on purpose: the stores import it, and the engine imports the
// stores, so it must not import either.

/** @typedef {'workspace' | 'connection' | 'settings'} SyncKind */
/** @typedef {{ kind: SyncKind, id: string, at: number }} Tombstone */

const TOMBSTONES_KEY = 'stroke:sync:tombstones'
/** Deleted ids are never reused; the cap only bounds the list. */
const MAX_TOMBSTONES = 1000

/** @type {Set<(kind?: SyncKind) => void>} */
const listeners = new Set()
let muted = 0

/** @param {(kind?: SyncKind) => void} cb @returns {() => void} */
export function onLocalChange(cb) {
  listeners.add(cb)
  return () => listeners.delete(cb)
}

/** A synced store changed on this device. Ignored while sync applies remote data. */
export function notifyLocalChange(/** @type {SyncKind | undefined} */ kind) {
  if (muted) return
  for (const cb of listeners) {
    try { cb(kind) } catch { /* a listener must not break a save */ }
  }
}

/**
 * Run `fn` with change signals and deletion records off: sync writing remote
 * data into the stores is not a local change.
 * @template T @param {() => T} fn @returns {T}
 */
export function withLocalChangesMuted(fn) {
  muted++
  try {
    return fn()
  } finally {
    muted--
  }
}

/** @returns {Tombstone[]} */
export function loadTombstones() {
  try {
    const list = JSON.parse(localStorage.getItem(TOMBSTONES_KEY) ?? '[]')
    return Array.isArray(list) ? list.filter((t) => t && typeof t.id === 'string' && typeof t.kind === 'string') : []
  } catch {
    return []
  }
}

/** @param {Tombstone[]} list */
export function saveTombstones(list) {
  try {
    if (list.length) localStorage.setItem(TOMBSTONES_KEY, JSON.stringify(list.slice(-MAX_TOMBSTONES)))
    else localStorage.removeItem(TOMBSTONES_KEY)
  } catch { /* the delete itself landed; sync just won't carry it */ }
}

/** The user deleted a synced record on this device. @param {SyncKind} kind @param {string} id */
export function recordDeletion(kind, id) {
  if (muted || !id) return
  const list = loadTombstones().filter((t) => !(t.kind === kind && t.id === id))
  list.push({ kind, id, at: Date.now() })
  saveTombstones(list)
  notifyLocalChange(kind)
}

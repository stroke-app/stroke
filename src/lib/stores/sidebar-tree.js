import { writable } from 'svelte/store'

/**
 * What the Tree sidebar (beta) remembers: the open nodes, per connection.
 *
 * Node ids are paths (see sidebar-tree.js), so one connection's set is just the
 * ids under its root. Kept apart from the Objects tab's open groups - the two
 * trees name their nodes differently and should not trample each other.
 */

const KEY = 'stroke:sidebar-tree'
/** A runaway set (every table of a 10k schema expanded) is trimmed to this. */
const MAX_PER_CONNECTION = 2000

/** @returns {Record<string, string[]>} */
function readAll() {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? '{}')
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {}
  } catch {
    return {}
  }
}

/**
 * The ids open under a connection, or null when nothing was saved for it.
 * @param {string} connectionId
 * @returns {string[] | null}
 */
export function loadTreeExpanded(connectionId) {
  if (!connectionId) return null
  const list = readAll()[connectionId]
  return Array.isArray(list) ? list.filter((x) => typeof x === 'string') : null
}

/**
 * @param {string} connectionId
 * @param {Iterable<string>} ids
 */
export function saveTreeExpanded(connectionId, ids) {
  if (!connectionId) return
  const all = readAll()
  all[connectionId] = [...ids].slice(-MAX_PER_CONNECTION)
  try {
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch { /* storage full or blocked: the tree just starts closed next time */ }
}

// ── Which connections the tree lists ─────────────────────────────────────────


const SCOPE_KEY = 'stroke:sidebar-tree-connections'

/** @typedef {'all' | 'connected'} TreeConnectionScope */

/** @returns {TreeConnectionScope} */
function readScope() {
  try { return localStorage.getItem(SCOPE_KEY) === 'connected' ? 'connected' : 'all' } catch { return 'all' }
}

/**
 * Every saved connection as a root of the tree ('all'), or only the open one
 * with the rest in the Connections list under it ('connected').
 */
export const treeConnectionScope = writable(/** @type {TreeConnectionScope} */ (readScope()))
treeConnectionScope.subscribe((v) => {
  try { localStorage.setItem(SCOPE_KEY, v) } catch { /* storage blocked */ }
})

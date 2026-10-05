/**
 * Saved-query folders: the tree the sidebar's Queries tab draws and the moves
 * it makes. A query carries a `folderId`; a connection's folders are a list
 * kept beside the queries (stores/query-folders.js). A query that predates
 * folders, or whose folder is gone, sits at the root, so none is ever hidden.
 */

/** @typedef {{ id: string, name: string }} QueryFolder */
/** @typedef {import('$lib/stores/query-history.js').SavedQuery} SavedQuery */
/** @typedef {{ root: SavedQuery[], folders: Array<{ folder: QueryFolder, queries: SavedQuery[] }> }} QueryTree */

/** @param {{ name: string }} a @param {{ name: string }} b */
const byName = (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })

/**
 * Whatever was persisted, as a clean folder list: unnamed ones get a name,
 * malformed and repeated ids are dropped.
 * @param {unknown} raw
 * @returns {QueryFolder[]}
 */
export function normalizeFolders(raw) {
  if (!Array.isArray(raw)) return []
  const seen = new Set()
  /** @type {QueryFolder[]} */
  const out = []
  for (const f of raw) {
    if (!f || typeof f !== 'object' || typeof f.id !== 'string' || !f.id || seen.has(f.id)) continue
    seen.add(f.id)
    out.push({ id: f.id, name: typeof f.name === 'string' && f.name.trim() ? f.name.trim() : 'Folder' })
  }
  return out
}

/**
 * The folder a query is shown in: its own when that folder exists, else none.
 * @param {SavedQuery} q @param {Set<string>} known
 */
export const folderOf = (q, known) => (q.folderId && known.has(q.folderId) ? q.folderId : null)

/**
 * Folders (by name) with their queries (by name), then the queries at the
 * root. With a filter, a query shows when its name or text matches, and a
 * folder when its name or one of its queries does; a folder matched by name
 * shows all of its queries.
 * @param {SavedQuery[]} queries @param {QueryFolder[]} folders @param {string} [filter]
 * @returns {QueryTree}
 */
export function buildQueryTree(queries, folders, filter = '') {
  const q = filter.trim().toLowerCase()
  /** @param {SavedQuery} s */
  const hit = (s) => !q || s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q)
  const known = new Set(folders.map((f) => f.id))
  /** @type {Map<string, SavedQuery[]>} */
  const inFolder = new Map(folders.map((f) => [f.id, []]))
  /** @type {SavedQuery[]} */
  const root = []
  for (const s of queries) {
    const fid = folderOf(s, known)
    if (fid) /** @type {SavedQuery[]} */ (inFolder.get(fid)).push(s)
    else if (hit(s)) root.push(s)
  }
  const groups = [...folders].sort(byName).flatMap((folder) => {
    const all = /** @type {SavedQuery[]} */ (inFolder.get(folder.id))
    const shown = !q || folder.name.toLowerCase().includes(q) ? all : all.filter(hit)
    return !q || shown.length || folder.name.toLowerCase().includes(q) ? [{ folder, queries: [...shown].sort(byName) }] : []
  })
  return { root: root.sort(byName), folders: groups }
}

/**
 * "Untitled query N", one past the highest N in use.
 * @param {Array<{ name: string }>} queries
 */
export function nextUntitledName(queries) {
  let n = 0
  for (const s of queries) {
    const m = /^Untitled query (\d+)$/.exec(s.name.trim())
    if (m) n = Math.max(n, Number(m[1]))
  }
  return `Untitled query ${n + 1}`
}

/**
 * A name for a copy: "x copy", then "x copy 2" and on.
 * @param {string} name @param {Array<{ name: string }>} queries
 */
export function copyName(name, queries) {
  const taken = new Set(queries.map((s) => s.name))
  const base = `${name} copy`
  if (!taken.has(base)) return base
  let i = 2
  while (taken.has(`${base} ${i}`)) i++
  return `${base} ${i}`
}

/**
 * A folder name not taken yet: "New folder", then "New folder 2" and on.
 * @param {QueryFolder[]} folders @param {string} [base]
 */
export function uniqueFolderName(folders, base = 'New folder') {
  const taken = new Set(folders.map((f) => f.name.toLowerCase()))
  if (!taken.has(base.toLowerCase())) return base
  let i = 2
  while (taken.has(`${base} ${i}`.toLowerCase())) i++
  return `${base} ${i}`
}

/**
 * Deleting a folder: by default its queries move to the root; only an explicit
 * `withQueries` deletes them.
 * @param {SavedQuery[]} queries @param {string} folderId @param {boolean} [withQueries]
 * @returns {{ toRoot: string[], remove: string[] }} query ids
 */
export function planFolderDelete(queries, folderId, withQueries = false) {
  const ids = queries.filter((s) => s.folderId === folderId).map((s) => s.id)
  return withQueries ? { toRoot: [], remove: ids } : { toRoot: ids, remove: [] }
}

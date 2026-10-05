/**
 * A connection's saved-query folders, in localStorage beside the other
 * per-connection lists (chart groups, diagrams). The queries themselves stay in
 * IndexedDB and point at a folder by id; see query-folders.js for the tree.
 */
import { normalizeFolders } from '$lib/query-folders.js'

/** @param {string | null | undefined} connId */
export const queryFoldersKey = (connId) => `stroke:query-folders:${connId || '_default'}`

/**
 * @param {string | null | undefined} connId
 * @returns {import('$lib/query-folders.js').QueryFolder[]}
 */
export function loadQueryFolders(connId) {
  try {
    return normalizeFolders(JSON.parse(localStorage.getItem(queryFoldersKey(connId)) ?? '[]'))
  } catch {
    return []
  }
}

/**
 * @param {string | null | undefined} connId
 * @param {import('$lib/query-folders.js').QueryFolder[]} folders
 */
export function saveQueryFolders(connId, folders) {
  try {
    if (folders.length) localStorage.setItem(queryFoldersKey(connId), JSON.stringify(normalizeFolders(folders)))
    else localStorage.removeItem(queryFoldersKey(connId))
  } catch {
    // Quota/serialization failure must not throw into the sidebar.
  }
}

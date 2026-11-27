import { get, writable } from 'svelte/store'
import { debounce } from '$lib/utils.js'

/** @typedef {{ id: string, name: string, code: string, group: string, createdAt: number, updatedAt: number }} SavedDiagram */

const KEY = (id) => id ? `stroke:saved-diagrams:${id}` : 'stroke:saved-diagrams'

function load(connId) {
  if (!connId) return []
  try { const r = localStorage.getItem(KEY(connId)); return r ? JSON.parse(r) : [] } catch { return [] }
}

function persist(v, connId) {
  if (!connId) return
  try { localStorage.setItem(KEY(connId), JSON.stringify(v)) } catch {}
}

function uid() {
  return `diag-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`
}

let _connId = ''

export const savedDiagrams = writable(/** @type {SavedDiagram[]} */ ([]))

// Debounced: diagram editing (node drags, code edits) fires rapidly; one
// JSON.stringify + localStorage write per change janks the main thread.
const persistDebounced = debounce((/** @type {SavedDiagram[]} */ v, /** @type {string} */ connId) => persist(v, connId), 300)
savedDiagrams.subscribe((v) => { persistDebounced(v, _connId) })
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => persistDebounced.flush())
}

/** Switch active connection. Loads diagrams for the given connection. */
export function switchDiagramsConnection(connectionId) {
  // Write out any pending edit of the OLD connection first - the set() below
  // re-arms the debounce with the new connection's data, dropping it otherwise.
  persistDebounced.flush()
  _connId = connectionId ?? ''
  savedDiagrams.set(load(_connId))
}

export function saveDiagram(name, code, group = 'AI Generated') {
  const d = { id: uid(), name, code, group, createdAt: Date.now(), updatedAt: Date.now() }
  savedDiagrams.update((all) => [d, ...all])
  return d
}

export function updateDiagram(id, patch) {
  savedDiagrams.update((all) =>
    all.map((d) => (d.id === id ? { ...d, ...patch, updatedAt: Date.now() } : d))
  )
}

/**
 * @param {string} id
 * @returns {(() => boolean) | null} puts it back in its place, for an Undo; false
 *   once the diagrams on screen belong to another connection
 */
export function deleteDiagram(id) {
  const list = get(savedDiagrams)
  const index = list.findIndex((d) => d.id === id)
  if (index < 0) return null
  const diagram = list[index]
  const conn = _connId
  savedDiagrams.update((all) => all.filter((d) => d.id !== id))
  return () => {
    if (_connId !== conn || get(savedDiagrams).some((d) => d.id === id)) return false
    savedDiagrams.update((all) => [...all.slice(0, index), diagram, ...all.slice(index)])
    return true
  }
}

import { writable } from 'svelte/store'

const KEY = 'stroke:vcols-v1'

/** @typedef {{ id: string, name: string, expression: string, enabled: boolean }} VColDef */
/** @typedef {Record<string, VColDef[]>} VColMap */

function load() {
  try { return /** @type {VColMap} */ (JSON.parse(localStorage.getItem(KEY) ?? '{}')) }
  catch { return {} }
}

function persist(data) {
  try { localStorage.setItem(KEY, JSON.stringify(data)) } catch {}
}

function create() {
  const { subscribe, update } = writable(/** @type {VColMap} */ (load()))

  return {
    subscribe,
    /** @param {string} tk - "schema.table" @param {Omit<VColDef,'id'>} def */
    add(tk, def) {
      update(d => {
        const next = { ...d, [tk]: [...(d[tk] ?? []), { ...def, id: crypto.randomUUID() }] }
        persist(next); return next
      })
    },
    /** @param {string} tk @param {string} id @param {Partial<VColDef>} patch */
    patch(tk, id, patch) {
      update(d => {
        const next = { ...d, [tk]: (d[tk] ?? []).map(c => c.id === id ? { ...c, ...patch } : c) }
        persist(next); return next
      })
    },
    /**
     * @param {string} tk @param {string} id
     * @returns {(() => boolean) | null} puts the column back in its place, for an Undo
     */
    remove(tk, id) {
      /** @type {VColDef | undefined} */
      let gone
      let index = -1
      update(d => {
        const list = d[tk] ?? []
        index = list.findIndex(c => c.id === id)
        gone = list[index]
        const next = { ...d, [tk]: list.filter(c => c.id !== id) }
        persist(next); return next
      })
      if (!gone || index < 0) return null
      const def = gone
      return () => {
        let put = false
        update(d => {
          const list = d[tk] ?? []
          if (list.some(c => c.id === def.id)) return d
          put = true
          const next = { ...d, [tk]: [...list.slice(0, index), def, ...list.slice(index)] }
          persist(next); return next
        })
        return put
      }
    },
  }
}

export const virtualColumnsStore = create()

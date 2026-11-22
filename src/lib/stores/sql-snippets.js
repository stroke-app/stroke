/**
 * SQL snippets of your own: a name, the trigger word typed in the editor, and
 * a body with tab stops (`${1:table}` is a field Tab walks to, the same syntax
 * as the built-in snippets in sql-complete-data.js). Kept in localStorage,
 * offered by the SQL editor's autocomplete next to the built-in ones, and
 * moved between machines as JSON (Settings > Snippets).
 */
import { writable, get } from 'svelte/store'

const STORAGE_KEY = 'stroke:sql-snippets'

/**
 * @typedef {{
 *   id: string,
 *   name: string,
 *   trigger: string,
 *   body: string,
 *   description: string,
 *   scope: string[],
 * }} UserSnippet
 * `scope` lists completion families (sqlFamily's values); empty means every engine.
 */

/** The engines a snippet can be limited to, as the editor's completion knows them. */
export const SNIPPET_SCOPES = [
  { id: 'postgres', label: 'PostgreSQL' },
  { id: 'mysql', label: 'MySQL' },
  { id: 'sqlite', label: 'SQLite' },
  { id: 'mssql', label: 'SQL Server' },
  { id: 'clickhouse', label: 'ClickHouse' },
  { id: 'duckdb', label: 'DuckDB' },
]
const SCOPE_IDS = new Set(SNIPPET_SCOPES.map((s) => s.id))

/** A trigger is one word the editor can complete: letters, digits, `_` and `-`. */
export const TRIGGER_RE = /^[A-Za-z_][A-Za-z0-9_-]{0,31}$/

function newId() {
  try { return crypto.randomUUID() } catch { return `s${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}` }
}

/**
 * A snippet as stored, from whatever was read or imported; null when it has no
 * name, no usable trigger or no body.
 * @param {any} raw
 * @returns {UserSnippet | null}
 */
export function normalizeSnippet(raw) {
  if (!raw || typeof raw !== 'object') return null
  const name = String(raw.name ?? '').trim()
  const trigger = String(raw.trigger ?? raw.prefix ?? '').trim()
  const body = String(raw.body ?? raw.sql ?? '').replace(/\s+$/, '')
  if (!name || !TRIGGER_RE.test(trigger) || !body.trim()) return null
  const scope = Array.isArray(raw.scope) ? raw.scope.map(String).filter((s) => SCOPE_IDS.has(s)) : []
  return {
    id: typeof raw.id === 'string' && raw.id ? raw.id : newId(),
    name: name.slice(0, 80),
    trigger,
    body,
    description: String(raw.description ?? '').trim().slice(0, 200),
    scope: [...new Set(scope)],
  }
}

/** @returns {UserSnippet[]} */
function load() {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]')
    return Array.isArray(raw) ? raw.map(normalizeSnippet).filter((s) => s !== null) : []
  } catch {
    return []
  }
}

export const userSnippets = writable(load())
userSnippets.subscribe((list) => {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(list)) } catch { /* storage full or off: kept for this session */ }
})

/** @param {Omit<UserSnippet, 'id'> & { id?: string }} snippet @returns {UserSnippet | null} */
export function saveSnippet(snippet) {
  const s = normalizeSnippet(snippet)
  if (!s) return null
  userSnippets.update((list) => {
    const i = list.findIndex((x) => x.id === s.id)
    if (i === -1) return [...list, s]
    const next = list.slice()
    next[i] = s
    return next
  })
  return s
}

/** @param {string} id */
export function removeSnippet(id) {
  userSnippets.update((list) => list.filter((s) => s.id !== id))
}

/**
 * The snippets that apply to a completion family.
 * @param {UserSnippet[]} list @param {string} family
 */
export function snippetsFor(list, family) {
  return list.filter((s) => s.scope.length === 0 || s.scope.includes(family))
}

/** @param {UserSnippet[]} list */
export function snippetsToJson(list) {
  return JSON.stringify(
    { format: 'stroke-sql-snippets', version: 1, snippets: list.map(({ id: _id, ...s }) => s) },
    null,
    2,
  )
}

/**
 * Snippets from an exported file: this app's format, or a bare array of
 * `{ name, trigger | prefix, body | sql }` objects.
 * @param {string} text
 * @returns {{ snippets: UserSnippet[], skipped: number } | { error: string }}
 */
export function snippetsFromJson(text) {
  let data
  try {
    data = JSON.parse(text)
  } catch {
    return { error: 'That file is not JSON.' }
  }
  const items = Array.isArray(data) ? data : Array.isArray(data?.snippets) ? data.snippets : null
  if (!items) return { error: 'No snippets in that file: expected a "snippets" list.' }
  const snippets = items.map((raw) => normalizeSnippet({ ...raw, id: undefined })).filter((s) => s !== null)
  return { snippets, skipped: items.length - snippets.length }
}

/**
 * Add imported snippets. One with the same trigger and name as a snippet
 * already here replaces it (re-importing a file updates it rather than
 * doubling every entry); anything else is added.
 * @param {UserSnippet[]} incoming
 * @returns {{ added: number, replaced: number }}
 */
export function importSnippets(incoming) {
  let added = 0
  let replaced = 0
  userSnippets.update((list) => {
    const next = list.slice()
    for (const s of incoming) {
      const i = next.findIndex((x) => x.trigger === s.trigger && x.name === s.name)
      if (i === -1) { next.push(s); added++ }
      else { next[i] = { ...s, id: next[i].id }; replaced++ }
    }
    return next
  })
  return { added, replaced }
}

/** Current list, for code outside components. */
export const currentSnippets = () => get(userSnippets)

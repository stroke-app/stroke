// Foreign key labels - `category_id = 8` reads `8 · Electronics`.
//
// The label is the referenced row's most human column (name, title, label,
// email, ...). Lookups are batched: every miss during a repaint is queued, and
// one request per referenced table fetches all of them (OR-ed `eq` filters
// through the same table-rows command the grid uses, so quoting and
// parameters are the backend's). When the answers land, `fkLabelVersion` ticks
// and the grid repaints with them. Misses are remembered too, so a value with
// no row (or a table with no label column) is asked about once.
import { writable } from 'svelte/store'
import { getTableRows } from '$lib/api.js'

/** Ticks when new labels land, so the grid repaints. */
export const fkLabelVersion = writable(0)

/** Column names tried in order for the label. */
const LABEL_NAMES = [
  'name', 'display_name', 'full_name', 'title', 'label', 'username', 'user_name', 'login', 'email',
  'slug', 'handle', 'code', 'sku', 'subject', 'headline', 'company', 'company_name', 'first_name', 'description',
]
const TEXT_TYPE = /char|text|string|varchar|citext|name|enum/i
const BATCH = 100

/** target key → (value → label | null). @type {Map<string, Map<string, string | null>>} */
const cache = new Map()
/** target key → values queued for the next flush. @type {Map<string, { fk: any, values: Set<string> }>} */
const queue = new Map()
/** Targets with no usable label column: never asked again. */
const noLabel = new Set()
let flushTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)

/** Forget everything (another connection, or a schema change). */
export function clearFkLabels() {
  cache.clear()
  queue.clear()
  noLabel.clear()
}

/** @param {any} fk */
function targetOf(fk) {
  const schema = fk.referencedSchema ?? fk.referenced_schema ?? ''
  const table = fk.referencedTable ?? fk.referenced_table
  const col = (fk.referencedColumns ?? fk.referenced_columns ?? [])[0]
  return { schema, table, col, key: `${schema}\u0000${table}\u0000${col}` }
}

/** @param {{ name: string, dataType?: string, data_type?: string }[]} cols @param {string} keyCol */
function pickLabelColumn(cols, keyCol) {
  const byName = new Map(cols.map((c, i) => [c.name.toLowerCase(), i]))
  for (const n of LABEL_NAMES) {
    const i = byName.get(n)
    if (i !== undefined && cols[i].name !== keyCol) return i
  }
  // first_name + last_name read better together than either alone.
  // Otherwise the first text column that is not the key.
  const i = cols.findIndex((c) => c.name !== keyCol && TEXT_TYPE.test(String(c.dataType ?? c.data_type ?? '')) && !/(^|_)(id|uuid|hash|token|password|secret)$/i.test(c.name))
  return i
}

async function flush() {
  flushTimer = null
  const jobs = [...queue.values()]
  queue.clear()
  let landed = false
  await Promise.all(jobs.map(async ({ fk, values }) => {
    const t = targetOf(fk)
    const known = cache.get(t.key) ?? new Map()
    cache.set(t.key, known)
    const all = [...values]
    for (let i = 0; i < all.length; i += BATCH) {
      const chunk = all.slice(i, i + BATCH)
      try {
        const res = await getTableRows(t.schema, t.table, chunk.length, 0, {
          filters: chunk.map((v, j) => ({ column: t.col, op: 'eq', value: v, conjunct: j === 0 ? undefined : 'or' })),
          includeMeta: false,
          includeCount: false,
        })
        const cols = res?.columns ?? []
        const keyIdx = cols.findIndex((/** @type {any} */ c) => c.name === t.col)
        const labelIdx = pickLabelColumn(cols, t.col)
        if (keyIdx < 0 || labelIdx < 0) { noLabel.add(t.key); for (const v of chunk) known.set(v, null); continue }
        const first = cols.findIndex((/** @type {any} */ c) => c.name.toLowerCase() === 'first_name')
        const last = cols.findIndex((/** @type {any} */ c) => c.name.toLowerCase() === 'last_name')
        for (const v of chunk) known.set(v, null)
        for (const row of res?.rows ?? []) {
          const k = row[keyIdx]
          if (k == null) continue
          let label = row[labelIdx]
          if (cols[labelIdx].name.toLowerCase() === 'first_name' && last >= 0 && row[last]) label = `${row[first]} ${row[last]}`
          known.set(String(k), label == null || label === '' ? null : String(label))
        }
        landed = true
      } catch {
        // No access, a dropped table, an engine without filters: stop asking.
        noLabel.add(t.key)
        for (const v of chunk) known.set(v, null)
      }
    }
  }))
  if (landed) fkLabelVersion.update((n) => n + 1)
}

/**
 * The label for a foreign-key value, or undefined while it is being fetched.
 * @param {any} fk @param {unknown} value
 * @returns {string | null | undefined}
 */
export function fkLabel(fk, value) {
  if (!fk || (fk.columns?.length ?? 1) !== 1) return null
  const t = targetOf(fk)
  if (!t.table || !t.col || noLabel.has(t.key)) return null
  const v = String(value)
  const hit = cache.get(t.key)?.get(v)
  if (hit !== undefined) return hit
  let q = queue.get(t.key)
  if (!q) { q = { fk, values: new Set() }; queue.set(t.key, q) }
  q.values.add(v)
  if (!flushTimer) flushTimer = setTimeout(() => void flush(), 40)
  return undefined
}

const defaultConfig = { style: 'trail' } // 'trail' (8 · Electronics) | 'replace' (Electronics)

export const fkLabels = {
  id: 'fk-labels',
  name: 'Foreign Key Labels',
  description: 'Show the referenced row\'s name next to a foreign key id: 8 · Electronics.',
  kind: 'formatter',
  defaultConfig,

  appliesTo() { return true },

  /**
   * @param {unknown} value @param {string} _type @param {Record<string, unknown>} config @param {string} _name
   * @param {{ fk?: any } | undefined} ctx
   */
  format(value, _type, config, _name, ctx) {
    const fk = ctx?.fk
    if (!fk) return null
    const label = fkLabel(fk, value)
    if (!label) return null
    const table = fk.referencedTable ?? fk.referenced_table
    const title = `${table} · ${label}`
    if ((config?.style ?? defaultConfig.style) === 'replace') return { display: label, title: `${title} (id ${value})` }
    return { trail: { text: `· ${label}` }, title }
  },
}

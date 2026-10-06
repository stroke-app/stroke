import { quoteName, tableRef } from './sql-ident.js'
import { normalizeForeignKeys } from '$lib/foreign-key-nav.js'

/** Sentinel used when the filter should match across every column. */
export const ANY_COLUMN = '__any__'

/**
 * Map a raw `getTableRows` response (which may use camelCase or snake_case keys)
 * into the canonical row-result fields the UI stores. Shared by both the active
 * grid loader and the per-tab prefetch so the snake/camel fallbacks live in one
 * place. Callers layer their own metadata-skip / column-shape logic on top.
 * @param {any} data
 * @returns {{ columns: any[], primaryKey: string[], foreignKeys: any[], rows: any[][], total: number, queryMs: number }}
 */
export function readRowsResponse(data) {
  return {
    columns: data.columns ?? [],
    primaryKey: data.primaryKey ?? data.primary_key ?? [],
    foreignKeys: normalizeForeignKeys(data.foreignKeys ?? data.foreign_keys),
    rows: data.rows ?? [],
    total: Number(data.total ?? 0),
    queryMs: Number(data.queryMs ?? data.query_ms ?? 0),
    // Columns this page fetched as a preview instead of a value. Empty on every
    // ordinary table; when it is not, the toolbar says so - a column quietly
    // showing "369 KB" where its contents should be is a bug report waiting to
    // happen.
    previewColumns: (data.previewColumns ?? data.preview_columns ?? []).map(
      (/** @type {any} */ c) => ({ name: String(c.name ?? ''), avgBytes: Number(c.avgBytes ?? c.avg_bytes ?? 0) }),
    ),
  }
}

/** @typedef {'asc' | 'desc'} SortDirection */

/** @typedef {{ column: string, direction: SortDirection }} TableSort */

/** @typedef {'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte' | 'contains' | 'not_contains' | 'starts_with' | 'ends_with' | 'is_null' | 'is_not_null' | 'between'} FilterOp */

/** @typedef {{ id: string, column: string, op: FilterOp, value: string, conjunct: 'and' | 'or' }} TableFilter */

export const PAGE_SIZE_OPTIONS = [25, 50, 100, 250, 1_000, 1_000_000, 5_000_000, -1]

export const DEFAULT_PAGE_SIZE = 50

export const MAX_PAGE_SIZE = 5_000_000

/** Sentinel page size meaning "load all rows" (uses the table's total count). */
export const PAGE_SIZE_ALL = -1

/** Compact label for a page size: "All", "5M", "1M", or the plain number. */
export function pageSizeLabel(n) {
  if (n === PAGE_SIZE_ALL) return 'All'
  if (n >= 1_000_000 && n % 1_000_000 === 0) return `${n / 1_000_000}M`
  return String(n)
}

// ── User-configurable default page size (persisted across sessions) ──────────
// When the user changes rows-per-page, that value becomes the default for every
// new table tab. Stored in localStorage so it survives restarts.
const DEFAULT_PAGE_SIZE_KEY = 'stroke:default-page-size'

/**
 * Rows to ask the backend for, given the page size the user picked.
 *
 * "All" has no number of its own, so it borrows the row count - and that is only
 * sound while the count describes the view about to be fetched. Filter a table
 * down to one row and then clear the filter: the count is still 1, and "All"
 * would ask for LIMIT 1 and draw a one-row grid under a footer reading
 * "1-752 of 752". When the count belongs to some other predicate, ask for the
 * ceiling instead and let the probe/window path size the view from a fresh count.
 *
 * @param {number} pageSize the chosen size, or PAGE_SIZE_ALL
 * @param {number} total last known row count (0 / -1 when unknown or counting)
 * @param {boolean} totalIsForThisView `total` was counted under the search and
 *   filters now applied
 */
export function fetchLimitFor(pageSize, total, totalIsForThisView) {
  if (pageSize !== PAGE_SIZE_ALL) return pageSize
  return totalIsForThisView && total > 0 ? Math.min(total, MAX_PAGE_SIZE) : MAX_PAGE_SIZE
}

/**
 * The page after a row is added at its top, as the grid shows an insert.
 *
 * A page of a fixed size keeps its size: the last row moves on to the next
 * page. "All" has no next page, so it grows by the row. It used to be cut to
 * its fetch limit like any page, and with "All" that limit is the row count
 * from before the insert: the new row went in at the top and the last row
 * dropped off the bottom, so the grid no longer matched the table.
 * @template T
 * @param {T[]} rows the page as shown
 * @param {T} row the inserted row
 * @param {number} pageSize the chosen size, or PAGE_SIZE_ALL
 * @param {number} limit the page's fetch limit (fetchLimitFor)
 * @returns {T[]}
 */
export function pageWithInsertedRow(rows, row, pageSize, limit) {
  const next = [row, ...rows]
  return pageSize !== PAGE_SIZE_ALL && next.length > limit ? next.slice(0, limit) : next
}

/** Clamp an arbitrary input to a valid page size (or the "All" sentinel). */
export function clampPageSize(/** @type {unknown} */ n) {
  const num = Number(n)
  if (num === PAGE_SIZE_ALL) return PAGE_SIZE_ALL
  if (!Number.isFinite(num) || num < 1) return DEFAULT_PAGE_SIZE
  return Math.min(Math.floor(num), MAX_PAGE_SIZE)
}

/** The user's saved default rows-per-page, or the built-in default. */
export function loadDefaultPageSize() {
  try {
    const raw = localStorage.getItem(DEFAULT_PAGE_SIZE_KEY)
    if (raw == null) return DEFAULT_PAGE_SIZE
    return clampPageSize(raw)
  } catch {
    return DEFAULT_PAGE_SIZE
  }
}

/** Persist the default rows-per-page for future tabs/sessions. @param {number} n */
export function saveDefaultPageSize(n) {
  try {
    localStorage.setItem(DEFAULT_PAGE_SIZE_KEY, String(clampPageSize(n)))
  } catch { /* storage unavailable - non-fatal */ }
}

export const FILTER_OPS = /** @type {{ value: FilterOp, label: string, needsValue: boolean }[]} */ ([
  { value: 'eq', label: 'equals', needsValue: true },
  { value: 'neq', label: 'not equal', needsValue: true },
  { value: 'contains', label: 'contains', needsValue: true },
  { value: 'not_contains', label: 'does not contain', needsValue: true },
  { value: 'starts_with', label: 'starts with', needsValue: true },
  { value: 'ends_with', label: 'ends with', needsValue: true },
  { value: 'gt', label: 'greater than', needsValue: true },
  { value: 'gte', label: 'greater or equal', needsValue: true },
  { value: 'lt', label: 'less than', needsValue: true },
  { value: 'lte', label: 'less or equal', needsValue: true },
  { value: 'between', label: 'is between', needsValue: true },
  { value: 'is_null', label: 'is null', needsValue: false },
  { value: 'is_not_null', label: 'is not null', needsValue: false },
])

/** Ops shown for boolean columns */
export const BOOL_FILTER_OPS = FILTER_OPS.filter((o) =>
  ['eq', 'neq', 'is_null', 'is_not_null'].includes(o.value),
)

/** Ops shown for date/timestamp columns */
export const DATE_FILTER_OPS = FILTER_OPS.filter((o) =>
  ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'between', 'is_null', 'is_not_null'].includes(o.value),
)

/** Ops shown for numeric columns */
export const NUM_FILTER_OPS = FILTER_OPS.filter((o) =>
  ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'between', 'is_null', 'is_not_null'].includes(o.value),
)

let filterSeq = 0

export function nextFilterId() {
  filterSeq += 1
  return `filter-${filterSeq}`
}

/** @returns {TableFilter} */
export function createFilter(column = '', op = /** @type {FilterOp} */ ('contains')) {
  return { id: nextFilterId(), column, op, value: '', conjunct: 'and' }
}

/** @param {TableFilter[]} filters */
export function activeFilters(filters) {
  return filters.filter((f) => {
    if (!f.column) return false
    const op = FILTER_OPS.find((o) => o.value === f.op)
    if (!op) return false
    // any-column only makes sense with a value; null-checks don't apply
    if (f.column === ANY_COLUMN) return op.needsValue && f.value.trim().length > 0
    if (!op.needsValue) return true
    return f.value.trim().length > 0
  })
}

/**
 * @param {TableFilter[]} filters
 * @param {{ name: string, dataType?: string }[]} [columns] - when provided, the
 *   column's dataType is included so the backend can cast parameters correctly
 *   and keep filter conditions SARGable (index-eligible) on typed columns.
 */
export function filtersForApi(filters, columns) {
  const typeMap = columns
    ? Object.fromEntries(columns.map((c) => [c.name, c.dataType ?? c.data_type]))
    : {}
  return activeFilters(filters).map(({ column, op, value, conjunct }, i) => ({
    column,
    op,
    value: value.trim() || undefined,
    conjunct: i === 0 ? undefined : (conjunct ?? 'and'),
    dataType: column !== ANY_COLUMN ? (typeMap[column] || undefined) : undefined,
  }))
}

/** Stable key for comparing applied (API) filter state. */
export function filtersApiSignature(filters) {
  return JSON.stringify(filtersForApi(filters))
}

/**
 * @param {TableSort | null} sort primary sort key
 * @param {TableSort[]} [more] additional sort keys (multi-column sort)
 * The primary key is always sent as sortColumn/sortDirection (so engines that
 * don't support multi-sort still order by it); the full ordered list is sent as
 * `sorts` only when there's more than one key.
 */
export function sortForApi(sort, more = []) {
  const all = [sort, ...(more ?? [])].filter((s) => s?.column)
  if (all.length === 0) return { sortColumn: undefined, sortDirection: undefined, sorts: undefined }
  return {
    sortColumn: all[0].column,
    sortDirection: all[0].direction,
    sorts: all.length > 1 ? all.map((s) => ({ column: s.column, direction: s.direction })) : undefined,
  }
}

/** @param {string} search @param {TableFilter[]} filters @param {TableSort | null} sort */
export function hasTableQuery(search, filters, sort) {
  return (
    search.trim().length > 0 ||
    activeFilters(filters).length > 0 ||
    Boolean(sort?.column)
  )
}

/**
 * Build a SELECT statement that reflects the current table view (columns,
 * filters, search, sort). Meant as an editable starting point in the SQL editor,
 * NOT a parameterized query - values are inlined and escaped for display.
 *
 * @param {object} opts
 * @param {string} [opts.schema]
 * @param {string} opts.table
 * @param {{ name: string }[]} [opts.columns]   - when given, emits an explicit column list
 * @param {TableFilter[]} [opts.filters]
 * @param {string} [opts.search]
 * @param {TableSort | null} [opts.sort]
 * @param {number} [opts.limit]
 * @param {string} [opts.engine]                - connection type, for quoting/casing
 * @param {import('./sql-ident.js').QuoteMode} [opts.quote] - quote names always, only where needed, or never
 * @param {boolean} [opts.qualify]               - false leaves out the engine's default schema
 * @returns {string}
 */
export function buildSelectSql({ schema, table, columns = [], filters = [], search = '', sort = null, limit = 100, engine = 'postgres', quote = 'always', qualify = true }) {
  const mysql = engine === 'mysql' || engine === 'mariadb'
  const pg = !mysql // postgres/sqlite/etc. use double-quote identifiers + ILIKE-ish
  /** quote identifier, per Settings → SQL editor → Quote object names */
  const q = (/** @type {string} */ id) => quoteName(id, engine, quote)
  /** quote a string literal */
  const lit = (/** @type {string} */ v) => `'${String(v).replace(/'/g, "''")}'`
  const like = pg ? 'ILIKE' : 'LIKE'
  /** text-cast a column for substring search */
  const asText = (/** @type {string} */ col) => (engine === 'postgres' ? `${col}::text` : col)

  const target = tableRef({ schema, table, engine, mode: quote, qualify })
  // SELECT * keeps the generated query clean and avoids referencing virtual /
  // hidden columns. `columns` is still used below to expand the search clause.
  const cols = '*'

  /** @param {TableFilter} f */
  const cond = (f) => {
    const col = q(f.column)
    const v = f.value.trim()
    switch (f.op) {
      case 'eq': return `${col} = ${lit(v)}`
      case 'neq': return `${col} <> ${lit(v)}`
      case 'gt': return `${col} > ${lit(v)}`
      case 'gte': return `${col} >= ${lit(v)}`
      case 'lt': return `${col} < ${lit(v)}`
      case 'lte': return `${col} <= ${lit(v)}`
      case 'contains': return `${asText(col)} ${like} ${lit('%' + v + '%')}`
      case 'not_contains': return `${asText(col)} NOT ${like} ${lit('%' + v + '%')}`
      case 'starts_with': return `${asText(col)} ${like} ${lit(v + '%')}`
      case 'ends_with': return `${asText(col)} ${like} ${lit('%' + v)}`
      case 'is_null': return `${col} IS NULL`
      case 'is_not_null': return `${col} IS NOT NULL`
      case 'between': {
        const [a = '', b = ''] = v.split(',').map((s) => s.trim())
        return `${col} BETWEEN ${lit(a)} AND ${lit(b)}`
      }
      default: return ''
    }
  }

  /** @type {string[]} */
  const where = []

  // Cross-column search (only when we know the columns)
  const s = search.trim()
  if (s && columns.length) {
    const ors = columns.map((c) => `${asText(q(c.name))} ${like} ${lit('%' + s + '%')}`)
    where.push(`(${ors.join(' OR ')})`)
  }

  // Column filters, chained with their and/or conjunct
  const fs = activeFilters(filters).filter((f) => f.column !== ANY_COLUMN)
  fs.forEach((f, i) => {
    const c = cond(f)
    if (!c) return
    where.push(i === 0 && where.length === 0 ? c : `${(f.conjunct ?? 'and').toUpperCase()} ${c}`)
  })

  let sql = `SELECT ${cols}\nFROM ${target}`
  if (where.length) {
    // First clause has no leading AND/OR; the search group (if present) leads.
    sql += `\nWHERE ${where[0].replace(/^(AND|OR)\s+/, '')}`
    for (let i = 1; i < where.length; i++) {
      sql += /^(AND|OR)\s/.test(where[i]) ? `\n  ${where[i]}` : `\n  AND ${where[i]}`
    }
  }
  if (sort?.column) sql += `\nORDER BY ${q(sort.column)} ${sort.direction === 'desc' ? 'DESC' : 'ASC'}`
  if (limit && limit > 0 && limit !== MAX_PAGE_SIZE) sql += `\nLIMIT ${limit}`
  return sql + ';'
}

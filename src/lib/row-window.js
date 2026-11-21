/**
 * Sizing rules for windowed row browsing.
 *
 * A "window" is one chunk of a large result set. What decides the chunk size is
 * BYTES, not rows: a window response crosses the Tauri bridge and is decoded in a
 * single task, so 5k narrow rows (~0.6MB) is nothing while 5k rows of a table with
 * an embedding column (~17KB each) is ~90MB and freezes the app for seconds.
 *
 * So a large load fetches a small probe first, measures its rows, and sizes
 * everything from that. Pure functions here; the stateful part (which windows are
 * resident, what's in flight) lives in StudioShell.
 */

/** Rows in the first, measuring fetch of a large load. */
export const WINDOW_PROBE = 200
/** Target payload for one window request. */
export const WINDOW_TARGET_BYTES = 2_000_000
export const WINDOW_ROWS_MIN = 100
export const WINDOW_ROWS_MAX = 10_000
/** Used when the payload can't be measured at all. */
export const WINDOW_ROWS_DEFAULT = 5_000
/**
 * Window any view longer than this…
 *
 * Deliberately low. The alternative to windowing is ONE request for the whole
 * view, and that is what makes a table feel stuck: a 156k-row table on "All rows"
 * against a remote database is a single response that takes tens of seconds, with
 * the tab spinning honestly the whole time. Windowed, the same table paints its
 * first rows immediately and streams the rest as you scroll. Below this bar a
 * single request is quick enough that the extra round-trips aren't worth it.
 */
export const WINDOW_THRESHOLD = 25_000
/** …or heavier than this, however few rows it has. */
export const WINDOW_HEAVY_BYTES = 8_000_000
/** Rows kept resident on each side of the viewport. */
export const WINDOW_KEEP_ROWS = 80_000

/**
 * Average serialized bytes per row, from up to three sampled rows.
 * Sampled rather than summed: stringifying a whole page of embedding rows to
 * measure it would cost exactly what the measurement exists to avoid.
 * @param {any[]} sample
 * @returns {number} bytes per row, or 0 when nothing could be measured
 */
export function measureRowBytes(sample) {
  if (!Array.isArray(sample) || sample.length === 0) return 0
  let bytes = 0
  let n = 0
  const step = Math.max(1, Math.floor(sample.length / 3))
  for (let i = 0; i < sample.length && n < 3; i += step) {
    try {
      bytes += JSON.stringify(sample[i]).length
      n += 1
    } catch {
      // Unstringifiable cell (cyclic / BigInt) - skip this sample row.
    }
  }
  return n && bytes ? bytes / n : 0
}

/**
 * Rows per window for rows of `bytesPerRow`.
 * @param {number} bytesPerRow
 */
export function pickWindowRows(bytesPerRow) {
  if (!bytesPerRow || bytesPerRow <= 0) return WINDOW_ROWS_DEFAULT
  const rows = Math.round(WINDOW_TARGET_BYTES / bytesPerRow)
  return Math.max(WINDOW_ROWS_MIN, Math.min(WINDOW_ROWS_MAX, rows))
}

/**
 * Whether a view of `rowCount` rows should be windowed rather than loaded whole.
 * Long OR heavy: 100k narrow rows are fine to hold, 10k embedding rows are not.
 * @param {{ rowCount: number, bytesPerRow: number, countKnown?: boolean }} view
 */
export function shouldWindow({ rowCount, bytesPerRow, countKnown = true }) {
  // Without an exact length a sparse array would be sized from a guess, leaving
  // skeleton rows past the end of the table forever.
  if (!countKnown || rowCount <= 0) return false
  if (rowCount > WINDOW_THRESHOLD) return true
  return rowCount * (bytesPerRow || 0) > WINDOW_HEAVY_BYTES
}

/**
 * Window indices completely filled by the first `rowsHave` rows. A window only
 * counts as loaded when every one of its slots is present - marking a partly
 * filled one would leave a band of rows permanently blank.
 * @param {number} rowsHave @param {number} rowsPerWindow
 */
export function windowsFullyCovered(rowsHave, rowsPerWindow) {
  /** @type {number[]} */
  const out = []
  if (!rowsPerWindow || rowsPerWindow <= 0) return out
  const full = Math.floor(rowsHave / rowsPerWindow)
  for (let w = 0; w < full; w++) out.push(w)
  return out
}

/**
 * Windows kept each side of the viewport - a row budget, so it tracks window size.
 * @param {number} rowsPerWindow
 */
export function windowKeepCount(rowsPerWindow) {
  return Math.max(3, Math.round(WINDOW_KEEP_ROWS / Math.max(1, rowsPerWindow)))
}

/**
 * The key a windowed view can *seek* by instead of paging with OFFSET, or null.
 *
 * `LIMIT n OFFSET 995000` makes Postgres walk 995k index entries before it can
 * return a row: measured at 345ms on a 1M-row table, where the same page by key
 * (`WHERE id > $last ORDER BY id LIMIT n`) takes 1.5ms. Every window slices one
 * total order (see stableWindowOrder), so the last row of the window before it
 * is exactly the anchor that page needs - a window next to one already resident
 * costs an index seek instead of a scan of everything before it.
 *
 * The conditions are narrow because being wrong here means missing rows, not
 * slow ones:
 * - one ordering column, and it is the whole primary key. The backend's keyset
 *   predicate carries no tiebreaker, so a non-unique key would skip every row
 *   sharing the anchor's value; a PK is also NOT NULL, which that predicate
 *   likewise can't order around.
 * - a cast the backend will accept. It rejects anything outside
 *   `[A-Za-z0-9_ ]` (a length like `varchar(20)`) and
 *   answers with the offset page instead, so taking the seek path there would
 *   have read-ahead waiting on anchors that buy nothing.
 * - Postgres. Other engines ignore the cursor entirely.
 *
 * @param {{
 *   order: { sorts: Array<{ column: string, direction: string }> } | null,
 *   columns: Array<{ name: string, dataType?: string, data_type?: string }>,
 *   primaryKey: string[],
 *   dialect: string,
 * }} view
 * @returns {{ column: string, sqlType: string, desc: boolean, index: number } | null}
 */
export function seekKeyFor({ order, columns, primaryKey, dialect }) {
  if (dialect !== 'postgres') return null
  if (!order || !Array.isArray(order.sorts) || order.sorts.length !== 1) return null
  const key = order.sorts[0]
  if (!Array.isArray(primaryKey) || primaryKey.length !== 1 || primaryKey[0] !== key.column) return null
  const index = (columns ?? []).findIndex((c) => c?.name === key.column)
  if (index < 0) return null
  const sqlType = columns[index]?.dataType ?? columns[index]?.data_type ?? ''
  if (!sqlType || !/^[A-Za-z0-9_ ]+$/.test(sqlType)) return null
  return { column: key.column, sqlType, desc: key.direction === 'desc', index }
}

/**
 * A deterministic ORDER BY for a windowed view.
 *
 * This is not a nicety - it is what makes a windowed view *correct*. Every window
 * is its own `LIMIT/OFFSET` query, and SQL guarantees nothing about the order of
 * rows between two queries that don't ask for one: Postgres ships with
 * `synchronize_seqscans = on`, so a second scan of the same table can legitimately
 * begin at a different page. Assembled from unordered windows, one view can show
 * the same row twice, miss others, and change what sits at a given row number
 * between fetches. Ordering by the primary key gives every window one total order
 * to slice, so row N is row N.
 *
 * A user sort is kept as the leading key with the PK appended as a tiebreaker -
 * ordering by a non-unique column alone has ties whose order is equally arbitrary.
 *
 * @param {{ column?: string, direction?: string } | null} sort
 * @param {Array<{ column?: string, direction?: string }>} more
 * @param {string[]} primaryKey
 * @returns {{ sortColumn: string, sortDirection: string, sorts: Array<{ column: string, direction: string }> } | null}
 *   null when no stable order exists (no primary key) - the caller must then not
 *   window, and load the result in a single query instead.
 */
export function stableWindowOrder(sort, more, primaryKey) {
  if (!Array.isArray(primaryKey) || primaryKey.length === 0) return null
  /** @type {Array<{ column: string, direction: string }>} */
  const keys = []
  const dir = (/** @type {string | undefined} */ d) => (d === 'desc' ? 'desc' : 'asc')
  if (sort?.column) keys.push({ column: sort.column, direction: dir(sort.direction) })
  for (const s of more ?? []) {
    if (s?.column) keys.push({ column: s.column, direction: dir(s.direction) })
  }
  const seen = new Set(keys.map((k) => k.column))
  for (const col of primaryKey) {
    if (!seen.has(col)) keys.push({ column: col, direction: 'asc' })
  }
  // `sorts` is always sent (even for one key) so the multi-key path builds the
  // full ORDER BY; `sortColumn` covers engines that only read the single key.
  return { sortColumn: keys[0].column, sortDirection: keys[0].direction, sorts: keys }
}

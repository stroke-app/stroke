export const COL_WIDTH_MIN = 48
export const COL_WIDTH_MAX = 1600

/** Sensible fixed default width (px) sized to typical content per data type. */
export function defaultColumnWidth(dataType) {
  const t = String(dataType ?? '')
    .toLowerCase()
    .replace(/\(.+\)$/, '')
    .trim()
  const has = (...needles) => needles.some((n) => t.includes(n))

  // Wide free-form content
  if (has('json')) return 420
  if (has('text')) return 420

  // Identifier-like strings - uuid (36 chars) / cuid are stored as
  // char/varchar/text; give them room to show the full value plus the
  // FK/copy affordances without truncating.
  if (has('uuid')) return 380
  if (has('char', 'varchar', 'string')) return 380

  // Date/time - timestamptz carries the +00 offset, so it needs the most room.
  if (has('timestamptz', 'with time zone')) return 300
  if (has('timestamp', 'datetime')) return 270
  if (has('date', 'time')) return 220

  // Compact scalars
  if (has('bool')) return 150
  if (has('int', 'serial', 'numeric', 'decimal', 'float', 'double', 'real', 'money')) return 210

  // Everything else - comfortable average
  return 240
}

/** @param {number} width */
export function clampColumnWidth(width) {
  return Math.round(Math.min(COL_WIDTH_MAX, Math.max(COL_WIDTH_MIN, width)))
}

/**
 * Whether a column's values are drawn flush right, per Settings → Appearance →
 * table alignment. `'numbers'` is the spreadsheet convention: quantities line up
 * by place value so magnitudes are comparable down the column, prose stays left
 * where the eye finds the start of each line.
 *
 * Shared so every surface that shows rows - the canvas grid and the related-rows
 * panel docked under it - reaches the same verdict for the same column.
 *
 * @param {unknown} dataType declared SQL type, e.g. `numeric(10,2)`
 * @param {string} mode `'left'` | `'numbers'` | `'right'`
 */
export function columnAlignsRight(dataType, mode) {
  if (mode === 'right') return true
  if (mode !== 'numbers') return false
  const t = String(dataType ?? '')
  // Booleans match /int/ on engines that spell them `tinyint(1)`, so they are
  // excluded before the numeric test.
  return !/bool/i.test(t) && /(int|numeric|decimal|real|double|float|money|number|serial)/i.test(t)
}

// ── Auto-fit ────────────────────────────────────────────────────────────────
// A column starts as wide as what it holds (header included), between these
// bounds, instead of a fixed width per type. Long text is the reason for the
// cap: a column of descriptions or JSON stops at AUTO_FIT_MAX and truncates
// with an ellipsis, so one wide value never pushes the rest off screen.

export const AUTO_FIT_MIN = 72
export const AUTO_FIT_MAX = 360
/** Rows read per column: enough to see the shape, cheap on a 10k-row page. */
export const AUTO_FIT_SAMPLE = 200
/** Longer values reach the cap anyway, so only this much is ever measured. */
export const AUTO_FIT_CHARS = 64

/**
 * The width (logical px) a column needs for its header and a sample of its
 * values. `measure` returns a string's drawn width in the same units.
 *
 * @param {{ headerW: number, values: ArrayLike<string | null | undefined>, measure: (s: string) => number, padX: number, max?: number, min?: number }} o
 */
export function fitColumnWidth({ headerW, values, measure, padX, max = AUTO_FIT_MAX, min = AUTO_FIT_MIN }) {
  const room = max - padX * 2
  let w = headerW
  const n = Math.min(values.length, AUTO_FIT_SAMPLE)
  for (let i = 0; i < n && w < room; i++) {
    const v = values[i]
    if (!v) continue
    // First line only: the grid draws one line per cell.
    const nl = v.indexOf('\n')
    let s = nl === -1 ? v : v.slice(0, nl)
    if (s.length > AUTO_FIT_CHARS) s = s.slice(0, AUTO_FIT_CHARS)
    const m = measure(s)
    if (m > w) w = m
  }
  return Math.round(Math.min(max, Math.max(min, w + padX * 2)))
}

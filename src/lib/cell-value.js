/** @typedef {{ ok: true, value: unknown } | { ok: false, message: string }} ParseResult */

// ── Oversized cells ──────────────────────────────────────────────────────────
// The backend replaces cells whose payload exceeds its per-cell cap with a
// small sentinel object (see src-tauri/src/db/sql_util.rs) so multi-MB values
// (file buffers in jsonb/text/blob columns) can't freeze the webview. Only a
// preview reaches the frontend; truncated cells must never be edited or used
// as filter values.

/** @typedef {{ dataType: string, bytes: number, preview: string }} OversizeCellInfo */

/**
 * Detect the backend's oversized-cell sentinel.
 * @param {unknown} value
 * @returns {OversizeCellInfo | null}
 */
export function oversizeCellInfo(value) {
  if (
    value !== null &&
    typeof value === 'object' &&
    /** @type {any} */ (value).__strokeOversize === true
  ) {
    const v = /** @type {any} */ (value)
    return {
      dataType: String(v.dataType ?? ''),
      bytes: Number(v.bytes) || 0,
      preview: String(v.preview ?? ''),
    }
  }
  return null
}

/** @param {number} bytes */
export function formatByteSize(bytes) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

/**
 * One-line marker + preview head used wherever a truncated cell becomes text
 * (grid, copy, row detail). The full preview stays in the JSON lightbox.
 * @param {OversizeCellInfo} over
 */
export function oversizeCellText(over) {
  // No preview is the normal case now: a page reports a wide value's size and
  // nothing else, because reading the first 16KB of a compressed out-of-line
  // value costs a full detoast of it. The cell says what is there and how big;
  // the dock loads it when someone opens it.
  if (!over.preview) return `${formatByteSize(over.bytes)} ${over.dataType || 'value'}`
  // The value first: a grid cell shows about forty characters, and with the
  // label in front all of them said "[jsonb · 15 KB, truncated]". The size
  // follows (and the cell's Load button says the rest is not here).
  const head = over.preview.length > 200 ? over.preview.slice(0, 200) : over.preview
  return `${head}… (${formatByteSize(over.bytes)} ${over.dataType || 'value'})`
}

/**
 * @param {unknown} value
 */
export function valueToEditString(value) {
  if (value === null || value === undefined) return ''
  if (typeof value === 'object') return JSON.stringify(value)
  return String(value)
}

/**
 * @param {string} dataType
 */
export function normalizeColumnType(dataType) {
  return String(dataType ?? '')
    .toLowerCase()
    .replace(/\(.+\)$/, '')
    .trim()
}

/** @param {string} dataType */
function normalizedType(dataType) {
  return normalizeColumnType(dataType)
}

/** @param {string} dataType */
export function isDateTimeType(dataType) {
  const t = normalizedType(dataType)
  return (
    t.includes('timestamp') ||
    t === 'datetime' ||
    t === 'timestamptz' ||
    (t.includes('date') && t.includes('time'))
  )
}

/** @param {string} dataType */
export function isDateOnlyType(dataType) {
  return normalizedType(dataType) === 'date'
}

/** @param {string} dataType */
export function isTimeOnlyType(dataType) {
  const t = normalizedType(dataType)
  return (t === 'time' || t === 'timetz') && !t.includes('stamp')
}

/** @param {number} n */
function pad2(n) {
  return String(n).padStart(2, '0')
}

/**
 * A trailing UTC offset, in any of the spellings a timestamp reaches us in:
 * `Z`, `+05:45`, `-0700`, `+00`, or the word `UTC` that chrono's
 * `DateTime<Utc>::to_string()` appends.
 */
const ZONE_SUFFIX_RE = /(?:\s*(?:UTC|GMT)|\s*[+-]\d{2}(?::?\d{2})?|Z)$/i

/**
 * Normalise a typed timestamp into text the database will read back as the
 * same instant.
 *
 * The rule that matters: a value carrying an explicit zone is handed through
 * with only its zone spelling normalised, and is never rebuilt out of a local
 * `Date`. `Date.parse` accepts `2026-05-22 11:28:40.501 UTC`, so reformatting
 * it with `getHours()` and friends re-rendered the instant in the machine's own
 * zone and then dropped the zone, which moved the value by the local offset on
 * every edit - and quietly, because the write succeeded. Fractional seconds are
 * kept for the same reason: the grid must not spend a column's precision to
 * save a value the user did not change.
 *
 * @param {string} raw
 */
export function formatTimestampForDb(raw) {
  const trimmed = raw.trim()
  if (!trimmed) return trimmed

  // Date only - already unambiguous.
  if (/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return trimmed

  // ISO-ish, with or without a `T`, with or without a zone. Postgres parses
  // this shape natively, so the only work is to make the separator a space and
  // spell the zone in a way it accepts (`UTC`/`GMT` -> `+00`).
  const isoish = /^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})(:\d{2})?(\.\d+)?\s*(Z|UTC|GMT|[+-]\d{2}(?::?\d{2})?)?$/i
  const m = isoish.exec(trimmed)
  if (m) {
    const [, date, hhmm, seconds, fraction, zone] = m
    let out = `${date} ${hhmm}${seconds ?? ':00'}${fraction ?? ''}`
    if (zone) {
      out += /^(Z|UTC|GMT)$/i.test(zone) ? '+00' : zone
    }
    return out
  }

  // Anything else (a locale string, `May 31, 2026 …`) has no zone we can trust,
  // so fall back to interpreting it locally - which is what the user typing in
  // their own zone means.
  const parsed = Date.parse(trimmed)
  if (!Number.isNaN(parsed)) {
    const d = new Date(parsed)
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`
  }
  return trimmed
}

/**
 * @param {{ enumValues?: string[], enum_values?: string[] } | null | undefined} col
 * @returns {string[] | null}
 */
export function getColumnEnumValues(col) {
  const values = col?.enumValues ?? col?.enum_values
  return Array.isArray(values) && values.length > 0 ? values : null
}

/**
 * @param {string} dataType
 */
export function isBooleanType(dataType) {
  const t = normalizedType(dataType)
  return t === 'boolean' || t === 'bool'
}

/**
 * A boolean column's value as `true`/`false`. SQLite, libSQL and D1 store a
 * boolean as 0 or 1; shown raw it reads as a number, and a true/false picker
 * given "0" matches none of its options. Anything else comes back unchanged.
 * @param {unknown} value
 * @returns {unknown}
 */
export function asBooleanValue(value) {
  if (value === 0 || value === '0') return false
  if (value === 1 || value === '1') return true
  return value
}

/**
 * @param {string} dataType
 */
export function isEditableType(dataType) {
  const t = normalizedType(dataType)
  if (!t || t.includes('bytea')) return false
  return true
}

/**
 * @param {string} raw
 * @param {string} dataType
 * @param {string[] | null} [enumValues]
 * @returns {ParseResult}
 */
export function parseCellInput(raw, dataType, enumValues = null) {
  const trimmed = raw.trim()
  const t = normalizedType(dataType)

  if (trimmed === '' || trimmed.toUpperCase() === 'NULL') {
    return { ok: true, value: null }
  }

  if (enumValues?.length) {
    if (!enumValues.includes(trimmed)) {
      return {
        ok: false,
        message: `Must be one of: ${enumValues.join(', ')}`,
      }
    }
    return { ok: true, value: trimmed }
  }

  if (
    t.includes('int') ||
    t === 'serial' ||
    t === 'bigserial' ||
    t === 'smallserial' ||
    t === 'oid'
  ) {
    if (!/^-?\d+$/.test(trimmed)) {
      return {
        ok: false,
        message: `Invalid integer for ${dataType}: "${raw}"`,
      }
    }
    const n = Number(trimmed)
    if (!Number.isSafeInteger(n)) {
      return { ok: false, message: 'Integer is out of range' }
    }
    return { ok: true, value: n }
  }

  if (
    t.includes('numeric') ||
    t.includes('decimal') ||
    t.includes('real') ||
    t.includes('double') ||
    t.includes('float') ||
    t === 'money'
  ) {
    if (!/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(trimmed)) {
      return {
        ok: false,
        message: `Invalid number for ${dataType}: "${raw}"`,
      }
    }
    const n = Number(trimmed)
    if (!Number.isFinite(n)) {
      return { ok: false, message: 'Number is not finite' }
    }
    return { ok: true, value: n }
  }

  if (t === 'boolean' || t === 'bool') {
    const lower = trimmed.toLowerCase()
    if (['true', 't', '1', 'yes', 'y'].includes(lower)) return { ok: true, value: true }
    if (['false', 'f', '0', 'no', 'n'].includes(lower)) return { ok: true, value: false }
    return {
      ok: false,
      message: `Invalid boolean: "${raw}" (use true/false)`,
    }
  }

  if (t === 'json' || t === 'jsonb') {
    try {
      return { ok: true, value: JSON.parse(trimmed) }
    } catch {
      return { ok: false, message: 'Invalid JSON' }
    }
  }

  if (t === 'uuid') {
    if (
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
        trimmed,
      )
    ) {
      return { ok: false, message: `Invalid UUID: "${raw}"` }
    }
    return { ok: true, value: trimmed }
  }

  if (isDateTimeType(dataType)) {
    return { ok: true, value: formatTimestampForDb(trimmed) }
  }

  if (isDateOnlyType(dataType)) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) {
      return { ok: false, message: `Invalid date: "${raw}" (use YYYY-MM-DD)` }
    }
    return { ok: true, value: trimmed }
  }

  if (isTimeOnlyType(dataType)) {
    if (!/^\d{2}:\d{2}(:\d{2})?$/.test(trimmed)) {
      return { ok: false, message: `Invalid time: "${raw}" (use HH:MM)` }
    }
    return { ok: true, value: trimmed }
  }

  return { ok: true, value: trimmed }
}

/**
 * @param {string} dataType
 * @param {string} columnName
 * @param {string[]} primaryKey
 */
export function isLikelyAutoColumn(dataType, columnName, primaryKey) {
  if (!primaryKey.includes(columnName)) return false
  const t = normalizedType(dataType)
  return t.includes('serial') || t === 'bigserial' || t === 'smallserial'
}

/**
 * Whether the database fills this column in for you.
 *
 * The catalog is the only thing that actually knows: a Postgres `serial` reports
 * its data type as `bigint`, an identity column as `integer`, so the type string
 * can never identify either. `autoGenerated` comes from the backend
 * (`attidentity`/`nextval` on Postgres, `EXTRA` on MySQL, the rowid alias on
 * SQLite); the type heuristic stays as the fallback for engines that don't
 * report it.
 *
 * @param {{ name: string, dataType?: string, data_type?: string, autoGenerated?: boolean }} col
 * @param {string[]} primaryKey
 */
export function isAutoColumn(col, primaryKey) {
  if (col?.autoGenerated) return true
  return isLikelyAutoColumn(col?.dataType ?? col?.data_type ?? '', col?.name ?? '', primaryKey)
}

/**
 * What the insert row should say about a column left blank.
 *  - `auto`     the database generates it; asking for a value is wrong
 *  - `default`  omitting it is legal because a DEFAULT fills it
 *  - `null`     omitting it stores NULL
 *  - `required` omitting it fails
 * @param {{ name: string, dataType?: string, data_type?: string, nullable?: boolean, autoGenerated?: boolean, hasDefault?: boolean }} col
 * @param {string[]} primaryKey
 * @returns {'auto' | 'default' | 'null' | 'required'}
 */
export function insertOmitBehaviour(col, primaryKey) {
  if (isAutoColumn(col, primaryKey)) return 'auto'
  if (col?.hasDefault) return 'default'
  if (col?.nullable !== false) return 'null'
  return 'required'
}

/**
 * @param {{ name: string, dataType?: string, data_type?: string, nullable?: boolean, enumValues?: string[], enum_values?: string[] }[]} columns
 * @param {string[]} primaryKey
 * @param {Record<string, string>} drafts
 * @returns {ParseResult & { values?: Record<string, unknown> }}
 */
export function buildInsertPayload(columns, primaryKey, drafts) {
  /** @type {Record<string, unknown>} */
  const values = {}
  let hasNonAutoEditableColumn = false

  for (const col of columns) {
    const dataType = col.dataType ?? col.data_type ?? 'text'
    if (!isEditableType(dataType)) continue

    if (!isAutoColumn(col, primaryKey)) {
      hasNonAutoEditableColumn = true
    }

    const raw = drafts[col.name] ?? ''
    const trimmed = raw.trim()

    // Empty → omit column (DB default / auto). Required NOT NULL columns are validated in Rust.
    if (trimmed === '') {
      continue
    }

    const parsed = parseCellInput(trimmed, dataType, getColumnEnumValues(col))
    if (!parsed.ok) {
      return { ok: false, message: `${col.name}: ${parsed.message}` }
    }
    values[col.name] = parsed.value
  }

  // Only block empty inserts when there are actual non-auto columns the user should fill.
  // If every column is auto-generated (e.g. a table with only a serial PK), allow the insert.
  if (Object.keys(values).length === 0 && hasNonAutoEditableColumn) {
    return {
      ok: false,
      message: 'Enter at least one column value (or leave auto columns empty)',
    }
  }

  return { ok: true, value: null, values }
}

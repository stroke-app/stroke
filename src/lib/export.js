/**
 * Convert columns + rows to a CSV string.
 * Values are quoted when they contain commas, quotes, or newlines.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @returns {string}
 */
export const csvEscape = (/** @type {unknown} */ v) => {
  if (v === null || v === undefined) return ''
  const s = typeof v === 'object' ? JSON.stringify(v) : String(v)
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
}

/** UTF-8 byte-order mark so Excel reads non-ASCII CSV/exports as UTF-8. */
export const UTF8_BOM = '﻿'

export function rowsToCsv(columns, rows) {
  const header = columns.map((c) => csvEscape(c.name)).join(',')
  const body = rows.map((row) => row.map(csvEscape).join(',')).join('\n')
  // TODO: surface backend row-cap/truncation here once a `truncated` flag is
  // threaded through to this layer (append a trailing note when capped).
  return UTF8_BOM + header + '\n' + body
}

/** A macrotask yield so the UI can paint between chunks of a big export. */
const yieldToUi = () => new Promise((res) => setTimeout(res, 0))

/**
 * Chunked, cooperative CSV builder for large exports - yields to the event loop
 * every `chunkSize` rows so the main thread stays responsive and a progress
 * indicator can animate instead of the app appearing frozen.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @param {{ chunkSize?: number, onProgress?: (done: number, total: number) => void }} [opts]
 * @returns {Promise<string>}
 */
export async function rowsToCsvAsync(columns, rows, opts = {}) {
  const { chunkSize = 5000, onProgress } = opts
  const total = rows.length
  const blocks = [UTF8_BOM + columns.map((c) => csvEscape(c.name)).join(',')]
  for (let i = 0; i < total; i += chunkSize) {
    const end = Math.min(i + chunkSize, total)
    let block = ''
    for (let r = i; r < end; r++) block += (r === i ? '' : '\n') + rows[r].map(csvEscape).join(',')
    blocks.push(block)
    onProgress?.(end, total)
    await yieldToUi()
  }
  // TODO: surface backend row-cap/truncation here once a `truncated` flag is
  // threaded through to this layer (append a trailing note when capped).
  return blocks.join('\n')
}

/** Flatten a value to a single line for tab/pipe-delimited formats. */
export const singleLine = (/** @type {unknown} */ v) => {
  if (v === null || v === undefined) return ''
  const s = typeof v === 'object' ? JSON.stringify(v) : String(v)
  return s.replace(/[\t\r\n]+/g, ' ')
}

/**
 * Convert columns + rows to a TSV string. Tabs/newlines inside values are
 * collapsed to spaces so each row stays one physical line.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @returns {string}
 */
export function rowsToTsv(columns, rows) {
  const header = columns.map((c) => singleLine(c.name)).join('\t')
  const body = rows.map((row) => row.map(singleLine).join('\t')).join('\n')
  return header + '\n' + body
}

export const mdEscape = (/** @type {unknown} */ v) => singleLine(v).replace(/\|/g, '\\|')

/** Padding cap so one huge cell doesn't blow up every row with spaces. Wide
 *  enough for the long-but-ordinary values (a 64-char hash, a UUID, an ISO
 *  timestamp, a URL): at 48 a sha256 column overflowed its own header and
 *  shoved every column after it out of line. Only real prose overflows now. */
const MD_PAD_MAX = 96

/**
 * Column widths for a padded (column-aligned) Markdown table, capped at
 * MD_PAD_MAX. Shared by rowsToMarkdown and the text view's lazy renderer so
 * what's displayed and what's copied are byte-identical.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @returns {number[]}
 */
export function markdownColumnWidths(columns, rows) {
  const widths = columns.map((c) => Math.max(3, mdEscape(c.name).length))
  for (const row of rows) {
    for (let i = 0; i < widths.length; i++) {
      if (widths[i] >= MD_PAD_MAX) continue
      const len = mdEscape(row[i]).length
      if (len > widths[i]) widths[i] = Math.min(len, MD_PAD_MAX)
    }
  }
  return widths
}

/** @param {string} s @param {number} w */
export const mdPad = (s, w) => (s.length >= w ? s : s + ' '.repeat(w - s.length))

/**
 * Convert columns + rows to a column-aligned GitHub-flavored Markdown table.
 * Cells longer than the padding cap simply overflow - still valid Markdown.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @param {number[]} [widths] precomputed via markdownColumnWidths
 * @returns {string}
 */
export function rowsToMarkdown(columns, rows, widths = markdownColumnWidths(columns, rows)) {
  const header = `| ${columns.map((c, i) => mdPad(mdEscape(c.name), widths[i])).join(' | ')} |`
  const sep = `| ${widths.map((w) => '-'.repeat(w)).join(' | ')} |`
  const body = rows.map((row) => `| ${row.map((v, i) => mdPad(mdEscape(v), widths[i])).join(' | ')} |`).join('\n')
  return body ? `${header}\n${sep}\n${body}` : `${header}\n${sep}`
}

/**
 * Convert columns + rows to JSON Lines (one JSON object per line).
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @returns {string}
 */
export function rowsToJsonl(columns, rows) {
  return rows
    .map((row) => {
      /** @type {Record<string, unknown>} */
      const obj = {}
      columns.forEach((col, i) => { obj[col.name] = row[i] ?? null })
      return JSON.stringify(obj)
    })
    .join('\n')
}

/**
 * Convert positional rows to plain objects keyed by column name. Column names
 * are resolved once up front (columns may be `{ name }` objects or strings),
 * not once per row.
 * @param {Array<{ name?: string } | string>} columns
 * @param {unknown[][]} rows
 * @returns {Record<string, unknown>[]}
 */
export function rowsToObjects(columns, rows) {
  const names = columns.map((c) => /** @type {string} */ ((typeof c === 'object' && c !== null ? c.name : undefined) ?? c))
  return rows.map((row) => Object.fromEntries(names.map((name, i) => [name, row[i]])))
}

/**
 * Convert columns + rows to a JSON array of objects.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @returns {string}
 */
export function rowsToJson(columns, rows) {
  const records = rows.map((row) => {
    /** @type {Record<string, unknown>} */
    const obj = {}
    columns.forEach((col, i) => { obj[col.name] = row[i] ?? null })
    return obj
  })
  return JSON.stringify(records, null, 2)
}

/**
 * Chunked, cooperative JSON builder for large exports (see rowsToCsvAsync).
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @param {{ chunkSize?: number, onProgress?: (done: number, total: number) => void }} [opts]
 * @returns {Promise<string>}
 */
export async function rowsToJsonAsync(columns, rows, opts = {}) {
  const { chunkSize = 5000, onProgress } = opts
  const total = rows.length
  if (total === 0) return '[]'
  const parts = ['[']
  for (let i = 0; i < total; i += chunkSize) {
    const end = Math.min(i + chunkSize, total)
    let block = ''
    for (let r = i; r < end; r++) {
      /** @type {Record<string, unknown>} */
      const obj = {}
      columns.forEach((col, ci) => { obj[col.name] = rows[r][ci] ?? null })
      block += (r === 0 ? '' : ',') + '\n  ' + JSON.stringify(obj)
    }
    parts.push(block)
    onProgress?.(end, total)
    await yieldToUi()
  }
  parts.push('\n]')
  return parts.join('')
}

/**
 * Convert columns + rows to a series of `INSERT` statements for `tableName`.
 * Identifiers are double-quoted; values become typed literals - numbers/booleans
 * raw, NULL for null/undefined, everything else single-quoted with `''` escaping
 * (objects serialized to JSON first). Safe to paste into any SQL console.
 * @param {Array<{ name: string }>} columns
 * @param {unknown[][]} rows
 * @param {string} [tableName]
 * @returns {string}
 */
export function rowsToSql(columns, rows, tableName = 'exported_table') {
  const ident = (/** @type {string} */ n) => `"${String(n).replace(/"/g, '""')}"`
  const lit = (/** @type {unknown} */ v) => {
    if (v === null || v === undefined) return 'NULL'
    if (typeof v === 'number' && Number.isFinite(v)) return String(v)
    if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE'
    const s = typeof v === 'object' ? JSON.stringify(v) : String(v)
    return `'${s.replace(/'/g, "''")}'`
  }
  const cols = columns.map((c) => ident(c.name)).join(', ')
  const table = ident(tableName)
  if (rows.length === 0) return `-- no rows to export from ${table}`
  return rows
    .map((row) => `INSERT INTO ${table} (${cols}) VALUES (${row.map(lit).join(', ')});`)
    .join('\n')
}

/**
 * Build a default filename like "users_2025-05-24.csv"
 * @param {string | null} tableName
 * @param {'csv' | 'json' | 'sql' | 'tsv' | 'md' | 'jsonl'} format
 */
export function buildExportFilename(tableName, format) {
  const base = tableName ?? 'export'
  const date = new Date().toISOString().slice(0, 10)
  return `${base}_${date}.${format}`
}

/** @type {Record<string, { name: string, mime: string }>} */
const FORMAT_META = {
  csv: { name: 'CSV files', mime: 'text/csv;charset=utf-8;' },
  json: { name: 'JSON files', mime: 'application/json' },
  sql: { name: 'SQL files', mime: 'text/plain' },
  tsv: { name: 'TSV files', mime: 'text/tab-separated-values' },
  md: { name: 'Markdown files', mime: 'text/markdown' },
  jsonl: { name: 'JSON Lines files', mime: 'application/x-ndjson' },
}

/**
 * Show a native "Save As" dialog and write content to the chosen path.
 * Falls back to a browser blob download when running outside Tauri.
 * @param {string} content
 * @param {string} defaultFilename  e.g. "users_2025-05-24.csv"
 * @param {'csv' | 'json' | 'sql' | 'tsv' | 'md' | 'jsonl'} format
 * @returns {Promise<boolean>} true if the file was saved, false if cancelled
 */
export async function saveExportFile(content, defaultFilename, format) {
  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  const meta = FORMAT_META[format] ?? { name: 'Files', mime: 'text/plain' }

  if (isTauri) {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { invoke } = await import('@tauri-apps/api/core')

    const path = await save({
      defaultPath: defaultFilename,
      filters: [{ name: meta.name, extensions: [format] }],
    })

    if (!path) return false
    await invoke('save_file', { path, content })
    return true
  }

  // Browser fallback
  const mime = meta.mime
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = defaultFilename
  try {
    document.body.appendChild(a)
    a.click()
  } finally {
    a.remove()
    // Defer the revoke so the click-triggered download has picked up the blob;
    // revoking synchronously can cancel the download in some browsers.
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }
  return true
}

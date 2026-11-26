/** @typedef {'sql' | 'markdown'} CellType */

/**
 * @typedef {object} CellResult
 * @property {Array<{ name: string, dataType?: string }>} columns
 * @property {unknown[][]} rows
 * @property {number} queryMs
 * @property {string} error
 * @property {string} message
 * @property {number} [ranAt]
 */

/**
 * @typedef {object} NotebookCell
 * @property {string} id
 * @property {CellType} type
 * @property {string} content
 * @property {CellResult | null} result
 * @property {boolean} collapsed
 */

/**
 * @typedef {object} Notebook
 * @property {string} version
 * @property {string} title
 * @property {NotebookCell[]} cells
 * @property {Array<{ name: string, value: string }>} [variables]
 * @property {string} createdAt
 * @property {string} updatedAt
 */

/** @returns {NotebookCell} */
export function createSqlCell(content = '') {
  return { id: crypto.randomUUID(), type: 'sql', content, result: null, collapsed: false }
}

/** @returns {NotebookCell} */
export function createMarkdownCell(content = '') {
  return { id: crypto.randomUUID(), type: 'markdown', content, result: null, collapsed: false }
}

/** @param {string} [title] @returns {Notebook} */
export function createNotebook(title = 'Untitled notebook') {
  const now = new Date().toISOString()
  return { version: '1', title, cells: [createSqlCell()], variables: [], createdAt: now, updatedAt: now }
}

/** @param {Notebook} notebook @returns {string} */
export function serializeNotebook(notebook) {
  return JSON.stringify({ ...notebook, updatedAt: new Date().toISOString() }, null, 2)
}

/** @param {string} json @returns {Notebook} */
export function deserializeNotebook(json) {
  const nb = JSON.parse(json)
  if (!nb.version || !Array.isArray(nb.cells)) throw new Error('Invalid .sqlnb format')
  nb.cells = nb.cells.map((c) => ({
    id: c.id ?? crypto.randomUUID(),
    type: c.type ?? 'sql',
    content: c.content ?? '',
    result: c.result ?? null,
    collapsed: c.collapsed ?? false,
  }))
  nb.variables = Array.isArray(nb.variables) ? nb.variables.filter((v) => v && typeof v.name === 'string').map((v) => ({ name: v.name, value: String(v.value ?? '') })) : []
  return nb
}

/** Derive a display title from the filename path. @param {string} path @returns {string} */
export function titleFromPath(path) {
  const base = path.split(/[/\\]/).pop() ?? path
  return base.replace(/\.sqlnb$/i, '')
}

// ── Variables ────────────────────────────────────────────────────────────────

/** @typedef {{ name: string, value: string }} NotebookVariable */

/** `{{ name }}` placeholders in a cell. @param {string} sql */
export function variableNames(sql) {
  return [...new Set([...String(sql).matchAll(/\{\{\s*([A-Za-z_][\w]*)\s*\}\}/g)].map((m) => m[1]))]
}

/**
 * Fill `{{ name }}` placeholders from the notebook's variables. Text, not
 * parameters: `'{{status}}'` quotes a string (its quotes doubled) and
 * `{{limit}}` stays a number. Unknown names are left for the database to
 * reject, with the list returned so the cell can say which.
 * @param {string} sql
 * @param {NotebookVariable[]} vars
 * @returns {{ sql: string, missing: string[] }}
 */
export function substituteVars(sql, vars = []) {
  const map = new Map(vars.filter((v) => v.name).map((v) => [v.name, v.value]))
  /** @type {string[]} */
  const missing = []
  const src = String(sql)
  const out = src.replace(/\{\{\s*([A-Za-z_][\w]*)\s*\}\}/g, (all, name, at) => {
    if (map.has(name)) {
      const value = /** @type {string} */ (map.get(name))
      // Inside a quoted string the value's own quotes are doubled: `O'Neil` in
      // `'{{name}}'` ended the literal early and broke the query (and let a
      // value write SQL of its own). Outside quotes it goes in as written.
      return insideQuotes(src, at) ? value.replace(/'/g, "''") : value
    }
    if (!missing.includes(name)) missing.push(name)
    return all
  })
  return { sql: out, missing }
}

/**
 * Whether `at` falls inside a single-quoted literal of `sql`. Comments are
 * skipped so an apostrophe in one does not flip the count; a doubled quote
 * inside a literal stays inside it.
 * @param {string} sql @param {number} at
 */
function insideQuotes(sql, at) {
  let inside = false
  for (let i = 0; i < at; i++) {
    const c = sql[i]
    if (!inside && c === '-' && sql[i + 1] === '-') { const nl = sql.indexOf('\n', i); if (nl < 0 || nl >= at) return false; i = nl; continue }
    if (!inside && c === '/' && sql[i + 1] === '*') { const end = sql.indexOf('*/', i + 2); if (end < 0 || end >= at) return false; i = end + 1; continue }
    if (c === "'") inside = !inside
  }
  return inside
}

// ── Import ───────────────────────────────────────────────────────────────────

/** @param {string} s */
const lines = (s) => (Array.isArray(s) ? s.join('') : String(s ?? ''))

/**
 * A notebook from another format, picked by file extension: .sqlnb, Jupyter
 * .ipynb (SQL code cells, `%%sql` magics, markdown), Markdown (```sql fences
 * become SQL cells, the prose between them markdown cells) or a .sql script
 * (cells split on `-- %%` markers, or one cell per statement group).
 * @param {string} name file name or path
 * @param {string} text
 * @returns {Notebook}
 */
export function importNotebook(name, text) {
  const ext = /\.([a-z0-9]+)$/i.exec(name)?.[1]?.toLowerCase() ?? ''
  const title = titleFromPath(name).replace(/\.(ipynb|md|markdown|sql)$/i, '')
  /** @type {NotebookCell[]} */
  let cells
  if (ext === 'sqlnb') return deserializeNotebook(text)
  if (ext === 'ipynb') cells = cellsFromIpynb(text)
  else if (ext === 'md' || ext === 'markdown') cells = cellsFromMarkdown(text)
  else cells = cellsFromSql(text)
  const nb = createNotebook(title || 'Imported notebook')
  nb.cells = cells.length ? cells : [createSqlCell()]
  return nb
}

/** @param {string} text @returns {NotebookCell[]} */
export function cellsFromIpynb(text) {
  const nb = JSON.parse(text)
  if (!Array.isArray(nb?.cells)) throw new Error('Not a Jupyter notebook: no cells')
  /** @type {NotebookCell[]} */
  const out = []
  for (const c of nb.cells) {
    const src = lines(c.source)
    if (!src.trim()) continue
    if (c.cell_type === 'markdown') out.push(createMarkdownCell(src.trim()))
    else if (c.cell_type === 'code') {
      // %%sql / %sql magics (ipython-sql, JupySQL) carry the SQL after them.
      const sql = src.replace(/^\s*%%sql[^\n]*\n?/, '').replace(/^\s*%sql\s+/gm, '')
      out.push(createSqlCell(sql.trim()))
    }
  }
  return out
}

/** @param {string} text @returns {NotebookCell[]} */
export function cellsFromMarkdown(text) {
  /** @type {NotebookCell[]} */
  const out = []
  const re = /^```(\w*)[^\n]*\n([\s\S]*?)^```\s*$/gm
  let last = 0
  for (const m of String(text).matchAll(re)) {
    const lang = m[1].toLowerCase()
    if (lang && !/^(sql|pgsql|postgres|postgresql|mysql|sqlite|plsql|tsql)$/.test(lang)) continue
    const before = text.slice(last, m.index).trim()
    if (before) out.push(createMarkdownCell(before))
    out.push(createSqlCell(m[2].trim()))
    last = /** @type {number} */ (m.index) + m[0].length
  }
  const rest = text.slice(last).trim()
  if (rest) out.push(createMarkdownCell(rest))
  return out
}

/** @param {string} text @returns {NotebookCell[]} */
export function cellsFromSql(text) {
  const src = String(text)
  // Explicit cells: `-- %%` (or `-- CELL`) lines, as VS Code and DataGrip write them.
  if (/^\s*--\s*(%%|cell\b)/im.test(src)) {
    return src
      .split(/^\s*--\s*(?:%%|cell\b)[^\n]*$/im)
      .map((s) => s.trim())
      .filter(Boolean)
      .map((s) => createSqlCell(s))
  }
  // Otherwise blank-line separated groups: a script's own paragraphs.
  return src
    .split(/\n\s*\n(?=\s*\S)/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => createSqlCell(s))
}

// ── Export ───────────────────────────────────────────────────────────────────

/** @param {unknown} v */
const cellText = (v) => (v === null || v === undefined ? 'NULL' : typeof v === 'object' ? JSON.stringify(v) : String(v))
const MAX_EXPORT_ROWS = 200

/** A result as a Markdown table. @param {CellResult} r */
function resultMarkdown(r) {
  if (r.error) return `> Error: ${r.error.replace(/\n/g, ' ')}`
  if (!r.columns.length) return r.message ? `> ${r.message}` : ''
  const esc = (/** @type {string} */ s) => s.replace(/\|/g, '\\|').replace(/\n/g, ' ')
  const head = `| ${r.columns.map((c) => esc(c.name)).join(' | ')} |`
  const sep = `| ${r.columns.map(() => '---').join(' | ')} |`
  const body = r.rows.slice(0, MAX_EXPORT_ROWS).map((row) => `| ${row.map((v) => esc(cellText(v))).join(' | ')} |`)
  const more = r.rows.length > MAX_EXPORT_ROWS ? [`\n_${r.rows.length - MAX_EXPORT_ROWS} more rows not shown._`] : []
  return [head, sep, ...body, ...more].join('\n')
}

/** @param {Notebook} nb @param {{ results?: boolean }} [o] */
export function toMarkdown(nb, { results = true } = {}) {
  const parts = [`# ${nb.title || 'Notebook'}`]
  for (const c of nb.cells) {
    if (c.type === 'markdown') parts.push(c.content.trim())
    else {
      parts.push('```sql\n' + c.content.trim() + '\n```')
      if (results && c.result) {
        const md = resultMarkdown(c.result)
        if (md) parts.push(md)
      }
    }
  }
  return parts.filter(Boolean).join('\n\n') + '\n'
}

/** @param {Notebook} nb */
export function toSqlScript(nb) {
  const parts = [`-- ${nb.title || 'Notebook'}`]
  for (const c of nb.cells) {
    if (c.type === 'markdown') parts.push(c.content.trim().split('\n').map((l) => `-- ${l}`.trimEnd()).join('\n'))
    else parts.push(`-- %%\n${c.content.trim()}`)
  }
  return parts.join('\n\n') + '\n'
}

/** Jupyter nbformat 4, SQL as %%sql cells so JupySQL runs them. @param {Notebook} nb */
export function toIpynb(nb) {
  const src = (/** @type {string} */ s) => s.split(/(?<=\n)/)
  return JSON.stringify({
    nbformat: 4,
    nbformat_minor: 5,
    metadata: { title: nb.title, language_info: { name: 'sql' } },
    cells: nb.cells.map((c) =>
      c.type === 'markdown'
        ? { cell_type: 'markdown', id: c.id.slice(0, 8), metadata: {}, source: src(c.content) }
        : { cell_type: 'code', id: c.id.slice(0, 8), metadata: {}, execution_count: null, outputs: [], source: src(`%%sql\n${c.content}`) },
    ),
  }, null, 1)
}

/** @param {string} s */
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

/**
 * A self-contained HTML report: prose, queries and their results, readable in
 * any browser and printable to PDF. `renderMd` turns a markdown cell into
 * HTML (the app passes its sanitising renderer).
 * @param {Notebook} nb
 * @param {(md: string) => string} renderMd
 */
export function toHtml(nb, renderMd) {
  const body = nb.cells.map((c) => {
    if (c.type === 'markdown') return `<section class="md">${renderMd(c.content)}</section>`
    let out = `<pre class="sql"><code>${esc(c.content.trim())}</code></pre>`
    const r = c.result
    if (r?.error) out += `<p class="err">${esc(r.error)}</p>`
    else if (r?.columns.length) {
      const rows = r.rows.slice(0, MAX_EXPORT_ROWS)
      out += `<div class="tbl"><table><thead><tr>${r.columns.map((col) => `<th>${esc(col.name)}</th>`).join('')}</tr></thead><tbody>${rows
        .map((row) => `<tr>${row.map((v) => (v === null || v === undefined ? '<td class="null">NULL</td>' : `<td>${esc(cellText(v))}</td>`)).join('')}</tr>`)
        .join('')}</tbody></table></div><p class="meta">${r.rows.length} row${r.rows.length === 1 ? '' : 's'}${r.rows.length > MAX_EXPORT_ROWS ? ` (first ${MAX_EXPORT_ROWS} shown)` : ''} · ${r.queryMs} ms</p>`
    } else if (r?.message) out += `<p class="meta">${esc(r.message)}</p>`
    return `<section>${out}</section>`
  }).join('\n')
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${esc(nb.title || 'Notebook')}</title><style>
:root{color-scheme:light dark;--bg:#fff;--fg:#1b1b1d;--muted:#6b6f76;--line:#e6e6e8;--code:#f6f6f7}
@media (prefers-color-scheme:dark){:root{--bg:#0b0b0c;--fg:#ececee;--muted:#9a9ea6;--line:#26262a;--code:#141416}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.6 ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif}
main{max-width:920px;margin:0 auto;padding:40px 20px}h1{font-size:28px;margin:0 0 24px}section{margin:18px 0}
pre.sql{background:var(--code);border:1px solid var(--line);border-radius:8px;padding:12px 14px;overflow:auto;font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace}
.tbl{overflow:auto;border:1px solid var(--line);border-radius:8px;margin-top:8px}table{border-collapse:collapse;font:12.5px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace;width:100%}
th,td{border-bottom:1px solid var(--line);padding:6px 10px;text-align:left;white-space:nowrap}th{position:sticky;top:0;background:var(--code);font-weight:600}
td.null{color:var(--muted);font-style:italic}.meta{color:var(--muted);font-size:12px;margin:6px 0 0}.err{color:#d14343;font:13px ui-monospace,monospace}
.md code{background:var(--code);padding:1px 5px;border-radius:4px}@media print{pre.sql,.tbl{break-inside:avoid}}
</style></head><body><main><h1>${esc(nb.title || 'Notebook')}</h1>
${body}
<p class="meta">Exported from Stroke · ${new Date().toLocaleString()}</p></main></body></html>`
}

// ── Recent notebooks ─────────────────────────────────────────────────────────

const RECENT_KEY = 'stroke:notebooks:recent'

/** @returns {Array<{ path: string, title: string, at: number }>} */
export function recentNotebooks() {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]')
    return Array.isArray(v) ? v : []
  } catch {
    return []
  }
}

/** @param {string} path @param {string} title */
export function rememberNotebook(path, title) {
  try {
    const list = recentNotebooks().filter((r) => r.path !== path)
    list.unshift({ path, title, at: Date.now() })
    localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 12)))
  } catch { /* not kept */ }
}

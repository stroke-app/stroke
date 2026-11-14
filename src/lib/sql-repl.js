/**
 * A SQL prompt drawn in xterm for engines with no interactive CLI (Cloudflare
 * D1: `wrangler d1 execute` runs one command and exits). Nothing is spawned:
 * each statement goes through the same `executeSqlMulti` path as the SQL
 * editor, against the connection the app already holds, and the results are
 * printed as tables.
 *
 * Line editing: printable input, Backspace/Delete, ←/→, Home/End (Ctrl+A/E),
 * ↑/↓ history, Ctrl+U (clear line), Ctrl+C (drop the statement), Ctrl+L
 * Word keys: Ctrl+W / Alt+Backspace / Ctrl+Backspace (delete word back),
 * Alt+D (delete word forward), Alt+←/→ and Ctrl+←/→ (move by word), Ctrl+K
 * (delete to end of line)
 * (clear screen). A statement runs once it ends with `;`, or on an empty line
 * while one is being continued. Dot-commands (.tables .schema [table] .clear
 * .help) and the shell words people type out of habit (ls, clear, help, exit)
 * work at any point, including in the middle of a statement.
 */

const DIM = '\x1b[2m'
const BOLD = '\x1b[1m'
const RED = '\x1b[31m'
const CYAN = '\x1b[36m'
const RESET = '\x1b[0m'

/** Word boundary for the line editor: one run of word chars or of punctuation, after spaces. */
function wordEdge(/** @type {string} */ v, /** @type {number} */ at, /** @type {boolean} */ forward) {
  const ws = (/** @type {string} */ c) => /\s/.test(c)
  const wc = (/** @type {string} */ c) => /[\p{L}\p{N}_]/u.test(c)
  if (forward) {
    let j = at
    while (j < v.length && ws(v[j])) j++
    if (j < v.length) { const w = wc(v[j]); while (j < v.length && !ws(v[j]) && wc(v[j]) === w) j++ }
    return j
  }
  let i = at
  while (i > 0 && ws(v[i - 1])) i--
  if (i > 0) { const w = wc(v[i - 1]); while (i > 0 && !ws(v[i - 1]) && wc(v[i - 1]) === w) i-- }
  return i
}
const MAX_ROWS = 200
const MAX_CELL = 48

const SQL_WORDS = ['SELECT', 'FROM', 'WHERE', 'AND', 'OR', 'NOT', 'NULL', 'IS', 'IN', 'LIKE', 'BETWEEN', 'ORDER', 'BY', 'GROUP', 'HAVING', 'LIMIT', 'OFFSET', 'JOIN', 'LEFT', 'INNER', 'ON', 'AS', 'DISTINCT', 'COUNT', 'INSERT', 'INTO', 'VALUES', 'UPDATE', 'SET', 'DELETE', 'CREATE', 'TABLE', 'INDEX', 'VIEW', 'DROP', 'ALTER', 'ADD', 'COLUMN', 'PRIMARY', 'KEY', 'DEFAULT', 'EXPLAIN', 'PRAGMA', 'BEGIN', 'COMMIT', 'ROLLBACK', 'ASC', 'DESC', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END', 'UNION', 'EXISTS', 'RETURNING']
const PARTIQL_WORDS = ['SELECT', 'FROM', 'WHERE', 'AND', 'OR', 'NOT', 'IN', 'BETWEEN', 'IS', 'MISSING', 'NULL', 'EXISTS', 'BEGINS_WITH', 'CONTAINS', 'SIZE', 'ATTRIBUTE_TYPE', 'INSERT', 'INTO', 'VALUE', 'UPDATE', 'SET', 'REMOVE', 'DELETE', 'RETURNING', 'ALL', 'OLD', 'NEW', 'ORDER', 'BY', 'ASC', 'DESC']
const DOT_COMMANDS = ['.tables', '.schema', '.clear', '.help', '.exit']
/** Keywords after which only a table name makes sense. */
const TABLE_CONTEXT = /\b(from|join|into|update|table|exists)\s+$/i

/** @param {unknown} v */
function cellText(v) {
  if (v === null || v === undefined) return 'NULL'
  const t = typeof v === 'object' ? JSON.stringify(v) : String(v)
  // Tabs and newlines jump the cursor in a terminal, so one in a cell pushed
  // that row's right border out of line. Every run of whitespace is one space.
  return t.replace(/\s+/g, ' ').replace(/[\x00-\x1f\x7f]/g, '').trim()
}

/** Display width, counting wide (CJK/emoji) code points as 2 columns. @param {string} s */
function width(s) {
  let w = 0
  for (const ch of s) w += /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]|[\u{1f300}-\u{1faff}]/u.test(ch) ? 2 : 1
  return w
}

/** @param {string} s @param {number} n */
function clip(s, n) {
  if (width(s) <= n) return s
  let out = ''
  for (const ch of s) {
    if (width(out + ch) > n - 1) break
    out += ch
  }
  return out + '…'
}

/**
 * Box-drawn table for one result set, narrowed to fit `maxWidth` columns:
 * the widest column gives way first, down to 8 characters.
 * @param {{ name: string }[]} columns @param {unknown[][]} rows @param {number} [maxWidth]
 */
export function formatTable(columns, rows, maxWidth = 120) {
  const shown = rows.slice(0, MAX_ROWS)
  const fullHead = columns.map((c) => cellText(c.name))
  const fullBody = shown.map((r) => columns.map((_, i) => cellText(r[i])))
  const numeric = columns.map((_, i) => shown.length > 0 && shown.every((r) => r[i] == null || typeof r[i] === 'number'))
  const caps = fullHead.map((h, i) => Math.min(MAX_CELL * 2, Math.max(width(h), ...fullBody.map((r) => width(r[i])))))
  // Borders and padding: 1 + 3 per column.
  const budget = Math.max(columns.length * 9, maxWidth - 1 - columns.length * 3)
  while (caps.reduce((a, b) => a + b, 0) > budget) {
    const i = caps.indexOf(Math.max(...caps))
    if (caps[i] <= 8) break
    caps[i] -= 1
  }
  const head = fullHead.map((h, i) => clip(h, caps[i]))
  const body = fullBody.map((r) => r.map((c, i) => clip(c, caps[i])))
  const w = head.map((h, i) => Math.max(width(h), ...body.map((r) => width(r[i]))))
  /** @param {string} s @param {number} i */
  const pad = (s, i) => (numeric[i] ? ' '.repeat(w[i] - width(s)) + s : s + ' '.repeat(w[i] - width(s)))
  const line = (/** @type {string} */ l, /** @type {string} */ m, /** @type {string} */ r) => DIM + l + w.map((n) => '─'.repeat(n + 2)).join(m) + r + RESET
  const out = [
    line('┌', '┬', '┐'),
    DIM + '│' + RESET + head.map((h, i) => ` ${BOLD}${pad(h, i)}${RESET} `).join(DIM + '│' + RESET) + DIM + '│' + RESET,
    line('├', '┼', '┤'),
    ...body.map((r) => DIM + '│' + RESET + r.map((c, i) => ` ${c === 'NULL' ? DIM + pad(c, i) + RESET : pad(c, i)} `).join(DIM + '│' + RESET) + DIM + '│' + RESET),
    line('└', '┴', '┘'),
  ]
  return out.join('\r\n')
}

/**
 * @param {{
 *   write: (s: string) => void,
 *   run: (sql: string) => Promise<any[]>,
 *   name: string,
 *   historyKey?: string,
 *   language?: string,
 *   cols?: () => number,
 *   listTables?: () => Promise<{ columns: { name: string }[], rows: unknown[][] }>,
 *   describe?: (table: string) => Promise<{ columns: { name: string }[], rows: unknown[][] }>,
 * }} opts
 */
export function createSqlRepl({ write, run, name, historyKey, language = 'SQL', listTables, describe, cols }) {
  /** @type {string[]} */
  let history = []
  try { history = JSON.parse(localStorage.getItem(historyKey ?? '') ?? '[]') } catch { history = [] }
  let histIdx = -1
  let line = ''
  let cursor = 0
  /** @type {string[]} */
  let pending = []
  let busy = false
  /** Table names for Tab completion; loaded on the first Tab, dropped after DDL. @type {string[] | null} */
  let tableCache = null
  /** Line + cursor of the last ambiguous Tab, so a second Tab lists the matches. */
  let lastTab = ''
  const keywords = language === 'PartiQL' ? PARTIQL_WORDS : SQL_WORDS

  const prompt = () => (pending.length ? `${DIM}${' '.repeat(Math.max(0, name.length - 2))}..> ${RESET}` : `${CYAN}${name}${RESET}${DIM}>${RESET} `)

  function redraw() {
    write('\r\x1b[2K' + prompt() + line)
    const back = width(line.slice(cursor))
    if (back) write(`\x1b[${back}D`)
  }

  function saveHistory() {
    if (!historyKey) return
    try { localStorage.setItem(historyKey, JSON.stringify(history.slice(-200))) } catch { /* private mode */ }
  }

  const colsOf = cols
  /** @param {any} r */
  function printResult(r) {
    const cols = r?.columns ?? []
    const rows = r?.rows ?? []
    const ms = r?.queryMs != null ? ` ${DIM}(${r.queryMs} ms)${RESET}` : ''
    if (cols.length) {
      write(formatTable(cols, rows, (colsOf?.() ?? 120) - 1) + '\r\n')
      const more = rows.length > MAX_ROWS ? `, first ${MAX_ROWS} shown` : ''
      write(`${DIM}${rows.length.toLocaleString()} ${rows.length === 1 ? 'row' : 'rows'}${more}${RESET}${ms}\r\n`)
    } else {
      const n = r?.rowCount
      write(`${DIM}${r?.message || (n != null ? `${n} ${n === 1 ? 'row' : 'rows'} affected` : 'OK')}${RESET}${ms}\r\n`)
    }
  }

  /** @param {string} sql */
  async function execute(sql) {
    if (/\b(create|drop|alter)\s+(table|view)\b/i.test(sql)) tableCache = null
    busy = true
    try {
      const results = await run(sql)
      for (const r of results ?? []) printResult(r)
    } catch (e) {
      write(`${RED}${String(e).replace(/^Error:\s*/, '')}${RESET}\r\n`)
    } finally {
      busy = false
    }
  }

  /** @param {string} cmd */
  async function dot(cmd) {
    const [c, arg] = cmd.trim().split(/\s+/, 2)
    if (c === '.tables') tableCache = null
    if (c === '.help') {
      write([
        `${BOLD}${language} statements${RESET} end with ${BOLD};${RESET} (or an empty line) and run against ${name} through the app's connection.`,
        `  .tables  (ls)          list tables`,
        `  .schema [table]        ${describe ? 'keys and indexes' : 'CREATE statements'}`,
        `  .clear   (clear)       clear the screen (Ctrl+L)`,
        `  Tab completes table names, keywords and commands (Tab twice lists the matches)`,
        `  ↑ ↓ history · Ctrl+C cancel the statement · Ctrl+U clear the line`,
      ].join('\r\n') + '\r\n')
    } else if (c === '.exit' || c === '.quit') {
      write(`${DIM}Close the panel with ⌘J. Nothing to exit: the prompt runs over the app's connection.${RESET}\r\n`)
    } else if (c === '.tables' && listTables) {
      await shown(listTables)
    } else if (c === '.schema' && describe) {
      if (!arg) write(`${DIM}Usage: .schema <table>${RESET}\r\n`)
      else await shown(() => describe(arg))
    } else if (c === '.tables') {
      await execute("SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%' ORDER BY name")
    } else if (c === '.schema') {
      const where = arg ? ` AND name = '${arg.replace(/'/g, "''")}'` : " AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%'"
      // As sqlite3 prints it: each statement as written, not squeezed into a cell.
      busy = true
      try {
        const res = await run(`SELECT sql FROM sqlite_master WHERE sql IS NOT NULL${where} ORDER BY tbl_name, type DESC, name`)
        const stmts = (res?.[0]?.rows ?? []).map((/** @type {unknown[]} */ r) => String(r[0]).replace(/\t/g, '  ').replace(/\r?\n/g, '\r\n') + ';')
        if (!stmts.length) write(`${DIM}${arg ? `No table named ${arg}` : 'No tables'}${RESET}\r\n`)
        else write(stmts.join('\r\n') + '\r\n')
      } catch (e) {
        write(`${RED}${String(e).replace(/^Error:\s*/, '')}${RESET}\r\n`)
      } finally {
        busy = false
      }
    } else if (c === '.clear') {
      write('\x1b[2J\x1b[3J\x1b[H')
    } else {
      write(`${RED}Unknown command ${c}. Try .help${RESET}\r\n`)
    }
  }

  /** @param {() => Promise<{ columns: { name: string }[], rows: unknown[][] }>} fn */
  async function shown(fn) {
    busy = true
    try {
      const r = await fn()
      printResult(r)
    } catch (e) {
      write(`${RED}${String(e).replace(/^Error:\s*/, '')}${RESET}\r\n`)
    } finally {
      busy = false
    }
  }

  async function loadTables() {
    if (tableCache) return tableCache
    try {
      if (listTables) {
        const r = await listTables()
        tableCache = r.rows.map((row) => String(row[0]))
      } else {
        const res = await run("SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%' ORDER BY name")
        tableCache = (res?.[0]?.rows ?? []).map((/** @type {unknown[]} */ row) => String(row[0]))
      }
    } catch {
      tableCache = []
    }
    return tableCache
  }

  /** A table name as it has to be typed: bare when it is a plain identifier. @param {string} n */
  const quoteName = (n) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(n) ? n : `"${n.replace(/"/g, '""')}"`)

  /** @param {string[]} xs */
  function commonPrefix(xs) {
    if (!xs.length) return ''
    let p = xs[0]
    for (const x of xs) while (!x.toLowerCase().startsWith(p.toLowerCase())) p = p.slice(0, -1)
    return p
  }

  /** @param {string[]} items */
  function printColumns(items) {
    const w = Math.max(...items.map(width)) + 2
    const per = Math.max(1, Math.floor(80 / w))
    const rows = []
    for (let i = 0; i < items.length; i += per) rows.push(items.slice(i, i + per).map((x) => x + ' '.repeat(w - width(x))).join(''))
    write('\r\n' + rows.join('\r\n') + '\r\n')
  }

  async function complete() {
    const before = line.slice(0, cursor)
    const atStart = !pending.length && !before.trim().includes(' ')
    // `.sch` → `.schema`; `.schema us` → a table.
    const dotArg = /^\s*\.schema\s+("?[^\s"]*)$/i.exec(before)
    const m = /("?)([^\s"(),;=<>]*)$/.exec(before)
    if (!m) return
    const typed = m[0]
    const quoted = m[1] === '"'
    const word = m[2]
    /** @type {{ text: string, label: string }[]} */
    let cands
    if (atStart && typed.startsWith('.') && !dotArg) {
      cands = DOT_COMMANDS.filter((c) => c.startsWith(typed.toLowerCase())).map((c) => ({ text: c + ' ', label: c }))
    } else {
      const ctx = before.slice(0, before.length - typed.length)
      const tablesOnly = !!dotArg || TABLE_CONTEXT.test(ctx) || quoted
      const names = await loadTables()
      // The line moved on while the names loaded: don't type into it.
      if (line.slice(0, cursor) !== before) return
      const lw = word.toLowerCase()
      cands = names.filter((n) => n.toLowerCase().startsWith(lw)).map((n) => ({ text: (dotArg ? n : quoted ? `"${n.replace(/"/g, '""')}"` : quoteName(n)) + ' ', label: n }))
      if (!tablesOnly && word) {
        const upper = word === word.toUpperCase() && /[A-Z]/.test(word)
        cands.push(...keywords.filter((k) => k.toLowerCase().startsWith(lw) && k.toLowerCase() !== lw).map((k) => ({ text: (upper ? k : k.toLowerCase()) + ' ', label: upper ? k : k.toLowerCase() })))
      }
    }
    if (!cands.length) { write('\x07'); return }
    /** @param {string} text */
    const insert = (text) => {
      line = line.slice(0, cursor - typed.length) + text + line.slice(cursor)
      cursor += text.length - typed.length
      redraw()
    }
    if (cands.length === 1) { lastTab = ''; insert(cands[0].text); return }
    // Several: extend to what they share; a second Tab lists them.
    const shared = commonPrefix(cands.map((c) => c.text.trimEnd()))
    // Extended to the shared part: the next Tab, with nothing more to add, lists them.
    if (shared.length > typed.length) { insert(shared); lastTab = line + '|' + cursor; return }
    const key = line + '|' + cursor
    if (lastTab === key) {
      lastTab = ''
      printColumns(cands.map((c) => c.label))
      redraw()
    } else {
      lastTab = key
      write('\x07')
    }
  }

  /** Words typed out of shell habit, as the dot-command they mean. */
  const SHELL_WORDS = /** @type {Record<string, string>} */ ({
    ls: '.tables', tables: '.tables', '\\dt': '.tables', 'show tables': '.tables',
    clear: '.clear', cls: '.clear',
    help: '.help', '?': '.help', '\\?': '.help',
    exit: '.exit', quit: '.exit', '\\q': '.exit',
  })

  async function submit() {
    write('\r\n')
    const text = line
    line = ''
    cursor = 0
    histIdx = -1
    const word = text.trim().replace(/;$/, '').toLowerCase()
    const cmd = text.trim().startsWith('.') ? text.trim() : SHELL_WORDS[word]
    if (cmd) {
      // Works mid-statement too: what was being typed is kept, not swallowed.
      if (history[history.length - 1] !== text.trim()) { history.push(text.trim()); saveHistory() }
      await dot(cmd)
      write(prompt())
      return
    }
    // An empty line while continuing runs what is there, so a forgotten `;`
    // never leaves you stuck at `..>`.
    const runNow = !text.trim() && pending.length > 0
    if (text.trim()) pending.push(text)
    const stmt = pending.join('\n').trim()
    if (!stmt) { write(prompt()); return }
    if (!runNow && !/;\s*$/.test(stmt)) {
      if (pending.length === 1) write(`${DIM}  … end with ; or press Enter on an empty line to run · Ctrl+C to cancel${RESET}\r\n`)
      write(prompt())
      return
    }
    pending = []
    if (history[history.length - 1] !== stmt) { history.push(stmt); saveHistory() }
    await execute(stmt)
    write(prompt())
  }

  /** @param {string} data */
  function input(data) {
    if (busy) return
    // Pasted text: run each complete line, keep the last as the input.
    if (data.length > 1 && /[\r\n]/.test(data) && !data.startsWith('\x1b')) {
      const parts = data.replace(/\r\n?/g, '\n').split('\n')
      const last = parts.pop() ?? ''
      void (async () => {
        for (const p of parts) { line += p; cursor = line.length; redraw(); await submit() }
        line = last; cursor = line.length; redraw()
      })()
      return
    }
    if (data !== '\t') lastTab = ''
    switch (data) {
      case '\t': void complete(); return
      case '\r': void submit(); return
      case '\x7f':
        if (cursor > 0) { line = line.slice(0, cursor - 1) + line.slice(cursor); cursor--; redraw() }
        return
      case '\x1b[3~':
        if (cursor < line.length) { line = line.slice(0, cursor) + line.slice(cursor + 1); redraw() }
        return
      // Ctrl+D: delete under the cursor, as bash, psql and sqlite3 do mid-line.
      // On an empty line there is no session to end, so it only rings.
      case '\x04':
        if (cursor < line.length) { line = line.slice(0, cursor) + line.slice(cursor + 1); redraw() }
        else if (!line) write('\x07')
        return
      // Ctrl+X: cut the line to the clipboard (what Windows users expect).
      case '\x18':
        if (!line) { write('\x07'); return }
        try { void navigator.clipboard?.writeText(line).catch(() => {}) } catch { /* no clipboard */ }
        line = ''; cursor = 0; redraw()
        return
      case '\x1b[D': if (cursor > 0) { cursor--; write('\x1b[D') } return
      case '\x1b[C': if (cursor < line.length) { cursor++; write('\x1b[C') } return
      case '\x1b[H': case '\x01': cursor = 0; redraw(); return
      case '\x1b[F': case '\x05': cursor = line.length; redraw(); return
      case '\x15': line = line.slice(cursor); cursor = 0; redraw(); return
      // Delete the word before the cursor: ^W, ESC DEL (Alt/Option+Backspace),
      // and ^H, which is how some terminals send Ctrl+Backspace.
      case '\x17': case '\x1b\x7f': case '\x08': {
        const from = wordEdge(line, cursor, false)
        if (from < cursor) { line = line.slice(0, from) + line.slice(cursor); cursor = from; redraw() }
        return
      }
      case '\x1bd': case '\x1b[3;5~': case '\x1b[3;3~': {
        const to = wordEdge(line, cursor, true)
        if (to > cursor) { line = line.slice(0, cursor) + line.slice(to); redraw() }
        return
      }
      case '\x0b': line = line.slice(0, cursor); redraw(); return
      case '\x1bb': case '\x1b[1;5D': case '\x1b[1;3D': cursor = wordEdge(line, cursor, false); redraw(); return
      case '\x1bf': case '\x1b[1;5C': case '\x1b[1;3C': cursor = wordEdge(line, cursor, true); redraw(); return
      case '\x1f': return // Ctrl+Z: nothing to undo here, and never a literal
      case '\x0c': write('\x1b[2J\x1b[3J\x1b[H'); redraw(); return
      case '\x03':
        write('^C\r\n'); pending = []; line = ''; cursor = 0; histIdx = -1; write(prompt()); return
      case '\x1b[A':
        if (!history.length) return
        histIdx = histIdx < 0 ? history.length - 1 : Math.max(0, histIdx - 1)
        line = history[histIdx].replace(/\n/g, ' '); cursor = line.length; redraw(); return
      case '\x1b[B':
        if (histIdx < 0) return
        histIdx = histIdx + 1 >= history.length ? -1 : histIdx + 1
        line = histIdx < 0 ? '' : history[histIdx].replace(/\n/g, ' '); cursor = line.length; redraw(); return
    }
    if (/^[^\x00-\x1f\x7f]+$/.test(data)) {
      line = line.slice(0, cursor) + data + line.slice(cursor)
      cursor += data.length
      redraw()
    }
  }

  function start() {
    write(`${DIM}Built-in ${language} prompt for ${name} · end statements with ; · .help for commands${RESET}\r\n`)
    write(prompt())
  }

  return { input, start }
}

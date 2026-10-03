/**
 * Lightweight SQL statement splitter used by the SQL editor for
 * statement-at-cursor actions (select / run) and the active-statement gutter.
 *
 * Understands enough SQL lexing to not split inside:
 *  - single/double-quoted strings and backtick identifiers ('' and \' escapes)
 *  - line comments (`-- …`) and block comments
 *  - Postgres dollar-quoted bodies ($$ … $$, $tag$ … $tag$)
 *
 * @typedef {{ text: string, start: number, end: number }} SqlStatement
 *   `start`/`end` are character offsets into the source text; `end` is
 *   exclusive and includes the terminating semicolon when present.
 */

/**
 * @param {string} text
 * @returns {SqlStatement[]}
 */
export function splitSqlStatements(text) {
  /** @type {SqlStatement[]} */
  const out = []
  const n = text.length
  let i = 0
  let start = 0

  /** @param {number} end exclusive boundary (just past the `;` or EOF) */
  function flush(end) {
    let s = start
    let e = end
    while (s < e && /\s/.test(text[s])) s++
    while (e > s && /\s/.test(text[e - 1])) e--
    if (e > s) {
      const t = text.slice(s, e)
      // Skip fragments that are only comments/semicolons
      const meaningful = t.replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, '').replace(/;/g, '').trim()
      if (meaningful) out.push({ text: t, start: s, end: e })
    }
    start = end
  }

  while (i < n) {
    const ch = text[i]
    const next = text[i + 1]
    if (ch === '-' && next === '-') {
      const nl = text.indexOf('\n', i + 2)
      i = nl === -1 ? n : nl + 1
    } else if (ch === '/' && next === '*') {
      const close = text.indexOf('*/', i + 2)
      i = close === -1 ? n : close + 2
    } else if (ch === "'" || ch === '"' || ch === '`') {
      i++
      while (i < n) {
        if (ch === "'" && text[i] === '\\') { i += 2; continue }
        if (text[i] === ch) {
          // '' inside a single-quoted string is an escaped quote, not the end
          if (ch === "'" && text[i + 1] === "'") { i += 2; continue }
          i++
          break
        }
        i++
      }
    } else if (ch === '$') {
      const m = /^\$[A-Za-z_][A-Za-z0-9_]*\$|^\$\$/.exec(text.slice(i))
      if (m) {
        const tag = m[0]
        const close = text.indexOf(tag, i + tag.length)
        i = close === -1 ? n : close + tag.length
      } else {
        i++
      }
    } else if (ch === ';') {
      i++
      flush(i)
    } else {
      i++
    }
  }
  flush(n)
  return out
}

/**
 * @typedef {{ message: string, severity: 'error' | 'warning', start: number, end: number }} SqlDiagnostic
 *   Offsets are into the source text; `end` exclusive.
 */

/**
 * Lightweight SQL lint - catches lexical problems worth flagging while typing:
 * unterminated strings/identifiers, unclosed block comments and dollar quotes,
 * unbalanced parentheses, and a `;` missing between two statements.
 *
 * @param {string} text
 * @returns {SqlDiagnostic[]}
 */
export function lintSql(text) {
  /** @type {SqlDiagnostic[]} */
  const diags = []
  const n = text.length
  let i = 0
  /** @type {number[]} open-paren offsets */
  const parens = []

  while (i < n) {
    const ch = text[i]
    const next = text[i + 1]
    if (ch === '-' && next === '-') {
      const nl = text.indexOf('\n', i + 2)
      i = nl === -1 ? n : nl + 1
    } else if (ch === '/' && next === '*') {
      const close = text.indexOf('*/', i + 2)
      if (close === -1) {
        diags.push({ message: 'Unclosed block comment: missing */', severity: 'warning', start: i, end: n })
        i = n
      } else {
        i = close + 2
      }
    } else if (ch === "'" || ch === '"' || ch === '`') {
      const qStart = i
      i++
      let closed = false
      while (i < n) {
        if (ch === "'" && text[i] === '\\') { i += 2; continue }
        if (text[i] === ch) {
          if (ch === "'" && text[i + 1] === "'") { i += 2; continue }
          i++
          closed = true
          break
        }
        i++
      }
      if (!closed) {
        diags.push({
          message: ch === "'" ? "Unterminated string, missing closing '" : `Unterminated quoted identifier, missing closing ${ch}`,
          severity: 'error',
          start: qStart,
          end: n,
        })
      }
    } else if (ch === '$') {
      const m = /^\$[A-Za-z_][A-Za-z0-9_]*\$|^\$\$/.exec(text.slice(i))
      if (m) {
        const tag = m[0]
        const close = text.indexOf(tag, i + tag.length)
        if (close === -1) {
          diags.push({ message: `Unterminated dollar-quoted string: missing closing ${tag}`, severity: 'error', start: i, end: n })
          i = n
        } else {
          i = close + tag.length
        }
      } else {
        i++
      }
    } else if (ch === '(') {
      parens.push(i)
      i++
    } else if (ch === ')') {
      if (parens.length === 0) {
        diags.push({ message: 'Unmatched closing parenthesis', severity: 'error', start: i, end: i + 1 })
      } else {
        parens.pop()
      }
      i++
    } else {
      i++
    }
  }

  for (const p of parens) {
    diags.push({ message: 'Unclosed parenthesis', severity: 'warning', start: p, end: p + 1 })
  }

  // A `;` missed between two statements: they would run as one and fail. The
  // tell is a blank line, then a statement keyword, outside any parentheses.
  // (This used to flag a last statement with no `;` - which runs fine - so
  // every one-line query carried a warning.)
  const statements = splitSqlStatements(text)
  // With several statements in the buffer, each one ends in its `;`: a
  // statement left open (often the last, half-written one) is warned at its
  // last word. A buffer holding one query stays clean, `;` or not: it runs fine.
  if (statements.length > 1) {
    for (const stmt of statements) {
      const body = text.slice(stmt.start, stmt.end).replace(/(\s|--[^\n]*)+$/, '')
      if (!body || body.endsWith(';')) continue
      const last = /[^\s]+$/.exec(body)
      const end = stmt.start + body.length
      diags.push({
        message: "Missing ';' at the end of this statement",
        severity: 'warning',
        start: last ? end - last[0].length : end - 1,
        end,
      })
    }
  }

  for (const stmt of statements) {
    const body = text.slice(stmt.start, stmt.end)
    // A CTE's main query, or a set operation's next arm, may follow a blank line.
    if (/^\s*with\b/i.test(body)) continue
    const re = /\n[ \t]*\r?\n\s*(select|insert|update|delete|with|create|alter|drop|truncate|explain|grant|revoke)\b/gi
    for (let m; (m = re.exec(body)); ) {
      const before = body.slice(0, m.index)
      if (parenDepth(before) !== 0) continue
      if (/(\(|,|\b(union|intersect|except|all|as|in|exists))\s*$/i.test(before)) continue
      const at = stmt.start + m.index + m[0].length - m[1].length
      diags.push({
        message: "Missing ';' - this starts a new statement, so the two would run as one",
        severity: 'warning',
        start: at,
        end: at + m[1].length,
      })
    }
  }

  return diags
}

/**
 * Open parentheses at the end of `text`, not counting any inside strings,
 * quoted names or comments.
 * @param {string} text
 */
function parenDepth(text) {
  let depth = 0
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (c === "'" || c === '"' || c === '`') {
      const close = text.indexOf(c, i + 1)
      if (close === -1) return depth
      i = close
    } else if (c === '-' && text[i + 1] === '-') {
      const nl = text.indexOf('\n', i)
      if (nl === -1) return depth
      i = nl
    } else if (c === '(') depth++
    else if (c === ')') depth = Math.max(0, depth - 1)
  }
  return depth
}

/**
 * Find the statement the cursor is in. When the cursor sits between two
 * statements (blank line after a `;`), prefer the previous statement; before
 * the first statement, return the first.
 *
 * @param {SqlStatement[]} statements
 * @param {number} offset
 * @returns {SqlStatement | null}
 */
export function statementAtOffset(statements, offset) {
  if (statements.length === 0) return null
  let prev = null
  for (const s of statements) {
    if (offset >= s.start && offset <= s.end) return s
    if (s.end < offset) prev = s
    else if (s.start > offset) return prev ?? s
  }
  return prev
}

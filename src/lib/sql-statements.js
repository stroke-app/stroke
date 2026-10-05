/**
 * Lightweight SQL statement splitter used by the SQL editor for
 * statement-at-cursor actions (select / run) and the active-statement gutter.
 *
 * Understands enough SQL lexing to not split inside:
 *  - single/double-quoted strings and backtick identifiers ('' and \' escapes)
 *  - line comments (`-- …`) and block comments
 *  - Postgres dollar-quoted bodies ($$ … $$, $tag$ … $tag$)
 *  - the BEGIN … END body of a routine or trigger: `CREATE TRIGGER … BEGIN
 *    UPDATE …; END;` is one statement (MySQL, SQLite, T-SQL, BEGIN ATOMIC)
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
  // Open blocks inside a routine or trigger body: BEGIN and CASE open one, END
  // closes one. END IF / END LOOP / END WHILE / END REPEAT close blocks this
  // never counted, so they leave it alone.
  let depth = 0
  /** Whether the statement being read has a body; worked out at its first BEGIN, CASE or END. @type {boolean | null} */
  let compound = null

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
      if (depth > 0) continue
      flush(i)
      compound = null
    } else if (/[A-Za-z_]/.test(ch) && (i === 0 || !/\w/.test(text[i - 1]))) {
      WORD.lastIndex = i
      const word = /** @type {RegExpExecArray} */ (WORD.exec(text))[0].toUpperCase()
      let end = i + word.length
      if (word === 'BEGIN' || word === 'CASE' || word === 'END') {
        compound ??= isCompoundHead(text.slice(start, i))
        if (compound) {
          NEXT_WORD.lastIndex = end
          const after = NEXT_WORD.exec(text)
          const next = after ? after[1].toUpperCase() : ''
          if (word === 'BEGIN') {
            // BEGIN TRAN in a T-SQL body starts a transaction, not a block.
            if (!['TRAN', 'TRANSACTION', 'WORK', 'DISTRIBUTED'].includes(next)) depth++
          } else if (word === 'CASE') {
            depth++
          } else if (next === 'CASE') {
            depth = Math.max(0, depth - 1)
            end = /** @type {RegExpExecArray} */ (after).index + after[0].length
          } else if (!['IF', 'LOOP', 'WHILE', 'REPEAT'].includes(next)) {
            depth = Math.max(0, depth - 1)
          }
        }
      }
      i = end
    } else {
      i++
    }
  }
  flush(n)
  return out
}

const WORD = /\w+/y
const NEXT_WORD = /\s*(\w*)/y

/**
 * Whether a statement, read up to its first BEGIN, CASE or END, defines a
 * routine or trigger whose body holds statements of its own. Only the part
 * before the first `(` counts: `CREATE TABLE event (…)` is not an event.
 * @param {string} head
 */
function isCompoundHead(head) {
  const text = head.replace(/^(?:\s+|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/)*/, '')
  if (!/^(create|alter)\b/i.test(text)) return false
  return /\b(trigger|procedure|proc|function|event)\b/i.test(text.split('(')[0])
}

/**
 * @typedef {{ label: string, from: number, to: number, insert: string }} SqlFix
 *   One edit that resolves the problem: replace `from`..`to` with `insert`.
 * @typedef {{ message: string, severity: 'error' | 'warning', start: number, end: number, fix?: SqlFix }} SqlDiagnostic
 *   Offsets are into the source text; `end` exclusive.
 */

/** End of the line `at` is on (before its newline). @param {string} text @param {number} at */
const lineEnd = (text, at) => { const nl = text.indexOf('\n', at); return nl === -1 ? text.length : nl }

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
        diags.push({ message: 'Unclosed block comment: missing */', severity: 'warning', start: i, end: n, fix: { label: 'Close the comment', from: n, to: n, insert: ' */' } })
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
        // Closed at the end of the line it opened on: a string or name
        // rarely means to swallow the rest of the script.
        const at = lineEnd(text, qStart)
        diags.push({
          message: ch === "'" ? "Unterminated string, missing closing '" : `Unterminated quoted identifier, missing closing ${ch}`,
          severity: 'error',
          start: qStart,
          end: n,
          fix: { label: ch === "'" ? 'Close the string' : 'Close the name', from: at, to: at, insert: ch },
        })
      }
    } else if (ch === '$') {
      const m = /^\$[A-Za-z_][A-Za-z0-9_]*\$|^\$\$/.exec(text.slice(i))
      if (m) {
        const tag = m[0]
        const close = text.indexOf(tag, i + tag.length)
        if (close === -1) {
          diags.push({ message: `Unterminated dollar-quoted string: missing closing ${tag}`, severity: 'error', start: i, end: n, fix: { label: `Close with ${tag}`, from: n, to: n, insert: tag } })
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
        diags.push({ message: 'Unmatched closing parenthesis', severity: 'error', start: i, end: i + 1, fix: { label: 'Remove )', from: i, to: i + 1, insert: '' } })
      } else {
        parens.pop()
      }
      i++
    } else {
      i++
    }
  }

  // A `;` missed between two statements: they would run as one and fail. The
  // tell is a blank line, then a statement keyword, outside any parentheses.
  // (This used to flag a last statement with no `;` - which runs fine - so
  // every one-line query carried a warning.)
  const statements = splitSqlStatements(text)

  for (const p of parens) {
    // The `)` goes at the end of the statement the bracket opened in, before
    // its `;`: the usual miss is the last one.
    const stmt = statements.find((st) => p >= st.start && p < st.end)
    const body = stmt ? text.slice(stmt.start, stmt.end).replace(/;\s*$/, '').replace(/(\s|--[^\n]*)+$/, '') : ''
    const at = stmt ? stmt.start + body.length : n
    diags.push({ message: 'Unclosed parenthesis', severity: 'warning', start: p, end: p + 1, fix: { label: 'Add )', from: at, to: at, insert: ')' } })
  }
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
        fix: { label: 'Add ;', from: end, to: end, insert: ';' },
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
      // Inside a routine body the statements before it end in their own `;`.
      if (/(\(|,|;|\b(union|intersect|except|all|as|in|exists|begin|then|else|do|loop|repeat))\s*$/i.test(before)) continue
      const at = stmt.start + m.index + m[0].length - m[1].length
      // After the last word of the statement before the blank line.
      const prevEnd = stmt.start + before.replace(/(\s|--[^\n]*)+$/, '').length
      diags.push({
        message: "Missing ';' - this starts a new statement, so the two would run as one",
        severity: 'warning',
        start: at,
        end: at + m[1].length,
        fix: { label: 'Add ; before it', from: prevEnd, to: prevEnd, insert: ';' },
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

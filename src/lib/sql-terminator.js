/**
 * When a statement wants its closing `;`.
 *
 * Two places ask. The editor shows a faint `;` after the caret when the
 * statement ending there looks finished, and Tab writes it. Statement
 * snippets (`sel`, `ins`, `upd`, `ct`...) end with one when they finish the
 * statement they are typed into. Both are rules on the text, not a model:
 * instant, offline, and never wrong in a way that is hard to see.
 *
 * "Looks finished" is deliberately cautious. A statement that ends on a word
 * that has to be followed by something (FROM, WHERE, AND, JOIN, SET, a comma,
 * an operator, an open bracket or string) is still being written, and so is a
 * bare `SELECT *`. Enter never adds the `;` by itself: after `FROM users` the
 * next line is as likely to be `WHERE` as a new statement.
 */

/** First words of a statement that can stand alone. */
const STARTERS = new Set([
  'select', 'with', 'values', 'table', 'insert', 'update', 'delete', 'merge', 'replace', 'upsert',
  'create', 'alter', 'drop', 'truncate', 'rename', 'comment', 'grant', 'revoke',
  'explain', 'analyze', 'analyse', 'vacuum', 'reindex', 'cluster', 'refresh', 'optimize',
  'show', 'describe', 'desc', 'pragma', 'call', 'exec', 'execute', 'set', 'reset', 'use',
  'begin', 'start', 'commit', 'rollback', 'savepoint', 'release', 'lock', 'copy', 'attach', 'detach',
])

/** Words a statement cannot end on: something has to follow them. */
const OPEN_ENDED = new Set([
  'select', 'from', 'where', 'and', 'or', 'not', 'join', 'inner', 'left', 'right', 'full', 'outer',
  'cross', 'natural', 'lateral', 'on', 'using', 'by', 'group', 'order', 'having', 'limit', 'offset',
  'fetch', 'set', 'values', 'into', 'as', 'in', 'is', 'like', 'ilike', 'between', 'when', 'then',
  'else', 'case', 'union', 'intersect', 'except', 'all', 'distinct', 'returning', 'to', 'add',
  'column', 'index', 'view', 'if', 'exists', 'references', 'check', 'constraint', 'primary',
  'foreign', 'unique', 'partition', 'over', 'window', 'filter', 'within', 'escape', 'similar',
  'with', 'recursive', 'insert', 'update', 'delete', 'table', 'create', 'alter', 'drop', 'truncate',
  'explain', 'describe', 'show', 'grant', 'revoke', 'rename', 'top', 'any', 'some',
])

/** Characters a finished statement cannot end on. */
const OPEN_CHARS = new Set([',', '(', '[', '=', '<', '>', '+', '-', '/', '|', '.', '%', '^', '&', '~', '!', '*', ':', '@', '#', '?'])

/**
 * Brackets closed, strings and comments closed, at the end of `sql`.
 * @param {string} sql
 */
function closedAtEnd(sql) {
  let depth = 0
  let i = 0
  const n = sql.length
  while (i < n) {
    const c = sql[i]
    if (c === '-' && sql[i + 1] === '-') {
      const nl = sql.indexOf('\n', i)
      if (nl < 0) return true // a line comment runs to the end; what came before stands
      i = nl + 1
      continue
    }
    if (c === '/' && sql[i + 1] === '*') {
      const close = sql.indexOf('*/', i + 2)
      if (close < 0) return false
      i = close + 2
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      let j = i + 1
      for (;;) {
        if (j >= n) return false
        if (sql[j] === '\\' && c === "'") { j += 2; continue }
        if (sql[j] === c) {
          if (sql[j + 1] === c) { j += 2; continue }
          break
        }
        j++
      }
      i = j + 1
      continue
    }
    if (c === '$') {
      const tag = sql.slice(i).match(/^\$([A-Za-z_][A-Za-z0-9_]*)?\$/)
      if (tag) {
        const close = sql.indexOf(tag[0], i + tag[0].length)
        if (close < 0) return false
        i = close + tag[0].length
        continue
      }
    }
    if (c === '(') depth++
    else if (c === ')') depth--
    i++
  }
  return depth === 0
}

/**
 * Whether a statement that ends here looks finished and lacks its `;`.
 * @param {string} statement the statement's text, from its first word to the caret
 */
export function wantsTerminator(statement) {
  const s = String(statement ?? '').replace(/\s+$/, '')
  if (!s || s.endsWith(';')) return false
  const first = s.match(/^[\s(]*([A-Za-z_]+)/)?.[1]?.toLowerCase()
  if (!first || !STARTERS.has(first)) return false
  if (!closedAtEnd(s)) return false
  // A trailing line comment says nothing about the statement: judge what is
  // before it.
  const code = s.replace(/--[^\n]*$/, '').replace(/\s+$/, '')
  if (!code) return false
  if (OPEN_CHARS.has(code[code.length - 1])) return false
  const word = code.match(/([A-Za-z_][A-Za-z0-9_]*)$/)?.[1]?.toLowerCase()
  if (word && OPEN_ENDED.has(word)) {
    // These stand alone as a whole statement: BEGIN, COMMIT, VACUUM, SHOW TABLES...
    return /^(begin|start|commit|rollback|vacuum|analyze|analyse|reindex|refresh|table)$/.test(code.trim().toLowerCase())
  }
  return true
}

/**
 * A snippet that writes a whole statement (as against a clause such as JOIN
 * or ORDER BY, which goes into one) and does not end with `;` already.
 * @param {string} body
 */
export function isStatementSnippet(body) {
  const b = String(body ?? '').trim()
  if (!b || b.endsWith(';')) return false
  return /^(select|with|explain|insert|update|delete|create|alter|drop|pragma|truncate|merge)\b/i.test(b)
}

/**
 * A statement snippet ends the statement it is typed into when nothing but
 * whitespace follows it on its line: then it gets the `;`. Inside a bracket
 * (`WITH x AS (sel|)`) or before more of the statement it does not.
 * @param {string} restOfLine the text after the inserted range, to the line's end
 */
export function snippetEndsStatement(restOfLine) {
  return !String(restOfLine ?? '').trim()
}

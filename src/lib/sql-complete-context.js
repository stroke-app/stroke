/**
 * Where the caret is in a SQL statement, for completion - no editor involved.
 *
 * lang-sql's own completion reads the syntax tree and knows nothing about
 * what a DML statement is being written: it offered nothing inside `""` (the
 * very place a column name goes in quoted SQL) and ranked UPPER() above
 * UPDATE at the start of a statement. This reads the text before the caret
 * with a small tokenizer instead, which is enough to tell "a table goes here"
 * from "a column goes here" from "the next clause goes here".
 */

/** Keywords a table name follows. */
const TABLE_KEYWORDS = new Set(['UPDATE', 'FROM', 'INTO', 'JOIN', 'TABLE'])

/** Keywords that open a clause, i.e. decide what the next thing is. */
const CLAUSES = new Set([
  'UPDATE', 'SET', 'WHERE', 'AND', 'OR', 'NOT', 'FROM', 'INTO', 'JOIN', 'ON', 'VALUES',
  'SELECT', 'DELETE', 'INSERT', 'RETURNING', 'ORDER', 'GROUP', 'BY', 'HAVING', 'LIMIT',
  'TABLE', 'WITH',
])

/** Words that leave an expression unfinished: the next token continues it. */
const OPEN_WORDS = new Set([
  ...CLAUSES, 'IS', 'IN', 'LIKE', 'ILIKE', 'BETWEEN', 'AS', 'DISTINCT', 'CASE', 'WHEN',
  'THEN', 'ELSE', 'EXISTS', 'ALL', 'ANY',
])

/** The keywords worth putting first, by the clause the caret is in. */
const NEXT = /** @type {Record<string, string[]>} */ ({
  '': ['UPDATE', 'INSERT', 'DELETE', 'SELECT', 'WITH'],
  UPDATE: ['SET'],
  SET: ['WHERE', 'RETURNING'],
  WHERE: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  AND: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  OR: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  NOT: ['NULL', 'IN', 'LIKE', 'EXISTS'],
  INSERT: ['INTO'],
  INTO: ['VALUES', 'DEFAULT', 'SELECT'],
  VALUES: ['RETURNING', 'DEFAULT', 'NULL'],
  DELETE: ['FROM'],
  FROM: ['WHERE', 'JOIN', 'ORDER', 'LIMIT'],
  SELECT: ['FROM'],
  RETURNING: [],
})

/**
 * @typedef {{ t: 'word' | 'qid' | 'str' | 'num' | 'punct', v: string }} Token
 * @typedef {{
 *   kind: 'statement' | 'tables' | 'columns' | 'keywords' | 'qualified',
 *   from: number,
 *   prefix: string,
 *   quote: string | null,
 *   qualifier: string | null,
 *   clause: string,
 *   next: string[],
 *   afterExpr: boolean,
 *   tables: string[],
 *   predicateColumn: ColumnRef | null,
 *   comparedColumn: (ColumnRef & { operator: string }) | null,
 *   verb: string,
 * }} SqlCompletionContext
 * `predicateColumn`: the column just written in a condition, a space behind
 * it (`WHERE price |`): an operator comes next. `comparedColumn`: the column
 * and operator before the caret (`WHERE price >= |`): a value comes next.
 * `verb`: the statement's first keyword (SELECT, UPDATE ...).
 * @typedef {{ name: string, qualifier: string | null }} ColumnRef
 */

/**
 * Tokens of the statement the text ends in, or where the text ends inside
 * something that is not SQL to complete.
 * @param {string} text
 * @returns {{ tokens: Token[], open: { quote: string, start: number } | null } | null}
 *   null inside a string literal or a comment.
 */
function scan(text) {
  /** @type {Token[]} */
  let tokens = []
  const n = text.length
  let i = 0
  while (i < n) {
    const c = text[i]
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue }
    if (c === '-' && text[i + 1] === '-') {
      const nl = text.indexOf('\n', i)
      if (nl === -1) return null
      i = nl + 1
      continue
    }
    if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2)
      if (end === -1) return null
      i = end + 2
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      // Doubled quote is an escaped one: 'it''s', "a""b".
      let j = i + 1
      for (;;) {
        const k = text.indexOf(c, j)
        if (k === -1) {
          if (c === "'") return null
          return { tokens, open: { quote: c, start: i } }
        }
        if (text[k + 1] === c) { j = k + 2; continue }
        j = k
        break
      }
      if (c === "'") tokens.push({ t: 'str', v: '' })
      else tokens.push({ t: 'qid', v: text.slice(i + 1, j).replaceAll(c + c, c) })
      i = j + 1
      continue
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w$]/.test(text[j])) j++
      tokens.push({ t: 'word', v: text.slice(i, j) })
      i = j
      continue
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w.]/.test(text[j])) j++
      tokens.push({ t: 'num', v: text.slice(i, j) })
      i = j
      continue
    }
    if (c === ';') tokens = []
    else tokens.push({ t: 'punct', v: c })
    i++
  }
  return { tokens, open: null }
}

/** @param {Token | undefined} tok */
const isName = (tok) => tok?.t === 'word' || tok?.t === 'qid'
/** @param {Token | undefined} tok */
const kw = (tok) => (tok?.t === 'word' ? tok.v.toUpperCase() : '')

/** Clauses whose body is a condition: a column there is compared to something. */
const PREDICATE_CLAUSES = new Set(['WHERE', 'AND', 'OR', 'ON', 'HAVING', 'WHEN', 'NOT'])
/** What a condition's column can follow. */
const PREDICATE_STARTS = new Set(['WHERE', 'AND', 'OR', 'ON', 'HAVING', 'WHEN', 'NOT'])
/** Words that look like names but are values or keywords. */
const NOT_A_COLUMN = new Set(['NULL', 'TRUE', 'FALSE', 'DEFAULT'])

/**
 * The column (`name` or `qualifier.name`) ending at token `end`, or null.
 * @param {Token[]} tokens @param {number} end exclusive
 * @returns {(ColumnRef & { start: number }) | null}
 */
function columnBefore(tokens, end) {
  const tok = tokens[end - 1]
  if (!isName(tok)) return null
  const k = kw(tok)
  if (k && (CLAUSES.has(k) || OPEN_WORDS.has(k) || NOT_A_COLUMN.has(k))) return null
  let start = end - 1
  let qualifier = null
  if (tokens[start - 1]?.v === '.' && isName(tokens[start - 2])) {
    qualifier = /** @type {Token} */ (tokens[start - 2]).v
    start -= 2
  }
  return { name: /** @type {Token} */ (tok).v, qualifier, start }
}

/**
 * Tables the statement names, so their columns can be offered.
 * @param {Token[]} tokens
 */
function referencedTables(tokens) {
  const out = []
  for (let i = 0; i < tokens.length - 1; i++) {
    if (!TABLE_KEYWORDS.has(kw(tokens[i])) || !isName(tokens[i + 1])) continue
    // schema.table: the table is the last name.
    if (tokens[i + 2]?.v === '.' && isName(tokens[i + 3])) out.push(tokens[i + 3].v)
    else out.push(tokens[i + 1].v)
  }
  return out
}

/**
 * @param {string} text the document up to the caret (a bounded slice is fine)
 * @returns {SqlCompletionContext | null} null where nothing should be offered
 */
export function sqlCompletionContext(text) {
  const scanned = scan(text)
  if (!scanned) return null
  const { tokens, open } = scanned

  let prefix = ''
  let from = text.length
  /** @type {string | null} */
  let quote = null
  if (open) {
    quote = open.quote
    from = open.start + 1
    prefix = text.slice(from)
  } else {
    const m = /[\w$]+$/.exec(text)
    if (m && tokens.at(-1)?.t === 'word') {
      prefix = m[0]
      from = text.length - prefix.length
      tokens.pop()
    } else if (m) {
      // A number being typed, e.g. `= 4`.
      return null
    }
  }

  const last = tokens.at(-1)
  let clause = ''
  for (let i = tokens.length - 1; i >= 0; i--) {
    const k = kw(tokens[i])
    if (CLAUSES.has(k)) { clause = k; break }
  }
  // ORDER BY / GROUP BY: the clause is the pair.
  if (clause === 'BY') clause = kw(tokens.findLast((t) => kw(t) === 'ORDER' || kw(t) === 'GROUP')) || 'BY'

  /** @type {SqlCompletionContext['kind']} */
  let kind
  let qualifier = null
  if (last?.v === '.' && isName(tokens.at(-2))) {
    kind = 'qualified'
    qualifier = /** @type {Token} */ (tokens.at(-2)).v
  } else if (!tokens.length) {
    kind = 'statement'
  } else if (TABLE_KEYWORDS.has(kw(last)) || (clause === 'FROM' && last?.v === ',')) {
    kind = 'tables'
  } else if (
    (clause === 'UPDATE' || clause === 'DELETE' || clause === 'INSERT') ||
    (clause === 'INTO' && (isName(last) || last?.v === ')')) ||
    (clause === 'FROM' && isName(last)) ||
    clause === 'VALUES'
  ) {
    // Past the table: what comes next is a keyword (SET, FROM, VALUES...).
    kind = 'keywords'
  } else {
    kind = 'columns'
  }
  // A quote only ever holds a name.
  if (quote && (kind === 'keywords' || kind === 'statement')) kind = 'columns'

  // Is the thing before the caret a finished value? Then the clause's next
  // word (FROM after `SELECT id`, WHERE after `SET a = 1`) is the likely one;
  // right after SELECT or `=` it is a column instead.
  const afterExpr =
    last?.t === 'qid' || last?.t === 'str' || last?.t === 'num' ||
    last?.v === ')' || last?.v === '*' ||
    (last?.t === 'word' && !OPEN_WORDS.has(kw(last)))

  const verb = kw(tokens[0])
  /** @type {SqlCompletionContext['predicateColumn']} */
  let predicateColumn = null
  /** @type {SqlCompletionContext['comparedColumn']} */
  let comparedColumn = null
  if (!quote && kind === 'columns' && /\s$/.test(text.slice(0, from))) {
    if (PREDICATE_CLAUSES.has(clause)) {
      // `WHERE price |`: the column, with what a condition can start with before it.
      const col = columnBefore(tokens, tokens.length)
      const before = col ? tokens[col.start - 1] : undefined
      if (col && (PREDICATE_STARTS.has(kw(before)) || before?.v === '(')) {
        predicateColumn = { name: col.name, qualifier: col.qualifier }
      }
    }
    if (PREDICATE_CLAUSES.has(clause) || clause === 'SET') {
      // `WHERE price >= |`, `SET status = |`, `WHERE name LIKE |`.
      let j = tokens.length
      let operator = ''
      while (j > 0 && tokens[j - 1].t === 'punct' && /^[=<>!~]$/.test(tokens[j - 1].v)) operator = tokens[--j].v + operator
      if (!operator && (kw(tokens[j - 1]) === 'LIKE' || kw(tokens[j - 1]) === 'ILIKE')) operator = kw(tokens[--j])
      const col = operator ? columnBefore(tokens, j) : null
      if (col) comparedColumn = { name: col.name, qualifier: col.qualifier, operator }
    }
  }

  return {
    kind,
    from,
    prefix,
    quote,
    qualifier,
    clause,
    // RETURNING only ends a write: after a SELECT's WHERE it is an error.
    next: (NEXT[clause] ?? []).filter((k) => k !== 'RETURNING' || ['UPDATE', 'DELETE', 'INSERT'].includes(verb)),
    afterExpr,
    tables: referencedTables(tokens),
    predicateColumn,
    comparedColumn,
    verb,
  }
}

import { format } from 'sql-formatter'
import { normalizeSqlFormat, sqlFormatOptions } from '$lib/sql-format-options.js'

/**
 * SQL formatting. The preferences themselves live in `sql-format-options.js`,
 * which is free of this module's `sql-formatter` import so the settings store can
 * validate and hold them without putting the library on the startup path.
 */

/** @typedef {import('$lib/sql-format-options.js').SqlFormatOptions} SqlFormatOptions */

/** sql-formatter's dialect for each engine. Parsing MySQL as PostgreSQL threw on every backtick. */
const LANGUAGE = /** @type {Record<string, import('sql-formatter').SqlLanguage>} */ ({
  postgres: 'postgresql', cockroachdb: 'postgresql',
  mysql: 'mysql', mariadb: 'mariadb', tidb: 'tidb',
  sqlite: 'sqlite', d1: 'sqlite', libsql: 'sqlite',
  mssql: 'transactsql', duckdb: 'duckdb', clickhouse: 'clickhouse',
})

/**
 * @param {string} sql
 * @param {Partial<SqlFormatOptions>} [overrides]
 * @param {string} [engine] the connection's engine, for its dialect (PostgreSQL when unknown)
 */
export function formatSql(sql, overrides, engine) {
  const trimmed = sql.trim()
  if (!trimmed) return sql
  const o = overrides ? normalizeSqlFormat({ ...sqlFormatOptions(), ...overrides }) : sqlFormatOptions()
  try {
    const out = format(trimmed, {
      language: LANGUAGE[engine ?? ''] ?? 'postgresql',
      tabWidth: o.tabWidth,
      useTabs: o.useTabs,
      keywordCase: o.keywordCase,
      dataTypeCase: o.dataTypeCase,
      functionCase: o.functionCase,
      identifierCase: o.identifierCase,
      logicalOperatorNewline: o.logicalOperatorNewline,
      expressionWidth: o.expressionWidth,
      linesBetweenQueries: o.linesBetweenQueries,
    })
    if (!o.compactClauses) return out
    return layoutSql(out, { width: o.lineWidth, groupWidth: o.expressionWidth, unit: o.useTabs ? '\t' : ' '.repeat(o.tabWidth) })
  } catch {
    return sql
  }
}

// ── Layout ───────────────────────────────────────────────────────────────────
//
// sql-formatter has one layout: every clause keyword on a line of its own, its
// body on the lines below, one list item per line. Four lines for
// `SELECT * FROM users`. Its output is a clean tree, though (each line's
// children are the deeper lines under it, a bracket or a CASE closes on a line
// at its own indent), so it is read back as one and laid out again the way
// people write SQL: anything that fits on a line stays on one line, from the
// whole statement down to a clause or a bracket, and only what does not fit is
// broken, clause by clause.

/** @typedef {{ text: string, kids: Block[], close: string | null }} Block */

const JOIN = /^((NATURAL\s+)?((LEFT|RIGHT|FULL)(\s+OUTER)?|INNER|CROSS)\s+)?JOIN\b/i
const LOGICAL = /^(AND|OR)\b/i
const WITH = /^WITH(\s+RECURSIVE)?$/i

/** @param {string} line */
const indentOf = (line) => line.length - line.trimStart().length

/**
 * Lines at one indent and everything under them, as blocks.
 * @param {string[]} lines @param {{ i: number }} at
 * @returns {Block[]}
 */
function parseBlocks(lines, at) {
  /** @type {Block[]} */
  const out = []
  const ind = at.i < lines.length ? indentOf(lines[at.i]) : 0
  while (at.i < lines.length && indentOf(lines[at.i]) >= ind) {
    const line = lines[at.i++]
    /** @type {Block} */
    const block = { text: line.trim(), kids: [], close: null }
    if (at.i < lines.length && indentOf(lines[at.i]) > indentOf(line)) block.kids = parseBlocks(lines, at)
    // A bracket closes on the next line at its own indent (`)`, `) AS t`, `),`), a CASE on its END.
    const closer = block.text.endsWith('(') ? ')' : /\bCASE$/i.test(block.text) ? 'END' : null
    const next = lines[at.i]
    if (closer && next !== undefined && indentOf(next) === indentOf(line) && next.trim().toUpperCase().startsWith(closer)) {
      block.close = next.trim()
      at.i++
    }
    out.push(block)
  }
  return out
}

/** Joins pieces of one line: no space just inside a bracket. @param {string[]} parts */
function joinParts(parts) {
  return parts.reduce((acc, p) => (!p ? acc : !acc ? p : acc.endsWith('(') || p.startsWith(')') ? acc + p : `${acc} ${p}`), '')
}

/** @param {Block} b @returns {string} */
const flat = (b) => joinParts([b.text, ...b.kids.map(flat), b.close ?? ''])

/** The block's last text, where a list item's comma or a statement's `;` is. @param {Block} b @returns {string} */
const lastText = (b) => b.close ?? (b.kids.length ? lastText(b.kids[b.kids.length - 1]) : b.text)

/** @param {Block} b */
const isCase = (b) => b.close !== null && !b.text.endsWith('(')

/** `a,` `b,` `c`: a comma list. @param {Block[]} kids */
const isList = (kids) => kids.length > 1 && kids.slice(0, -1).every((k) => lastText(k).endsWith(','))

/**
 * @param {string} sql sql-formatter's standard output
 * @param {{ width: number, groupWidth: number, unit: string }} opts width: longest line;
 *   groupWidth: longest bracket kept on one line (the expressionWidth setting); unit: one indent
 */
export function layoutSql(sql, { width, groupWidth, unit }) {
  /**
   * May `b` go on one line at all? Not past a line comment, not a CASE with
   * several WHENs, not a clause with a JOIN (joins start their own line), not
   * a bracket longer than groupWidth.
   * @param {Block} b @returns {boolean}
   */
  const flattens = (b) => {
    if (b.text.includes('--') || b.close?.includes('--')) return false
    if (isCase(b) && b.kids.filter((k) => /^WHEN\b/i.test(k.text)).length > 1) return false
    if (b.kids.some((k) => JOIN.test(k.text))) return false
    if (b.text.endsWith('(') && joinParts(b.kids.map(flat)).length > groupWidth) return false
    return b.kids.every(flattens)
  }

  /**
   * Lines for `b` at depth `d`, its first line led by `lead` (the keyword it hangs on).
   * @param {Block} b @param {number} d @param {string} [lead] @returns {string[]}
   */
  const render = (b, d, lead = '') => {
    const pad = unit.repeat(d)
    if (!b.kids.length && b.close === null) return [pad + lead + b.text]
    if (flattens(b)) {
      const line = pad + lead + flat(b)
      if (line.length <= width) return [line]
    }
    if (b.close !== null) return [pad + lead + b.text, ...renderBody(b, d + 1), pad + b.close]
    return renderClause(b, d, lead)
  }

  /** A bracket's or a CASE's inside. @param {Block} b @param {number} d */
  const renderBody = (b, d) => {
    if (isCase(b)) return b.kids.flatMap((k) => render(k, d))
    if (b.kids.some((k) => k.kids.length > 0 && k.close === null)) return renderStatement(b.kids, d)
    // Plain values (`IN (1, 2, ...)`, a function's arguments) fill each line:
    // two hundred ids are a few lines, not two hundred. Anything with words in
    // it (a column definition) keeps a line of its own.
    if (isList(b.kids) && b.kids.every((k) => !k.kids.length && k.close === null && !/\s/.test(k.text) && !k.text.includes('--'))) {
      const pad = unit.repeat(d)
      /** @type {string[]} */
      const lines = []
      let line = ''
      for (const k of b.kids) {
        if (line && (pad + line + ' ' + k.text).length > width) { lines.push(pad + line); line = '' }
        line = line ? `${line} ${k.text}` : k.text
      }
      return [...lines, pad + line]
    }
    return b.kids.flatMap((k) => render(k, d))
  }

  /**
   * A clause: keyword and body. A list that does not fit goes one item per
   * line under the keyword. Otherwise the first item stays beside it and the
   * rest follow: AND/OR one level in, a JOIN level with the FROM it belongs to.
   * @param {Block} b @param {number} d @param {string} lead @returns {string[]}
   */
  const renderClause = (b, d, lead) => {
    const pad = unit.repeat(d)
    const kw = b.text
    // WITH a AS (...), b AS (...): the first beside WITH, each next one under it.
    if (WITH.test(kw)) return b.kids.flatMap((k, i) => render(k, d, i === 0 ? `${lead}${kw} ` : ''))
    if (isList(b.kids)) return [pad + lead + kw, ...b.kids.flatMap((k) => render(k, d + 1))]
    /** @type {string[]} */
    const out = []
    for (let i = 0; i < b.kids.length; i++) {
      const k = b.kids[i]
      if (i === 0) { out.push(...render(k, d, `${lead}${kw} `)); continue }
      if (!JOIN.test(k.text)) { out.push(...render(k, d + 1)); continue }
      // A join and the AND/OR lines of its ON condition after it.
      let end = i + 1
      while (end < b.kids.length && LOGICAL.test(b.kids[end].text)) end++
      const parts = b.kids.slice(i, end)
      const line = pad + joinParts(parts.map(flat))
      if (parts.every(flattens) && line.length <= width) out.push(line)
      else out.push(...render(k, d), ...parts.slice(1).flatMap((p) => render(p, d + 1)))
      i = end - 1
    }
    return out
  }

  /**
   * One statement: on one line when it fits, else a line per clause. After a
   * WITH, the main query gets the same chance on its own.
   * @param {Block[]} blocks @param {number} d @returns {string[]}
   */
  const renderStatement = (blocks, d) => {
    if (blocks.every(flattens)) {
      const line = unit.repeat(d) + joinParts(blocks.map(flat))
      if (line.length <= width) return [line]
    }
    if (blocks.length > 1 && WITH.test(blocks[0].text)) return [...render(blocks[0], d), ...renderStatement(blocks.slice(1), d)]
    return blocks.flatMap((b) => render(b, d))
  }

  /** Statements are separated by blank lines, or follow each other at a `;`. @param {string[]} lines */
  const renderChunk = (lines) => {
    // A block comment over several lines has no tree to read; leave it as formatted.
    if (lines.some((l) => l.includes('/*') && !l.includes('*/'))) return lines
    const blocks = parseBlocks(lines, { i: 0 })
    /** @type {string[]} */
    const out = []
    let start = 0
    blocks.forEach((b, i) => {
      if (!lastText(b).endsWith(';') && i < blocks.length - 1) return
      out.push(...renderStatement(blocks.slice(start, i + 1), 0))
      start = i + 1
    })
    return out
  }

  /** @type {string[]} */
  const out = []
  /** @type {string[]} */
  let chunk = []
  for (const line of sql.split('\n')) {
    if (line.trim()) { chunk.push(line); continue }
    out.push(...renderChunk(chunk), line)
    chunk = []
  }
  out.push(...renderChunk(chunk))
  return out.join('\n')
}

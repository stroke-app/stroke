import { format } from 'sql-formatter'
import { normalizeSqlFormat, sqlFormatOptions } from '$lib/sql-format-options.js'

/**
 * SQL formatting. The preferences themselves live in `sql-format-options.js`,
 * which is free of this module's `sql-formatter` import so the settings store can
 * validate and hold them without putting the library on the startup path.
 */

/** @typedef {import('$lib/sql-format-options.js').SqlFormatOptions} SqlFormatOptions */

/** @param {string} sql @param {Partial<SqlFormatOptions>} [overrides] */
export function formatSql(sql, overrides) {
  const trimmed = sql.trim()
  if (!trimmed) return sql
  const o = overrides ? normalizeSqlFormat({ ...sqlFormatOptions(), ...overrides }) : sqlFormatOptions()
  try {
    const out = format(trimmed, {
      language: 'postgresql',
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
    return o.compactClauses ? compactClauses(out, o.lineWidth, o.useTabs ? '\t' : ' '.repeat(o.tabWidth)) : out
  } catch {
    return sql
  }
}

/** Clause keywords sql-formatter puts alone on their line, body indented below. */
const CLAUSE_LINE = /^(select(\s+distinct)?|from|where|set|group\s+by|order\s+by|having|limit|offset|returning|values|insert\s+into|delete\s+from|partition\s+by|window|using)$/i
/** A join starts its own line even inside FROM. */
const JOIN_LINE = /^((natural\s+)?((left|right|full)(\s+outer)?|inner|cross)\s+)?join\b/i

/**
 * Put each clause back on its keyword's line where it fits.
 *
 * sql-formatter has no compact layout: every clause keyword gets a line of its
 * own and its body goes on the next, indented - four lines for
 * `SELECT * FROM users`. This folds a clause into one line when its whole body
 * is one level deep and fits in `width`; otherwise it moves just the first item
 * up beside the keyword (`WHERE a = 1` / `  AND b = 2`), except for a list
 * (`SELECT` / `  a,` / `  b`), which keeps the library's layout.
 * @param {string} sql formatted by sql-formatter's standard indent style
 * @param {number} width @param {string} unit one indentation level
 */
export function compactClauses(sql, width, unit) {
  const lines = sql.split('\n')
  const indentOf = (/** @type {string} */ l) => l.length - l.trimStart().length
  /** @type {string[]} */
  const out = []
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    const keyword = line.trim()
    if (!CLAUSE_LINE.test(keyword)) { out.push(line); continue }
    const ind = indentOf(line)
    let end = i + 1
    while (end < lines.length && lines[end].trim() !== '' && indentOf(lines[end]) > ind) end++
    const body = lines.slice(i + 1, end)
    if (!body.length) { out.push(line); continue }
    const lead = line.slice(0, ind)
    const itemIndent = ind + unit.length
    const flat = body.every((l) => indentOf(l) === itemIndent && !l.includes('--') && !JOIN_LINE.test(l.trim()))
    const joined = `${lead}${keyword} ${body.map((l) => l.trim()).join(' ')}`
    if (flat && joined.length <= width) {
      out.push(joined)
      i = end - 1
      continue
    }
    const first = body[0]
    if (indentOf(first) === itemIndent && !first.trimEnd().endsWith(',')) {
      out.push(`${lead}${keyword} ${first.trim()}`)
      i++ // the first item is placed; the rest stay as they are
      continue
    }
    out.push(line)
  }
  return out.join('\n')
}

/** @param {typeof import('monaco-editor')} monaco */
export function registerMonacoSqlFormatter(monaco) {
  if (registerMonacoSqlFormatter.done) return
  registerMonacoSqlFormatter.done = true

  /** @param {import('monaco-editor').editor.ITextModel} model */
  /** @param {import('monaco-editor').Range} [range] */
  function editsFor(model, range) {
    const text = range ? model.getValueInRange(range) : model.getValue()
    const formatted = formatSql(text)
    if (formatted === text) return []
    const target = range ?? model.getFullModelRange()
    return [{ range: target, text: formatted }]
  }

  monaco.languages.registerDocumentFormattingEditProvider('sql', {
    provideDocumentFormattingEdits: (model) => editsFor(model),
  })

  monaco.languages.registerDocumentRangeFormattingEditProvider('sql', {
    provideDocumentRangeFormattingEdits: (model, range) => editsFor(model, range),
  })
}

registerMonacoSqlFormatter.done = false

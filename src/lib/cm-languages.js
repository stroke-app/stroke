/**
 * Languages for the CodeMirror editors, by the ids the views ask for.
 *
 * Every editor in the app runs on CodeMirror. The views that used to run on
 * Monaco named their languages by Monaco's ids ('json', 'typescript',
 * 'plaintext' ...) plus three of their own (CSV and TSV for the table's text
 * view, Prisma for the ORM schema); this maps those ids onto CodeMirror
 * language support, so a view says what it shows and nothing more.
 */
import { StreamLanguage } from '@codemirror/language'
import { json } from '@codemirror/lang-json'
import { html } from '@codemirror/lang-html'
import { sql } from '@codemirror/lang-sql'
import { javascript } from '@codemirror/lang-javascript'
import { sqlDialectFor } from '$lib/cm-sql-dialects.js'

/** @typedef {import('@codemirror/state').Extension} Extension */

/**
 * Delimited text (CSV, TSV, a Markdown table): quoted cells, delimiters,
 * numbers and booleans / NULL, read a line at a time.
 * @param {string} delim one character
 */
function delimited(delim) {
  const isDelim = (/** @type {string} */ ch) => ch === delim
  return StreamLanguage.define({
    name: delim === ',' ? 'csv' : delim === '\t' ? 'tsv' : 'table',
    token(stream) {
      if (stream.peek() === '"') {
        stream.next()
        for (let ch; (ch = stream.next()) != null; ) {
          if (ch === '"') { if (stream.peek() === '"') stream.next(); else break }
        }
        return 'string'
      }
      const ch = stream.next() ?? ''
      if (isDelim(ch)) return 'punctuation'
      // The rest of the cell, up to the next delimiter or quote.
      let cell = ch
      while (!stream.eol() && !isDelim(stream.peek() ?? '') && stream.peek() !== '"') cell += stream.next()
      const v = cell.trim()
      if (/^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?$/.test(v)) return 'number'
      if (/^(?:true|false)$/i.test(v)) return 'bool'
      if (/^null$/i.test(v)) return 'null'
      return null
    },
  })
}

const PRISMA_BLOCKS = new Set(['model', 'enum', 'datasource', 'generator', 'type', 'view'])
const PRISMA_TYPES = new Set([
  'String', 'Int', 'BigInt', 'Float', 'Decimal', 'Boolean', 'DateTime', 'Json', 'Bytes', 'Unsupported',
])

/** A Prisma schema: blocks, field types, @attributes, strings and comments. */
const prisma = StreamLanguage.define({
  name: 'prisma',
  token(stream) {
    if (stream.eatSpace()) return null
    if (stream.match('//')) { stream.skipToEnd(); return 'comment' }
    if (stream.peek() === '"') {
      stream.next()
      for (let ch; (ch = stream.next()) != null; ) { if (ch === '\\') stream.next(); else if (ch === '"') break }
      return 'string'
    }
    if (stream.match(/^@@?[\w.]+/)) return 'meta'
    if (stream.match(/^-?\d+(?:\.\d+)?/)) return 'number'
    const word = stream.match(/^[A-Za-z_]\w*/)
    if (word) {
      const w = /** @type {RegExpMatchArray} */ (word)[0]
      if (PRISMA_BLOCKS.has(w)) return 'keyword'
      if (PRISMA_TYPES.has(w)) return 'typeName'
      if (w === 'true' || w === 'false') return 'bool'
      return /^[A-Z]/.test(w) ? 'typeName' : 'variableName'
    }
    stream.next()
    return /[{}()[\],?=]/.test(stream.current()) ? 'punctuation' : null
  },
})

const LANGS = /** @type {Record<string, (dialect: string) => Extension>} */ ({
  sql: (dialect) => sql({ dialect: sqlDialectFor(dialect) }),
  json: () => json(),
  jsonl: () => json(),
  html: () => html(),
  javascript: () => javascript(),
  typescript: () => javascript({ typescript: true }),
  prisma: () => prisma,
  csv: () => delimited(','),
  tsv: () => delimited('\t'),
  // The text view's Markdown is a table: a pipe-delimited grid.
  markdown: () => delimited('|'),
})

/** Other names the views use for the same languages. */
const ALIASES = /** @type {Record<string, string>} */ ({
  'stroke-csv': 'csv', 'stroke-tsv': 'tsv', js: 'javascript', ts: 'typescript', md: 'markdown',
})

/**
 * The language extension for an id, or none (plain text) for an id it does
 * not know: 'plaintext', 'text', ''.
 * @param {string} id @param {string} [dialect] for SQL
 * @returns {Extension}
 */
export function languageExtension(id, dialect = '') {
  const key = ALIASES[id] ?? id
  return LANGS[key]?.(dialect) ?? []
}

/** @param {string} id */
export function isKnownLanguage(id) {
  return (ALIASES[id] ?? id) in LANGS
}

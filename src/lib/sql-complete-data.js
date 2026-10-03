/**
 * The SQL completion vocabulary and query analysis, editor-agnostic.
 *
 * Moved out of the Monaco completion provider when the SQL editor moved to
 * CodeMirror (cm-sql-complete.js), so the curated keyword list, the function
 * signatures, the snippets and the alias analysis carried over unchanged.
 */

/** @typedef {{
 *   schemas?: string[]
 *   activeSchema?: string
 *   tables?: string[]
 *   columnsByTable?: Record<string, Array<string | { name: string, type?: string }>>
 *   enumValues?: Record<string, string[]>
 *   userFunctions?: Array<{name: string, signature: string, returnType: string, kind: string}>
 *   loadColumns?: (tables: string[]) => Promise<unknown>
 * }} SqlSchemaHints
 * `loadColumns` fetches columns not in `columnsByTable` yet; completion awaits
 * it (briefly) when a statement names such a table, then reads the hints again. */

// ── Keywords ─────────────────────────────────────────────────────────────────

export const PG_KEYWORDS = [
  // DML
  'SELECT', 'INSERT', 'UPDATE', 'DELETE', 'RETURNING',
  // FROM / joins
  'FROM', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'FULL', 'CROSS', 'OUTER', 'LATERAL', 'ON', 'USING',
  // WHERE / predicates
  'WHERE', 'AND', 'OR', 'NOT', 'IN', 'EXISTS', 'BETWEEN', 'LIKE', 'ILIKE', 'IS', 'NULL', 'TRUE', 'FALSE',
  // Grouping / sorting
  'GROUP', 'ORDER', 'BY', 'HAVING', 'ASC', 'DESC', 'NULLS', 'FIRST', 'LAST',
  // Pagination
  'LIMIT', 'OFFSET', 'FETCH',
  // Set ops
  'UNION', 'ALL', 'EXCEPT', 'INTERSECT', 'DISTINCT',
  // DML clauses
  'INTO', 'VALUES', 'SET', 'AS',
  // CTE
  'WITH', 'RECURSIVE',
  // CASE
  'CASE', 'WHEN', 'THEN', 'ELSE', 'END',
  // Window
  'OVER', 'PARTITION', 'ROWS', 'RANGE', 'UNBOUNDED', 'PRECEDING', 'FOLLOWING',
  // DDL
  'CREATE', 'TABLE', 'VIEW', 'INDEX', 'ALTER', 'DROP', 'TRUNCATE',
  'PRIMARY', 'KEY', 'FOREIGN', 'REFERENCES', 'UNIQUE', 'DEFAULT', 'CONSTRAINT', 'CHECK',
  // Transactions
  'BEGIN', 'COMMIT', 'ROLLBACK', 'SAVEPOINT',
  // Query tools
  'EXPLAIN', 'ANALYZE', 'VACUUM',
  // Misc
  'CAST', 'COALESCE', 'FILTER', 'WITHIN',
]

// Keywords that are relevant in TABLE context (after FROM/JOIN)
export const TABLE_CTX_KWS = new Set([
  'AS', 'WHERE', 'ON', 'USING',
  'JOIN', 'LEFT', 'RIGHT', 'INNER', 'FULL', 'CROSS', 'OUTER', 'LATERAL',
  'GROUP', 'ORDER', 'BY', 'HAVING', 'LIMIT', 'OFFSET', 'FETCH',
  'UNION', 'INTERSECT', 'EXCEPT', 'ALL',
  'WITH', 'RECURSIVE',
])

// Keywords that are relevant in COLUMN context (after SELECT/WHERE/etc.)
export const COLUMN_CTX_KWS = new Set([
  'AS', 'AND', 'OR', 'NOT', 'IN', 'EXISTS', 'BETWEEN', 'LIKE', 'ILIKE', 'IS', 'NULL', 'TRUE', 'FALSE',
  'DISTINCT', 'ALL', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END',
  'CAST', 'COALESCE', 'NULLIF', 'GREATEST', 'LEAST',
  'ASC', 'DESC', 'NULLS', 'FIRST', 'LAST',
  'OVER', 'PARTITION', 'BY', 'ROWS', 'RANGE', 'UNBOUNDED', 'PRECEDING', 'FOLLOWING',
  'FILTER', 'WITHIN', 'RETURNING',
  'GROUP', 'ORDER', 'HAVING', 'LIMIT', 'OFFSET',
])

// SQL reserved words that should never be treated as table aliases
export const SQL_KW_SET = new Set([
  'SELECT', 'FROM', 'WHERE', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'FULL', 'CROSS', 'OUTER', 'LATERAL',
  'ON', 'USING', 'AND', 'OR', 'NOT', 'IN', 'BETWEEN', 'LIKE', 'ILIKE', 'IS', 'NULL', 'TRUE', 'FALSE',
  'ORDER', 'GROUP', 'BY', 'HAVING', 'LIMIT', 'OFFSET', 'UNION', 'INTERSECT', 'EXCEPT',
  'INSERT', 'UPDATE', 'DELETE', 'AS', 'WITH', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END',
  'ALL', 'DISTINCT', 'INTO', 'VALUES', 'SET', 'RETURNING', 'EXISTS', 'RECURSIVE',
])

// ── Context detection keywords ─────────────────────────────────────────────

const TABLE_CONTEXT_KWS = new Set(['FROM', 'JOIN', 'INTO', 'UPDATE', 'TABLE', 'EXISTS', 'TRUNCATE'])
const COLUMN_CONTEXT_KWS = new Set([
  'SELECT', 'WHERE', 'ON', 'HAVING', 'SET', 'RETURNING', 'BY',
  'BETWEEN', 'LIKE', 'ILIKE', 'AND', 'OR', 'THEN', 'ELSE', 'WHEN',
])

// ── Functions ─────────────────────────────────────────────────────────────────

/** @type {Array<{label:string, sig:string, doc:string}>} */
export const PG_FUNCTIONS = [
  // Aggregates
  { label: 'count',          sig: 'count(${1:*})',                                    doc: 'Count rows. count(*) counts all rows; count(col) excludes NULLs.' },
  { label: 'sum',            sig: 'sum(${1:expression})',                              doc: 'Sum of a numeric column, ignoring NULLs.' },
  { label: 'avg',            sig: 'avg(${1:expression})',                              doc: 'Arithmetic mean, ignoring NULLs.' },
  { label: 'min',            sig: 'min(${1:expression})',                              doc: 'Minimum value in the column.' },
  { label: 'max',            sig: 'max(${1:expression})',                              doc: 'Maximum value in the column.' },
  { label: 'string_agg',    sig: "string_agg(${1:expression}, ${2:', '})",            doc: "Concatenate non-NULL values with a delimiter." },
  { label: 'array_agg',     sig: 'array_agg(${1:expression})',                        doc: 'Collect values into a PostgreSQL array.' },
  { label: 'json_agg',      sig: 'json_agg(${1:expression})',                         doc: 'Collect values as a JSON array.' },
  { label: 'jsonb_agg',     sig: 'jsonb_agg(${1:expression})',                        doc: 'Collect values as a JSONB array.' },
  { label: 'bool_and',      sig: 'bool_and(${1:expression})',                         doc: 'True only if all boolean values are true.' },
  { label: 'bool_or',       sig: 'bool_or(${1:expression})',                          doc: 'True if any boolean value is true.' },
  // Window
  { label: 'row_number',    sig: 'row_number()',                                      doc: 'Unique sequential integer for each row in its partition.' },
  { label: 'rank',          sig: 'rank()',                                             doc: 'Rank with gaps for ties: 1, 1, 3, ...' },
  { label: 'dense_rank',    sig: 'dense_rank()',                                      doc: 'Rank without gaps for ties: 1, 1, 2, ...' },
  { label: 'ntile',         sig: 'ntile(${1:buckets})',                               doc: 'Distribute rows into n buckets, returns bucket number 1-n.' },
  { label: 'lag',           sig: 'lag(${1:value}, ${2:1})',                           doc: 'Value from a previous row.' },
  { label: 'lead',          sig: 'lead(${1:value}, ${2:1})',                          doc: 'Value from a following row.' },
  { label: 'first_value',   sig: 'first_value(${1:expression})',                      doc: 'First value in the current window frame.' },
  { label: 'last_value',    sig: 'last_value(${1:expression})',                       doc: 'Last value in the current window frame.' },
  { label: 'percent_rank',  sig: 'percent_rank()',                                    doc: 'Relative rank as a fraction between 0 and 1.' },
  // String
  { label: 'lower',         sig: 'lower(${1:string})',                                doc: 'Convert string to lowercase.' },
  { label: 'upper',         sig: 'upper(${1:string})',                                doc: 'Convert string to uppercase.' },
  { label: 'trim',          sig: 'trim(${1:string})',                                 doc: 'Remove leading and trailing whitespace.' },
  { label: 'ltrim',         sig: 'ltrim(${1:string})',                                doc: 'Remove leading (left) whitespace.' },
  { label: 'rtrim',         sig: 'rtrim(${1:string})',                                doc: 'Remove trailing (right) whitespace.' },
  { label: 'length',        sig: 'length(${1:string})',                               doc: 'Number of characters in the string.' },
  { label: 'substr',        sig: 'substr(${1:string}, ${2:from}, ${3:count})',        doc: 'Extract substring starting at position from, up to count chars.' },
  { label: 'substring',     sig: 'substring(${1:string} from ${2:1} for ${3:n})',    doc: 'Extract part of a string.' },
  { label: 'left',          sig: 'left(${1:string}, ${2:n})',                         doc: 'First n characters of a string.' },
  { label: 'right',         sig: 'right(${1:string}, ${2:n})',                        doc: 'Last n characters of a string.' },
  { label: 'split_part',    sig: "split_part(${1:string}, ${2:'.'}, ${3:1})",        doc: 'Split by delimiter and return the nth part (1-based).' },
  { label: 'replace',       sig: "replace(${1:string}, ${2:'from'}, ${3:'to'})",     doc: 'Replace all occurrences of a substring.' },
  { label: 'regexp_replace',sig: "regexp_replace(${1:string}, ${2:'pattern'}, ${3:'replacement'})", doc: 'Replace matches of a POSIX regex.' },
  { label: 'concat',        sig: 'concat(${1:val1}, ${2:val2})',                      doc: 'Concatenate values, ignoring NULLs.' },
  { label: 'concat_ws',     sig: "concat_ws(${1:', '}, ${2:val1}, ${3:val2})",       doc: 'Concatenate with separator, skipping NULLs.' },
  { label: 'lpad',          sig: "lpad(${1:string}, ${2:length}, ${3:' '})",         doc: 'Left-pad string to length with fill character.' },
  { label: 'rpad',          sig: "rpad(${1:string}, ${2:length}, ${3:' '})",         doc: 'Right-pad string to length with fill character.' },
  { label: 'initcap',       sig: 'initcap(${1:string})',                              doc: 'Capitalize the first letter of each word.' },
  { label: 'md5',           sig: 'md5(${1:string})',                                  doc: 'MD5 hash of the string, returned as hex.' },
  { label: 'format',        sig: "format(${1:'%s'}, ${2:arg})",                       doc: 'Format a string with printf-style substitutions.' },
  // Numeric
  { label: 'abs',           sig: 'abs(${1:n})',                                       doc: 'Absolute value of n.' },
  { label: 'ceil',          sig: 'ceil(${1:n})',                                      doc: 'Round up to the nearest integer.' },
  { label: 'floor',         sig: 'floor(${1:n})',                                     doc: 'Round down to the nearest integer.' },
  { label: 'round',         sig: 'round(${1:n}, ${2:0})',                             doc: 'Round to d decimal places.' },
  { label: 'trunc',         sig: 'trunc(${1:n})',                                     doc: 'Truncate fractional part toward zero.' },
  { label: 'mod',           sig: 'mod(${1:dividend}, ${2:divisor})',                  doc: 'Remainder after integer division.' },
  { label: 'power',         sig: 'power(${1:base}, ${2:exponent})',                   doc: 'Base raised to the power of exponent.' },
  { label: 'sqrt',          sig: 'sqrt(${1:n})',                                      doc: 'Square root of n.' },
  { label: 'random',        sig: 'random()',                                          doc: 'Random float between 0.0 and 1.0.' },
  { label: 'greatest',      sig: 'greatest(${1:val1}, ${2:val2})',                    doc: 'Largest of the provided values, ignoring NULLs.' },
  { label: 'least',         sig: 'least(${1:val1}, ${2:val2})',                       doc: 'Smallest of the provided values, ignoring NULLs.' },
  // Date / time
  { label: 'now',           sig: 'now()',                                             doc: 'Current date and time with timezone.' },
  { label: 'current_date',  sig: 'current_date',                                     doc: 'Current date (no time component).' },
  { label: 'current_time',  sig: 'current_time',                                     doc: 'Current time with timezone.' },
  { label: 'date_trunc',    sig: "date_trunc(${1:'month'}, ${2:timestamp})",         doc: "Truncate to a time unit: 'year' 'month' 'week' 'day' 'hour' 'minute'." },
  { label: 'date_part',     sig: "date_part(${1:'month'}, ${2:timestamp})",          doc: "Extract a date/time field as a number." },
  { label: 'extract',       sig: 'extract(${1:year} FROM ${2:timestamp})',            doc: 'Extract a date/time field.' },
  { label: 'age',           sig: 'age(${1:timestamp})',                               doc: 'Interval elapsed from timestamp to now.' },
  { label: 'to_char',       sig: "to_char(${1:value}, ${2:'YYYY-MM-DD'})",           doc: 'Format a date or number as text.' },
  { label: 'to_timestamp',  sig: "to_timestamp(${1:string}, ${2:'YYYY-MM-DD'})",     doc: 'Parse a text string into a timestamptz.' },
  { label: 'to_date',       sig: "to_date(${1:string}, ${2:'YYYY-MM-DD'})",          doc: 'Parse a text string into a date.' },
  // JSON / JSONB
  { label: 'json_build_object',  sig: "json_build_object(${1:'key'}, ${2:value})",   doc: 'Build a JSON object from alternating key/value arguments.' },
  { label: 'jsonb_build_object', sig: "jsonb_build_object(${1:'key'}, ${2:value})",  doc: 'Build a JSONB object from alternating key/value arguments.' },
  { label: 'jsonb_set',     sig: "jsonb_set(${1:target}, ${2:'{key}'}::text[], ${3:new_value}::jsonb)", doc: 'Replace value at path in a JSONB document.' },
  { label: 'row_to_json',   sig: 'row_to_json(${1:row})',                             doc: 'Convert a table row to a JSON object.' },
  { label: 'to_jsonb',      sig: 'to_jsonb(${1:expression})',                         doc: 'Convert any SQL value to JSONB.' },
  // Null / conditional
  { label: 'coalesce',      sig: 'coalesce(${1:val1}, ${2:val2})',                    doc: 'Return the first non-NULL argument.' },
  { label: 'nullif',        sig: 'nullif(${1:val1}, ${2:val2})',                      doc: 'Return NULL if val1 = val2, otherwise return val1.' },
  // Array
  { label: 'array_length',  sig: 'array_length(${1:array}, ${2:1})',                 doc: 'Length of the nth array dimension (1 = outermost).' },
  { label: 'unnest',        sig: 'unnest(${1:array})',                                doc: 'Expand an array to a set of rows.' },
  { label: 'generate_series', sig: 'generate_series(${1:start}, ${2:stop})',         doc: 'Generate a series of values.' },
  // Type
  { label: 'cast',          sig: 'cast(${1:expression} AS ${2:type})',                doc: 'Explicit type cast. Equivalent to expression::type.' },
  // Utility
  { label: 'pg_size_pretty', sig: 'pg_size_pretty(${1:bytes})',                      doc: 'Format a byte count as a human-readable string (KB, MB, GB).' },
  { label: 'pg_typeof',     sig: 'pg_typeof(${1:expression})',                        doc: 'Return the data type of an expression as text.' },
]

// ── Snippets ───────────────────────────────────────────────────────────────────

/**
 * Snippets. `name` is what the list shows and what typing matches - it starts
 * with the statement's own keyword, so `sel`, `ins`, `upd`, `join` all find
 * theirs. `alias` is the short trigger the Monaco editor had, still matched
 * and shown beside the name. `pg` marks Postgres-only SQL. Bodies are one line
 * wherever the SQL allows (a DDL column list and a transaction's statements
 * are the exceptions): a snippet is a starting point, and Format lays it out.
 * Field numbers set the Tab order, not the reading order: a SELECT asks for
 * its table first, so the column fields after it complete that table's columns.
 * @type {Array<{name: string, alias: string, body: string, pg?: boolean}>}
 */
export const SQL_SNIPPETS = [
  // Reading
  { name: 'SELECT … FROM',            alias: 'sel',   body: 'SELECT ${2:*} FROM ${1:table}' },
  { name: 'SELECT … WHERE',           alias: 'selw',  body: 'SELECT ${2:*} FROM ${1:table} WHERE ${3:condition}' },
  { name: 'SELECT … LIMIT',           alias: 'sell',  body: 'SELECT ${2:*} FROM ${1:table} LIMIT ${3:100}' },
  { name: 'SELECT … ORDER BY',        alias: 'selo',  body: 'SELECT ${2:*} FROM ${1:table} ORDER BY ${3:column} ${4:DESC} LIMIT ${5:100}' },
  { name: 'SELECT DISTINCT',          alias: 'seld',  body: 'SELECT DISTINCT ${2:column} FROM ${1:table}' },
  { name: 'SELECT COUNT(*)',          alias: 'selct', body: 'SELECT COUNT(*) FROM ${1:table}' },
  { name: 'SELECT … GROUP BY',        alias: 'selg',  body: 'SELECT ${2:column}, COUNT(*) AS count FROM ${1:table} GROUP BY ${2:column} ORDER BY count DESC' },
  { name: 'SELECT … JOIN',            alias: 'selj',  body: 'SELECT ${3:*} FROM ${1:table} t1 JOIN ${2:other} t2 ON t2.${4:id} = t1.${5:other_id}' },
  { name: 'SELECT duplicates',        alias: 'dup',   body: 'SELECT ${2:column}, COUNT(*) AS count FROM ${1:table} GROUP BY ${2:column} HAVING COUNT(*) > 1 ORDER BY count DESC' },
  { name: 'SELECT last N days',       alias: 'seldt', body: "SELECT ${2:*} FROM ${1:table} WHERE ${3:created_at} >= now() - interval '${4:7 days}' ORDER BY ${3:created_at} DESC", pg: true },
  { name: 'WITH … AS (CTE)',          alias: 'cte',   body: 'WITH ${1:cte} AS (${2:SELECT 1}) SELECT * FROM ${1:cte}' },
  { name: 'EXPLAIN ANALYZE',          alias: 'expl',  body: 'EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) ${1:SELECT * FROM table}', pg: true },
  // Writing
  { name: 'INSERT INTO … VALUES',     alias: 'ins',   body: 'INSERT INTO ${1:table} (${2:columns}) VALUES (${3:values})' },
  { name: 'INSERT … RETURNING',       alias: 'insr',  body: 'INSERT INTO ${1:table} (${2:columns}) VALUES (${3:values}) RETURNING *', pg: true },
  { name: 'INSERT … ON CONFLICT (upsert)', alias: 'ups', body: 'INSERT INTO ${1:table} (${2:id}, ${3:column}) VALUES (${4:1}, ${5:value}) ON CONFLICT (${2:id}) DO UPDATE SET ${3:column} = EXCLUDED.${3:column}', pg: true },
  { name: 'UPDATE … SET … WHERE',     alias: 'upd',   body: 'UPDATE ${1:table} SET ${2:column} = ${3:value} WHERE ${4:id} = ${5:1}' },
  { name: 'UPDATE … RETURNING',       alias: 'updr',  body: 'UPDATE ${1:table} SET ${2:column} = ${3:value} WHERE ${4:id} = ${5:1} RETURNING *', pg: true },
  { name: 'DELETE FROM … WHERE',      alias: 'del',   body: 'DELETE FROM ${1:table} WHERE ${2:id} = ${3:1}' },
  { name: 'BEGIN … COMMIT',           alias: 'tx',    body: 'BEGIN;\n\n${1}\n\nCOMMIT;' },
  // Schema
  { name: 'CREATE TABLE',             alias: 'ct',    body: 'CREATE TABLE ${1:name} (\n  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,\n  ${2:column} ${3:text} NOT NULL,\n  created_at timestamptz NOT NULL DEFAULT now()\n)', pg: true },
  { name: 'CREATE INDEX',             alias: 'ci',    body: 'CREATE INDEX ${1:idx_name} ON ${2:table} (${3:column})' },
  { name: 'ALTER TABLE ADD COLUMN',   alias: 'atc',   body: 'ALTER TABLE ${1:table} ADD COLUMN ${2:column} ${3:text}' },
  // Clauses
  { name: 'JOIN … ON',                alias: 'ij',    body: 'JOIN ${1:table} ${2:t} ON ${2:t}.${3:id} = ${4:other}.${5:id}' },
  { name: 'LEFT JOIN … ON',           alias: 'lj',    body: 'LEFT JOIN ${1:table} ${2:t} ON ${2:t}.${3:id} = ${4:other}.${5:id}' },
  { name: 'WHERE … =',                alias: 'wb',    body: 'WHERE ${1:column} = ${2:value}' },
  { name: 'WHERE EXISTS (…)',         alias: 'wex',   body: 'WHERE EXISTS (SELECT 1 FROM ${1:table} WHERE ${2:condition})' },
  { name: 'ORDER BY',                 alias: 'ob',    body: 'ORDER BY ${1:column} ${2:DESC}' },
  { name: 'GROUP BY',                 alias: 'gb',    body: 'GROUP BY ${1:column}' },
  { name: 'LIMIT … OFFSET',           alias: 'lim',   body: 'LIMIT ${1:100} OFFSET ${2:0}' },
  { name: 'CASE WHEN … END',          alias: 'case',  body: 'CASE WHEN ${1:condition} THEN ${2:result} ELSE ${3:default} END' },
  { name: 'ROW_NUMBER() OVER (…)',    alias: 'win',   body: 'ROW_NUMBER() OVER (PARTITION BY ${1:column} ORDER BY ${2:column} DESC)' },
  // Postgres housekeeping
  { name: 'SELECT table sizes',       alias: 'size',  body: "SELECT relname AS table, pg_size_pretty(pg_total_relation_size(relid)) AS total FROM pg_catalog.pg_statio_user_tables ORDER BY pg_total_relation_size(relid) DESC", pg: true },
  { name: 'SELECT running queries',   alias: 'act',   body: "SELECT pid, state, now() - query_start AS running_for, query FROM pg_stat_activity WHERE state <> 'idle' ORDER BY running_for DESC", pg: true },
  { name: 'SELECT columns of a table', alias: 'cols', body: "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = '${1:table}' ORDER BY ordinal_position" },
]

// ── Context analysis ──────────────────────────────────────────────────────────

/**
 * Analyzes the SQL before the cursor to determine:
 * - What kind of identifier is expected (table / column / any)
 * - Which tables are already referenced in the query (for column prioritization)
 * - A map of alias → canonical table name (for dot-completion)
 *
 * @param {string} textBeforeCursor
 * @param {string[]} knownTables
 * @returns {{ kind: 'table'|'column'|'any', referencedTables: string[], aliasMap: Record<string,string> }}
 */
export function analyzeQuery(textBeforeCursor, knownTables) {
  const cleaned = textBeforeCursor
    .replace(/--[^\n]*/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/\s+/g, ' ')
    .trim()

  const tableSet = new Set(knownTables.map((t) => t.toLowerCase()))
  /** @type {Record<string, string>} */
  const aliasMap = {}

  // Pass 1 - FROM/JOIN/UPDATE/INTO [schema.]tableName [AS] alias
  const joinRe = /\b(?:FROM|JOIN|UPDATE|INTO)\s+((?:[\w"`]+\.)?[\w"`]+)(?:\s+(?:AS\s+)?([\w"`]+))?/gi
  let m
  while ((m = joinRe.exec(cleaned)) !== null) {
    const tbl = (m[1].split('.').pop() ?? '').replace(/["`]/g, '').toLowerCase()
    if (!tableSet.has(tbl)) continue
    aliasMap[tbl] = tbl
    const cand = (m[2] ?? '').replace(/["`]/g, '').toLowerCase()
    if (cand && !SQL_KW_SET.has(cand.toUpperCase())) {
      aliasMap[cand] = tbl
    }
  }

  // Pass 2 - comma-separated tables in FROM clause: FROM t1 a1, t2 a2
  const fromPart = cleaned.match(/\bFROM\s+(.*?)(?=\s*\b(?:WHERE|GROUP|ORDER|HAVING|LIMIT|OFFSET|UNION|INTERSECT|EXCEPT|JOIN|;|$))/i)
  if (fromPart) {
    const segment = fromPart[1].split(/\b(?:INNER|LEFT|RIGHT|FULL|CROSS|OUTER|LATERAL)?\s*JOIN\b/i)[0]
    for (const part of segment.split(',')) {
      const pm = part.trim().match(/^([\w"]+)(?:\s+(?:AS\s+)?([\w"]+))?/)
      if (!pm) continue
      const tbl = pm[1].replace(/"/g, '').toLowerCase()
      if (!tableSet.has(tbl)) continue
      aliasMap[tbl] = tbl
      const cand = (pm[2] ?? '').replace(/"/g, '').toLowerCase()
      if (cand && !SQL_KW_SET.has(cand.toUpperCase())) {
        aliasMap[cand] = tbl
      }
    }
  }

  const referencedTables = [...new Set(Object.values(aliasMap))]

  // Detect context kind from last clause keyword before cursor
  const upper = cleaned.toUpperCase()
  const tokens = upper.split(/[\s,;()\[\]]+/).filter(Boolean)
  let kind = /** @type {'table'|'column'|'any'} */ ('any')
  for (let i = tokens.length - 1; i >= 0; i--) {
    const t = tokens[i]
    if (TABLE_CONTEXT_KWS.has(t)) { kind = 'table'; break }
    if (COLUMN_CONTEXT_KWS.has(t)) { kind = 'column'; break }
  }

  return { kind, referencedTables, aliasMap }
}

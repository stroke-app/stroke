/**
 * SQL builders for the Create trigger / sequence / table dialogs.
 *
 * Each builder takes the dialog's form state and returns the statement(s) it
 * would run. Parts the user has not filled in yet come out as placeholders,
 * `‹label›`, which the preview renders in muted italics and the editor turns
 * into snippet fields. A statement that still holds a placeholder is never run:
 * the dialogs gate their primary action on the matching `validate*` helper.
 */
import { quoteIdent } from './dml-preview.js'

// ── Placeholders ──────────────────────────────────────────────────────────────

/** A still-missing part of a statement. @param {string} label */
export const ph = (label) => `‹${label}›`

/** Matches one placeholder; group 1 is its label. */
export const PLACEHOLDER_RE = /‹([^›]*)›/g

/** @param {string} sql */
export const hasPlaceholder = (sql) => /‹[^›]*›/.test(sql)

/** The statement with each placeholder reduced to its label. @param {string} sql */
export const placeholderText = (sql) => sql.replace(PLACEHOLDER_RE, '$1')

/**
 * The statement as a CodeMirror snippet: every placeholder becomes a field Tab
 * walks to, the same label sharing one field. Braces are snippet syntax, so
 * literal ones are escaped (the object templates do the same).
 * @param {string} sql
 */
export function toSnippet(sql) {
  /** @type {Map<string, number>} */
  const fields = new Map()
  return sql
    .replace(/[{}]/g, '\\$&')
    .replace(PLACEHOLDER_RE, (_, label) => {
      if (!fields.has(label)) fields.set(label, fields.size + 1)
      return `\${${fields.get(label)}:${label}}`
    })
}

/** Single-quoted SQL string literal. @param {string} s */
export const sqlString = (s) => "'" + String(s).replace(/'/g, "''") + "'"

const INT_RE = /^-?\d+$/

// ── Trigger (PostgreSQL) ──────────────────────────────────────────────────────

export const TRIGGER_TIMINGS = /** @type {const} */ (['BEFORE', 'AFTER', 'INSTEAD OF'])
export const TRIGGER_EVENTS = /** @type {const} */ (['INSERT', 'UPDATE', 'DELETE', 'TRUNCATE'])

/**
 * @typedef {{
 *   schema: string, name: string, table: string, timing: string,
 *   events: string[], updateColumns?: string[], rowLevel: boolean,
 *   when?: string, functionName: string, functionArgs?: string,
 * }} TriggerSpec
 */

/** @param {TriggerSpec} t */
export function buildTriggerSql(t) {
  const q = (/** @type {string} */ s) => quoteIdent(s, 'postgres')
  const name = t.name.trim()
  const fn = t.functionName.trim()
  const cols = t.updateColumns ?? []
  const events = TRIGGER_EVENTS
    .filter((e) => t.events.includes(e))
    .map((e) => (e === 'UPDATE' && cols.length ? `UPDATE OF ${cols.map(q).join(', ')}` : e))
  const lines = [
    `CREATE TRIGGER ${name ? q(name) : ph('trigger name')}`,
    `  ${t.timing} ${events.length ? events.join(' OR ') : ph('event')}`,
    `  ON ${q(t.schema)}.${t.table ? q(t.table) : ph('table')}`,
    `  FOR EACH ${t.rowLevel ? 'ROW' : 'STATEMENT'}`,
  ]
  const when = (t.when ?? '').trim()
  if (when) lines.push(`  WHEN (${when})`)
  lines.push(`  EXECUTE FUNCTION ${fn ? `${q(t.schema)}.${q(fn)}` : ph('function')}(${(t.functionArgs ?? '').trim()});`)
  return lines.join('\n')
}

/**
 * Field → message for what stops the trigger from being created.
 * @param {TriggerSpec} t
 * @returns {Record<string, string>}
 */
export function validateTrigger(t) {
  /** @type {Record<string, string>} */
  const e = {}
  if (!t.name.trim()) e.name = 'Name the trigger'
  if (!t.table) e.table = 'Pick a table'
  if (!t.functionName.trim()) e.functionName = 'Pick the function the trigger runs'
  if (!t.events.length) e.events = 'Pick at least one event'
  if (t.events.includes('TRUNCATE') && t.rowLevel) e.events = 'TRUNCATE triggers fire once per statement'
  if (t.timing === 'INSTEAD OF') {
    if (t.events.includes('TRUNCATE')) e.events = 'INSTEAD OF triggers cannot fire on TRUNCATE'
    if ((t.updateColumns ?? []).length) e.events = 'INSTEAD OF triggers cannot list UPDATE OF columns'
    if ((t.when ?? '').trim()) e.when = 'INSTEAD OF triggers cannot have a WHEN condition'
  }
  return e
}

/**
 * A new trigger function to write in the editor, as a snippet.
 * @param {string} schema
 */
export function triggerFunctionSnippet(schema) {
  const q = schema && schema !== 'public' ? `${quoteIdent(schema, 'postgres')}.` : ''
  return `CREATE OR REPLACE FUNCTION ${q}\${1:trigger_fn}()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    \${0:-- NEW is the incoming row, OLD the previous one}
    RETURN NEW;
END;
$$;`
}

// ── Sequence (PostgreSQL) ─────────────────────────────────────────────────────

/** Each data type's range: the sequence defaults and the bounds it is checked against. */
export const SEQUENCE_TYPES = {
  smallint: { lo: '-32768', hi: '32767' },
  integer: { lo: '-2147483648', hi: '2147483647' },
  bigint: { lo: '-9223372036854775808', hi: '9223372036854775807' },
}

/**
 * @typedef {{
 *   schema: string, name: string, dataType: keyof typeof SEQUENCE_TYPES,
 *   increment: string, minValue: string, maxValue: string, startValue: string,
 *   cache: string, cycle: boolean, ownedTable?: string, ownedColumn?: string,
 * }} SequenceSpec
 */

/** @param {SequenceSpec} s */
export function buildSequenceSql(s) {
  const q = (/** @type {string} */ x) => quoteIdent(x, 'postgres')
  const name = s.name.trim()
  const v = (/** @type {string} */ x) => x.trim()
  const lines = [
    `CREATE SEQUENCE ${q(s.schema)}.${name ? q(name) : ph('sequence name')}`,
    `  AS ${s.dataType}`,
    `  INCREMENT BY ${v(s.increment) || '1'}`,
    v(s.minValue) ? `  MINVALUE ${v(s.minValue)}` : '  NO MINVALUE',
    v(s.maxValue) ? `  MAXVALUE ${v(s.maxValue)}` : '  NO MAXVALUE',
  ]
  if (v(s.startValue)) lines.push(`  START WITH ${v(s.startValue)}`)
  if (v(s.cache)) lines.push(`  CACHE ${v(s.cache)}`)
  lines.push(`  ${s.cycle ? 'CYCLE' : 'NO CYCLE'}`)
  if (s.ownedTable) {
    lines.push(`  OWNED BY ${q(s.schema)}.${q(s.ownedTable)}.${s.ownedColumn ? q(s.ownedColumn) : ph('column')}`)
  }
  return lines.join('\n') + ';'
}

/**
 * @param {SequenceSpec} s
 * @returns {Record<string, string>}
 */
export function validateSequence(s) {
  /** @type {Record<string, string>} */
  const e = {}
  if (!s.name.trim()) e.name = 'Name the sequence'
  const range = SEQUENCE_TYPES[s.dataType]
  const lo = BigInt(range.lo)
  const hi = BigInt(range.hi)
  /** @param {string} key @param {string} raw @param {boolean} [required] */
  const num = (key, raw, required = false) => {
    const t = raw.trim()
    if (!t) { if (required) e[key] = 'Required'; return null }
    if (!INT_RE.test(t)) { e[key] = 'Whole number'; return null }
    const n = BigInt(t)
    if (n < lo || n > hi) { e[key] = `Out of range for ${s.dataType}`; return null }
    return n
  }
  const inc = num('increment', s.increment)
  const min = num('minValue', s.minValue)
  const max = num('maxValue', s.maxValue)
  const start = num('startValue', s.startValue)
  if (inc === 0n) e.increment = 'Cannot be zero'
  if (min != null && max != null && min >= max) e.maxValue = 'Must be above the min value'
  if (start != null && min != null && start < min) e.startValue = 'Below the min value'
  if (start != null && max != null && start > max) e.startValue = 'Above the max value'
  const cache = s.cache.trim()
  if (cache && (!/^\d+$/.test(cache) || BigInt(cache) < 1n)) e.cache = 'At least 1'
  if (s.ownedTable && !s.ownedColumn) e.ownedColumn = 'Pick a column'
  return e
}

// ── Table ─────────────────────────────────────────────────────────────────────

/** @typedef {'postgres' | 'mysql' | 'sqlite'} TableFamily */

/** @param {string} dbType @returns {TableFamily} */
export function tableFamily(dbType) {
  if (dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql') return 'sqlite'
  if (dbType === 'mysql' || dbType === 'mariadb') return 'mysql'
  return 'postgres'
}

/**
 * A column type: `params` 1 takes a length (varchar(n)), 2 a precision and
 * scale (numeric(p,s)); `len` is the length filled in when it is picked.
 * @typedef {{ name: string, params?: 0 | 1 | 2, len?: string, scale?: string, group: string }} TypeDef
 */

/** @param {string} group @param {string[]} names @returns {TypeDef[]} */
const plain = (group, names) => names.map((name) => ({ name, group }))

/** @type {Record<TableFamily, TypeDef[]>} */
export const COLUMN_TYPES = {
  postgres: [
    ...plain('Numeric', ['serial', 'bigserial', 'smallint', 'integer', 'bigint']),
    { name: 'numeric', params: 2, len: '10', scale: '2', group: 'Numeric' },
    ...plain('Numeric', ['real', 'double precision']),
    ...plain('Text', ['text']),
    { name: 'varchar', params: 1, len: '255', group: 'Text' },
    { name: 'char', params: 1, len: '1', group: 'Text' },
    ...plain('Date & time', ['timestamptz', 'timestamp', 'date', 'time', 'interval']),
    ...plain('Other', ['boolean', 'uuid', 'jsonb', 'json', 'bytea', 'inet', 'text[]', 'integer[]']),
  ],
  mysql: [
    ...plain('Numeric', ['INT', 'BIGINT', 'SMALLINT', 'TINYINT']),
    { name: 'DECIMAL', params: 2, len: '10', scale: '2', group: 'Numeric' },
    ...plain('Numeric', ['FLOAT', 'DOUBLE']),
    { name: 'VARCHAR', params: 1, len: '255', group: 'Text' },
    { name: 'CHAR', params: 1, len: '36', group: 'Text' },
    ...plain('Text', ['TEXT', 'MEDIUMTEXT', 'LONGTEXT']),
    ...plain('Date & time', ['DATETIME', 'TIMESTAMP', 'DATE', 'TIME']),
    { name: 'BINARY', params: 1, len: '16', group: 'Other' },
    ...plain('Other', ['BOOLEAN', 'JSON', 'BLOB']),
  ],
  sqlite: plain('Affinity', ['INTEGER', 'TEXT', 'REAL', 'NUMERIC', 'BLOB']),
}

/** @param {TableFamily} family @param {string} type */
export const typeDef = (family, type) => COLUMN_TYPES[family].find((t) => t.name === type)

/** Quick picks for a column default. @type {Record<TableFamily, string[]>} */
export const DEFAULT_SUGGESTIONS = {
  postgres: ['now()', 'gen_random_uuid()', '0', "''", 'true', 'false'],
  mysql: ['CURRENT_TIMESTAMP', '(UUID())', '0', "''", 'AUTO_INCREMENT'],
  sqlite: ['CURRENT_TIMESTAMP', '0', "''"],
}

/** Referential actions; MySQL's InnoDB has no SET DEFAULT. @param {TableFamily} family */
export const fkActions = (family) =>
  ['NO ACTION', 'RESTRICT', 'CASCADE', 'SET NULL', ...(family === 'mysql' ? [] : ['SET DEFAULT'])]

/**
 * @typedef {{ id: string, name: string, type: string, length?: string, scale?: string,
 *   pk: boolean, notNull: boolean, unique?: boolean, defaultVal: string, comment?: string }} ColDef
 * @typedef {{ id: string, column: string, refTable: string, refColumn: string, onDelete: string, onUpdate: string }} FkDef
 * @typedef {{ id: string, columns: string[], unique: boolean }} IndexDef
 * @typedef {{ dbType: string, schema: string, name: string, columns: ColDef[],
 *   foreignKeys?: FkDef[], indexes?: IndexDef[], comment?: string }} TableSpec
 */

/** The column's full type, with its length / precision when it takes one. @param {TableFamily} family @param {ColDef} c */
export function columnType(family, c) {
  const def = typeDef(family, c.type)
  const len = (c.length ?? '').trim()
  if (!def?.params || !len) return c.type
  const scale = (c.scale ?? '').trim()
  return def.params === 2 && scale ? `${c.type}(${len},${scale})` : `${c.type}(${len})`
}

/** `<table>_<cols>_idx` (or `_key` when unique), cut to Postgres' 63-byte name limit. @param {string} table @param {IndexDef} ix */
export function indexName(table, ix) {
  return `${table}_${ix.columns.join('_')}_${ix.unique ? 'key' : 'idx'}`.slice(0, 63)
}

/**
 * The statements that create the table: CREATE TABLE, then (Postgres) its
 * comments, then its indexes. Each is run on its own so every engine accepts it.
 * @param {TableSpec} t
 * @returns {string[]}
 */
export function buildTableSql(t) {
  const family = tableFamily(t.dbType)
  const q = (/** @type {string} */ n) => quoteIdent(n, family === 'mysql' ? 'mysql' : 'postgres')
  const name = t.name.trim()
  const schema = t.schema || (family === 'mysql' ? '' : 'public')
  const qualify = (/** @type {string} */ n) => (family === 'sqlite' || !schema ? n : `${q(schema)}.${n}`)
  const target = qualify(name ? q(name) : ph('table name'))
  const colName = (/** @type {ColDef} */ c) => (c.name.trim() ? q(c.name.trim()) : ph('column name'))

  const cols = t.columns
  const pkCols = cols.filter((c) => c.pk)
  const isSerial = (/** @type {string} */ type) => type === 'serial' || type === 'bigserial'

  const lines = cols.map((c) => {
    const dflt = c.defaultVal.trim()
    const isAuto = isSerial(c.type) || dflt.toUpperCase() === 'AUTO_INCREMENT'
    let def = `  ${colName(c)} ${columnType(family, c)}`
    if (dflt && dflt.toUpperCase() !== 'AUTO_INCREMENT') def += ` DEFAULT ${dflt}`
    if (isAuto && family === 'mysql') def += ' AUTO_INCREMENT'
    // One key column is declared inline (in SQLite that makes an INTEGER
    // column the rowid alias). Several become one table constraint below -
    // SQLite rejects a PRIMARY KEY on more than one column definition.
    if (pkCols.length === 1 && c.pk) def += ' PRIMARY KEY'
    if (c.notNull && !isAuto) def += ' NOT NULL'
    if (c.unique && !(pkCols.length === 1 && c.pk)) def += ' UNIQUE'
    const comment = (c.comment ?? '').trim()
    if (comment && family === 'mysql') def += ` COMMENT ${sqlString(comment)}`
    return def
  })

  if (pkCols.length > 1) lines.push(`  PRIMARY KEY (${pkCols.map(colName).join(', ')})`)

  for (const fk of t.foreignKeys ?? []) {
    if (!fk.column && !fk.refTable) continue
    let line = `  FOREIGN KEY (${fk.column ? q(fk.column) : ph('column')}) REFERENCES ${
      fk.refTable ? qualify(q(fk.refTable)) : ph('table')
    } (${fk.refColumn ? q(fk.refColumn) : ph('column')})`
    if (fk.onDelete && fk.onDelete !== 'NO ACTION') line += ` ON DELETE ${fk.onDelete}`
    if (fk.onUpdate && fk.onUpdate !== 'NO ACTION') line += ` ON UPDATE ${fk.onUpdate}`
    lines.push(line)
  }

  const tableComment = (t.comment ?? '').trim()
  const tail = tableComment && family === 'mysql' ? ` COMMENT=${sqlString(tableComment)}` : ''
  const out = [`CREATE TABLE ${target} (\n${lines.join(',\n')}\n)${tail};`]

  if (family === 'postgres') {
    if (tableComment) out.push(`COMMENT ON TABLE ${target} IS ${sqlString(tableComment)};`)
    for (const c of cols) {
      const comment = (c.comment ?? '').trim()
      if (comment) out.push(`COMMENT ON COLUMN ${target}.${colName(c)} IS ${sqlString(comment)};`)
    }
  }

  for (const ix of t.indexes ?? []) {
    const ixName = name && ix.columns.length ? q(indexName(name, ix)) : ph('index name')
    const on = ix.columns.length ? ix.columns.map(q).join(', ') : ph('columns')
    out.push(`CREATE ${ix.unique ? 'UNIQUE ' : ''}INDEX ${ixName} ON ${target} (${on});`)
  }
  return out
}

/**
 * Keyed messages: `name`, `columns`, and per-row ids for columns, foreign keys
 * and indexes.
 * @param {TableSpec} t
 * @returns {Record<string, string>}
 */
export function validateTable(t) {
  /** @type {Record<string, string>} */
  const e = {}
  if (!t.name.trim()) e.name = 'Name the table'
  if (!t.columns.length) e.columns = 'Add at least one column'
  const family = tableFamily(t.dbType)
  /** @type {Set<string>} */
  const seen = new Set()
  for (const c of t.columns) {
    const n = c.name.trim()
    if (!n) { e[c.id] = 'Name this column or remove it'; continue }
    if (seen.has(n)) e[c.id] = `“${n}” is already a column`
    seen.add(n)
    const def = typeDef(family, c.type)
    if (def?.params) {
      const len = (c.length ?? '').trim()
      const scale = (c.scale ?? '').trim()
      if (len && !/^\d+$/.test(len)) e[c.id] = 'Length must be a whole number'
      else if (scale && !/^\d+$/.test(scale)) e[c.id] = 'Scale must be a whole number'
      else if (!len && family === 'mysql' && c.type === 'VARCHAR') e[c.id] = 'VARCHAR needs a length'
    }
  }
  for (const fk of t.foreignKeys ?? []) {
    if (!fk.column && !fk.refTable) continue
    if (!fk.column || !fk.refTable || !fk.refColumn) e[fk.id] = 'Pick the column, table and referenced column'
  }
  for (const ix of t.indexes ?? []) {
    if (!ix.columns.length) e[ix.id] = 'Pick at least one column'
  }
  return e
}

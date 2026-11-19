import { getStudioDb, STORES } from '$lib/stores/studio-db.js'
import { executeSql, executeSqlOnConnection, listSchemas, listTables, getSchemaColumnStructure, listSchemasOnConnection, listTablesOnConnection } from '$lib/api.js'
import { isSystemSchema } from '$lib/system-schemas.js'

/**
 * @typedef {object} SnapshotColumn
 * @property {string} name
 * @property {string} dataType
 * @property {boolean} nullable
 * @property {string | null} defaultValue
 * @property {number} ordinalPosition
 */

/**
 * @typedef {object} SnapshotTable
 * @property {string} name
 * @property {string} kind
 * @property {SnapshotColumn[]} columns
 */

/**
 * @typedef {object} SchemaSnapshot
 * @property {string} id
 * @property {string} connectionId
 * @property {string} connectionLabel
 * @property {string} [title]
 * @property {number} capturedAt
 * @property {{ [schema: string]: SnapshotTable[] }} tables
 * @property {{ [key: string]: string }} [ddl]
 */

/**
 * @typedef {object} ColumnDiff
 * @property {string} name
 * @property {SnapshotColumn} before
 * @property {SnapshotColumn} after
 * @property {Array<'dataType'|'nullable'|'defaultValue'>} changed
 */

/**
 * @typedef {object} TableDiff
 * @property {string} schema
 * @property {string} name
 * @property {SnapshotColumn[]} addedColumns
 * @property {SnapshotColumn[]} removedColumns
 * @property {ColumnDiff[]} modifiedColumns
 */

/**
 * @typedef {object} SnapshotDiff
 * @property {Array<{ schema: string, name: string, columns: SnapshotColumn[] }>} addedTables
 * @property {Array<{ schema: string, name: string }>} removedTables
 * @property {TableDiff[]} modifiedTables
 */

const SYSTEM_SCHEMAS = new Set(['information_schema', 'pg_catalog', 'pg_toast', 'pg_temp_1', 'pg_toast_temp_1'])

/**
 * Capture the current schema state and persist it to IndexedDB.
 * @param {string} connectionId
 * @param {string} connectionLabel
 * @param {string} dbType - 'postgres' | 'mysql' | 'sqlite' | 'd1' | 'libsql'
 * @param {import('$lib/stores/connections.js').SavedConnection | null} savedConnection - when non-null, use cross-connection APIs
 * @param {Set<string> | null} [schemasFilter] - if set, only include these schemas
 * @param {string} [title] - optional user-defined snapshot title
 * @returns {Promise<SchemaSnapshot>}
 */
export async function captureSnapshot(connectionId, connectionLabel, dbType = 'postgres', savedConnection = null, schemasFilter = null, title = '') {
  const tables = /** @type {{ [schema: string]: SnapshotTable[] }} */ ({})

  const runSql = savedConnection
    ? (/** @type {string} */ sql) => executeSqlOnConnection(savedConnection, sql)
    : (/** @type {string} */ sql) => executeSql(sql)

  /** @type {unknown} */
  let captureError = null
  try {
    if (dbType === 'postgres') {
      const colsResult = await runSql(`
        SELECT
          c.table_schema,
          c.table_name,
          t.table_type,
          c.column_name,
          c.data_type,
          c.is_nullable,
          c.column_default,
          c.ordinal_position
        FROM information_schema.columns c
        JOIN information_schema.tables t
          ON t.table_schema = c.table_schema AND t.table_name = c.table_name
        WHERE c.table_schema NOT IN ('information_schema','pg_catalog','pg_toast')
        ORDER BY c.table_schema, c.table_name, c.ordinal_position
      `)
      _buildTablesFromInfoSchema(colsResult, tables)
    } else if (dbType === 'mysql') {
      const colsResult = await runSql(`
        SELECT
          c.table_schema,
          c.table_name,
          t.table_type,
          c.column_name,
          c.data_type,
          c.is_nullable,
          c.column_default,
          c.ordinal_position
        FROM information_schema.columns c
        JOIN information_schema.tables t
          ON t.table_schema = c.table_schema AND t.table_name = c.table_name
        WHERE c.table_schema = DATABASE()
        ORDER BY c.table_name, c.ordinal_position
      `)
      _buildTablesFromInfoSchema(colsResult, tables)
    } else if (!SQLITE_FAMILY.has(dbType)) {
      // Every other engine (Oracle, SQL Server, ClickHouse, DuckDB, MongoDB,
      // Redis, DynamoDB...): the same schema reader the sidebar and the ER
      // diagram use, so a snapshot works wherever browsing does. This used to
      // fall through to the SQLite branch and ask Oracle for sqlite_master.
      await _captureWithSchemaReader(tables, savedConnection, schemasFilter)
    } else {
      // SQLite / D1 / LibSQL
      const tablesResult = await runSql(
        `SELECT name, type FROM sqlite_master WHERE type IN ('table','view') ORDER BY name`
      )
      const schema = 'main'
      tables[schema] = []
      for (const row of tablesResult.rows ?? []) {
        const name = String(row[0])
        const kind = String(row[1])
        const cols = /** @type {SnapshotColumn[]} */ ([])
        try {
          // SQL identifier escaping: double any embedded double-quote
          // (JSON.stringify's backslash escapes are not valid SQL).
          const pragma = await runSql(`PRAGMA table_info("${name.replace(/"/g, '""')}")`)
          for (const cr of pragma.rows ?? []) {
            cols.push({
              name: String(cr[1]),
              dataType: String(cr[2]),
              nullable: cr[3] === 0 || cr[3] === '0',
              defaultValue: cr[4] != null ? String(cr[4]) : null,
              ordinalPosition: Number(cr[0]) + 1,
            })
          }
        } catch { /* ignore per-table errors */ }
        tables[schema].push({ name, kind, columns: cols })
      }
    }
  } catch (err) { captureError = err }

  // A total capture failure must not persist an empty snapshot - the Schema
  // Timeline would diff it against a real one and report every table as
  // removed. Partial per-table failures above are legitimate and kept.
  if (captureError && Object.keys(tables).length === 0) {
    const msg = captureError instanceof Error ? captureError.message : String(captureError)
    throw new Error(`Snapshot capture failed: ${msg}`)
  }

  // Apply schema filter (client-side, no extra query)
  if (schemasFilter && schemasFilter.size > 0) {
    for (const schema of Object.keys(tables)) {
      if (!schemasFilter.has(schema)) delete tables[schema]
    }
  }

  // Generate DDL inline from already-captured column data - no extra API calls
  const ddl = /** @type {{ [key: string]: string }} */ ({})
  for (const [schema, tableList] of Object.entries(tables)) {
    for (const t of tableList) {
      const d = _buildPseudoDdl(schema, t.name, t.columns)
      if (d) ddl[`${schema}.${t.name}`] = d
    }
  }

  const snapshot = /** @type {SchemaSnapshot} */ ({
    id: crypto.randomUUID(),
    connectionId,
    connectionLabel,
    ...(title ? { title } : {}),
    capturedAt: Date.now(),
    tables,
    ddl,
  })

  const db = await getStudioDb()
  await db.put(STORES.schemaSnapshots, snapshot)
  return snapshot
}

const SQLITE_FAMILY = new Set(['sqlite', 'd1', 'libsql', 'turso'])

/**
 * Tables and columns through the app's own schema reader. The active
 * connection gets columns; another saved connection gets table names only
 * (the reader works on the open session).
 * @param {{ [schema: string]: SnapshotTable[] }} tables
 * @param {any} savedConnection
 * @param {Set<string> | null} schemasFilter
 */
async function _captureWithSchemaReader(tables, savedConnection, schemasFilter) {
  const wanted = (/** @type {string} */ s) => !isSystemSchema(s) && (!schemasFilter || schemasFilter.size === 0 || schemasFilter.has(s))
  if (savedConnection) {
    const schemas = (await listSchemasOnConnection(savedConnection)).filter(wanted)
    for (const schema of schemas) {
      const names = await listTablesOnConnection(savedConnection, schema).catch(() => [])
      tables[schema] = names.map((name) => ({ name: String(name), kind: 'table', columns: [] }))
    }
    return
  }
  const schemas = /** @type {string[]} */ (await listSchemas()).filter(wanted)
  for (const schema of schemas) {
    const list = /** @type {{ name: string, kind?: string }[]} */ (await listTables(schema).catch(() => []))
    /** @type {Map<string, SnapshotColumn[]>} */
    const byTable = new Map()
    try {
      for (const t of await getSchemaColumnStructure(schema)) {
        byTable.set(t.table, (t.columns ?? []).map((/** @type {any} */ c, /** @type {number} */ i) => ({
          name: String(c.name),
          dataType: String(c.dataType ?? ''),
          nullable: !!c.isNullable,
          defaultValue: c.columnDefault != null ? String(c.columnDefault) : null,
          ordinalPosition: Number(c.ordinalPosition ?? i + 1),
        })))
      }
    } catch { /* names alone still make a useful snapshot */ }
    tables[schema] = list.map((t) => ({
      name: t.name,
      kind: String(t.kind ?? 'table').includes('view') ? 'view' : 'table',
      columns: byTable.get(t.name) ?? [],
    }))
  }
}

/**
 * @param {{ columns: any[], rows: any[][] }} result
 * @param {{ [schema: string]: SnapshotTable[] }} tables
 */
function _buildTablesFromInfoSchema(result, tables) {
  const idxMap = /** @type {Record<string,number>} */ ({})
  ;(result.columns ?? []).forEach((c, i) => { idxMap[c.name] = i })

  const get = (row, col) => row[idxMap[col]]

  for (const row of result.rows ?? []) {
    const schema = String(get(row, 'table_schema'))
    const tableName = String(get(row, 'table_name'))
    const tableType = String(get(row, 'table_type') ?? '').toLowerCase()
    const kind = tableType.includes('view') ? 'view' : 'table'

    if (SYSTEM_SCHEMAS.has(schema)) continue

    if (!tables[schema]) tables[schema] = []
    let tbl = tables[schema].find((t) => t.name === tableName)
    if (!tbl) {
      tbl = { name: tableName, kind, columns: [] }
      tables[schema].push(tbl)
    }

    const colName = String(get(row, 'column_name'))
    if (colName) {
      tbl.columns.push({
        name: colName,
        dataType: String(get(row, 'data_type') ?? ''),
        nullable: String(get(row, 'is_nullable') ?? 'YES').toUpperCase() !== 'NO',
        defaultValue: get(row, 'column_default') != null ? String(get(row, 'column_default')) : null,
        ordinalPosition: Number(get(row, 'ordinal_position') ?? 0),
      })
    }
  }
}

/** @param {string} connectionId @returns {Promise<SchemaSnapshot[]>} */
export async function listSnapshots(connectionId) {
  if (!connectionId) return []
  const db = await getStudioDb()
  const all = await db.getAllFromIndex(STORES.schemaSnapshots, 'connectionId', connectionId)
  return all.sort((a, b) => b.capturedAt - a.capturedAt)
}

/** @returns {Promise<SchemaSnapshot[]>} */
export async function listAllSnapshots() {
  const db = await getStudioDb()
  const all = await db.getAll(STORES.schemaSnapshots)
  return all.sort((a, b) => b.capturedAt - a.capturedAt)
}

/** @param {string} id */
export async function deleteSnapshot(id) {
  const db = await getStudioDb()
  await db.delete(STORES.schemaSnapshots, id)
}

export async function deleteAllSnapshots() {
  const db = await getStudioDb()
  await db.clear(STORES.schemaSnapshots)
}

/**
 * @param {SchemaSnapshot} before
 * @param {SchemaSnapshot} after
 * @returns {SnapshotDiff}
 */
export function diffSnapshots(before, after) {
  const addedTables = /** @type {SnapshotDiff['addedTables']} */ ([])
  const removedTables = /** @type {SnapshotDiff['removedTables']} */ ([])
  const modifiedTables = /** @type {TableDiff[]} */ ([])

  const allSchemas = new Set([
    ...Object.keys(before.tables ?? {}),
    ...Object.keys(after.tables ?? {}),
  ])

  for (const schema of allSchemas) {
    const beforeTables = new Map((before.tables?.[schema] ?? []).map((t) => [t.name, t]))
    const afterTables = new Map((after.tables?.[schema] ?? []).map((t) => [t.name, t]))

    for (const [name, t] of afterTables) {
      if (!beforeTables.has(name)) addedTables.push({ schema, name, columns: t.columns })
    }
    for (const [name] of beforeTables) {
      if (!afterTables.has(name)) removedTables.push({ schema, name })
    }

    for (const [name, bt] of beforeTables) {
      const at = afterTables.get(name)
      if (!at) continue

      const bc = new Map((bt.columns ?? []).map((c) => [c.name, c]))
      const ac = new Map((at.columns ?? []).map((c) => [c.name, c]))

      const addedColumns = /** @type {SnapshotColumn[]} */ ([])
      const removedColumns = /** @type {SnapshotColumn[]} */ ([])
      const modifiedColumns = /** @type {ColumnDiff[]} */ ([])

      for (const [cn, col] of ac) { if (!bc.has(cn)) addedColumns.push(col) }
      for (const [cn, col] of bc) { if (!ac.has(cn)) removedColumns.push(col) }
      for (const [cn, bcol] of bc) {
        const acol = ac.get(cn)
        if (!acol) continue
        const changed = /** @type {ColumnDiff['changed']} */ ([])
        if (bcol.dataType !== acol.dataType) changed.push('dataType')
        if (bcol.nullable !== acol.nullable) changed.push('nullable')
        if (bcol.defaultValue !== acol.defaultValue) changed.push('defaultValue')
        if (changed.length) {
          modifiedColumns.push({ name: cn, before: bcol, after: acol, changed })
        }
      }

      if (addedColumns.length || removedColumns.length || modifiedColumns.length) {
        modifiedTables.push({ schema, name, addedColumns, removedColumns, modifiedColumns })
      }
    }
  }

  return { addedTables, removedTables, modifiedTables }
}

/** @param {SnapshotDiff} diff */
export function isDiffEmpty(diff) {
  return !diff.addedTables.length && !diff.removedTables.length && !diff.modifiedTables.length
}

/**
 * @param {string} schema
 * @param {string} tableName
 * @param {SnapshotColumn[]} columns
 */
function _buildPseudoDdl(schema, tableName, columns) {
  if (!columns.length) return ''
  const lines = columns.map((col) => {
    let line = `  "${col.name}" ${col.dataType}`
    if (col.defaultValue != null) line += ` DEFAULT ${col.defaultValue}`
    if (!col.nullable) line += ` NOT NULL`
    return line
  })
  return `CREATE TABLE "${schema}"."${tableName}" (\n${lines.join(',\n')}\n);`
}
